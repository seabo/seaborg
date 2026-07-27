"""The PyTorch NNUE model, mirroring the design contract's float topology.

The architecture and every dimension the contract marks variable live here so
the trainer, the quantized export, and the Rust inference path all describe the
same network. The contract (`docs/nnue-design-contract.md`) is authoritative;
this module is its float embodiment:

    feature transformer (768 -> H, per perspective)
        -> concat(acc[stm], acc[nstm])          side-to-move first
        -> clipped ReLU                          activation
        -> linear (2H -> 1)                      scalar output `fout`

The scalar output is in "win-probability logit" units: at inference the integer
path multiplies it by SCALE/(QA*QB) to reach centipawns, so `fout == eval_cp /
SCALE`, and training compares `sigmoid(fout)` against a win-probability target
built with the same SCALE. That shared SCALE is what keeps the value the network
learns to emit and the value search consumes the same quantity.

The feature transformer is an ``nn.EmbeddingBag`` whose weight is laid out
``[input_dim, H]`` -- exactly the feature-major order the on-disk ``W_ft`` block
uses (one feature's H weights contiguous), so quantized export (`export.py`)
serialises the weight without transposing it. A position's active features (one
per piece) are summed by the bag, which is both the fast sparse operation and a
direct model of the accumulator.

Quantization-aware training. The engine runs an integer network: the
feature-transformer weights become i16 at scale ``QA``, the output weights i16 at
scale ``QB``, and activations are clipped into ``[0, QA]``. If the float model
trained oblivious to that, its exported integer form would round to a different
function -- the ``QB = 64`` output-weight grid alone shifts the score by tens of
centipawns. So when ``quantization_aware`` is set, the forward pass rounds weights
and activations onto exactly those integer grids (with a straight-through
gradient, so the rounding does not block learning). Training then optimises the
quantized behaviour directly, and the exported network reproduces what the model
already computed rather than a nearby float function it never saw. Independently,
:meth:`NnueModel.clamp_for_quantization` bounds the feature-transformer weights so
the i16 accumulator cannot overflow for any legal position; the contract makes
that overflow a defect, not a wrap.
"""

from __future__ import annotations

from dataclasses import dataclass, field

import torch
from torch import nn
from torch.nn import functional as F

# Activation ids from the contract's file-format header. v1 Rust inference only
# implements CReLU (id 0); SCReLU (id 1) is reserved there but usable as a
# training-side choice, so both are exposed as configuration here.
ACTIVATION_IDS = {"crelu": 0, "screlu": 1}

# Feature-set id 0 is the perspective-doubled 768-input piece-square set.
PERSPECTIVE_768_ID = 0
PERSPECTIVE_768_DIM = 768

# The most features that can be active in one perspective: one per piece, and a
# legal position holds at most 32 pieces. The accumulator is the bias plus this
# many feature-transformer weight columns, so bounding those magnitudes bounds the
# accumulator -- see :meth:`NnueModel.clamp_for_quantization`.
MAX_ACTIVE_FEATURES = 32

# The i16 accumulator saturates here; the contract forbids reaching it.
_I16_MAX = 32767

# The symmetric int8 range the version-2 output-stack weights are held to.
_I8_MAX = 127


class _FakeQuantize(torch.autograd.Function):
    """Round to the integer grid ``round(x * scale) / scale`` on the forward pass
    while passing the gradient straight through unchanged.

    This is the standard quantization-aware-training estimator: the forward value
    is exactly what the integer engine will compute (``torch.round`` rounds halves
    to even, matching the NumPy/PyTorch rounding the exporter uses), but rounding
    has a zero gradient almost everywhere, so treating it as the identity for the
    backward pass lets the weights keep learning through it."""

    @staticmethod
    def forward(ctx, x: torch.Tensor, scale: float) -> torch.Tensor:
        return torch.round(x * scale) / scale

    @staticmethod
    def backward(ctx, grad_output):
        # Straight-through: identity in x, and scale is a constant.
        return grad_output, None


def fake_quantize(x: torch.Tensor, scale: float) -> torch.Tensor:
    """Snap ``x`` onto the ``1 / scale`` grid with a straight-through gradient."""
    return _FakeQuantize.apply(x, float(scale))


