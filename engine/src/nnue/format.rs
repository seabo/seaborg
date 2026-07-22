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

/// File-format version this build reads and writes. A file with any other
/// version is rejected rather than reinterpreted under `v1` assumptions.
pub const FORMAT_VERSION: u16 = 1;

/// Feature-set id for the perspective-doubled 768-input piece-square set — the
/// only set this build implements.
pub const FEATURE_SET_PERSPECTIVE_768: u16 = 0;

/// Input dimension implied by [`FEATURE_SET_PERSPECTIVE_768`]:
/// `2 colours × 6 piece types × 64 squares`, one perspective's sparse input.
pub const INPUT_DIM: u32 = 768;

/// Activation id for clipped ReLU — the only activation this build implements.
pub const ACTIVATION_CRELU: u16 = 0;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    hidden_width: u32,
    qa: u16,
    qb: u16,
    scale: i32,
    /// Feature-transformer weights, `INPUT_DIM × H`, feature-major: the `H`
    /// weights for feature `f` are contiguous at `f · H`.
    w_ft: Vec<i16>,
    /// Feature-transformer bias, length `H`.
    b_ft: Vec<i16>,
    /// Output weights, length `2H`: own-perspective block then enemy block.
    w_out: Vec<i16>,
    /// Output bias, length [`OUTPUT_DIM`].
    b_out: Vec<i32>,
}

