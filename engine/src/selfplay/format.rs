//! The packed on-disk format for a labelled self-play position.
//!
//! Training consumes hundreds of millions of positions, so each one is stored as
//! a small fixed-size record that streams sequentially and can be indexed by
//! multiplication. A record is the trio the trainer needs: the position, the
//! search score for it, and the eventual game outcome. Nothing else the game
//! loop tracks (move history, the chosen move, adjudication reason) is stored —
//! those do not train the network.
//!
//! # Record layout (32 bytes, little-endian)
//!
//! | Bytes | Field        | Meaning                                              |
//! |-------|--------------|------------------------------------------------------|
//! | 0..8  | occupancy    | Bit *i* set iff square *i* (A1=0) holds a piece.      |
//! | 8..24 | pieces       | One 4-bit code per occupied square, in ascending     |
//! |       |              | square order; low nibble of a byte first. Codes match |
//! |       |              | the piece encoding 1=P…6=K white, 7=p…12=k black.     |
//! | 24    | flags        | bit0 side to move (0 White, 1 Black); bits1..5        |
//! |       |              | castling WK, WQ, BK, BQ.                             |
//! | 25    | ep           | En-passant target square index, or 0xFF for none.    |
//! | 26    | halfmove     | Fifty-move half-move clock (saturates at 255).       |
//! | 27..29| fullmove     | Full move number (saturates at u16::MAX).            |
//! | 29..31| score        | Search score, the raw [`Score`] i16 (mate band kept).|
//! | 31    | wdl          | Outcome from the side to move: 0 loss, 1 draw, 2 win.|
//!
//! A stream of records is prefixed by an 8-byte header — a magic tag, a format
//! version, and the record size — so a reader can reject a file written by an
//! incompatible version rather than misinterpret its bytes. The version is the
//! hook for evolving this layout later without silently corrupting old data.

use std::io::{self, Read, Write};

use chess::position::{Piece, Position, Square};

use super::{Sample, Wdl};
use crate::score::Score;

/// Bytes per packed record.
pub const RECORD_SIZE: usize = 32;

/// Tag at the head of a sample stream, identifying the format.
const MAGIC: [u8; 4] = *b"SBRG";

/// The format version this build writes and is willing to read. Bump it on any
/// change to the record layout so old and new files never mix silently.
pub const FORMAT_VERSION: u16 = 1;

/// Bytes in the stream header: magic (4) + version (2) + record size (2).
pub const HEADER_SIZE: usize = 8;

/// Sentinel byte for "no en-passant square".
const NO_EP: u8 = 0xFF;

/// First byte of the piece-nibble block, just past the 8-byte occupancy word.
const PIECE_BLOCK_START: usize = 8;
/// One past the last byte of the piece-nibble block, where the flags byte begins.
const PIECE_BLOCK_END: usize = 24;
/// The largest number of pieces a record can encode: two nibbles per byte across
/// the piece block. A legal chess position never exceeds 32 pieces.
const MAX_PIECES: u32 = (PIECE_BLOCK_END as u32 - PIECE_BLOCK_START as u32) * 2;

/// An error decoding a packed record or stream header.
#[derive(Debug)]
pub enum FormatError {
    /// The stream did not begin with the expected magic tag.
    BadMagic,
    /// The stream declares a format version this build cannot read.
    UnsupportedVersion(u16),
    /// The stream declares a record size this build cannot read.
    BadRecordSize(u16),
    /// The stream ended partway through the header or a record.
    UnexpectedEof,
    /// A piece nibble held a code outside 1..=12.
    InvalidPieceCode(u8),
    /// The occupancy names more squares than the piece block can encode.
    TooManyPieces(u32),
    /// The outcome byte was not one of 0, 1, 2.
    InvalidWdl(u8),
    /// The decoded fields did not form a legal position.
    InvalidPosition(String),
    /// The underlying reader or writer failed.
    Io(io::Error),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::BadMagic => write!(f, "not a Seaborg sample stream (bad magic)"),
            FormatError::UnsupportedVersion(v) => {
                write!(f, "unsupported sample format version {v}")
            }
            FormatError::BadRecordSize(n) => write!(f, "unexpected record size {n}"),
            FormatError::UnexpectedEof => write!(f, "stream ended mid-record"),
            FormatError::InvalidPieceCode(c) => write!(f, "invalid piece code {c}"),
            FormatError::TooManyPieces(n) => {
                write!(f, "occupancy names {n} pieces; at most 32 fit")
            }
            FormatError::InvalidWdl(b) => write!(f, "invalid outcome byte {b}"),
            FormatError::InvalidPosition(msg) => write!(f, "decoded an illegal position: {msg}"),
            FormatError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for FormatError {}

impl From<io::Error> for FormatError {
    fn from(e: io::Error) -> Self {
        FormatError::Io(e)
    }
}

/// One labelled position in its packed on-disk form.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct PackedSample {
    bytes: [u8; RECORD_SIZE],
}