@dataclass
class NnueConfig:
    """The parameterizable dimensions of the network, as the contract defines
    them. Fields that a loader stores in the file header are named to match it.

    Only the dimensions the contract marks variable are configurable; the fixed
    structure (two perspectives, side-to-move-first concatenation, single hidden
    stage) is baked into :class:`NnueModel`.
    """

    hidden: int = 256
    activation: str = "crelu"
    scale: int = 400
    qa: int = 255
    qb: int = 64
    feature_set_id: int = PERSPECTIVE_768_ID
    input_dim: int = PERSPECTIVE_768_DIM
    output_dim: int = 1
    # Version-2 topology. ``output_stack`` is the tuple of hidden output-stack
    # dimensions (e.g. ``(16, 32)`` for ``2H -> 16 -> 32 -> 1``); ``None`` selects
    # the version-1 single linear output. ``num_buckets`` is the number of
    # piece-count-selected stacks (only meaningful when ``output_stack`` is set).
    # ``output_stack_scales`` are the per-layer int8 weight scales ``QB_k`` (one
    # per stack layer including the final ``-> 1``); ``None`` defaults every layer
    # to ``qb``. See docs/nnue-topology-v2.md.
    num_buckets: int = 1
    output_stack: tuple[int, ...] | None = None
    output_stack_scales: tuple[int, ...] | None = None

    @property
    def is_bucketed(self) -> bool:
        """Whether this is a version-2 bucketed multi-layer network."""
        return self.output_stack is not None

    @property
    def stack_layer_dims(self) -> tuple[int, ...]:
        """The per-layer output dimensions of the stack, ending in ``output_dim``."""
        return tuple(self.output_stack or ()) + (self.output_dim,)

    @property
    def stack_layer_scales(self) -> tuple[int, ...]:
        """The per-layer int8 weight scales ``QB_k``; defaults every layer to ``qb``."""
        if self.output_stack_scales is not None:
            return tuple(self.output_stack_scales)
        return tuple(self.qb for _ in self.stack_layer_dims)

    def validate(self) -> None:
        """Reject a configuration the contract forbids, with the same rules the
        file-format loader applies, so an invalid net is caught at construction
        rather than at export or load."""
        if self.feature_set_id != PERSPECTIVE_768_ID:
            raise ValueError(f"unknown feature_set_id {self.feature_set_id}")
        if self.input_dim != PERSPECTIVE_768_DIM:
            raise ValueError(
                f"feature_set_id {self.feature_set_id} requires input_dim "
                f"{PERSPECTIVE_768_DIM}, got {self.input_dim}"
            )
        # H must be a positive multiple of 16 so one file loads unchanged into
        # both the scalar and the AVX2 inference paths (16 i16 lanes at a time).
        if self.hidden <= 0 or self.hidden % 16 != 0:
            raise ValueError(f"hidden width must be a positive multiple of 16, got {self.hidden}")
        if self.output_dim != 1:
            raise ValueError(f"output_dim must be 1, got {self.output_dim}")
        if self.activation not in ACTIVATION_IDS:
            raise ValueError(f"unknown activation {self.activation!r}")
        if self.qa <= 0 or self.qb <= 0 or self.scale <= 0:
            raise ValueError("qa, qb, and scale must all be positive")

        if self.is_bucketed:
            if not 1 <= self.num_buckets <= 32:
                raise ValueError(f"num_buckets must be in 1..=32, got {self.num_buckets}")
            for d in self.output_stack:
                # Each hidden stack dim is a positive multiple of 16 so every layer's
                # input dimension is, keeping the SIMD kernel remainder-free.
                if d <= 0 or d % 16 != 0:
                    raise ValueError(f"output-stack dim {d} must be a positive multiple of 16")
            scales = self.stack_layer_scales
            if len(scales) != len(self.stack_layer_dims):
                raise ValueError("output_stack_scales must have one entry per stack layer")
            if any(s <= 0 for s in scales):
                raise ValueError("every output-stack scale must be positive")
            if scales[-1] != self.qb:
                raise ValueError(f"final output-stack scale {scales[-1]} must equal qb {self.qb}")
        elif self.num_buckets != 1:
            raise ValueError("num_buckets applies only to a bucketed (output_stack) network")

    @property
    def activation_id(self) -> int:
        return ACTIVATION_IDS[self.activation]


