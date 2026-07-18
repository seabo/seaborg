//! Transposition table.

use super::score::Score;
use core::mov::{Move, MoveType};
use core::position::{PieceType, Position, Square};

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

/// The validity of a stored `Score`.
///
/// Sometimes, we will store exact values in the transposition table. Other times, a node will
/// experience a cutoff but we will still store the lower or upper bound.
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum Bound {
    Exact = 0,
    Upper,
    Lower,
}

/// A live transposition-table generation.
///
/// Zero is excluded because the packed entry representation reserves it for empty slots. The
/// remaining six-bit values are the live generations used by the table.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Generation(u8);

impl Generation {
    const FIRST: Self = Self(1);
    const LAST: Self = Self(63);

    fn next(self) -> Option<Self> {
        (self != Self::LAST).then(|| Self(self.0 + 1))
    }
}

impl TryFrom<u8> for Generation {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if (Self::FIRST.0..=Self::LAST.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err("generation must be in 1..=63")
        }
    }
}

/// A single byte packing an entry's generation and score bound.
///
/// A raw zero byte represents an empty entry. Live entries are constructed with a checked
/// [`Generation`], so their generation bits cannot alias empty or exceed the six-bit range.
#[derive(Copy, Clone, Debug, Default)]
pub struct GenBound(u8);

impl GenBound {
    /// Pack a valid live generation with its bound.
    pub fn new(generation: Generation, bound: Bound) -> Self {
        let v = (generation.0 << 2) | bound as u8;
        Self(v)
    }

    /// Extract the generation and `Bound` from this `GenBound`.
    #[inline(always)]
    pub fn to_raw_parts(&self) -> (u8, Bound) {
        (self.gen(), self.bound())
    }

    /// Extract the generation.
    #[inline(always)]
    pub fn gen(&self) -> u8 {
        (self.0 & (0xFF << 2)) >> 2
    }

    /// Extract the bound.
    #[inline(always)]
    pub fn bound(&self) -> Bound {
        match self.0 & 0x3 {
            0 => Bound::Exact,
            1 => Bound::Upper,
            2 => Bound::Lower,
            _ => unreachable!("invalid bound bits"),
        }
    }
}

/// A condensed move representation to save space in the transposition table.
///
/// Our normal `Move` struct records 4 bytes of information: two for the from/to squares, another
/// for the possible promotion type (queen, rook, bishop, knight), and a final byte with flags
/// indicating features of the move (promotion, en passant, castling, capture, quiet).
///
/// In the transposition table, we don't need this detail. We can just record the origin square (6
/// bits), the destination square (6 bits), the promotion piece if applicable (3 bits) and a flag
/// for whether the move is null (because in some cases we want to store an entry without a move).
///
/// The null flag is inverted - 0 means non-null, 1 means null. This is so that we can represent a
/// null move with PackedMove(0_u16) which feels cleanest.
///
/// The scheme is, reading from LSB to MSB:
///
/// 0 0 0 0   0 0 0 0   0 0 0 0   0 0 0 0
/// ^ |---|   |-----------| |-----------|
/// |   ^           ^          ^
/// |   |           |          |___ orig square
/// |   |           |___ dest square
/// |   |___ promotion piece
/// |___ null flag
#[derive(Copy, Clone, Debug)]
pub struct PackedMove(u16);

const ORIG_MASK: u16 = 0x3F;
const DEST_MASK: u16 = 0x0FC0;
const PROMO_MASK: u16 = 0x7000;

impl PackedMove {
    /// Create a `PackedMove` from a `Move`.
    pub fn from_move(mov: &Move) -> Self {
        if mov.is_null() {
            PackedMove(0)
        } else {
            let null = 1 << 15;
            let orig = mov.orig().index() as u16;
            let dest = (mov.dest().index() as u16) << 6;
            let promo = match mov.promo_piece_type() {
                Some(p) => ((p as u8 - 1) as u16) << 12,
                None => 0,
            };

            PackedMove(null ^ orig ^ dest ^ promo)
        }
    }