impl PackedSample {
    /// Pack a scored self-play position. The sample's best move is intentionally
    /// not stored: it exists only to filter positions before this point.
    pub fn from_sample(sample: &Sample) -> Self {
        Self::from_parts(&sample.position, sample.score, sample.outcome)
    }

    /// Pack an explicit position, score, and outcome.
    pub fn from_parts(position: &Position, score: Score, outcome: Wdl) -> Self {
        let mut bytes = [0u8; RECORD_SIZE];

        let mut occupancy: u64 = 0;
        let mut piece_index = 0usize;
        for i in 0..64u8 {
            let sq = Square::try_from(i).expect("index below 64 is a valid square");
            let piece = position.piece_at_sq(sq);
            if piece.is_none() {
                continue;
            }
            occupancy |= 1u64 << i;
            let code = piece as u8; // 1..=12; empty squares are skipped above.
            let byte = PIECE_BLOCK_START + piece_index / 2;
            if piece_index.is_multiple_of(2) {
                bytes[byte] = code;
            } else {
                bytes[byte] |= code << 4;
            }
            piece_index += 1;
        }
        bytes[0..8].copy_from_slice(&occupancy.to_le_bytes());

        let mut flags = 0u8;
        if position.turn().is_black() {
            flags |= 1 << 0;
        }
        let castling = position.castling_rights();
        if castling.white_kingside() {
            flags |= 1 << 1;
        }
        if castling.white_queenside() {
            flags |= 1 << 2;
        }
        if castling.black_kingside() {
            flags |= 1 << 3;
        }
        if castling.black_queenside() {
            flags |= 1 << 4;
        }
        bytes[24] = flags;

        bytes[25] = position.ep_square().map_or(NO_EP, |sq| sq.index());
        // The clocks cannot legally reach these caps — the fifty-move rule bounds
        // the half-move clock at 100 — so saturating never loses a real value.
        bytes[26] = u8::try_from(position.half_move_clock()).unwrap_or(u8::MAX);
        let fullmove = u16::try_from(position.move_number()).unwrap_or(u16::MAX);
        bytes[27..29].copy_from_slice(&fullmove.to_le_bytes());

        bytes[29..31].copy_from_slice(&score.to_i16().to_le_bytes());
        bytes[31] = outcome_byte(outcome);

        Self { bytes }
    }

    /// Wrap 32 raw bytes as a record. The bytes are validated lazily, when a
    /// field is decoded.
    pub fn from_bytes(bytes: [u8; RECORD_SIZE]) -> Self {
        Self { bytes }
    }

    /// The raw record bytes, as written to disk.
    pub fn as_bytes(&self) -> &[u8; RECORD_SIZE] {
        &self.bytes
    }

