//! The `SBNN` versioned network file format: header layout, in-memory
//! [`Network`], writer, and the deterministic loader.
//!
//! Everything a reader needs to interpret the bytes — the architecture
//! dimensions and the quantization scales — is stored in a fixed 64-byte
//! little-endian header and re-validated on load. A file whose header names an
//! architecture this build cannot evaluate, or whose blob does not hash to the
//! stored value, is rejected before any weight is allocated or interpreted. The
//! byte layout is normative and shared with the Python trainer; see
//! `docs/nnue-design-contract.md`.

use std::io::{self, Read, Write};

/// Header magic identifying a Seaborg NNUE file: the ASCII bytes `SBNN`.
pub const MAGIC: [u8; 4] = *b"SBNN";

/// Length of the fixed header in bytes. Every field that determines how the
/// parameter blob is read lives inside this prefix.
pub const HEADER_LEN: usize = 64;

/// Format version 1: a single linear output layer (`2H → 1`), i16 output
/// weights at scale `QB`, i32 output bias. This is what [`Network::write`] emits
/// for a [`OutputStack::Single`] network, and what the built-in default network
/// carries. See `docs/nnue-design-contract.md`.
pub const FORMAT_VERSION_V1: u16 = 1;

/// Format version 2: a bucketed multi-layer output stack with an int8 dense
/// tail. See `docs/nnue-topology-v2.md`.
pub const FORMAT_VERSION_V2: u16 = 2;

/// The baseline format version, retained under its original name for callers and
/// tests that predate the version-2 topology. New code should prefer the explicit
/// [`FORMAT_VERSION_V1`] / [`FORMAT_VERSION_V2`].
pub const FORMAT_VERSION: u16 = FORMAT_VERSION_V1;

/// A network's output stack must have at least this many buckets and at most this
/// many, so the piece-count bucket rule maps into a real stack and the count fits
/// the byte binning the selection uses.
const MIN_BUCKETS: u16 = 1;
const MAX_BUCKETS: u16 = 32;

/// Feature-set id for the perspective-doubled 768-input piece-square set — the
/// only set this build implements.
pub const FEATURE_SET_PERSPECTIVE_768: u16 = 0;

/// Input dimension implied by [`FEATURE_SET_PERSPECTIVE_768`]:
/// `2 colours × 6 piece types × 64 squares`, one perspective's sparse input.
pub const INPUT_DIM: u32 = 768;

/// Activation id for clipped ReLU: `a = clamp(x, 0, QA)`.
pub const ACTIVATION_CRELU: u16 = 0;

/// Activation id for squared clipped ReLU: `a = round_div(clamp(x, 0, QA)^2, QA)`.
pub const ACTIVATION_SCRELU: u16 = 1;

/// The elementwise activation applied to the concatenated perspectives before the
/// output layer. It is the only stage that differs between activation ids; every
/// other stage of the forward pass is identical, and both variants produce a value
/// in `[0, QA]` so they share the same i16 activation domain and output kernels.
/// See `docs/nnue-design-contract.md` for the normative integer arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activation {
    /// Clipped ReLU: `a = clamp(x, 0, QA)`.
    ClippedRelu,
    /// Squared clipped ReLU: `a = round_div(clamp(x, 0, QA)^2, QA)`.
    SquaredClippedRelu,
}

impl Activation {
    /// The on-disk `activation_id` this variant serializes as.
    pub fn id(self) -> u16 {
        match self {
            Activation::ClippedRelu => ACTIVATION_CRELU,
            Activation::SquaredClippedRelu => ACTIVATION_SCRELU,
        }
    }

    /// The variant for an on-disk `activation_id`, or `None` for an id this build
    /// does not implement.
    fn from_id(id: u16) -> Option<Self> {
        match id {
            ACTIVATION_CRELU => Some(Activation::ClippedRelu),
            ACTIVATION_SCRELU => Some(Activation::SquaredClippedRelu),
            _ => None,
        }
    }
}

/// Output dimension: the network emits a single scalar.
pub const OUTPUT_DIM: u16 = 1;

/// Hidden width must be a positive multiple of this so one file loads unchanged
/// into both the scalar path and the future AVX2 path, whose i16 lanes process
/// this many elements at a time.
const HIDDEN_WIDTH_MULTIPLE: u32 = 16;

// Header field byte offsets. The layout is fixed by the design contract; naming
// each offset keeps the reader and writer from drifting apart.
const OFF_MAGIC: usize = 0;
const OFF_FORMAT_VERSION: usize = 4;
const OFF_FEATURE_SET_ID: usize = 6;
const OFF_INPUT_DIM: usize = 8;
const OFF_HIDDEN_WIDTH: usize = 12;
const OFF_OUTPUT_DIM: usize = 16;
const OFF_ACTIVATION_ID: usize = 18;
const OFF_QA: usize = 20;
const OFF_QB: usize = 22;
const OFF_SCALE: usize = 24;
const OFF_PARAM_BYTES: usize = 28;
const OFF_PARAM_HASH: usize = 32;
const OFF_RESERVED: usize = 40;
const RESERVED_LEN: usize = HEADER_LEN - OFF_RESERVED;

// Version-2 header fields, carved out of the version-1 reserved region. A
// version-1 file requires all of `40..64` to be zero; a version-2 file reads
// these two counts and requires only `44..64` to be zero.
const OFF_NUM_BUCKETS: usize = 40;
const OFF_NUM_OUTPUT_LAYERS: usize = 42;
const OFF_V2_RESERVED: usize = 44;
const V2_RESERVED_LEN: usize = HEADER_LEN - OFF_V2_RESERVED;

/// A quantized NNUE network held in memory: the parameterizable architecture
/// dimensions plus the four quantized weight blocks, in the exact integer types
/// the file stores.
///
/// The type carries its own invariant — every block's length agrees with the
/// hidden width, and the scales are positive — so a value that exists is always
/// serializable and reloadable. Construct one with [`Network::new`] (which
/// enforces the invariant) or by [`Network::read`]ing a valid file.
/// The four quantized weight blocks of a network, in their on-disk integer
/// types. Grouping them keeps [`Network::new`] to a readable signature and lets
/// a caller assemble the parameters in one place before validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameters {
    /// Feature-transformer weights, `INPUT_DIM × H`, feature-major.
    pub w_ft: Vec<i16>,
    /// Feature-transformer bias, length `H`.
    pub b_ft: Vec<i16>,
    /// Output weights, length `2H`: own-perspective block then enemy block.
    pub w_out: Vec<i16>,
    /// Output bias, length [`OUTPUT_DIM`].
    pub b_out: Vec<i32>,
}

/// The parameters of a version-2 bucketed network, grouped so [`Network::new_bucketed`]
/// keeps a readable signature. The feature transformer plus the shared stack shape
/// (`layer_dims`, `layer_scales`) and the per-bucket layers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BucketedParameters {
    /// Feature-transformer weights, `INPUT_DIM × H`, feature-major.
    pub w_ft: Vec<i16>,
    /// Feature-transformer bias, length `H`.
    pub b_ft: Vec<i16>,
    /// Per-layer output dimensions; the last is [`OUTPUT_DIM`].
    pub layer_dims: Vec<u32>,
    /// Per-layer int8 weight scales `QB_k`, shared across buckets; the last is the
    /// network's `qb`.
    pub layer_scales: Vec<u32>,
    /// `buckets[b][k]` is bucket `b`'s layer `k`.
    pub buckets: Vec<Vec<StackLayer>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    hidden_width: u32,
    activation: Activation,
    qa: u16,
    qb: u16,
    scale: i32,
    /// Feature-transformer weights, `INPUT_DIM × H`, feature-major: the `H`
    /// weights for feature `f` are contiguous at `f · H`.
    w_ft: Vec<i16>,
    /// Feature-transformer bias, length `H`.
    b_ft: Vec<i16>,
    /// What sits after the two per-perspective accumulators: either the version-1
    /// single linear layer or the version-2 bucketed int8 stack.
    output: OutputStack,
}

/// The output computation a network carries after its feature transformer.
///
/// The feature transformer and accumulator are identical across format versions;
/// only this differs. [`Single`](OutputStack::Single) is the format-version-1
/// single linear layer (`2H → 1`, i16 weights at `QB`, i32 bias);
/// [`Bucketed`](OutputStack::Bucketed) is the format-version-2 multi-layer int8
/// stack selected per evaluation by piece count. See `docs/nnue-topology-v2.md`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputStack {
    /// Format version 1: `s = b_out + Σ_j a[j] · W_out[j]` over the `2H`
    /// activations, own-perspective block then enemy block.
    Single {
        /// Output weights, length `2H`.
        w_out: Vec<i16>,
        /// Output bias, length [`OUTPUT_DIM`].
        b_out: Vec<i32>,
    },
    /// Format version 2: `num_buckets` independent int8 stacks; the piece-count
    /// rule selects exactly one per evaluation.
    Bucketed(BucketedStack),
}