class NnueModel(nn.Module):
    """The float NNUE network. A forward pass takes the sparse active features of
    a batch, in the ``EmbeddingBag`` (flat indices + per-sample offsets) form the
    dataloader produces, for the side-to-move and non-side-to-move perspectives
    separately."""

    def __init__(
        self, config: NnueConfig | None = None, *, quantization_aware: bool = False
    ) -> None:
        super().__init__()
        self.config = config or NnueConfig()
        self.config.validate()
        # When set, the forward pass rounds weights and activations onto the
        # engine's integer grids so training optimises the quantized behaviour
        # directly. Off by default: a plain-float forward is what callers that only
        # want the fp32 model (and the architecture-property tests) expect.
        self.quantization_aware = quantization_aware

        # One shared feature transformer feeds both perspectives; weight layout
        # [input_dim, H] matches the on-disk W_ft feature-major order.
        self.feature_transformer = nn.EmbeddingBag(
            self.config.input_dim, self.config.hidden, mode="sum"
        )
        self.ft_bias = nn.Parameter(torch.zeros(self.config.hidden))

        if self.config.is_bucketed:
            # A version-2 bucketed stack: for each layer, a weight tensor
            # [num_buckets, out, in] and bias [num_buckets, out], so a sample routes
            # through its bucket by index-select. The final layer emits one scalar.
            self.output = None
            self.stack_weights = nn.ParameterList()
            self.stack_biases = nn.ParameterList()
            in_dim = 2 * self.config.hidden
            for out_dim in self.config.stack_layer_dims:
                self.stack_weights.append(
                    nn.Parameter(torch.empty(self.config.num_buckets, out_dim, in_dim))
                )
                self.stack_biases.append(
                    nn.Parameter(torch.zeros(self.config.num_buckets, out_dim))
                )
                in_dim = out_dim
        else:
            self.output = nn.Linear(2 * self.config.hidden, self.config.output_dim)
            self.stack_weights = None
            self.stack_biases = None

        self._reset_parameters()

    def _reset_parameters(self) -> None:
        # Small feature-transformer weights keep the summed accumulator (up to 32
        # active features) inside the clipped-ReLU active band, so the network is
        # not born saturated at 0 or 1 with no gradient.
        nn.init.normal_(self.feature_transformer.weight, mean=0.0, std=0.1)
        nn.init.zeros_(self.ft_bias)
        if self.config.is_bucketed:
            # Kaiming-style small init per bucket keeps early stack activations in
            # the clipped-ReLU active band across all buckets.
            for weight in self.stack_weights:
                fan_in = weight.shape[-1]
                nn.init.normal_(weight, mean=0.0, std=(1.0 / fan_in) ** 0.5)

    def accumulator(self, indices: torch.Tensor, offsets: torch.Tensor) -> torch.Tensor:
        """The per-perspective accumulator: sum of the active features' weight
        columns plus the shared bias.

        Under quantization-aware training the feature-transformer weights and bias
        are rounded onto the i16 grid at scale ``QA`` first, so the summed
        accumulator matches the integer engine's ``b_ft + Σ W_ft`` exactly."""
        weight = self.feature_transformer.weight
        bias = self.ft_bias
        if self.quantization_aware:
            weight = fake_quantize(weight, self.config.qa)
            bias = fake_quantize(bias, self.config.qa)
        return F.embedding_bag(indices, weight, offsets, mode="sum") + bias

    def _activate(self, x: torch.Tensor) -> torch.Tensor:
        clipped = torch.clamp(x, 0.0, 1.0)
        activated = clipped * clipped if self.config.activation == "screlu" else clipped
        if self.quantization_aware:
            # The integer path holds activations as i16 in [0, QA]; 1.0 maps to QA,
            # so the float activation lives on the same 1/QA grid.
            activated = fake_quantize(activated, self.config.qa)
        return activated

    def forward(
        self,
        stm_indices: torch.Tensor,
        stm_offsets: torch.Tensor,
        nstm_indices: torch.Tensor,
        nstm_offsets: torch.Tensor,
        bucket: torch.Tensor | None = None,
    ) -> torch.Tensor:
        """Return the scalar output `fout` for each sample in the batch, in
        SCALE-normalised units (`fout == eval_cp / SCALE`).

        For a bucketed (version-2) network ``bucket`` is a per-sample ``LongTensor``
        of stack indices; it is ignored by a single-layer (version-1) network."""
        stm_acc = self.accumulator(stm_indices, stm_offsets)
        nstm_acc = self.accumulator(nstm_indices, nstm_offsets)
        # Side-to-move first: this ordering (not colour order) is what makes a
        # position and its colour-flipped mirror evaluate equal and opposite.
        x = torch.cat((stm_acc, nstm_acc), dim=1)
        x = self._activate(x)

        if self.config.is_bucketed:
            return self._bucketed_output(x, bucket)

        weight = self.output.weight
        bias = self.output.bias
        if self.quantization_aware:
            # Output weights quantize to i16 at scale QB, the output bias to i32 at
            # scale QA*QB (it is added into the same accumulator as activation*weight
            # products, whose scale is QA*QB).
            weight = fake_quantize(weight, self.config.qb)
            if bias is not None:
                bias = fake_quantize(bias, self.config.qa * self.config.qb)
        return F.linear(x, weight, bias).squeeze(1)

    def _bucketed_output(self, activated: torch.Tensor, bucket: torch.Tensor) -> torch.Tensor:
        """Run the activated ``2H`` input through each sample's selected bucket stack.

        Each layer's per-sample weights are gathered by bucket and applied with a
        batched matmul; the header activation is applied between layers (fake-quantized
        onto the ``1/QA`` grid under QAT), and the final layer emits the raw scalar.
        Under QAT each layer's weights are fake-quantized onto its own ``1/QB_k`` int8
        grid and its bias onto ``1/(QA*QB_k)``, mirroring the integer export."""
        if bucket is None:
            raise ValueError("a bucketed network requires a per-sample bucket tensor")
        scales = self.config.stack_layer_scales
        cur = activated
        last = len(self.stack_weights) - 1
        for k, (weight, bias) in enumerate(zip(self.stack_weights, self.stack_biases)):
            if self.quantization_aware:
                weight = fake_quantize(weight, scales[k])
                bias = fake_quantize(bias, self.config.qa * scales[k])
            # Gather this sample's bucket: [batch, out, in] and [batch, out].
            w = weight[bucket]
            b = bias[bucket]
            acc = torch.bmm(w, cur.unsqueeze(-1)).squeeze(-1) + b
            if k == last:
                return acc.squeeze(-1)
            cur = self._activate(acc)
        # Unreachable: the loop returns at the final layer.
        raise AssertionError("bucketed stack has no layers")

    @torch.no_grad()
    def clamp_for_quantization(self) -> None:
        """Bound the feature-transformer weights and bias so the i16 accumulator
        cannot overflow for any legal position.

        The accumulator for a hidden unit is ``b_ft + Σ W_ft`` over the active
        features, at most :data:`MAX_ACTIVE_FEATURES` of them. Quantized, that is
        ``round(b_ft·QA) + Σ round(W_ft·QA)``. Bounding every float magnitude by
        ``limit`` bounds the quantized accumulator by ``(1 + MAX_ACTIVE_FEATURES) ·
        (limit·QA + 0.5)``; requiring that not to exceed ``i16::MAX`` gives the
        ``limit`` below. The learned weights sit far inside it -- a single feature
        column of order 1 already saturates the clipped activation -- so this is a
        guard against pathological growth, not a constraint the optimiser feels."""
        terms = 1 + MAX_ACTIVE_FEATURES
        limit = (_I16_MAX / terms - 0.5) / self.config.qa
        self.feature_transformer.weight.clamp_(-limit, limit)
        self.ft_bias.clamp_(-limit, limit)

        if self.config.is_bucketed:
            # Each stack layer's weights quantize to i8 at that layer's scale, so
            # bound them to the largest float that still rounds inside [-127, 127].
            scales = self.config.stack_layer_scales
            for weight, scale in zip(self.stack_weights, scales):
                weight_limit = (_I8_MAX - 0.5) / scale
                weight.clamp_(-weight_limit, weight_limit)
