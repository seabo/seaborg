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
uses (one feature's H weights contiguous), so quantized export (a later task)
serialises the weight without transposing it. A position's active features (one
per piece) are summed by the bag, which is both the fast sparse operation and a
direct model of the accumulator.
"""

from __future__ import annotations

from dataclasses import dataclass, field

import torch
from torch import nn

# Activation ids from the contract's file-format header. v1 Rust inference only
# implements CReLU (id 0); SCReLU (id 1) is reserved there but usable as a
# training-side choice, so both are exposed as configuration here.
ACTIVATION_IDS = {"crelu": 0, "screlu": 1}

# Feature-set id 0 is the perspective-doubled 768-input piece-square set.
PERSPECTIVE_768_ID = 0
PERSPECTIVE_768_DIM = 768


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

    @property
    def activation_id(self) -> int:
        return ACTIVATION_IDS[self.activation]


class NnueModel(nn.Module):
    """The float NNUE network. A forward pass takes the sparse active features of
    a batch, in the ``EmbeddingBag`` (flat indices + per-sample offsets) form the
    dataloader produces, for the side-to-move and non-side-to-move perspectives
    separately."""

    def __init__(self, config: NnueConfig | None = None) -> None:
        super().__init__()
        self.config = config or NnueConfig()
        self.config.validate()

        # One shared feature transformer feeds both perspectives; weight layout
        # [input_dim, H] matches the on-disk W_ft feature-major order.
        self.feature_transformer = nn.EmbeddingBag(
            self.config.input_dim, self.config.hidden, mode="sum"
        )
        self.ft_bias = nn.Parameter(torch.zeros(self.config.hidden))
        self.output = nn.Linear(2 * self.config.hidden, self.config.output_dim)

        self._reset_parameters()

    def _reset_parameters(self) -> None:
        # Small feature-transformer weights keep the summed accumulator (up to 32
        # active features) inside the clipped-ReLU active band, so the network is
        # not born saturated at 0 or 1 with no gradient.
        nn.init.normal_(self.feature_transformer.weight, mean=0.0, std=0.1)
        nn.init.zeros_(self.ft_bias)

    def accumulator(self, indices: torch.Tensor, offsets: torch.Tensor) -> torch.Tensor:
        """The per-perspective accumulator: sum of the active features' weight
        columns plus the shared bias."""
        return self.feature_transformer(indices, offsets) + self.ft_bias

    def _activate(self, x: torch.Tensor) -> torch.Tensor:
        clipped = torch.clamp(x, 0.0, 1.0)
        if self.config.activation == "screlu":
            return clipped * clipped
        return clipped

    def forward(
        self,
        stm_indices: torch.Tensor,
        stm_offsets: torch.Tensor,
        nstm_indices: torch.Tensor,
        nstm_offsets: torch.Tensor,
    ) -> torch.Tensor:
        """Return the scalar output `fout` for each sample in the batch, in
        SCALE-normalised units (`fout == eval_cp / SCALE`)."""
        stm_acc = self.accumulator(stm_indices, stm_offsets)
        nstm_acc = self.accumulator(nstm_indices, nstm_offsets)
        # Side-to-move first: this ordering (not colour order) is what makes a
        # position and its colour-flipped mirror evaluate equal and opposite.
        x = torch.cat((stm_acc, nstm_acc), dim=1)
        x = self._activate(x)
        return self.output(x).squeeze(1)