/// The version-2 bucketed output stack: a shared per-layer shape and int8 weight
/// scale, and one independent set of layer weights per bucket.
///
/// Layer `k` maps `in_dim_k → out_dim_k` with `in_dim_0 = 2H` and
/// `in_dim_k = out_dim_{k-1}`; `layer_dims[k] = out_dim_k` and the last is
/// [`OUTPUT_DIM`]. `layer_scales[k]` is the int8 weight scale `QB_k`, shared
/// across every bucket, and its last entry equals the network's `qb`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BucketedStack {
    num_buckets: u16,
    layer_dims: Vec<u32>,
    layer_scales: Vec<u32>,
    /// `buckets[b][k]` is bucket `b`'s layer `k`. Outer length `num_buckets`,
    /// each inner length `layer_dims.len()`.
    buckets: Vec<Vec<StackLayer>>,
}

/// One affine layer of a bucket's output stack: int8 weights output-major
/// (`out_dim × in_dim`, element `(o, i)` at `o · in_dim + i`) and an i32 bias of
/// length `out_dim`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackLayer {
    /// Weights, `out_dim · in_dim`, output-major.
    pub w: Vec<i8>,
    /// Bias, length `out_dim`.
    pub b: Vec<i32>,
}

impl Network {
    /// Builds a network from its dimensions, scales, and weight blocks,
    /// enforcing the type invariant.
    ///
    /// Fails if the hidden width is not a positive multiple of 16, if any scale
    /// is non-positive, or if a weight block's length disagrees with the width.
    pub fn new(
        hidden_width: u32,
        activation: Activation,
        qa: u16,
        qb: u16,
        scale: i32,
        params: Parameters,
    ) -> Result<Self, BuildError> {
        if hidden_width == 0 || !hidden_width.is_multiple_of(HIDDEN_WIDTH_MULTIPLE) {
            return Err(BuildError::InvalidHiddenWidth(hidden_width));
        }
        check_scale("qa", i64::from(qa))?;
        check_scale("qb", i64::from(qb))?;
        check_scale("scale", i64::from(scale))?;

        let Parameters {
            w_ft,
            b_ft,
            w_out,
            b_out,
        } = params;
        let h = u64::from(hidden_width);
        check_block_len("w_ft", u64::from(INPUT_DIM) * h, w_ft.len())?;
        check_block_len("b_ft", h, b_ft.len())?;
        check_block_len("w_out", 2 * h, w_out.len())?;
        check_block_len("b_out", u64::from(OUTPUT_DIM), b_out.len())?;

        Ok(Self {
            hidden_width,
            activation,
            qa,
            qb,
            scale,
            w_ft,
            b_ft,
            output: OutputStack::Single { w_out, b_out },
        })
    }

    /// Builds a version-2 bucketed-stack network, enforcing the full type
    /// invariant `docs/nnue-topology-v2.md` fixes.
    ///
    /// `layer_dims` are the per-layer output dimensions (last must be
    /// [`OUTPUT_DIM`], each earlier a positive multiple of 16). `layer_scales` are
    /// the per-layer int8 weight scales `QB_k` (each positive); the network's `qb`
    /// is the last of them. `buckets[b][k]` is bucket `b`'s layer `k`, with int8
    /// weights output-major (`out_dim_k · in_dim_k`) and an i32 bias of length
    /// `out_dim_k`, where `in_dim_0 = 2H` and `in_dim_k = out_dim_{k-1}`.
    ///
    /// Fails with a [`BuildError`] if any of those shape or range rules is
    /// violated, so a bucketed network that exists is always serializable and
    /// reloadable.
    pub fn new_bucketed(
        hidden_width: u32,
        activation: Activation,
        qa: u16,
        scale: i32,
        params: BucketedParameters,
    ) -> Result<Self, BuildError> {
        if hidden_width == 0 || !hidden_width.is_multiple_of(HIDDEN_WIDTH_MULTIPLE) {
            return Err(BuildError::InvalidHiddenWidth(hidden_width));
        }
        check_scale("qa", i64::from(qa))?;
        check_scale("scale", i64::from(scale))?;

        let BucketedParameters {
            w_ft,
            b_ft,
            layer_dims,
            layer_scales,
            buckets,
        } = params;
        let h = u64::from(hidden_width);
        check_block_len("w_ft", u64::from(INPUT_DIM) * h, w_ft.len())?;
        check_block_len("b_ft", h, b_ft.len())?;

        let stack = BucketedStack::build(hidden_width, layer_dims, layer_scales, buckets)?;
        // `qb` mirrors the final layer's scale; the stack builder has already
        // checked the scale fits u16 and matches the on-disk `qb` field.
        let qb = stack.qb();
        Ok(Self {
            hidden_width,
            activation,
            qa,
            qb,
            scale,
            w_ft,
            b_ft,
            output: OutputStack::Bucketed(stack),
        })
    }

    /// The feature-transformer output width per perspective (`H`).
    pub fn hidden_width(&self) -> u32 {
        self.hidden_width
    }

    /// The elementwise activation applied before the output layer.
    pub fn activation(&self) -> Activation {
        self.activation
    }

    /// The feature-transformer / activation scale (`QA`).
    pub fn qa(&self) -> u16 {
        self.qa
    }

    /// The output-weight scale (`QB`).
    pub fn qb(&self) -> u16 {
        self.qb
    }

    /// The internal-output-to-centipawn scale (`SCALE`).
    pub fn scale(&self) -> i32 {
        self.scale
    }

    /// Feature-transformer weights, `INPUT_DIM × H`, feature-major.
    pub fn feature_transformer_weights(&self) -> &[i16] {
        &self.w_ft
    }

    /// Feature-transformer bias, length `H`.
    pub fn feature_transformer_bias(&self) -> &[i16] {
        &self.b_ft
    }

    /// What sits after the feature transformer: the version-1 single linear layer
    /// or the version-2 bucketed stack. The forward pass matches on this.
    pub fn output(&self) -> &OutputStack {
        &self.output
    }

    /// Output weights of the version-1 single linear layer, length `2H`.
    ///
    /// # Panics
    ///
    /// Panics for a bucketed (version-2) network, which has no single output
    /// weight block. This accessor exists for the version-1 forward path and its
    /// tests; a caller that may hold either kind must match on [`Network::output`].
    pub fn output_weights(&self) -> &[i16] {
        match &self.output {
            OutputStack::Single { w_out, .. } => w_out,
            OutputStack::Bucketed(_) => {
                panic!(
                    "output_weights is a version-1 single-layer accessor; this network is bucketed"
                )
            }
        }
    }

    /// Output bias of the version-1 single linear layer, length [`OUTPUT_DIM`].
    ///
    /// # Panics
    ///
    /// Panics for a bucketed (version-2) network; see [`Network::output_weights`].
    pub fn output_bias(&self) -> &[i32] {
        match &self.output {
            OutputStack::Single { b_out, .. } => b_out,
            OutputStack::Bucketed(_) => {
                panic!("output_bias is a version-1 single-layer accessor; this network is bucketed")
            }
        }
    }

    /// The FNV-1a hash of the parameter blob — the value this network's header
    /// records, and the field that distinguishes two networks of identical
    /// architecture.
    ///
    /// Recomputed from the weights rather than remembered from the file, so it
    /// describes the network in memory and is equally available for one that was
    /// built rather than loaded. That costs an encode of the blob, which is why
    /// this is for reporting and not for a hot path.
    pub fn param_hash(&self) -> u64 {
        fnv1a_64(&self.encode_blob())
    }

    /// The on-disk format version this network serializes as: version 1 for a
    /// single linear output, version 2 for a bucketed stack.
    pub fn format_version(&self) -> u16 {
        match &self.output {
            OutputStack::Single { .. } => FORMAT_VERSION_V1,
            OutputStack::Bucketed(_) => FORMAT_VERSION_V2,
        }
    }

    /// Number of bytes the feature-transformer blocks occupy on disk.
    fn ft_bytes(&self) -> u64 {
        2 * self.w_ft.len() as u64 + 2 * self.b_ft.len() as u64
    }

    /// Number of bytes the parameter blob occupies on disk: the feature
    /// transformer plus whatever the output stack encodes.
    fn param_bytes(&self) -> u64 {
        self.ft_bytes() + self.output.blob_bytes()
    }