    /// Convert a `PackedMove` back to the corresponding `Move` for a given `Position`.
    pub fn to_move(&self, pos: &Position) -> Move {
        debug_assert!(!self.is_null());

        let orig = Square::try_from((self.0 & ORIG_MASK) as u8)
            .expect("packed origin is masked to six bits");
        let dest = Square::try_from(((self.0 & DEST_MASK) >> 6) as u8)
            .expect("packed destination is masked to six bits");
        let promo = ((self.0 & PROMO_MASK) >> 12) as u8;
        let mut move_type = MoveType::empty();
        let promo_piece = if promo == 0 {
            None
        } else {
            move_type |= MoveType::PROMOTION;
            Some(PieceType::try_from(promo + 1).expect("should never fail"))
        };

        if !pos.piece_at_sq(dest).is_none() {
            move_type |= MoveType::CAPTURE;
        }

        let piece = pos.piece_at_sq(orig);

        if let Some(ep) = pos.ep_square() {
            if ep == dest && piece.type_of() == PieceType::Pawn {
                move_type |= MoveType::EN_PASSANT | MoveType::CAPTURE;
            }
        }

        if piece.type_of() == PieceType::King {
            match (orig, dest) {
                (Square::E1, Square::G1) => move_type |= MoveType::CASTLE,
                (Square::E1, Square::C1) => move_type |= MoveType::CASTLE,
                (Square::E8, Square::G8) => move_type |= MoveType::CASTLE,
                (Square::E8, Square::C8) => move_type |= MoveType::CASTLE,
                _ => {}
            }
        }

        if move_type.is_empty() {
            move_type = MoveType::QUIET;
        }

        Move::build(orig, dest, promo_piece, move_type)
    }

    pub fn is_null(&self) -> bool {
        ((self.0 >> 15) & 1) == 0
    }

    /// Create a null move.
    pub fn null() -> Self {
        PackedMove(0)
    }
}

impl Default for PackedMove {
    fn default() -> Self {
        Self::null()
    }
}

/// An entry in the transposition table.
#[derive(Copy, Clone, Debug, Default)]
#[repr(align(8))]
pub struct Entry {
    pub sig: u16,
    pub depth: u8,
    pub gen_bound: GenBound,
    pub score: Score,
    pub mov: PackedMove,
}

impl Entry {
    const SIG_SHIFT: u32 = 0;
    const DEPTH_SHIFT: u32 = 16;
    const GEN_BOUND_SHIFT: u32 = 24;
    const SCORE_SHIFT: u32 = 32;
    const MOVE_SHIFT: u32 = 48;

    #[inline(always)]
    fn pack(self) -> u64 {
        ((self.sig as u64) << Self::SIG_SHIFT)
            | ((self.depth as u64) << Self::DEPTH_SHIFT)
            | ((self.gen_bound.0 as u64) << Self::GEN_BOUND_SHIFT)
            | (((self.score.to_i16() as u16) as u64) << Self::SCORE_SHIFT)
            | ((self.mov.0 as u64) << Self::MOVE_SHIFT)
    }

    #[inline(always)]
    fn unpack(value: u64) -> Self {
        Self {
            sig: (value >> Self::SIG_SHIFT) as u16,
            depth: (value >> Self::DEPTH_SHIFT) as u8,
            gen_bound: GenBound((value >> Self::GEN_BOUND_SHIFT) as u8),
            score: Score::from_i16((value >> Self::SCORE_SHIFT) as u16 as i16),
            mov: PackedMove((value >> Self::MOVE_SHIFT) as u16),
        }
    }

    /// Returns the generation of this entry.
    #[inline(always)]
    pub fn gen(&self) -> u8 {
        self.gen_bound.to_raw_parts().0
    }

    /// Returns the `Bound` of this entry.
    #[inline(always)]
    pub fn bound(&self) -> Bound {
        self.gen_bound.to_raw_parts().1
    }

    /// Returns if this entry is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.gen() == 0
    }

    /// Write data to an entry
    pub fn write(&mut self) {}
}

/// Represents a transposition table entry that can be written to.
#[derive(Debug)]
pub struct WritableEntry<'a> {
    slot: &'a AtomicU64,
    generation: Generation,
}

impl<'a> WritableEntry<'a> {
    /// Create a `WritableEntry` from an atomic table slot.
    #[inline]
    fn new(slot: &'a AtomicU64, generation: Generation) -> Self {
        Self { slot, generation }
    }

    /// Write data to the entry.
    ///
    /// Scores are stored verbatim. Mate scores are position-relative (see [`Score::from_i16`]), so
    /// no ply adjustment is applied on the way in or out of the table.
    #[inline]
    pub fn write(&self, pos: &Position, score: Score, depth: u8, bound: Bound, mov: &Move) {
        let sig = (pos.zobrist().0 >> 48) as u16;

        let entry = Entry {
            sig,
            depth,
            gen_bound: GenBound::new(self.generation, bound),
            score,
            mov: PackedMove::from_move(mov),
        };
        self.slot.store(entry.pack(), Ordering::Relaxed);
    }