    /// Decode the stored position.
    pub fn position(&self) -> Result<Position, FormatError> {
        let occupancy = u64::from_le_bytes(self.bytes[0..8].try_into().unwrap());
        let piece_count = occupancy.count_ones();
        if piece_count > MAX_PIECES {
            return Err(FormatError::TooManyPieces(piece_count));
        }

        // Rebuild the board as FEN characters, then hand the whole position to
        // the FEN parser. Reusing that path means the reconstructed position is
        // built and validated exactly as any other parsed position — castling,
        // en-passant canonicalisation, hash and derived state included — instead
        // of duplicating that logic here.
        let mut squares = [None; 64];
        let mut remaining = occupancy;
        let mut piece_index = 0usize;
        while remaining != 0 {
            let sq = remaining.trailing_zeros() as usize;
            remaining &= remaining - 1;
            let byte = self.bytes[PIECE_BLOCK_START + piece_index / 2];
            let code = if piece_index.is_multiple_of(2) {
                byte & 0x0F
            } else {
                byte >> 4
            };
            squares[sq] = Some(piece_fen_char(code)?);
            piece_index += 1;
        }

        let fen = self.build_fen(&squares);
        Position::from_fen(&fen).map_err(|e| FormatError::InvalidPosition(e.to_string()))
    }

    /// Decode the stored search score.
    pub fn score(&self) -> Score {
        Score::from_i16(i16::from_le_bytes(self.bytes[29..31].try_into().unwrap()))
    }

    /// Decode the stored game outcome.
    pub fn outcome(&self) -> Result<Wdl, FormatError> {
        match self.bytes[31] {
            0 => Ok(Wdl::Loss),
            1 => Ok(Wdl::Draw),
            2 => Ok(Wdl::Win),
            other => Err(FormatError::InvalidWdl(other)),
        }
    }

    /// Assemble a FEN string from the decoded board and metadata bytes.
    fn build_fen(&self, squares: &[Option<char>; 64]) -> String {
        let mut fen = String::with_capacity(90);

        // Placement, from rank 8 down to rank 1.
        for rank in (0..8).rev() {
            let mut empties = 0u8;
            for file in 0..8 {
                match squares[rank * 8 + file] {
                    Some(piece) => {
                        if empties != 0 {
                            fen.push((b'0' + empties) as char);
                            empties = 0;
                        }
                        fen.push(piece);
                    }
                    None => empties += 1,
                }
            }
            if empties != 0 {
                fen.push((b'0' + empties) as char);
            }
            if rank != 0 {
                fen.push('/');
            }
        }

        let flags = self.bytes[24];
        fen.push(' ');
        fen.push(if flags & 1 == 0 { 'w' } else { 'b' });

        fen.push(' ');
        let mut any_castle = false;
        for (bit, ch) in [(1, 'K'), (2, 'Q'), (3, 'k'), (4, 'q')] {
            if flags & (1 << bit) != 0 {
                fen.push(ch);
                any_castle = true;
            }
        }
        if !any_castle {
            fen.push('-');
        }

        fen.push(' ');
        match self.bytes[25] {
            NO_EP => fen.push('-'),
            ep => {
                let file = (ep % 8) as usize;
                let rank = (ep / 8) as usize;
                fen.push((b'a' + file as u8) as char);
                fen.push((b'1' + rank as u8) as char);
            }
        }

        let fullmove = u16::from_le_bytes(self.bytes[27..29].try_into().unwrap());
        fen.push(' ');
        fen.push_str(&self.bytes[26].to_string());
        fen.push(' ');
        fen.push_str(&fullmove.to_string());

        fen
    }
}

fn outcome_byte(outcome: Wdl) -> u8 {
    match outcome {
        Wdl::Loss => 0,
        Wdl::Draw => 1,
        Wdl::Win => 2,
    }
}

/// Map a stored piece code (1..=12) to its FEN character.
fn piece_fen_char(code: u8) -> Result<char, FormatError> {
    let ch = match code {
        c if c == Piece::WhitePawn as u8 => 'P',
        c if c == Piece::WhiteKnight as u8 => 'N',
        c if c == Piece::WhiteBishop as u8 => 'B',
        c if c == Piece::WhiteRook as u8 => 'R',
        c if c == Piece::WhiteQueen as u8 => 'Q',
        c if c == Piece::WhiteKing as u8 => 'K',
        c if c == Piece::BlackPawn as u8 => 'p',
        c if c == Piece::BlackKnight as u8 => 'n',
        c if c == Piece::BlackBishop as u8 => 'b',
        c if c == Piece::BlackRook as u8 => 'r',
        c if c == Piece::BlackQueen as u8 => 'q',
        c if c == Piece::BlackKing as u8 => 'k',
        other => return Err(FormatError::InvalidPieceCode(other)),
    };
    Ok(ch)
}