    /// Serializes the network to `out`: the 64-byte header followed by the
    /// parameter blob in contract order.
    pub fn write<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let blob = self.encode_blob();
        let param_bytes = u32::try_from(blob.len()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "parameter blob exceeds the u32 length the header can record",
            )
        })?;

        let mut header = [0u8; HEADER_LEN];
        header[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC);
        header[OFF_FORMAT_VERSION..OFF_FORMAT_VERSION + 2]
            .copy_from_slice(&self.format_version().to_le_bytes());
        header[OFF_FEATURE_SET_ID..OFF_FEATURE_SET_ID + 2]
            .copy_from_slice(&FEATURE_SET_PERSPECTIVE_768.to_le_bytes());
        header[OFF_INPUT_DIM..OFF_INPUT_DIM + 4].copy_from_slice(&INPUT_DIM.to_le_bytes());
        header[OFF_HIDDEN_WIDTH..OFF_HIDDEN_WIDTH + 4]
            .copy_from_slice(&self.hidden_width.to_le_bytes());
        header[OFF_OUTPUT_DIM..OFF_OUTPUT_DIM + 2].copy_from_slice(&OUTPUT_DIM.to_le_bytes());
        header[OFF_ACTIVATION_ID..OFF_ACTIVATION_ID + 2]
            .copy_from_slice(&self.activation.id().to_le_bytes());
        header[OFF_QA..OFF_QA + 2].copy_from_slice(&self.qa.to_le_bytes());
        header[OFF_QB..OFF_QB + 2].copy_from_slice(&self.qb.to_le_bytes());
        header[OFF_SCALE..OFF_SCALE + 4].copy_from_slice(&self.scale.to_le_bytes());
        header[OFF_PARAM_BYTES..OFF_PARAM_BYTES + 4].copy_from_slice(&param_bytes.to_le_bytes());
        header[OFF_PARAM_HASH..OFF_PARAM_HASH + 8].copy_from_slice(&fnv1a_64(&blob).to_le_bytes());
        // A bucketed network records its bucket and layer counts in the region a
        // version-1 file leaves reserved; the rest stays zero either way, so an
        // older loader rejects any future flag set here rather than ignoring it.
        if let OutputStack::Bucketed(stack) = &self.output {
            header[OFF_NUM_BUCKETS..OFF_NUM_BUCKETS + 2]
                .copy_from_slice(&stack.num_buckets.to_le_bytes());
            header[OFF_NUM_OUTPUT_LAYERS..OFF_NUM_OUTPUT_LAYERS + 2]
                .copy_from_slice(&stack.num_output_layers().to_le_bytes());
        }

        out.write_all(&header)?;
        out.write_all(&blob)?;
        Ok(())
    }

    /// The parameter blob in the fixed on-disk order: the feature transformer
    /// (`W_ft`, `b_ft`) followed by the output stack's encoding, each element
    /// little-endian.
    fn encode_blob(&self) -> Vec<u8> {
        let mut blob = Vec::with_capacity(self.param_bytes() as usize);
        self.output.encode_prefix(&mut blob);
        for &w in &self.w_ft {
            blob.extend_from_slice(&w.to_le_bytes());
        }
        for &b in &self.b_ft {
            blob.extend_from_slice(&b.to_le_bytes());
        }
        self.output.encode_suffix(&mut blob);
        blob
    }

    /// Reads and validates a network from `input`.
    ///
    /// The entire header is parsed and every field that governs interpretation
    /// is checked before a single weight is allocated, so an unknown or
    /// mismatched file is rejected with a specific [`LoadError`] rather than
    /// misread. Each rejection rule maps to a distinct error variant.
    pub fn read<R: Read>(input: &mut R) -> Result<Self, LoadError> {
        let mut header = [0u8; HEADER_LEN];
        read_exact_or_truncated(input, &mut header)?;

        let magic: [u8; 4] = header[OFF_MAGIC..OFF_MAGIC + 4].try_into().unwrap();
        if magic != MAGIC {
            return Err(LoadError::BadMagic(magic));
        }

        let format_version = u16_le(&header, OFF_FORMAT_VERSION);
        if format_version != FORMAT_VERSION_V1 && format_version != FORMAT_VERSION_V2 {
            return Err(LoadError::UnsupportedVersion(format_version));
        }

        let feature_set_id = u16_le(&header, OFF_FEATURE_SET_ID);
        if feature_set_id != FEATURE_SET_PERSPECTIVE_768 {
            return Err(LoadError::UnsupportedFeatureSet(feature_set_id));
        }
        let activation_id = u16_le(&header, OFF_ACTIVATION_ID);
        let activation = Activation::from_id(activation_id)
            .ok_or(LoadError::UnsupportedActivation(activation_id))?;

        // Architecture consistency. `feature_set_id` fixes the input dimension,
        // so a disagreeing `input_dim` is a corrupt or foreign file.
        let input_dim = u32_le(&header, OFF_INPUT_DIM);
        if input_dim != INPUT_DIM {
            return Err(LoadError::InputDimMismatch {
                feature_set_id,
                expected: INPUT_DIM,
                found: input_dim,
            });
        }
        let hidden_width = u32_le(&header, OFF_HIDDEN_WIDTH);
        if hidden_width == 0 || !hidden_width.is_multiple_of(HIDDEN_WIDTH_MULTIPLE) {
            return Err(LoadError::InvalidHiddenWidth(hidden_width));
        }
        let output_dim = u16_le(&header, OFF_OUTPUT_DIM);
        if output_dim != OUTPUT_DIM {
            return Err(LoadError::InvalidOutputDim(output_dim));
        }

        let qa = u16_le(&header, OFF_QA);
        let qb = u16_le(&header, OFF_QB);
        let scale = i32_le(&header, OFF_SCALE);
        reject_non_positive_scale("qa", i64::from(qa))?;
        reject_non_positive_scale("qb", i64::from(qb))?;
        reject_non_positive_scale("scale", i64::from(scale))?;

        let declared_bytes = u32_le(&header, OFF_PARAM_BYTES);
        let declared_hash = u64_le(&header, OFF_PARAM_HASH);

        if format_version == FORMAT_VERSION_V1 {
            Self::read_v1_body(
                input,
                &header,
                hidden_width,
                activation,
                qa,
                qb,
                scale,
                input_dim,
                output_dim,
                declared_bytes,
                declared_hash,
            )
        } else {
            Self::read_v2_body(
                input,
                &header,
                hidden_width,
                activation,
                qa,
                qb,
                scale,
                input_dim,
                output_dim,
                declared_bytes,
                declared_hash,
            )
        }
    }

    /// Reads the version-1 body: the single-linear-layer blob, after the shared
    /// header validation. All of the reserved region must be zero.
    // The parameters are the header fields `read` has already parsed and
    // validated before dispatching to a version body; each is consumed
    // individually and a struct would only shuttle the same fields through this
    // one private dispatch, so the count mirrors the header layout rather than a
    // missing abstraction.
    #[allow(clippy::too_many_arguments)]
    fn read_v1_body<R: Read>(
        input: &mut R,
        header: &[u8; HEADER_LEN],
        hidden_width: u32,
        activation: Activation,
        qa: u16,
        qb: u16,
        scale: i32,
        input_dim: u32,
        output_dim: u16,
        declared_bytes: u32,
        declared_hash: u64,
    ) -> Result<Self, LoadError> {
        if header[OFF_RESERVED..OFF_RESERVED + RESERVED_LEN]
            .iter()
            .any(|&b| b != 0)
        {
            return Err(LoadError::ReservedNotZero);
        }

        // The dimensions fully determine the blob size; a declared `param_bytes`
        // that disagrees means the header and body describe different networks.
        let h = u64::from(hidden_width);
        let expected_bytes =
            2 * u64::from(input_dim) * h + 2 * h + 2 * (2 * h) + 4 * u64::from(output_dim);
        if u64::from(declared_bytes) != expected_bytes {
            return Err(LoadError::ParamBytesMismatch {
                declared: declared_bytes,
                expected: expected_bytes,
            });
        }

        let blob = read_exact_bytes(input, expected_bytes)?;
        reject_trailing(input)?;
        reject_hash_mismatch(declared_hash, &blob)?;

        let h = hidden_width as usize;
        let mut cursor = BlobCursor::new(&blob);
        let w_ft = cursor.take_i16(INPUT_DIM as usize * h);
        let b_ft = cursor.take_i16(h);
        let w_out = cursor.take_i16(2 * h);
        let b_out = cursor.take_i32(OUTPUT_DIM as usize);
        Ok(Self {
            hidden_width,
            activation,
            qa,
            qb,
            scale,
            w_ft,
            b_ft,
            output: OutputStack::Single { w_out, b_out },
        })
    }

    /// Reads the version-2 body: the bucketed int8 stack. The `stack_dims` and
    /// `stack_scales` prefix is read and validated first — it determines the rest
    /// of the blob's size — then the feature transformer and per-bucket layers.
    /// Only the header bytes past the two v2 counts must be zero.
    // Same as `read_v1_body`: these are the header fields `read` has already
    // parsed and validated, threaded into this private version arm; the count
    // mirrors the header layout rather than a missing abstraction.
    #[allow(clippy::too_many_arguments)]
    fn read_v2_body<R: Read>(
        input: &mut R,
        header: &[u8; HEADER_LEN],
        hidden_width: u32,
        activation: Activation,
        qa: u16,
        qb: u16,
        scale: i32,
        input_dim: u32,
        output_dim: u16,
        declared_bytes: u32,
        declared_hash: u64,
    ) -> Result<Self, LoadError> {
        if header[OFF_V2_RESERVED..OFF_V2_RESERVED + V2_RESERVED_LEN]
            .iter()
            .any(|&b| b != 0)
        {
            return Err(LoadError::ReservedNotZero);
        }

        let num_buckets = u16_le(header, OFF_NUM_BUCKETS);
        if !(MIN_BUCKETS..=MAX_BUCKETS).contains(&num_buckets) {
            return Err(LoadError::InvalidBucketCount(num_buckets));
        }
        let num_layers = u16_le(header, OFF_NUM_OUTPUT_LAYERS);
        if num_layers < 1 {
            return Err(LoadError::InvalidLayerCount(num_layers));
        }
        let num_layers = num_layers as usize;

        // The dims/scales prefix has a header-fixed size, so it can be read before
        // the rest of the blob whose size it determines.
        let prefix_bytes = 2 * num_layers as u64 * 4;
        let mut blob = read_exact_bytes(input, prefix_bytes)?;
        let (layer_dims, layer_scales) = {
            let mut cursor = BlobCursor::new(&blob);
            let dims = cursor.take_u32(num_layers);
            let scales = cursor.take_u32(num_layers);
            (dims, scales)
        };
        validate_stack_dims(&layer_dims, output_dim)?;
        validate_stack_scales(&layer_scales, qb)?;

        // Now the blob's total size is known: prefix, feature transformer, and the
        // per-bucket int8 layers.
        let h = hidden_width as u64;
        let (weights_per_bucket, biases_per_bucket) = stack_counts(hidden_width, &layer_dims);
        let ft_bytes = 2 * u64::from(input_dim) * h + 2 * h;
        let per_bucket_bytes = weights_per_bucket + 4 * biases_per_bucket;
        let expected_bytes = prefix_bytes + ft_bytes + u64::from(num_buckets) * per_bucket_bytes;
        if u64::from(declared_bytes) != expected_bytes {
            return Err(LoadError::ParamBytesMismatch {
                declared: declared_bytes,
                expected: expected_bytes,
            });
        }

        let mut rest = read_exact_bytes(input, expected_bytes - prefix_bytes)?;
        reject_trailing(input)?;
        blob.append(&mut rest);
        reject_hash_mismatch(declared_hash, &blob)?;

        // Decode: the prefix was already consumed conceptually, so skip it and read
        // the feature transformer and then each bucket's layers in order.
        let h = hidden_width as usize;
        let mut cursor = BlobCursor::new(&blob);
        cursor.skip(prefix_bytes as usize);
        let w_ft = cursor.take_i16(INPUT_DIM as usize * h);
        let b_ft = cursor.take_i16(h);
        let buckets = (0..num_buckets)
            .map(|_| {
                let mut in_dim = 2 * h;
                layer_dims
                    .iter()
                    .map(|&out_dim| {
                        let out_dim = out_dim as usize;
                        let w = cursor.take_i8(out_dim * in_dim);
                        let b = cursor.take_i32(out_dim);
                        in_dim = out_dim;
                        StackLayer { w, b }
                    })
                    .collect()
            })
            .collect();

        let stack = BucketedStack {
            num_buckets,
            layer_dims,
            layer_scales,
            buckets,
        };
        Ok(Self {
            hidden_width,
            activation,
            qa,
            qb,
            scale,
            w_ft,
            b_ft,
            output: OutputStack::Bucketed(stack),
        })
    }
}