    /// Read a consistent snapshot of the current entry.
    #[inline(always)]
    pub fn read(&self) -> Entry {
        let entry = Entry::unpack(self.slot.load(Ordering::Relaxed));
        if entry.gen() == self.generation.0 {
            entry
        } else {
            Entry::default()
        }
    }
}

/// The transposition table.
///
/// Each entry is stored as one packed atomic word. Relaxed loads and stores retain the intended
/// lockless behavior while ensuring readers never observe a torn or data-racy entry.
pub struct Table {
    /// The storage buffer.
    data: Box<[AtomicU64]>,
    mask: usize,
    generation: AtomicU8,
}

impl std::fmt::Debug for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Table {{ data: Box<[AtomicU64]>; mask: {} }}", self.mask)
    }
}

impl Table {
    /// Create a new transposition table of `size` megabytes.
    ///
    /// The size will be treated as a guide only. The transposition table prioritises efficiency
    /// and as such must use a power-of-2 number of entries. The table will be sized as close as
    /// possible to the desired value, while using a power-of-2 entries.
    ///
    /// Note that we define 1MB = 1_024 * 1_024 bytes.
    pub fn new(size: usize) -> Self {
        let entries = Table::size_from_mb(size);
        let mut v = Vec::with_capacity(entries);
        v.resize_with(entries, || AtomicU64::new(Entry::default().pack()));

        Table {
            data: v.into_boxed_slice(),
            mask: entries - 1,
            generation: AtomicU8::new(Generation::FIRST.0),
        }
    }

    /// Explicitly invalidate every currently live entry by advancing the shared generation.
    ///
    /// This is an owner-level operation for new-game or explicit-clear boundaries. Search workers
    /// must not call it: advancing the generation also invalidates entries used by sibling workers.
    pub fn clear(&self) {
        loop {
            let current = self.current_generation();
            if let Some(next) = current.next() {
                if self
                    .generation
                    .compare_exchange(current.0, next.0, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    return;
                }
            } else {
                // Generation 1 may still be present from the previous cycle. Physically remove
                // every entry before publishing generation 1 again, so it can never become live
                // merely because the six-bit epoch identifier wrapped.
                for slot in &*self.data {
                    slot.store(Entry::default().pack(), Ordering::Relaxed);
                }
                if self
                    .generation
                    .compare_exchange(
                        Generation::LAST.0,
                        Generation::FIRST.0,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return;
                }
            }
        }
    }

    fn current_generation(&self) -> Generation {
        Generation::try_from(self.generation.load(Ordering::Relaxed))
            .expect("table generation invariant violated")
    }

    fn size_from_mb(size: usize) -> usize {
        let desired_entries = size * 1_024 * 1_024 / std::mem::size_of::<Entry>();
        let log_desired_entries = (desired_entries as f64).log(2.).round();

        2_usize.pow(log_desired_entries as u32)
    }

    /// Returns the capacity of the transposition table in number of entries.
    #[inline(always)]
    pub fn capacity_entries(&self) -> usize {
        self.mask + 1
    }

    /// Returns the capacity of the transposition table in megabytes.
    #[inline(always)]
    pub fn capacity_mb(&self) -> usize {
        self.capacity_entries() * std::mem::size_of::<Entry>() / 1_024 / 1_024
    }

    /// Returns the idx for a given key. Uses bitwise operation to take the modulus of the key
    /// with the (power-of-2) capacity of the underlying storage.
    #[inline(always)]
    pub fn idx(&self, key: u64) -> usize {
        self.mask & (key as usize)
    }

    /// Probe the table for a given `Position`. If an entry is in the table already, a shared
    /// reference to it is returned. If not, a unique reference to the entry slot is returned,
    /// which can be overwritten once search has produced a result.
    #[inline(always)]
    pub fn probe<'tt>(&'tt self, pos: &Position) -> Probe<'tt> {
        let idx = self.idx(pos.zobrist().0);
        let sig = (pos.zobrist().0 >> 48) as u16;
        let generation = self.current_generation();

        // SAFETY: `idx` is masked by `capacity - 1`, and the capacity is a non-zero power of two.
        let slot = unsafe { self.data.get_unchecked(idx) };
        let entry = Entry::unpack(slot.load(Ordering::Relaxed));
        let writable_entry = WritableEntry::new(slot, generation);

        use Probe::*;
        if entry.gen() != generation.0 {
            Empty(writable_entry)
        } else if sig == entry.sig {
            Hit(writable_entry)
        } else {
            Clash(writable_entry)
        }
    }

    /// Calculate an approximation of the transposition table usage.
    ///
    /// Works by iterating the first 1000 entries and counting how many are empty.
    ///
    /// This is used in info reports to the GUI via UCI, among others.
    pub fn hashfull(&self) -> u16 {
        let mut c = 0;
        let generation = self.current_generation();
        for e in &self.data[0..1000] {
            let entry = Entry::unpack(e.load(Ordering::Relaxed));
            if entry.gen() != generation.0 {
                c += 1;
            }
        }

        1000 - c
    }
}

