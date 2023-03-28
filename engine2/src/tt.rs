//! Transposition table.

use super::score::Score;
use core::mov::{Move, MoveType};
use core::position::{PieceType, Position, Square};

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use std::cell::UnsafeCell;
use std::marker::PhantomData;

/// The validity of a stored `Score`.
///
/// Sometimes, we will store exact values in the transposition table. Other times, a node will
/// experience a cutoff but we will still store the lower or upper bound.
#[derive(Debug, PartialEq, FromPrimitive)]
#[repr(u8)]
pub enum Bound {
    Exact = 0,
    Upper,
    Lower,
}

/// A single byte, packing information about the aging generation of an entry, and what bound it
/// represents.
// TODO: make `Generation(u8)` an actual type wrapping `u8`.
#[derive(Clone, Debug, Default)]
pub struct GenBound(u8);

impl GenBound {
    /// Create a `GenerationBound` from its raw parts.
    ///
    /// Since a `Bound` consumes 2 bits, we pack the generation number into the remaining 6 bits -
    /// therefore, a generation can be any number 0-63. If a larger number than 63 is passed, the
    /// remaining bits will be dropped and lost.
    pub fn from_raw_parts(gen: u8, bound: Bound) -> Self {
        debug_assert!(gen < 64);

        let v = (gen << 2) ^ bound as u8;
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
        FromPrimitive::from_u8(self.0 & 0x3).unwrap()
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
#[derive(Clone, Debug)]
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
            let orig = mov.orig().0 as u16;
            let dest = (mov.dest().0 as u16) << 6;
            let promo = match mov.promo_piece_type() {
                Some(p) => ((p as u8 - 1) as u16) << 12,
                None => 0,
            };

            PackedMove(null ^ orig ^ dest ^ promo)
        }
    }

    pub fn to_move(&self, pos: &Position) -> Move {
        let orig = Square((self.0 & ORIG_MASK) as u8);
        let dest = Square(((self.0 & DEST_MASK) >> 6) as u8);
        let promo = ((self.0 & PROMO_MASK) >> 12) as u8;
        let mut move_type = MoveType::empty();
        let promo_piece = if promo == 0 {
            None
        } else {
            move_type |= MoveType::PROMOTION;
            Some(FromPrimitive::from_u8(promo + 1).expect("should never fail"))
        };

        if !pos.piece_at_sq(dest).is_none() {
            move_type |= MoveType::CAPTURE;
        }

        let piece = pos.piece_at_sq(orig);

        match pos.ep_square() {
            Some(ep) => {
                if ep == dest && piece.type_of() == PieceType::Pawn {
                    move_type |= MoveType::EN_PASSANT | MoveType::CAPTURE;
                }
            }
            None => {}
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
#[derive(Clone, Debug, Default)]
#[repr(align(8))]
pub struct Entry {
    pub sig: u16,
    pub depth: u8,
    pub gen_bound: GenBound,
    pub score: Score,
    pub mov: PackedMove,
}

impl Entry {
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
    ptr: *mut Entry,
    _marker: PhantomData<&'a Entry>,
}

impl<'a> WritableEntry<'a> {
    /// Create a `WritableEntry` from a raw pointer to the underlying `Entry`.
    #[inline]
    fn from_raw_ptr(ptr: *mut Entry) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Write data to the entry.
    #[inline]
    pub fn write(&self, pos: &Position, score: Score, depth: u8, bound: Bound, mov: &Move) {
        let sig = (pos.zobrist().0 >> 48) as u16;

        // SAFETY: we know that the `'a` reference will be outlived by the table, so we can never
        // end up writing to a completely unrelated address. However, this may well be racy or
        // break mutability guarantees. All we know is that we are writing into the table, which is
        // enough for our use case.
        unsafe {
            *self.ptr = Entry {
                sig,
                depth,
                gen_bound: GenBound::from_raw_parts(1, bound),
                score,
                mov: PackedMove::from_move(mov),
            }
        }
    }

    /// Get a shared reference to the `Entry` in order to read its current data.
    ///
    /// Contrary to all Rust's usual rules, owning this shared reference _does not_ guarantee that
    /// the data is not being modified. It is possible for a mutable reference to exist at the same
    /// time.
    pub fn read(&self) -> &Entry {
        unsafe { &*self.ptr }
    }
}

/// The transposition table.
///
/// The underlying storage for a `Table` uses `UnsafeCell`. Given our use patterns, we are actually
/// comfortable with this data structure being racy and breaking mutability / pointer aliasing
/// guarantees. In particular, it is theoretically possible for multiple `&mut` pointers to coexist
/// for the same entry in the transposition table. When this happens, it is of course possible for
/// data to get corrupted in an entry, but this should be extremely rare, and will cause less of a
/// performance hit than using more reliable data structures. Data should never be corrupted in
/// such a way as to actually cause a crash - just possibly-incorrect results for the probing code.
pub struct Table {
    /// The storage buffer.
    data: Box<[UnsafeCell<Entry>]>,
    mask: usize,
}

unsafe impl Sync for Table {}

impl std::fmt::Debug for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Table {{ data: Box<[Entry]>; mask: {} }}", self.mask)
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
        v.resize_with(entries, || UnsafeCell::new(Default::default()));

        Table {
            data: v.into_boxed_slice(),
            mask: entries - 1,
        }
    }

    fn size_from_mb(size: usize) -> usize {
        let desired_entries = size * 1_024 * 1_024 / std::mem::size_of::<Entry>();
        let log_desired_entries = (desired_entries as f64).log(2.).round();
        let actual_entries = 2_usize.pow(log_desired_entries as u32);
        actual_entries
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
    pub fn probe<'tt>(&'_ self, pos: &'_ Position) -> Probe<'tt> {
        let idx = self.idx(pos.zobrist().0);
        let sig = (pos.zobrist().0 >> 48) as u16;

        // We don't need to bounds check `idx` because it is guaranteed to be in bounds.
        let entry = unsafe { self.data.get_unchecked(idx).get() };
        let writable_entry = WritableEntry::from_raw_ptr(entry);

        use Probe::*;
        unsafe {
            if (*entry).is_empty() {
                Empty(writable_entry)
            } else if sig == (*entry).sig {
                Hit(writable_entry)
            } else {
                Clash(writable_entry)
            }
        }
    }

    /// Calculate an approximation of the transposition table usage.
    ///
    /// Works by iterating the first 1000 entries and counting how many are empty.
    ///
    /// This is used in info reports to the GUI via UCI, among others.
    pub fn hashfull(&self) -> u16 {
        let mut c = 0;
        for e in &self.data[0..1000] {
            // SAFETY: we are just reading the value. Not emitting a reference to safe code, nor
            // mutating the value.
            let entry = unsafe { &*e.get() };
            if entry.is_empty() {
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
/// returned in case the caller would like to replace it with the result of a more recent search.
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
        use Probe::*;
        match self {
            Hit(_) => true,
            _ => false,
        }
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

    #[rustfmt::skip]
    #[test]
    fn gen_bound() {
        assert_eq!(GenBound::from_raw_parts(62, Bound::Lower).to_raw_parts(), (62, Bound::Lower));
        assert_eq!(GenBound::from_raw_parts(3, Bound::Exact).to_raw_parts(), (3, Bound::Exact));
        assert_eq!(GenBound::from_raw_parts(41, Bound::Upper).to_raw_parts(), (41, Bound::Upper));
        assert_eq!(GenBound::from_raw_parts(2, Bound::Upper).to_raw_parts(), (2, Bound::Upper));
        assert_eq!(GenBound::from_raw_parts(0, Bound::Lower).to_raw_parts(), (0, Bound::Lower));
        assert_eq!(GenBound::from_raw_parts(64, Bound::Lower).to_raw_parts(), (0, Bound::Lower));
        assert_eq!(GenBound::from_raw_parts(63, Bound::Upper).to_raw_parts(), (63, Bound::Upper));
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

        let mut pos = Position::start_pos();

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
}