impl OutputStack {
    /// Bytes this output contributes to the parameter blob — everything but the
    /// feature transformer. For v1 that is the output weight and bias blocks; for
    /// v2 the `stack_dims`/`stack_scales` prefix plus every bucket's int8 layers.
    fn blob_bytes(&self) -> u64 {
        match self {
            OutputStack::Single { w_out, b_out } => 2 * w_out.len() as u64 + 4 * b_out.len() as u64,
            OutputStack::Bucketed(stack) => stack.blob_bytes(),
        }
    }

    /// Writes the blob region that precedes the feature transformer: nothing for
    /// v1, the `stack_dims` then `stack_scales` u32 tables for v2.
    fn encode_prefix(&self, blob: &mut Vec<u8>) {
        if let OutputStack::Bucketed(stack) = self {
            for &dim in &stack.layer_dims {
                blob.extend_from_slice(&dim.to_le_bytes());
            }
            for &scale in &stack.layer_scales {
                blob.extend_from_slice(&scale.to_le_bytes());
            }
        }
    }

    /// Writes the blob region that follows the feature transformer: the v1 output
    /// weight/bias blocks, or every bucket's int8 layers (weights then bias) in
    /// order.
    fn encode_suffix(&self, blob: &mut Vec<u8>) {
        match self {
            OutputStack::Single { w_out, b_out } => {
                for &w in w_out {
                    blob.extend_from_slice(&w.to_le_bytes());
                }
                for &b in b_out {
                    blob.extend_from_slice(&b.to_le_bytes());
                }
            }
            OutputStack::Bucketed(stack) => {
                for bucket in &stack.buckets {
                    for layer in bucket {
                        // An i8 is one byte; `as u8` reinterprets the bits, the
                        // little-endian encoding a single signed byte round-trips.
                        for &w in &layer.w {
                            blob.push(w as u8);
                        }
                        for &b in &layer.b {
                            blob.extend_from_slice(&b.to_le_bytes());
                        }
                    }
                }
            }
        }
    }
}

impl BucketedStack {
    /// Builds and validates a bucketed stack from its shape and per-bucket layers,
    /// enforcing every invariant `docs/nnue-topology-v2.md` fixes so a value that
    /// exists is always serializable and reloadable.
    fn build(
        hidden_width: u32,
        layer_dims: Vec<u32>,
        layer_scales: Vec<u32>,
        buckets: Vec<Vec<StackLayer>>,
    ) -> Result<Self, BuildError> {
        let num_layers = layer_dims.len();
        if num_layers == 0 {
            return Err(BuildError::InvalidLayerCount(0));
        }
        if layer_scales.len() != num_layers {
            return Err(BuildError::StackScaleCountMismatch {
                expected: num_layers,
                found: layer_scales.len(),
            });
        }
        let num_buckets = u16::try_from(buckets.len())
            .ok()
            .filter(|n| (MIN_BUCKETS..=MAX_BUCKETS).contains(n))
            .ok_or(BuildError::InvalidBucketCount(buckets.len()))?;

        check_stack_dims_build(&layer_dims)?;
        check_stack_scales_build(&layer_scales)?;

        for (b, bucket) in buckets.iter().enumerate() {
            if bucket.len() != num_layers {
                return Err(BuildError::StackLayerCountMismatch {
                    bucket: b,
                    expected: num_layers,
                    found: bucket.len(),
                });
            }
            let mut in_dim = 2 * u64::from(hidden_width);
            for (k, layer) in bucket.iter().enumerate() {
                let out_dim = u64::from(layer_dims[k]);
                check_block_len("stack_layer_w", out_dim * in_dim, layer.w.len())?;
                check_block_len("stack_layer_b", out_dim, layer.b.len())?;
                in_dim = out_dim;
            }
        }

        Ok(Self {
            num_buckets,
            layer_dims,
            layer_scales,
            buckets,
        })
    }

    /// The network `qb`: the final layer's int8 weight scale. Validated to fit u16
    /// during construction.
    fn qb(&self) -> u16 {
        *self
            .layer_scales
            .last()
            .expect("a bucketed stack has at least one layer") as u16
    }

    /// The number of layers in each stack.
    fn num_output_layers(&self) -> u16 {
        self.layer_dims.len() as u16
    }

    /// Bytes this stack contributes after the header: the two u32 tables plus every
    /// bucket's int8 weights and i32 biases.
    fn blob_bytes(&self) -> u64 {
        let num_layers = self.layer_dims.len() as u64;
        let mut bytes = 4 * num_layers + 4 * num_layers;
        for bucket in &self.buckets {
            for layer in bucket {
                bytes += layer.w.len() as u64 + 4 * layer.b.len() as u64;
            }
        }
        bytes
    }

    /// The number of buckets (independent stacks).
    pub fn num_buckets(&self) -> u16 {
        self.num_buckets
    }

    /// The per-layer output dimensions; the last is [`OUTPUT_DIM`].
    pub fn layer_dims(&self) -> &[u32] {
        &self.layer_dims
    }

    /// The per-layer int8 weight scales `QB_k`, shared across buckets.
    pub fn layer_scales(&self) -> &[u32] {
        &self.layer_scales
    }

    /// The layers of bucket `index`, in order from the `2H` input to the scalar
    /// output.
    ///
    /// # Panics
    ///
    /// Panics if `index >= num_buckets`; the caller selects a bucket with the
    /// piece-count rule, which is bounded to `0..num_buckets`.
    pub fn bucket(&self, index: usize) -> &[StackLayer] {
        &self.buckets[index]
    }
}