/// The result of probing the table.
///
/// We can have three outcomes:
/// * A `Hit`. We found the position we wanted (module hash collisions).
/// * A `Clash`. We found a different position sharing the same hash table location. This is
///   returned in case the caller would like to replace it with the result of a more recent search.
/// * `Empty`. The entry was empty, and can be written to.
pub enum Probe<'a> {
    /// Represents finding the exact position we wanted. Note that this may still actually not be a
    /// real hit because of hash collisions (when two distinct positions have equal Zobrist hashes).
    Hit(WritableEntry<'a>),
    /// Represents finding a different position to what we wanted. This happens when two distinct
    /// positions with distinct hashes nevertheless share a slot in the table because the table has
    /// limited size. This can be common. The caller should consider replacement strategy when
    /// deciding whether to write data from a new search into this slot.
    Clash(WritableEntry<'a>),
    /// Represents finding an as-yet-unwritten slot in the table. The caller can safely write new
    /// data to it without checking any replacement conditions.
    Empty(WritableEntry<'a>),
}

impl<'a> Probe<'a> {
    #[inline(always)]
    pub fn into_inner(self) -> WritableEntry<'a> {
        use Probe::*;
        match self {
            Hit(e) => e,
            Clash(e) => e,
            Empty(e) => e,
        }
    }

    #[inline(always)]
    pub fn is_hit(&self) -> bool {
        matches!(self, Probe::Hit(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::position::Position;
    use std::time::Instant;
    use Probe::*;

    #[test]
    fn sizes() {
        assert_eq!(Table::size_from_mb(1), 131072);
        assert_eq!(Table::size_from_mb(2), 262144);
        assert_eq!(Table::size_from_mb(4), 524288);
        assert_eq!(Table::size_from_mb(8), 1048576);
        assert_eq!(Table::size_from_mb(16), 2097152);
        assert_eq!(Table::size_from_mb(32), 4194304);
        assert_eq!(Table::size_from_mb(64), 8388608);
        assert_eq!(Table::size_from_mb(100), 16777216);
        assert_eq!(Table::size_from_mb(128), 16777216);
        assert_eq!(Table::size_from_mb(200), 33554432);
        assert_eq!(Table::size_from_mb(256), 33554432);
        assert_eq!(Table::size_from_mb(300), 33554432);
        assert_eq!(Table::size_from_mb(400), 67108864);
        assert_eq!(Table::size_from_mb(500), 67108864);
        assert_eq!(Table::size_from_mb(512), 67108864);
        assert_eq!(Table::size_from_mb(1000), 134217728);
        assert_eq!(Table::size_from_mb(1024), 134217728);
    }

    #[test]
    fn generations_reject_empty_and_out_of_range_values() {
        assert_eq!(Generation::try_from(0), Err("generation must be in 1..=63"));
        assert_eq!(
            Generation::try_from(64),
            Err("generation must be in 1..=63")
        );
        assert_eq!(
            Generation::try_from(u8::MAX),
            Err("generation must be in 1..=63")
        );
    }

    #[test]
    fn gen_bound_distinguishes_empty_and_live_generations() {
        assert_eq!(GenBound::default().to_raw_parts(), (0, Bound::Exact));

        for raw_generation in [1, 2, 41, 62, 63] {
            let generation = Generation::try_from(raw_generation).unwrap();
            for bound in [Bound::Exact, Bound::Upper, Bound::Lower] {
                assert_eq!(
                    GenBound::new(generation, bound).to_raw_parts(),
                    (raw_generation, bound)
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn make_tt() {
        let size = 1000;
        let start = Instant::now();
        let tt = Table::new(size);
        println!(
            "Took {}ms to initialize {}MB transposition table. Requested size was {}MB",
            start.elapsed().as_millis(),
            tt.capacity_mb(),
            size
        );
        println!("{:?}", tt);
        assert_eq!(tt.capacity_mb(), 1_024);
    }

    #[test]
    fn stores_stuff() {
        core::init::init_globals();

        let size = 1;
        let tt = Table::new(size);

        let pos = Position::start_pos();

        match tt.probe(&pos) {
            Hit(entry) => {
                println!("hit; found entry {:?}", entry);
                println!("reading entry {:?}", entry.read());
                println!("writing entry while i have a shared reference!");
                entry.write(&pos, Score::cp(23), 3, Bound::Upper, &Move::null());
                println!("reading from the _same_ reference {:?}", entry.read());
            }
            Clash(entry) => {
                println!("clash; found entry {:?}", entry);
            }
            Empty(entry) => {
                println!("writing an entry");
                entry.write(&pos, Score::cp(240), 5, Bound::Exact, &Move::null());
            }
        }

        match tt.probe(&pos) {
            Hit(entry) => {
                println!("hit; found entry {:?}", entry);
                println!("reading entry {:?}", entry.read());
                println!("writing entry while i have a shared reference!");
                entry.write(&pos, Score::cp(23), 10, Bound::Lower, &Move::null());
                println!("reading from the _same_ reference {:?}", entry.read());
            }
            Clash(entry) => {
                println!("clash; found entry {:?}", entry);
            }
            Empty(entry) => {
                println!("writing an entry");
                entry.write(&pos, Score::cp(240), 14, Bound::Exact, &Move::null());
            }
        }
    }

    #[test]
    fn packed_entry_round_trips() {
        let entry = Entry {
            sig: 0x9abc,
            depth: 42,
            gen_bound: GenBound::new(Generation::try_from(37).unwrap(), Bound::Lower),
            score: Score::mate(-7),
            mov: PackedMove(0xd321),
        };

        let unpacked = Entry::unpack(entry.pack());
        assert_eq!(unpacked.sig, entry.sig);
        assert_eq!(unpacked.depth, entry.depth);
        assert_eq!(
            unpacked.gen_bound.to_raw_parts(),
            entry.gen_bound.to_raw_parts()
        );
        assert_eq!(unpacked.score, entry.score);
        assert_eq!(unpacked.mov.0, entry.mov.0);
    }

    #[test]
    fn clear_invalidates_previous_generation() {
        core::init::init_globals();

        let table = Table::new(1);
        let pos = Position::start_pos();
        table
            .probe(&pos)
            .into_inner()
            .write(&pos, Score::cp(12), 4, Bound::Exact, &Move::null());
        assert!(table.probe(&pos).is_hit());

        table.clear();
        assert!(!table.probe(&pos).is_hit());
        assert!(table.probe(&pos).into_inner().read().is_empty());
    }

    #[test]
    fn mate_scores_are_stored_position_relative() {
        core::init::init_globals();

        // Mate scores are position-relative: the distance stored for a position is intrinsic to
        // that position and must be returned unchanged however the position is later reached. A
        // transposition that arrives at a different ply therefore preserves the mate distance.
        let table = Table::new(1);
        let pos = Position::start_pos();
        let entry = table.probe(&pos).into_inner();

        entry.write(&pos, Score::mate(7), 8, Bound::Exact, &Move::null());
        assert_eq!(entry.read().score, Score::mate(7));

        entry.write(&pos, Score::mate(-7), 8, Bound::Exact, &Move::null());
        assert_eq!(entry.read().score, Score::mate(-7));

        // Centipawn and infinity scores are likewise stored verbatim.
        entry.write(&pos, Score::cp(42), 8, Bound::Exact, &Move::null());
        assert_eq!(entry.read().score, Score::cp(42));
    }

    #[test]
    fn concurrent_probes_share_the_live_generation() {
        core::init::init_globals();

        let table = std::sync::Arc::new(Table::new(1));
        let pos = Position::start_pos();
        table
            .probe(&pos)
            .into_inner()
            .write(&pos, Score::cp(17), 4, Bound::Exact, &Move::null());

        std::thread::scope(|scope| {
            for _ in 0..8 {
                let table = std::sync::Arc::clone(&table);
                let pos = pos.clone();
                scope.spawn(move || {
                    for _ in 0..1_000 {
                        let entry = table.probe(&pos).into_inner().read();
                        assert_eq!(entry.score, Score::cp(17));
                    }
                });
            }
        });

        assert!(table.probe(&pos).is_hit());
    }

    #[test]
    fn generation_wrap_physically_clears_entries_before_reusing_first() {
        let table = Table::new(1);
        let stale_entry = Entry {
            gen_bound: GenBound::new(Generation::FIRST, Bound::Exact),
            ..Entry::default()
        };
        table.data[0].store(stale_entry.pack(), Ordering::Relaxed);
        table
            .generation
            .store(Generation::LAST.0, Ordering::Relaxed);

        table.clear();

        assert_eq!(table.current_generation(), Generation::FIRST);
        assert_eq!(
            table.data[0].load(Ordering::Relaxed),
            Entry::default().pack()
        );
    }
}