impl Network {
    /// Builds a network from its dimensions, scales, and weight blocks,
    /// enforcing the type invariant.
    ///
    /// Fails if the hidden width is not a positive multiple of 16, if any scale
    /// is non-positive, or if a weight block's length disagrees with the width.
    pub fn new(
        hidden_width: u32,
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
            qa,
            qb,
            scale,
            w_ft,
            b_ft,
            w_out,
            b_out,
        })
    }

    /// The feature-transformer output width per perspective (`H`).
    pub fn hidden_width(&self) -> u32 {
        self.hidden_width
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

    /// Output weights, length `2H`.
    pub fn output_weights(&self) -> &[i16] {
        &self.w_out
    }

    /// Output bias, length [`OUTPUT_DIM`].
    pub fn output_bias(&self) -> &[i32] {
        &self.b_out
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

    /// Number of bytes the parameter blob occupies on disk.
    fn param_bytes(&self) -> u64 {
        2 * self.w_ft.len() as u64
            + 2 * self.b_ft.len() as u64
            + 2 * self.w_out.len() as u64
            + 4 * self.b_out.len() as u64
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
            .copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        header[OFF_FEATURE_SET_ID..OFF_FEATURE_SET_ID + 2]
            .copy_from_slice(&FEATURE_SET_PERSPECTIVE_768.to_le_bytes());
        header[OFF_INPUT_DIM..OFF_INPUT_DIM + 4].copy_from_slice(&INPUT_DIM.to_le_bytes());
        header[OFF_HIDDEN_WIDTH..OFF_HIDDEN_WIDTH + 4]
            .copy_from_slice(&self.hidden_width.to_le_bytes());
        header[OFF_OUTPUT_DIM..OFF_OUTPUT_DIM + 2].copy_from_slice(&OUTPUT_DIM.to_le_bytes());
        header[OFF_ACTIVATION_ID..OFF_ACTIVATION_ID + 2]
            .copy_from_slice(&ACTIVATION_CRELU.to_le_bytes());
        header[OFF_QA..OFF_QA + 2].copy_from_slice(&self.qa.to_le_bytes());
        header[OFF_QB..OFF_QB + 2].copy_from_slice(&self.qb.to_le_bytes());
        header[OFF_SCALE..OFF_SCALE + 4].copy_from_slice(&self.scale.to_le_bytes());
        header[OFF_PARAM_BYTES..OFF_PARAM_BYTES + 4].copy_from_slice(&param_bytes.to_le_bytes());
        header[OFF_PARAM_HASH..OFF_PARAM_HASH + 8].copy_from_slice(&fnv1a_64(&blob).to_le_bytes());
        // Reserved bytes stay zero: an older loader rejects any future flag set
        // here rather than silently ignoring it.

        out.write_all(&header)?;
        out.write_all(&blob)?;
        Ok(())
    }

    /// The parameter blob in the fixed on-disk order: `W_ft`, `b_ft`, `W_out`,
    /// `b_out`, each element little-endian.
    fn encode_blob(&self) -> Vec<u8> {
        let mut blob = Vec::with_capacity(self.param_bytes() as usize);
        for &w in &self.w_ft {
            blob.extend_from_slice(&w.to_le_bytes());
        }
        for &b in &self.b_ft {
            blob.extend_from_slice(&b.to_le_bytes());
        }
        for &w in &self.w_out {
            blob.extend_from_slice(&w.to_le_bytes());
        }
        for &b in &self.b_out {
            blob.extend_from_slice(&b.to_le_bytes());
        }
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
        if format_version != FORMAT_VERSION {
            return Err(LoadError::UnsupportedVersion(format_version));
        }

        let feature_set_id = u16_le(&header, OFF_FEATURE_SET_ID);
        if feature_set_id != FEATURE_SET_PERSPECTIVE_768 {
            return Err(LoadError::UnsupportedFeatureSet(feature_set_id));
        }
        let activation_id = u16_le(&header, OFF_ACTIVATION_ID);
        if activation_id != ACTIVATION_CRELU {
            return Err(LoadError::UnsupportedActivation(activation_id));
        }

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
        let declared_bytes = u32_le(&header, OFF_PARAM_BYTES);
        if u64::from(declared_bytes) != expected_bytes {
            return Err(LoadError::ParamBytesMismatch {
                declared: declared_bytes,
                expected: expected_bytes,
            });
        }

        // Read exactly the expected blob without pre-sizing a buffer from the
        // untrusted length: `take` bounds the read, and a short file grows the
        // buffer only to what is actually present before it is caught as
        // truncation.
        let mut blob = Vec::new();
        input
            .take(expected_bytes)
            .read_to_end(&mut blob)
            .map_err(LoadError::Io)?;
        if blob.len() as u64 != expected_bytes {
            return Err(LoadError::Truncated);
        }
        // Any byte beyond the accounted-for blob means the file is longer than
        // its header claims — refuse it rather than silently ignore the tail.
        let mut extra = [0u8; 1];
        match input.read(&mut extra) {
            Ok(0) => {}
            Ok(_) => return Err(LoadError::TrailingBytes),
            Err(e) => return Err(LoadError::Io(e)),
        }

        let declared_hash = u64_le(&header, OFF_PARAM_HASH);
        let computed_hash = fnv1a_64(&blob);
        if declared_hash != computed_hash {
            return Err(LoadError::HashMismatch {
                declared: declared_hash,
                computed: computed_hash,
            });
        }

        Ok(Self::decode_blob(hidden_width, qa, qb, scale, &blob))
    }

    /// Splits a validated blob into the four weight blocks. The blob length was
    /// already checked against these dimensions, so the slicing is exact.
    fn decode_blob(hidden_width: u32, qa: u16, qb: u16, scale: i32, blob: &[u8]) -> Self {
        let h = hidden_width as usize;
        let mut cursor = BlobCursor::new(blob);
        let w_ft = cursor.take_i16(INPUT_DIM as usize * h);
        let b_ft = cursor.take_i16(h);
        let w_out = cursor.take_i16(2 * h);
        let b_out = cursor.take_i32(OUTPUT_DIM as usize);
        Self {
            hidden_width,
            qa,
            qb,
            scale,
            w_ft,
            b_ft,
            w_out,
            b_out,
        }
    }
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
                write!(f, "unsupported network format version {v}; this build reads version {FORMAT_VERSION}")
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
        bytes[OFF_FORMAT_VERSION..OFF_FORMAT_VERSION + 2].copy_from_slice(&2u16.to_le_bytes());
        // The version is checked before the hash, so a stale hash is irrelevant.
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::UnsupportedVersion(2))
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
        bytes[OFF_ACTIVATION_ID..OFF_ACTIVATION_ID + 2].copy_from_slice(&1u16.to_le_bytes());
        assert!(matches!(
            Network::read(&mut bytes.as_slice()),
            Err(LoadError::UnsupportedActivation(1))
        ));
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
            Network::new(17, QA, QB, SCALE, empty_params()),
            Err(BuildError::InvalidHiddenWidth(17))
        ));
        // Non-positive scale.
        assert!(matches!(
            Network::new(H, 0, QB, SCALE, empty_params()),
            Err(BuildError::NonPositiveScale { field: "qa", .. })
        ));
        // Right width and scales but a short weight block.
        let h = H as usize;
        assert!(matches!(
            Network::new(
                H,
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
}