/// Reads exactly `n` bytes into a fresh buffer, mapping a short read to
/// [`LoadError::Truncated`]. `take` bounds the read to the untrusted length so a
/// short file grows the buffer only to what is present.
fn read_exact_bytes<R: Read>(input: &mut R, n: u64) -> Result<Vec<u8>, LoadError> {
    let mut buf = Vec::new();
    input.take(n).read_to_end(&mut buf).map_err(LoadError::Io)?;
    if buf.len() as u64 != n {
        return Err(LoadError::Truncated);
    }
    Ok(buf)
}

/// Rejects any byte past the accounted-for blob: the file is longer than its
/// header claims.
fn reject_trailing<R: Read>(input: &mut R) -> Result<(), LoadError> {
    let mut extra = [0u8; 1];
    match input.read(&mut extra) {
        Ok(0) => Ok(()),
        Ok(_) => Err(LoadError::TrailingBytes),
        Err(e) => Err(LoadError::Io(e)),
    }
}

/// Rejects a blob whose FNV-1a hash disagrees with the header's `param_hash`.
fn reject_hash_mismatch(declared: u64, blob: &[u8]) -> Result<(), LoadError> {
    let computed = fnv1a_64(blob);
    if declared != computed {
        return Err(LoadError::HashMismatch { declared, computed });
    }
    Ok(())
}

/// The per-bucket weight and bias element counts a stack of the given hidden width
/// and per-layer output dims holds. Used to size the blob before the weights are
/// read.
fn stack_counts(hidden_width: u32, layer_dims: &[u32]) -> (u64, u64) {
    let mut in_dim = 2 * u64::from(hidden_width);
    let mut weights = 0u64;
    let mut biases = 0u64;
    for &out_dim in layer_dims {
        let out_dim = u64::from(out_dim);
        weights += out_dim * in_dim;
        biases += out_dim;
        in_dim = out_dim;
    }
    (weights, biases)
}

/// Validates the on-disk `stack_dims` against the topology-v2 rules: the last
/// entry is the scalar `output_dim`, and every earlier entry is a positive
/// multiple of 16 (so every layer's input dimension is a multiple of 16).
fn validate_stack_dims(dims: &[u32], output_dim: u16) -> Result<(), LoadError> {
    let n = dims.len();
    for (i, &dim) in dims.iter().enumerate() {
        if i + 1 == n {
            if dim != u32::from(output_dim) {
                return Err(LoadError::StackFinalDimMismatch {
                    found: dim,
                    expected: u32::from(output_dim),
                });
            }
        } else if dim == 0 || !dim.is_multiple_of(HIDDEN_WIDTH_MULTIPLE) {
            return Err(LoadError::InvalidStackHiddenDim { index: i, dim });
        }
    }
    Ok(())
}

/// Validates the on-disk `stack_scales`: each positive and within the u16 scale
/// range, and the last equal to the header `qb`.
fn validate_stack_scales(scales: &[u32], qb: u16) -> Result<(), LoadError> {
    let n = scales.len();
    for (i, &scale) in scales.iter().enumerate() {
        if scale == 0 {
            return Err(LoadError::NonPositiveStackScale { index: i });
        }
        if scale > u32::from(u16::MAX) {
            return Err(LoadError::StackScaleTooLarge {
                index: i,
                value: scale,
            });
        }
        if i + 1 == n && scale != u32::from(qb) {
            return Err(LoadError::StackFinalScaleMismatch { found: scale, qb });
        }
    }
    Ok(())
}

/// The in-memory-builder counterpart of [`validate_stack_dims`].
fn check_stack_dims_build(dims: &[u32]) -> Result<(), BuildError> {
    let n = dims.len();
    for (i, &dim) in dims.iter().enumerate() {
        if i + 1 == n {
            if dim != u32::from(OUTPUT_DIM) {
                return Err(BuildError::StackFinalDimMismatch {
                    found: dim,
                    expected: u32::from(OUTPUT_DIM),
                });
            }
        } else if dim == 0 || !dim.is_multiple_of(HIDDEN_WIDTH_MULTIPLE) {
            return Err(BuildError::InvalidStackHiddenDim { index: i, dim });
        }
    }
    Ok(())
}

/// The in-memory-builder counterpart of [`validate_stack_scales`]. The last scale
/// must fit u16 because it becomes the network's `qb`.
fn check_stack_scales_build(scales: &[u32]) -> Result<(), BuildError> {
    for (i, &scale) in scales.iter().enumerate() {
        if scale == 0 {
            return Err(BuildError::NonPositiveStackScale { index: i });
        }
        if scale > u32::from(u16::MAX) {
            return Err(BuildError::StackScaleTooLarge {
                index: i,
                value: scale,
            });
        }
    }
    Ok(())
}

/// Walks a byte blob, decoding fixed-count little-endian integer runs in order.
struct BlobCursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> BlobCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take_i16(&mut self, count: usize) -> Vec<i16> {
        (0..count)
            .map(|_| {
                let v = i16::from_le_bytes([self.bytes[self.pos], self.bytes[self.pos + 1]]);
                self.pos += 2;
                v
            })
            .collect()
    }

    fn take_i32(&mut self, count: usize) -> Vec<i32> {
        (0..count)
            .map(|_| {
                let v = i32::from_le_bytes([
                    self.bytes[self.pos],
                    self.bytes[self.pos + 1],
                    self.bytes[self.pos + 2],
                    self.bytes[self.pos + 3],
                ]);
                self.pos += 4;
                v
            })
            .collect()
    }

    fn take_u32(&mut self, count: usize) -> Vec<u32> {
        (0..count)
            .map(|_| {
                let v = u32::from_le_bytes([
                    self.bytes[self.pos],
                    self.bytes[self.pos + 1],
                    self.bytes[self.pos + 2],
                    self.bytes[self.pos + 3],
                ]);
                self.pos += 4;
                v
            })
            .collect()
    }

    fn take_i8(&mut self, count: usize) -> Vec<i8> {
        (0..count)
            .map(|_| {
                let v = self.bytes[self.pos] as i8;
                self.pos += 1;
                v
            })
            .collect()
    }

    /// Advances the cursor past `count` bytes without decoding them, used to skip
    /// a blob prefix that was already parsed.
    fn skip(&mut self, count: usize) {
        self.pos += count;
    }
}