/// Writes a versioned stream of packed samples.
pub struct SampleWriter<W: Write> {
    inner: W,
}

impl<W: Write> SampleWriter<W> {
    /// Begin a stream, writing its header immediately.
    pub fn new(mut inner: W) -> io::Result<Self> {
        let mut header = [0u8; HEADER_SIZE];
        header[0..4].copy_from_slice(&MAGIC);
        header[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        header[6..8].copy_from_slice(&(RECORD_SIZE as u16).to_le_bytes());
        inner.write_all(&header)?;
        Ok(Self { inner })
    }

    /// Append one already-packed record.
    pub fn write(&mut self, packed: &PackedSample) -> io::Result<()> {
        self.inner.write_all(packed.as_bytes())
    }

    /// Pack and append one labelled sample.
    pub fn write_sample(&mut self, sample: &Sample) -> io::Result<()> {
        self.write(&PackedSample::from_sample(sample))
    }

    /// Recover the underlying writer, e.g. to flush a buffered file.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

/// Reads a versioned stream of packed samples.
pub struct SampleReader<R: Read> {
    inner: R,
}

impl<R: Read> SampleReader<R> {
    /// Begin reading a stream, validating and consuming its header.
    pub fn new(mut inner: R) -> Result<Self, FormatError> {
        let mut header = [0u8; HEADER_SIZE];
        fill(&mut inner, &mut header)?.ok_or(FormatError::UnexpectedEof)?;
        if header[0..4] != MAGIC {
            return Err(FormatError::BadMagic);
        }
        let version = u16::from_le_bytes(header[4..6].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(FormatError::UnsupportedVersion(version));
        }
        let record_size = u16::from_le_bytes(header[6..8].try_into().unwrap());
        if record_size as usize != RECORD_SIZE {
            return Err(FormatError::BadRecordSize(record_size));
        }
        Ok(Self { inner })
    }

    /// Read the next record, or `None` at a clean end of stream.
    pub fn read(&mut self) -> Result<Option<PackedSample>, FormatError> {
        let mut bytes = [0u8; RECORD_SIZE];
        match fill(&mut self.inner, &mut bytes)? {
            Some(()) => Ok(Some(PackedSample::from_bytes(bytes))),
            None => Ok(None),
        }
    }
}

/// Fill `buf` completely. Returns `Ok(Some(()))` on success, `Ok(None)` if the
/// stream ended cleanly at the buffer boundary (nothing read), and
/// `Err(UnexpectedEof)` if it ended partway through.
fn fill(reader: &mut impl Read, buf: &mut [u8]) -> Result<Option<()>, FormatError> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => {
                if filled == 0 {
                    return Ok(None);
                }
                return Err(FormatError::UnexpectedEof);
            }
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(FormatError::Io(e)),
        }
    }
    Ok(Some(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess::init::init_globals;

    fn position(fen: &str) -> Position {
        init_globals();
        Position::from_fen(fen).expect("valid FEN")
    }

    /// A representative set of positions exercising every field: initial,
    /// castling rights, an en-passant target, promotions, Black to move, and a
    /// non-trivial clock and move number.
    fn sample_positions() -> Vec<&'static str> {
        vec![
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 5 12",
            "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3",
            "8/P6k/8/8/8/8/6Kp/8 b - - 0 40",
            "4k3/8/8/8/8/8/8/4K3 b - - 99 200",
        ]
    }

    #[test]
    fn positions_round_trip_through_the_packing() {
        for fen in sample_positions() {
            let pos = position(fen);
            let packed = PackedSample::from_parts(&pos, Score::cp(37), Wdl::Win);
            let decoded = packed.position().expect("decodes");
            // A packed record drops move history, so equality is judged by the
            // canonical FEN — every board and metadata field the format stores.
            assert_eq!(decoded.to_fen(), pos.to_fen(), "mismatch for {fen}");
        }
    }

    #[test]
    fn score_and_outcome_round_trip() {
        let pos = position("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
        for score in [
            Score::cp(0),
            Score::cp(-123),
            Score::cp(456),
            Score::mate(5),
        ] {
            for outcome in [Wdl::Win, Wdl::Draw, Wdl::Loss] {
                let packed = PackedSample::from_parts(&pos, score, outcome);
                assert_eq!(packed.score(), score);
                assert_eq!(packed.outcome().unwrap(), outcome);
            }
        }
    }

    #[test]
    fn bytes_round_trip_through_from_bytes() {
        let pos = position("r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 5 12");
        let packed = PackedSample::from_parts(&pos, Score::mate(-3), Wdl::Loss);
        let reparsed = PackedSample::from_bytes(*packed.as_bytes());
        assert_eq!(reparsed, packed);
        assert_eq!(reparsed.position().unwrap().to_fen(), pos.to_fen());
        assert_eq!(reparsed.score(), Score::mate(-3));
        assert_eq!(reparsed.outcome().unwrap(), Wdl::Loss);
    }

    #[test]
    fn a_record_is_exactly_the_declared_size() {
        let pos = position("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
        let packed = PackedSample::from_parts(&pos, Score::zero(), Wdl::Draw);
        assert_eq!(packed.as_bytes().len(), RECORD_SIZE);
    }

    #[test]
    fn stream_writes_and_reads_back_every_sample() {
        let packed: Vec<PackedSample> = sample_positions()
            .into_iter()
            .enumerate()
            .map(|(i, fen)| {
                PackedSample::from_parts(&position(fen), Score::cp(i as i16), Wdl::Draw)
            })
            .collect();

        let mut writer = SampleWriter::new(Vec::new()).unwrap();
        for record in &packed {
            writer.write(record).unwrap();
        }
        let buffer = writer.into_inner();

        assert_eq!(buffer.len(), HEADER_SIZE + packed.len() * RECORD_SIZE);

        let mut reader = SampleReader::new(buffer.as_slice()).unwrap();
        let mut read_back = Vec::new();
        while let Some(record) = reader.read().unwrap() {
            read_back.push(record);
        }
        assert_eq!(read_back, packed);
    }

    #[test]
    fn reader_rejects_a_foreign_stream() {
        let bytes = *b"XXXX\x01\x00\x20\x00";
        assert!(matches!(
            SampleReader::new(bytes.as_slice()),
            Err(FormatError::BadMagic)
        ));
    }

    #[test]
    fn reader_rejects_an_unsupported_version() {
        let mut header = [0u8; HEADER_SIZE];
        header[0..4].copy_from_slice(&MAGIC);
        header[4..6].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        header[6..8].copy_from_slice(&(RECORD_SIZE as u16).to_le_bytes());
        assert!(matches!(
            SampleReader::new(header.as_slice()),
            Err(FormatError::UnsupportedVersion(v)) if v == FORMAT_VERSION + 1
        ));
    }

    #[test]
    fn a_truncated_record_is_an_error_not_a_silent_stop() {
        let pos = position("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
        let packed = PackedSample::from_parts(&pos, Score::zero(), Wdl::Draw);
        let mut writer = SampleWriter::new(Vec::new()).unwrap();
        writer.write(&packed).unwrap();
        let mut buffer = writer.into_inner();
        // Lop off the last byte of the single record.
        buffer.pop();

        let mut reader = SampleReader::new(buffer.as_slice()).unwrap();
        assert!(matches!(reader.read(), Err(FormatError::UnexpectedEof)));
    }

    #[test]
    fn an_invalid_outcome_byte_is_reported() {
        let pos = position("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
        let mut packed = PackedSample::from_parts(&pos, Score::zero(), Wdl::Draw);
        packed.bytes[31] = 7;
        assert!(matches!(packed.outcome(), Err(FormatError::InvalidWdl(7))));
    }
}