/// A network in memory could not be constructed because its dimensions or
/// scales are inconsistent. This is a programming error at the writer, distinct
/// from [`LoadError`], which describes an untrusted file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildError {
    /// The hidden width is zero or not a multiple of 16.
    InvalidHiddenWidth(u32),
    /// A scale (`qa`, `qb`, or `scale`) is not strictly positive.
    NonPositiveScale { field: &'static str, value: i64 },
    /// A weight block's length does not match the hidden width.
    WeightCountMismatch {
        block: &'static str,
        expected: u64,
        found: usize,
    },
    /// A bucketed stack was built with zero layers.
    InvalidLayerCount(usize),
    /// A bucketed stack's bucket count is zero or exceeds the supported maximum.
    InvalidBucketCount(usize),
    /// The `layer_scales` vector length disagrees with the number of layers.
    StackScaleCountMismatch { expected: usize, found: usize },
    /// The final stack layer's output dimension is not the scalar [`OUTPUT_DIM`].
    StackFinalDimMismatch { found: u32, expected: u32 },
    /// A hidden stack layer's output dimension is zero or not a multiple of 16.
    InvalidStackHiddenDim { index: usize, dim: u32 },
    /// A stack layer's int8 weight scale is zero.
    NonPositiveStackScale { index: usize },
    /// A stack layer's int8 weight scale exceeds the u16 the header records.
    StackScaleTooLarge { index: usize, value: u32 },
    /// A bucket has a different number of layers than the stack shape declares.
    StackLayerCountMismatch {
        bucket: usize,
        expected: usize,
        found: usize,
    },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::InvalidHiddenWidth(h) => {
                write!(f, "hidden width {h} must be a positive multiple of 16")
            }
            BuildError::NonPositiveScale { field, value } => {
                write!(
                    f,
                    "quantization scale `{field}` must be positive, got {value}"
                )
            }
            BuildError::WeightCountMismatch {
                block,
                expected,
                found,
            } => write!(
                f,
                "weight block `{block}` has {found} elements, expected {expected}"
            ),
            BuildError::InvalidLayerCount(n) => {
                write!(f, "output stack must have at least one layer, got {n}")
            }
            BuildError::InvalidBucketCount(n) => write!(
                f,
                "bucket count {n} must be in {MIN_BUCKETS}..={MAX_BUCKETS}"
            ),
            BuildError::StackScaleCountMismatch { expected, found } => {
                write!(f, "stack has {expected} layers but {found} layer scales")
            }
            BuildError::StackFinalDimMismatch { found, expected } => write!(
                f,
                "final stack layer output dimension {found} must be {expected}"
            ),
            BuildError::InvalidStackHiddenDim { index, dim } => write!(
                f,
                "hidden stack layer {index} dimension {dim} must be a positive multiple of 16"
            ),
            BuildError::NonPositiveStackScale { index } => {
                write!(f, "stack layer {index} weight scale must be positive")
            }
            BuildError::StackScaleTooLarge { index, value } => write!(
                f,
                "stack layer {index} weight scale {value} exceeds the u16 the header stores"
            ),
            BuildError::StackLayerCountMismatch {
                bucket,
                expected,
                found,
            } => write!(f, "bucket {bucket} has {found} layers, expected {expected}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// A network file was rejected. Each variant corresponds to a distinct
/// rejection rule so a file is never silently misinterpreted; the variant names
/// exactly which guarantee the file failed.
#[derive(Debug)]
pub enum LoadError {
    /// The stream ended before a complete header or the full parameter blob.
    Truncated,
    /// The file carries more bytes than its header's `param_bytes` accounts for.
    TrailingBytes,
    /// The leading four bytes are not the `SBNN` magic.
    BadMagic([u8; 4]),
    /// The `format_version` is one this build does not implement.
    UnsupportedVersion(u16),
    /// The `feature_set_id` is one this build does not implement.
    UnsupportedFeatureSet(u16),
    /// The `activation_id` is one this build does not implement.
    UnsupportedActivation(u16),
    /// The `input_dim` is inconsistent with the declared feature set.
    InputDimMismatch {
        feature_set_id: u16,
        expected: u32,
        found: u32,
    },
    /// The hidden width is zero or not a multiple of 16.
    InvalidHiddenWidth(u32),
    /// The `output_dim` is not the single scalar this build supports.
    InvalidOutputDim(u16),
    /// A scale (`qa`, `qb`, or `scale`) is not strictly positive.
    NonPositiveScale { field: &'static str, value: i64 },
    /// A reserved header byte is non-zero, so a future flag would be ignored.
    ReservedNotZero,
    /// The header's `param_bytes` disagrees with the size the dimensions imply.
    ParamBytesMismatch { declared: u32, expected: u64 },
    /// The parameter blob does not hash to the header's `param_hash`.
    HashMismatch { declared: u64, computed: u64 },
    /// A version-2 `num_buckets` outside the supported range.
    InvalidBucketCount(u16),
    /// A version-2 `num_output_layers` below one.
    InvalidLayerCount(u16),
    /// The final `stack_dims` entry is not the scalar `output_dim`.
    StackFinalDimMismatch { found: u32, expected: u32 },
    /// A hidden `stack_dims` entry is zero or not a multiple of 16.
    InvalidStackHiddenDim { index: usize, dim: u32 },
    /// A `stack_scales` entry is zero.
    NonPositiveStackScale { index: usize },
    /// A `stack_scales` entry exceeds the u16 the header's `qb` records.
    StackScaleTooLarge { index: usize, value: u32 },
    /// The final `stack_scales` entry disagrees with the header `qb`.
    StackFinalScaleMismatch { found: u32, qb: u16 },
    /// An I/O error other than a clean end-of-stream occurred while reading.
    Io(io::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Truncated => {
                write!(f, "network file is truncated: fewer bytes than the header and declared blob require")
            }
            LoadError::TrailingBytes => {
                write!(f, "network file has trailing bytes beyond the declared parameter blob")
            }
            LoadError::BadMagic(found) => {
                write!(f, "not a Seaborg network file: magic {found:?} is not `SBNN`")
            }
            LoadError::UnsupportedVersion(v) => {
                write!(f, "unsupported network format version {v}; this build reads versions {FORMAT_VERSION_V1} and {FORMAT_VERSION_V2}")
            }
            LoadError::UnsupportedFeatureSet(id) => {
                write!(f, "unsupported feature set id {id}")
            }
            LoadError::UnsupportedActivation(id) => {
                write!(f, "unsupported activation id {id}")
            }
            LoadError::InputDimMismatch {
                feature_set_id,
                expected,
                found,
            } => write!(
                f,
                "input dimension {found} is inconsistent with feature set {feature_set_id} (expected {expected})"
            ),
            LoadError::InvalidHiddenWidth(h) => {
                write!(f, "hidden width {h} must be a positive multiple of 16")
            }
            LoadError::InvalidOutputDim(d) => {
                write!(f, "output dimension {d} is unsupported; this build supports {OUTPUT_DIM}")
            }
            LoadError::NonPositiveScale { field, value } => {
                write!(f, "quantization scale `{field}` must be positive, got {value}")
            }
            LoadError::ReservedNotZero => {
                write!(f, "reserved header bytes are non-zero")
            }
            LoadError::ParamBytesMismatch { declared, expected } => write!(
                f,
                "declared parameter length {declared} disagrees with the {expected} bytes the architecture implies"
            ),
            LoadError::HashMismatch { declared, computed } => write!(
                f,
                "parameter blob hash {computed:#018x} does not match the header's {declared:#018x}"
            ),
            LoadError::InvalidBucketCount(n) => write!(
                f,
                "bucket count {n} must be in {MIN_BUCKETS}..={MAX_BUCKETS}"
            ),
            LoadError::InvalidLayerCount(n) => {
                write!(f, "output stack must have at least one layer, got {n}")
            }
            LoadError::StackFinalDimMismatch { found, expected } => write!(
                f,
                "final stack layer output dimension {found} must be {expected}"
            ),
            LoadError::InvalidStackHiddenDim { index, dim } => write!(
                f,
                "hidden stack layer {index} dimension {dim} must be a positive multiple of 16"
            ),
            LoadError::NonPositiveStackScale { index } => {
                write!(f, "stack layer {index} weight scale must be positive")
            }
            LoadError::StackScaleTooLarge { index, value } => write!(
                f,
                "stack layer {index} weight scale {value} exceeds the u16 the header stores"
            ),
            LoadError::StackFinalScaleMismatch { found, qb } => write!(
                f,
                "final stack layer scale {found} must equal the header qb {qb}"
            ),
            LoadError::Io(e) => write!(f, "error reading network file: {e}"),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadError::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// Fills `buf` completely, mapping a clean early end-of-stream to
/// [`LoadError::Truncated`] rather than a bare I/O error.
fn read_exact_or_truncated<R: Read>(input: &mut R, buf: &mut [u8]) -> Result<(), LoadError> {
    match input.read_exact(buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Err(LoadError::Truncated),
        Err(e) => Err(LoadError::Io(e)),
    }
}

fn check_scale(field: &'static str, value: i64) -> Result<(), BuildError> {
    if value <= 0 {
        Err(BuildError::NonPositiveScale { field, value })
    } else {
        Ok(())
    }
}

fn check_block_len(block: &'static str, expected: u64, found: usize) -> Result<(), BuildError> {
    if found as u64 == expected {
        Ok(())
    } else {
        Err(BuildError::WeightCountMismatch {
            block,
            expected,
            found,
        })
    }
}

fn reject_non_positive_scale(field: &'static str, value: i64) -> Result<(), LoadError> {
    if value <= 0 {
        Err(LoadError::NonPositiveScale { field, value })
    } else {
        Ok(())
    }
}

fn u16_le(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap())
}

fn u32_le(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn i32_le(bytes: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn u64_le(bytes: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap())
}

/// 64-bit FNV-1a hash of the parameter blob. It only guards the blob against
/// corruption and truncation, so a fast non-cryptographic hash with no
/// dependency is exactly the right tool; the constants are the canonical FNV-1a
/// 64-bit offset basis and prime.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: u32 = 32;
    const QA: u16 = 255;
    const QB: u16 = 64;
    const SCALE: i32 = 400;

    /// Builds a small but structurally valid network with distinct, patterned
    /// weights so a round trip that dropped or reordered a block would change a
    /// value rather than coincidentally still compare equal.
    fn sample_network() -> Network {
        let h = H as usize;
        let w_ft: Vec<i16> = (0..INPUT_DIM as usize * h)
            .map(|i| (i as i32 % 251 - 125) as i16)
            .collect();
        let b_ft: Vec<i16> = (0..h).map(|i| (i as i16) - 16).collect();
        let w_out: Vec<i16> = (0..2 * h).map(|i| (i as i16) * 3 - 90).collect();
        let b_out: Vec<i32> = vec![-1_234_567];
        Network::new(
            H,
            Activation::ClippedRelu,
            QA,
            QB,
            SCALE,
            Parameters {
                w_ft,
                b_ft,
                w_out,
                b_out,
            },
        )
        .unwrap()
    }

    fn to_bytes(net: &Network) -> Vec<u8> {
        let mut buf = Vec::new();
        net.write(&mut buf).unwrap();
        buf
    }

    #[test]
    fn valid_file_round_trips_to_identical_weights_and_metadata() {
        let net = sample_network();
        let bytes = to_bytes(&net);

        // Header length plus the blob the dimensions imply.
        let expected_blob =
            2 * (INPUT_DIM as usize * H as usize) + 2 * H as usize + 2 * (2 * H as usize) + 4;
        assert_eq!(bytes.len(), HEADER_LEN + expected_blob);
        assert_eq!(&bytes[..4], &MAGIC);

        let reloaded = Network::read(&mut bytes.as_slice()).unwrap();
        assert_eq!(reloaded, net);
        // Metadata specifically, in case a future `PartialEq` change narrows.
        assert_eq!(reloaded.hidden_width(), H);
        assert_eq!(reloaded.qa(), QA);
        assert_eq!(reloaded.qb(), QB);
        assert_eq!(reloaded.scale(), SCALE);
        assert_eq!(
            reloaded.feature_transformer_weights(),
            net.feature_transformer_weights()
        );
        assert_eq!(reloaded.output_bias(), net.output_bias());
    }

    #[test]
    fn truncated_file_is_rejected() {
        let bytes = to_bytes(&sample_network());

        // Cut inside the header.
        let mut header_cut = &bytes[..HEADER_LEN - 1];
        assert!(matches!(
            Network::read(&mut header_cut),
            Err(LoadError::Truncated)
        ));

        // Full header but a blob short by one byte.
        let mut blob_cut = &bytes[..bytes.len() - 1];
        assert!(matches!(
            Network::read(&mut blob_cut),
            Err(LoadError::Truncated)
        ));
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = to_bytes(&sample_network());
        bytes.push(0);
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::TrailingBytes)
        ));
    }

    #[test]
    fn unknown_version_is_rejected() {
        let mut bytes = to_bytes(&sample_network());
        // Versions 1 and 2 are implemented; 3 is not.
        bytes[OFF_FORMAT_VERSION..OFF_FORMAT_VERSION + 2].copy_from_slice(&3u16.to_le_bytes());
        // The version is checked before the hash, so a stale hash is irrelevant.
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::UnsupportedVersion(3))
        ));
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = to_bytes(&sample_network());
        bytes[0] = b'X';
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::BadMagic(m)) if m == *b"XBNN"
        ));
    }

    #[test]
    fn unknown_feature_set_is_rejected() {
        let mut bytes = to_bytes(&sample_network());
        bytes[OFF_FEATURE_SET_ID..OFF_FEATURE_SET_ID + 2].copy_from_slice(&7u16.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::UnsupportedFeatureSet(7))
        ));
    }

    #[test]
    fn unknown_activation_is_rejected() {
        let mut bytes = to_bytes(&sample_network());
        // Id 0 (CReLU) and 1 (SCReLU) are implemented; 2 is not.
        bytes[OFF_ACTIVATION_ID..OFF_ACTIVATION_ID + 2].copy_from_slice(&2u16.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::UnsupportedActivation(2))
        ));
    }

    #[test]
    fn screlu_activation_round_trips() {
        // A network's activation is part of its architecture: it survives a write
        // and reload, and the reloaded network reports the SCReLU variant.
        let net = Network::new(
            H,
            Activation::SquaredClippedRelu,
            QA,
            QB,
            SCALE,
            Parameters {
                w_ft: vec![0i16; INPUT_DIM as usize * H as usize],
                b_ft: vec![0i16; H as usize],
                w_out: vec![0i16; 2 * H as usize],
                b_out: vec![0i32],
            },
        )
        .unwrap();
        assert_eq!(net.activation(), Activation::SquaredClippedRelu);

        let bytes = to_bytes(&net);
        assert_eq!(
            u16_le(&bytes, OFF_ACTIVATION_ID),
            ACTIVATION_SCRELU,
            "the stored activation id is written, not a hardcoded CReLU"
        );
        let reloaded = Network::read(&mut bytes.as_slice()).unwrap();
        assert_eq!(reloaded.activation(), Activation::SquaredClippedRelu);
        assert_eq!(reloaded, net);
    }

    #[test]
    fn architecture_mismatch_in_input_dim_is_rejected() {
        let mut bytes = to_bytes(&sample_network());
        bytes[OFF_INPUT_DIM..OFF_INPUT_DIM + 4].copy_from_slice(&769u32.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::InputDimMismatch {
                expected: 768,
                found: 769,
                ..
            })
        ));
    }

    #[test]
    fn hidden_width_not_multiple_of_16_is_rejected() {
        let mut bytes = to_bytes(&sample_network());
        bytes[OFF_HIDDEN_WIDTH..OFF_HIDDEN_WIDTH + 4].copy_from_slice(&24u32.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::InvalidHiddenWidth(24))
        ));
    }

    #[test]
    fn zero_hidden_width_is_rejected() {
        let mut bytes = to_bytes(&sample_network());
        bytes[OFF_HIDDEN_WIDTH..OFF_HIDDEN_WIDTH + 4].copy_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::InvalidHiddenWidth(0))
        ));
    }

    #[test]
    fn wrong_output_dim_is_rejected() {
        let mut bytes = to_bytes(&sample_network());
        bytes[OFF_OUTPUT_DIM..OFF_OUTPUT_DIM + 2].copy_from_slice(&2u16.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::InvalidOutputDim(2))
        ));
    }

    #[test]
    fn non_positive_scale_is_rejected() {
        for (off, field) in [(OFF_QA, "qa"), (OFF_QB, "qb")] {
            let mut bytes = to_bytes(&sample_network());
            bytes[off..off + 2].copy_from_slice(&0u16.to_le_bytes());
            match Network::read(&mut bytes.as_slice()) {
                Err(LoadError::NonPositiveScale { field: f, value: 0 }) => assert_eq!(f, field),
                other => panic!("expected non-positive `{field}`, got {other:?}"),
            }
        }

        // `scale` is signed, so a negative value must also be caught.
        let mut bytes = to_bytes(&sample_network());
        bytes[OFF_SCALE..OFF_SCALE + 4].copy_from_slice(&(-1i32).to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::NonPositiveScale {
                field: "scale",
                value: -1
            })
        ));
    }

    #[test]
    fn non_zero_reserved_byte_is_rejected() {
        let mut bytes = to_bytes(&sample_network());
        bytes[OFF_RESERVED + 5] = 1;
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::ReservedNotZero)
        ));
    }

    #[test]
    fn param_bytes_disagreeing_with_dimensions_is_rejected() {
        let mut bytes = to_bytes(&sample_network());
        let wrong = u32_le(&bytes, OFF_PARAM_BYTES) + 2;
        bytes[OFF_PARAM_BYTES..OFF_PARAM_BYTES + 4].copy_from_slice(&wrong.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::ParamBytesMismatch { .. })
        ));
    }

    #[test]
    fn corrupt_blob_fails_the_hash_check() {
        let mut bytes = to_bytes(&sample_network());
        // Flip a bit in the first weight; length and every header field stay
        // valid, so only the hash can catch it.
        bytes[HEADER_LEN] ^= 0x01;
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::HashMismatch { .. })
        ));
    }

    #[test]
    fn empty_input_is_truncated_not_a_panic() {
        let mut empty: &[u8] = &[];
        assert!(matches!(
            Network::read(&mut empty),
            Err(LoadError::Truncated)
        ));
    }

    fn empty_params() -> Parameters {
        Parameters {
            w_ft: vec![],
            b_ft: vec![],
            w_out: vec![],
            b_out: vec![],
        }
    }

    #[test]
    fn new_rejects_bad_hidden_width_and_scales_and_lengths() {
        // Not a multiple of 16.
        assert!(matches!(
            Network::new(17, Activation::ClippedRelu, QA, QB, SCALE, empty_params()),
            Err(BuildError::InvalidHiddenWidth(17))
        ));
        // Non-positive scale.
        assert!(matches!(
            Network::new(H, Activation::ClippedRelu, 0, QB, SCALE, empty_params()),
            Err(BuildError::NonPositiveScale { field: "qa", .. })
        ));
        // Right width and scales but a short weight block.
        let h = H as usize;
        assert!(matches!(
            Network::new(
                H,
                Activation::ClippedRelu,
                QA,
                QB,
                SCALE,
                Parameters {
                    w_ft: vec![0; INPUT_DIM as usize * h],
                    b_ft: vec![0; h],
                    w_out: vec![0; 2 * h - 1],
                    b_out: vec![0; 1],
                },
            ),
            Err(BuildError::WeightCountMismatch { block: "w_out", .. })
        ));
    }

    // --- Format version 2: bucketed multi-layer int8 stack ---

    const V2_H: u32 = 16;
    const V2_QA: u16 = 255;
    const V2_SCALE: i32 = 400;
    // Distinct per-layer scales (last is the network's qb), so a round trip or a
    // reader that ignored `stack_scales` and assumed a uniform scale would differ.
    const V2_DIMS: [u32; 3] = [16, 32, 1];
    const V2_SCALES: [u32; 3] = [64, 128, 256];
    const V2_BUCKETS: u16 = 3;

    /// Builds a structurally valid bucketed network with patterned weights that
    /// vary by bucket, layer, and position, so a serialization that dropped or
    /// reordered any block changes a value rather than comparing equal by chance.
    fn sample_bucketed_network() -> Network {
        let h = V2_H as usize;
        let w_ft: Vec<i16> = (0..INPUT_DIM as usize * h)
            .map(|i| (i as i32 % 251 - 125) as i16)
            .collect();
        let b_ft: Vec<i16> = (0..h).map(|i| (i as i16) - 8).collect();

        let buckets: Vec<Vec<StackLayer>> = (0..V2_BUCKETS as usize)
            .map(|bucket| {
                let mut in_dim = 2 * h;
                V2_DIMS
                    .iter()
                    .enumerate()
                    .map(|(k, &out_dim)| {
                        let out_dim = out_dim as usize;
                        let w: Vec<i8> = (0..out_dim * in_dim)
                            .map(|i| ((i + 7 * k + 3 * bucket) % 251) as i32 as i8)
                            .map(|v| v.clamp(-127, 127))
                            .collect();
                        let b: Vec<i32> = (0..out_dim)
                            .map(|o| (o as i32 * 101 + bucket as i32 * 13 - 500) * (k as i32 + 1))
                            .collect();
                        in_dim = out_dim;
                        StackLayer { w, b }
                    })
                    .collect()
            })
            .collect();

        Network::new_bucketed(
            V2_H,
            Activation::SquaredClippedRelu,
            V2_QA,
            V2_SCALE,
            BucketedParameters {
                w_ft,
                b_ft,
                layer_dims: V2_DIMS.to_vec(),
                layer_scales: V2_SCALES.to_vec(),
                buckets,
            },
        )
        .expect("the bucketed sample satisfies the build invariant")
    }

    #[test]
    fn bucketed_network_round_trips_to_identical_weights_and_metadata() {
        let net = sample_bucketed_network();
        assert_eq!(net.format_version(), FORMAT_VERSION_V2);
        // qb mirrors the final layer's scale.
        assert_eq!(net.qb(), *V2_SCALES.last().unwrap() as u16);

        let bytes = to_bytes(&net);
        assert_eq!(&bytes[..4], &MAGIC);
        assert_eq!(u16_le(&bytes, OFF_FORMAT_VERSION), FORMAT_VERSION_V2);
        assert_eq!(u16_le(&bytes, OFF_NUM_BUCKETS), V2_BUCKETS);
        assert_eq!(u16_le(&bytes, OFF_NUM_OUTPUT_LAYERS), V2_DIMS.len() as u16);

        let reloaded = Network::read(&mut bytes.as_slice()).unwrap();
        assert_eq!(reloaded, net);
        match reloaded.output() {
            OutputStack::Bucketed(stack) => {
                assert_eq!(stack.num_buckets(), V2_BUCKETS);
                assert_eq!(stack.layer_dims(), V2_DIMS);
                assert_eq!(stack.layer_scales(), V2_SCALES);
            }
            OutputStack::Single { .. } => panic!("expected a bucketed stack"),
        }
    }

    #[test]
    fn bucketed_header_records_counts_and_leaves_v2_reserved_zero() {
        let bytes = to_bytes(&sample_bucketed_network());
        // The two v2 counts live where v1 leaves reserved; the remaining reserved
        // bytes stay zero so an older loader still refuses a future flag.
        assert!(bytes[OFF_V2_RESERVED..HEADER_LEN].iter().all(|&b| b == 0));
    }

    #[test]
    fn a_v1_loader_rule_still_rejects_nonzero_bytes_past_the_v2_counts() {
        let mut bytes = to_bytes(&sample_bucketed_network());
        bytes[OFF_V2_RESERVED + 3] = 1;
        // The hash would also fail, but the reserved check runs first on the header.
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::ReservedNotZero)
        ));
    }

    #[test]
    fn zero_bucket_count_is_rejected() {
        let mut bytes = to_bytes(&sample_bucketed_network());
        bytes[OFF_NUM_BUCKETS..OFF_NUM_BUCKETS + 2].copy_from_slice(&0u16.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::InvalidBucketCount(0))
        ));
    }

    #[test]
    fn too_many_buckets_is_rejected() {
        let mut bytes = to_bytes(&sample_bucketed_network());
        bytes[OFF_NUM_BUCKETS..OFF_NUM_BUCKETS + 2]
            .copy_from_slice(&(MAX_BUCKETS + 1).to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::InvalidBucketCount(n)) if n == MAX_BUCKETS + 1
        ));
    }

    #[test]
    fn zero_output_layers_is_rejected() {
        let mut bytes = to_bytes(&sample_bucketed_network());
        bytes[OFF_NUM_OUTPUT_LAYERS..OFF_NUM_OUTPUT_LAYERS + 2]
            .copy_from_slice(&0u16.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::InvalidLayerCount(0))
        ));
    }

    /// The `stack_dims` and `stack_scales` tables start at the blob, right after
    /// the header; each entry is a little-endian u32.
    fn stack_dim_offset(index: usize) -> usize {
        HEADER_LEN + index * 4
    }
    fn stack_scale_offset(index: usize, num_layers: usize) -> usize {
        HEADER_LEN + (num_layers + index) * 4
    }

    #[test]
    fn final_stack_dim_not_output_dim_is_rejected() {
        let mut bytes = to_bytes(&sample_bucketed_network());
        let off = stack_dim_offset(V2_DIMS.len() - 1);
        bytes[off..off + 4].copy_from_slice(&2u32.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::StackFinalDimMismatch {
                found: 2,
                expected: 1
            })
        ));
    }

    #[test]
    fn hidden_stack_dim_not_multiple_of_16_is_rejected() {
        let mut bytes = to_bytes(&sample_bucketed_network());
        let off = stack_dim_offset(0);
        bytes[off..off + 4].copy_from_slice(&24u32.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::InvalidStackHiddenDim { index: 0, dim: 24 })
        ));
    }

    #[test]
    fn zero_stack_scale_is_rejected() {
        let mut bytes = to_bytes(&sample_bucketed_network());
        let off = stack_scale_offset(0, V2_DIMS.len());
        bytes[off..off + 4].copy_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::NonPositiveStackScale { index: 0 })
        ));
    }

    #[test]
    fn final_stack_scale_disagreeing_with_qb_is_rejected() {
        let mut bytes = to_bytes(&sample_bucketed_network());
        // Change the final scale in the blob but leave the header qb; they must match.
        let off = stack_scale_offset(V2_DIMS.len() - 1, V2_DIMS.len());
        bytes[off..off + 4].copy_from_slice(&257u32.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::StackFinalScaleMismatch { found: 257, .. })
        ));
    }

    #[test]
    fn bucketed_param_bytes_disagreeing_is_rejected() {
        let mut bytes = to_bytes(&sample_bucketed_network());
        let wrong = u32_le(&bytes, OFF_PARAM_BYTES) + 1;
        bytes[OFF_PARAM_BYTES..OFF_PARAM_BYTES + 4].copy_from_slice(&wrong.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::ParamBytesMismatch { .. })
        ));
    }

    #[test]
    fn bucketed_corrupt_blob_fails_the_hash_check() {
        let mut bytes = to_bytes(&sample_bucketed_network());
        // Flip a bit in a bucket weight, well past the header and prefix.
        let idx = bytes.len() - 5;
        bytes[idx] ^= 0x01;
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::HashMismatch { .. })
        ));
    }

    #[test]
    fn bucketed_truncated_and_trailing_are_rejected() {
        let bytes = to_bytes(&sample_bucketed_network());
        let mut short = &bytes[..bytes.len() - 1];
        assert!(matches!(
            Network::read(&mut short),
            Err(LoadError::Truncated)
        ));

        let mut extended = bytes.clone();
        extended.push(0);
        assert!(matches!(
            Network::read(&mut extended.as_slice()),
            Err(LoadError::TrailingBytes)
        ));
    }

    #[test]
    fn new_bucketed_rejects_bad_shapes() {
        let h = V2_H as usize;
        let w_ft = vec![0i16; INPUT_DIM as usize * h];
        let b_ft = vec![0i16; h];
        let good_layer = |out: usize, inp: usize| StackLayer {
            w: vec![0i8; out * inp],
            b: vec![0i32; out],
        };
        // Final dim not OUTPUT_DIM.
        assert!(matches!(
            Network::new_bucketed(
                V2_H,
                Activation::ClippedRelu,
                V2_QA,
                V2_SCALE,
                BucketedParameters {
                    w_ft: w_ft.clone(),
                    b_ft: b_ft.clone(),
                    layer_dims: vec![16, 2],
                    layer_scales: vec![64, 64],
                    buckets: vec![vec![good_layer(16, 2 * h), good_layer(2, 16)]],
                },
            ),
            Err(BuildError::StackFinalDimMismatch { found: 2, .. })
        ));
        // A short weight block.
        assert!(matches!(
            Network::new_bucketed(
                V2_H,
                Activation::ClippedRelu,
                V2_QA,
                V2_SCALE,
                BucketedParameters {
                    w_ft,
                    b_ft,
                    layer_dims: vec![16, 1],
                    layer_scales: vec![64, 64],
                    buckets: vec![vec![
                        StackLayer {
                            w: vec![0i8; 16 * (2 * h) - 1],
                            b: vec![0i32; 16]
                        },
                        good_layer(1, 16)
                    ]],
                },
            ),
            Err(BuildError::WeightCountMismatch {
                block: "stack_layer_w",
                ..
            })
        ));
    }
}
