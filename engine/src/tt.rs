//! Transposition table.
//!
//! The table is a fixed array of cache-line-sized clusters shared by every search worker through
//! one `Arc`. Nothing in it is owned by a worker: probes, replacement selection, stores and
//! telemetry all go through `&self`, so a Lazy SMP search can hand the same allocation to an
//! arbitrary number of threads without partitioning it or coordinating between them.
//!
//! # Reading and writing
//!
//! There are exactly two hot-path operations:
//!
//! * [`Table::probe`] takes a Zobrist key and returns an owned [`Snapshot`] value, or `None`.
//! * [`Table::store`] takes a Zobrist key and a result, and chooses its own victim slot.
//!
//! They are deliberately not connected. A probe hands back a *value*, not a borrow of the slot it
//! came from, so a concurrent replacement between the probe and the point where the caller consumes
//! the result cannot change what the caller consumes. Replacement selection happens inside `store`,
//! at store time, against the cluster as it is then — never against a slot reserved earlier.
//!
//! # Entry identity
//!
//! A slot holds the full 64-bit key rather than a truncated signature, and the key is what a probe
//! is verified against. See [`Slot`] for the publication and validation protocol, and the module
//! tests for the adverse schedules it is checked against.
//!
//! ## Why the full key, and not a signature
//!
//! Storing a signature is the cheaper option, and it is what the previous table did with 16 bits.
//! It is not good enough here. Take a 1GB table: 16,777,216 clusters, so the index fixes 24 bits of
//! the key and a 16-bit signature verifies 16 more. Forty bits are checked and 24 are not, so two
//! distinct positions landing in the same cluster agree by accident once in 2^16 times. A long
//! search probes on the order of 10^9 times, so at a realistic hit rate that is on the order of
//! 10^4 accepted entries per search belonging to a different position — each one a score, a bound
//! and a depth for a position that is not the one being searched.
//!
//! Engines live with that because a stored move is usually illegal in the wrong position, so most
//! false hits are filtered by legality. That filter is not proof of identity: it says nothing about
//! an entry stored without a move, and a move can be pseudo-legal in both positions. Precisely the
//! entries the search cuts off against — the move-less bounds — are the ones it does not cover.
//!
//! Verifying all 64 bits removes the failure mode instead of filtering it. Accepting another
//! position's entry now requires a genuine Zobrist collision, which is a property of the hash
//! function rather than of the table, and which the engine already has to accept. The cost is
//! entry width: 16 bytes rather than 8, so half as many entries per megabyte. See BENCHMARKS.md
//! for what that trade measures out at.
//!
//! # Target requirements
//!
//! The table requires native lock-free 64-bit atomics. There is no fallback: on a target without
//! them the build fails rather than silently substituting a lock-based `AtomicU64`, which would put
//! a mutex under every probe and store on the search hot path and quietly invalidate the
//! lock-freedom this module documents.

#[cfg(not(target_has_atomic = "64"))]
compile_error!(
    "the transposition table requires native 64-bit atomics; probe and store must be lock-free"
);

use super::score::Score;
use core::mov::{Move, MoveType};
use core::position::{PieceType, Position, Square};

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

/// The validity of a stored `Score`.
///
/// Sometimes, we will store exact values in the transposition table. Other times, a node will
/// experience a cutoff but we will still store the lower or upper bound.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Bound {
    Exact = 0,
    Upper = 1,
    Lower = 2,
}

impl Bound {
    /// Recover a `Bound` from its two packed bits.
    ///
    /// The fourth encoding is unused. It is unreachable through [`Table::store`], which only ever
    /// packs a real `Bound`, but a raw word can carry it, so it maps to `Exact` rather than
    /// panicking: telemetry and replacement must stay total functions of an arbitrary word.
    #[inline(always)]
    fn from_bits(bits: u64) -> Self {
        match bits & 0x3 {
            1 => Bound::Upper,
            2 => Bound::Lower,
            _ => Bound::Exact,
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
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
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

// Packed data-word layout. The word is the second half of a slot; see [`Slot`].
//
//   bit 63      | occupied flag: 1 for a live entry, 0 for an empty slot
//   bits 48..63 | reserved, always zero
//   bits 42..48 | age
//   bits 40..42 | bound
//   bits 32..40 | depth
//   bits 16..32 | score, as the little-endian bit pattern of an i16
//   bits  0..16 | packed move
//
// Two invariants hold for every word ever written by [`Table::store`], and both are asserted in
// `Snapshot::from_data`:
//
// 1. The reserved bits are zero. They are the only spare capacity in the entry, so leaving them
//    provably zero is what lets a future field be added without a migration.
// 2. A live entry has the occupied flag set. That is what makes zeroed memory empty by
//    construction, so allocation and [`Table::clear`] need only write zeroes.
const MOVE_SHIFT: u32 = 0;
const SCORE_SHIFT: u32 = 16;
const DEPTH_SHIFT: u32 = 32;
const BOUND_SHIFT: u32 = 40;
const AGE_SHIFT: u32 = 42;
const RESERVED_MASK: u64 = 0x7FFF << 48;
const OCCUPIED: u64 = 1 << 63;

/// The number of distinct ages. Six bits, wrapping.
const AGE_MODULUS: u8 = 64;
const AGE_MASK: u8 = AGE_MODULUS - 1;

/// An immutable, verified transposition-table hit.
///
/// Every field came from one atomic state of one slot: the key was checked against the same pair of
/// words the move, depth, bound and score were decoded from. The value is owned, so nothing that
/// happens to the table afterwards can change it. That is the whole point of returning a snapshot
/// rather than a handle — a caller that verifies a hit and then consumes it several steps later is
/// consuming exactly what it verified.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    key: u64,
    mov: PackedMove,
    score: Score,
    depth: u8,
    bound: Bound,
    age: u8,
}

impl Snapshot {
    /// Decode a data word that has already been verified against `key`.
    #[inline(always)]
    fn from_data(key: u64, data: u64) -> Self {
        debug_assert_eq!(data & RESERVED_MASK, 0, "reserved entry bits must be zero");
        debug_assert_ne!(data & OCCUPIED, 0, "snapshot decoded from an empty slot");

        Self {
            key,
            mov: PackedMove((data >> MOVE_SHIFT) as u16),
            score: Score::from_i16((data >> SCORE_SHIFT) as u16 as i16),
            depth: (data >> DEPTH_SHIFT) as u8,
            bound: Bound::from_bits(data >> BOUND_SHIFT),
            age: ((data >> AGE_SHIFT) as u8) & AGE_MASK,
        }
    }

    /// The full Zobrist key this entry was verified against.
    #[inline(always)]
    pub fn key(&self) -> u64 {
        self.key
    }

    /// The stored best move, or `None` if the entry was stored without one.
    #[inline(always)]
    pub fn mov(&self) -> Option<PackedMove> {
        (!self.mov.is_null()).then_some(self.mov)
    }

    /// The stored score.
    ///
    /// Mate scores are position-relative (see [`Score::from_i16`]), so no ply adjustment is applied
    /// on the way into or out of the table.
    #[inline(always)]
    pub fn score(&self) -> Score {
        self.score
    }

    /// The depth this entry was searched to.
    #[inline(always)]
    pub fn depth(&self) -> u8 {
        self.depth
    }

    /// Whether the score is exact, or an upper or lower bound.
    #[inline(always)]
    pub fn bound(&self) -> Bound {
        self.bound
    }

    /// The table age current when this entry was stored.
    #[inline(always)]
    pub fn age(&self) -> u8 {
        self.age
    }
}

/// One transposition-table slot: a full key and its data, published as two atomic words.
///
/// # Publication and validation
///
/// An entry does not fit in one atomic word, so it is split across two. Rather than guard the pair
/// with a lock or a sequence counter, the key word stores `key ^ data`. A reader loads both words
/// and accepts the slot only when `key_xor_data ^ data` equals the key it is looking for.
///
/// That check is what makes a torn pair unobservable. If a reader loads `key_a ^ data_a` from one
/// write and `data_b` from another, it computes `key_a ^ data_a ^ data_b`, which equals the probe
/// key only if the probe key happens to be exactly that — a 64-bit coincidence, of the same order
/// as a genuine Zobrist collision, and far below the rate at which the search's own hash collisions
/// occur. A hybrid entry therefore cannot be consumed, and a verified snapshot's key, move, depth,
/// bound and score necessarily all came from one write.
///
/// Both words are written and read `Relaxed`, and the writer publishes `data` before
/// `key_xor_data`. The ordering is not load-bearing: the entry is self-contained, so a store
/// publishes no other memory that a reader must see, and the validation is a property of the two
/// values rather than of the order they were observed in. Reordering by the compiler or the CPU
/// changes only *which* pairing a reader observes, and every pairing is validated. Relaxed is
/// therefore both sufficient and the cheapest option on the hot path.
///
/// Reads and writes are bounded and unconditional: there is no compare-exchange, no retry loop, and
/// no blocking, so a probe or store costs a fixed number of instructions however contended the slot
/// is. A losing writer loses its entry, never its progress.
#[derive(Debug, Default)]
struct Slot {
    key_xor_data: AtomicU64,
    data: AtomicU64,
}

impl Slot {
    /// Load both words and decode the entry, without checking any key.
    ///
    /// Used by replacement and telemetry, which care about what a slot currently holds rather than
    /// about a particular position. The pair may be torn, which for those callers is harmless: a
    /// torn read can only misjudge a victim's quality or an occupancy estimate.
    #[inline(always)]
    fn load(&self) -> (u64, u64) {
        let key_xor_data = self.key_xor_data.load(Ordering::Relaxed);
        let data = self.data.load(Ordering::Relaxed);
        (key_xor_data, data)
    }

    /// Return the snapshot held here if this slot currently holds a live entry for `key`.
    #[inline(always)]
    fn probe(&self, key: u64) -> Option<Snapshot> {
        let (key_xor_data, data) = self.load();
        ((key_xor_data ^ data) == key && (data & OCCUPIED) != 0)
            .then(|| Snapshot::from_data(key, data))
    }

    /// Publish `key` and `data` into this slot.
    #[inline(always)]
    fn store(&self, key: u64, data: u64) {
        debug_assert_ne!(data & OCCUPIED, 0, "stored entry must be marked occupied");
        debug_assert_eq!(data & RESERVED_MASK, 0, "reserved entry bits must be zero");

        self.data.store(data, Ordering::Relaxed);
        self.key_xor_data.store(key ^ data, Ordering::Relaxed);
    }
}

/// The number of slots in a cluster.
///
/// Four 16-byte slots fill a 64-byte cache line exactly. A probe therefore examines four candidate
/// entries while touching one line, and the alignment guarantees no cluster straddles two lines, so
/// neither a probe nor a store can pull in a neighbouring cluster's traffic.
pub const CLUSTER_SLOTS: usize = 4;

/// A cache-line-aligned group of candidate slots sharing one index.
#[repr(C, align(64))]
#[derive(Debug, Default)]
struct Cluster {
    slots: [Slot; CLUSTER_SLOTS],
}

/// How strongly a bound that is exact is preferred over one that is not, in depth-equivalent units.
const EXACT_BONUS: i32 = 4;
/// How strongly each step of relative age counts against an entry, in depth-equivalent units.
const AGE_PENALTY: i32 = 8;

/// The transposition table.
///
/// One allocation, shared by every worker through `Arc<Table>` and used entirely through `&self` on
/// the search hot path. `Table` is `Send + Sync` by construction: its storage is `AtomicU64` words
/// and its age is an `AtomicU8`, so the compiler derives both without an `unsafe impl`.
pub struct Table {
    clusters: Box<[Cluster]>,
    /// `clusters.len() - 1`. The cluster count is a non-zero power of two, so this masks a key down
    /// to a cluster index.
    mask: usize,
    /// The age stamped onto entries stored from now on. Six bits, wrapping.
    age: AtomicU8,
}

impl std::fmt::Debug for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Table")
            .field("clusters", &self.clusters.len())
            .field("entries", &self.capacity_entries())
            .field("bytes", &self.capacity_bytes())
            .field("age", &self.current_age())
            .finish()
    }
}

impl Table {
    /// Create a new transposition table of at most `size` megabytes.
    ///
    /// The table uses a power-of-two number of clusters, so the requested size is a *ceiling* that
    /// is rounded down to: a request the allocation cannot match exactly yields the largest table
    /// that fits inside it, never a larger one. Requesting 100MB gives 64MB, not 128MB.
    ///
    /// A request too small for a single cluster still yields one cluster, which is the smallest
    /// table the index arithmetic admits. Note that we define 1MB = 1_024 * 1_024 bytes.
    pub fn new(size: usize) -> Self {
        let clusters = Self::clusters_from_mb(size);
        let mut v = Vec::new();
        v.resize_with(clusters, Cluster::default);

        Table {
            clusters: v.into_boxed_slice(),
            mask: clusters - 1,
            age: AtomicU8::new(0),
        }
    }

    /// The number of clusters to allocate for a request of `size` megabytes.
    ///
    /// Saturating arithmetic keeps a nonsensical request from wrapping into a small allocation: a
    /// `usize::MAX` megabyte request saturates, and the resulting cluster count is capped at the
    /// largest power of two that can be addressed. The caller learns what it actually got from
    /// [`Table::capacity_mb`].
    fn clusters_from_mb(size: usize) -> usize {
        let bytes = size.saturating_mul(1_024 * 1_024);
        let clusters = bytes / std::mem::size_of::<Cluster>();
        if clusters <= 1 {
            // Zero bytes, or less than one cluster's worth: one cluster is the floor, since the
            // index mask needs a non-zero power-of-two cluster count.
            1
        } else {
            // Round *down* to a power of two, so the allocation never exceeds the request.
            1_usize << (usize::BITS - 1 - clusters.leading_zeros())
        }
    }

    /// The number of clusters in the table.
    #[inline(always)]
    pub fn capacity_clusters(&self) -> usize {
        self.mask + 1
    }

    /// The capacity of the transposition table in number of entries.
    #[inline(always)]
    pub fn capacity_entries(&self) -> usize {
        self.capacity_clusters() * CLUSTER_SLOTS
    }

    /// The size of the allocation in bytes.
    #[inline(always)]
    pub fn capacity_bytes(&self) -> usize {
        self.capacity_clusters() * std::mem::size_of::<Cluster>()
    }

    /// The size of the allocation in whole megabytes, rounded down.
    #[inline(always)]
    pub fn capacity_mb(&self) -> usize {
        self.capacity_bytes() / 1_024 / 1_024
    }

    /// The cluster index for a given key.
    ///
    /// Any bits of the key would serve, because the full key is stored and verified; the low bits
    /// are used because masking them is a single instruction.
    #[inline(always)]
    pub fn cluster_index(&self, key: u64) -> usize {
        self.mask & (key as usize)
    }

    #[inline(always)]
    fn cluster(&self, key: u64) -> &Cluster {
        let index = self.cluster_index(key);
        // SAFETY: `cluster_index` masks with `capacity_clusters() - 1`, and the cluster count is a
        // non-zero power of two, so the result is always in bounds.
        unsafe { self.clusters.get_unchecked(index) }
    }

    /// The age currently stamped onto new entries.
    #[inline(always)]
    pub fn current_age(&self) -> u8 {
        self.age.load(Ordering::Relaxed) & AGE_MASK
    }

    /// Advance the age used to prioritise replacement, at the start of a root search.
    ///
    /// Unlike [`Table::clear`] this takes `&self`, and safely so: age never decides whether an
    /// entry is *valid*, only how attractive it is as a victim. A worker that advanced it, or that
    /// observed a stale value, would at worst perturb replacement quality. The six-bit counter
    /// wraps, which makes an entry 64 searches old look current again; that too costs only
    /// replacement quality, and the alternative — treating a wrap as invalidation — would discard
    /// live results a long game still needs.
    pub fn advance_age(&self) {
        self.age.fetch_add(1, Ordering::Relaxed);
    }

    /// Discard every entry in the table.
    ///
    /// This is the administrative new-game boundary, and taking `&mut self` is what enforces it.
    /// The table is reached through an `Arc` shared with every worker, so an exclusive reference
    /// can only be obtained (via `Arc::get_mut`) once no worker holds a clone — that is, once every
    /// search that could be relying on the current contents has finished. A worker cannot call this
    /// even by mistake, because a worker only ever holds `&Table`.
    ///
    /// Clearing is physical rather than a generation bump. That makes invalidation exact — no entry
    /// survives to be revived by a counter wrapping — and it costs one linear pass over an
    /// allocation that has just been declared worthless. Exclusive access also means the pass can
    /// write plain zeroes with no atomic traffic.
    pub fn clear(&mut self) {
        for cluster in self.clusters.iter_mut() {
            for slot in cluster.slots.iter_mut() {
                *slot.key_xor_data.get_mut() = 0;
                *slot.data.get_mut() = 0;
            }
        }
        *self.age.get_mut() = 0;
    }

    /// Look for a live entry for `key`.
    ///
    /// Returns an owned [`Snapshot`] whose fields all came from one atomic state of one slot, or
    /// `None` if the cluster holds no verified entry for this key. The result borrows nothing, so a
    /// replacement that lands between this call and the caller consuming the result cannot alter
    /// what the caller consumes.
    ///
    /// Bounded and lock-free: at most [`CLUSTER_SLOTS`] pairs of relaxed loads within a single
    /// cache line, with no retry.
    #[inline]
    pub fn probe(&self, key: u64) -> Option<Snapshot> {
        let cluster = self.cluster(key);
        for slot in &cluster.slots {
            if let Some(snapshot) = slot.probe(key) {
                return Some(snapshot);
            }
        }
        None
    }

    /// Store a search result, choosing the slot to replace.
    ///
    /// Replacement is decided here, against the cluster as it is now, and never against a slot
    /// chosen at probe time. Two cases are distinguished:
    ///
    /// * **Same-key update.** If the cluster already holds this key, that slot is the target. The
    ///   entry is refreshed unless the existing one is strictly more valuable — deeper, stamped
    ///   with the current age, and not being upgraded to an exact bound — because re-searching a
    ///   position to a shallower depth is not a reason to throw away a deeper result. A store
    ///   without a move keeps the move already recorded, which is otherwise a common way for a
    ///   fail-low re-search to erase a usable ordering hint.
    /// * **Clash.** Otherwise a victim is chosen from the whole cluster by quality: shallower is
    ///   more replaceable, an exact bound is worth [`EXACT_BONUS`] plies of depth, and each step of
    ///   relative age costs [`AGE_PENALTY`] plies. Empty slots are always taken first. So a shallow
    ///   or weak entry cannot evict a deeper exact result while any weaker candidate exists.
    ///
    /// No slot is ever reserved for a particular writer and nothing about the store depends on
    /// which worker is calling, so every worker's entries are visible to and replaceable by every
    /// other worker. Concurrent stores to one slot can lose each other's entries — the table may
    /// forget, which costs only a re-search — but a reader can never observe an entry that was
    /// never written, because acceptance requires the full key to match the data it is paired with.
    ///
    /// Bounded and lock-free: one cluster scan and two relaxed stores, with no compare-exchange and
    /// no retry.
    #[inline]
    pub fn store(&self, key: u64, score: Score, depth: u8, bound: Bound, mov: &Move) {
        let cluster = self.cluster(key);
        let age = self.current_age();
        let mut packed = PackedMove::from_move(mov);

        let mut victim = 0;
        let mut victim_quality = i32::MAX;

        for (index, slot) in cluster.slots.iter().enumerate() {
            let (key_xor_data, data) = slot.load();
            let occupied = (data & OCCUPIED) != 0;

            if occupied && (key_xor_data ^ data) == key {
                let existing = Snapshot::from_data(key, data);

                if existing.depth > depth
                    && existing.age == age
                    && !(bound == Bound::Exact && existing.bound != Bound::Exact)
                {
                    return;
                }

                if packed.is_null() {
                    packed = existing.mov;
                }

                slot.store(key, pack(packed, score, depth, bound, age));
                return;
            }

            let quality = if occupied {
                let existing = Snapshot::from_data(key_xor_data ^ data, data);
                existing.depth as i32
                    + if existing.bound == Bound::Exact {
                        EXACT_BONUS
                    } else {
                        0
                    }
                    - AGE_PENALTY * relative_age(age, existing.age) as i32
            } else {
                i32::MIN
            };

            if quality < victim_quality {
                victim = index;
                victim_quality = quality;
            }
        }

        cluster.slots[victim].store(key, pack(packed, score, depth, bound, age));
    }

    /// An estimate of table occupancy, in per mille, for UCI `hashfull` reporting.
    ///
    /// The sample is spread across the whole allocation on a stride rather than taken from one
    /// contiguous run at the start. A contiguous prefix is unrepresentative — the first clusters of
    /// a large table are hit by whatever the search touched first — and, more bluntly, a fixed
    /// 1000-entry prefix cannot be read at all from a table with fewer than 1000 entries. Striding
    /// makes the estimate meaningful and total for every supported capacity, down to one cluster.
    ///
    /// Slots are read without key verification, so a concurrent store can be counted torn. That
    /// only perturbs an estimate that is already a sample.
    pub fn hashfull(&self) -> u16 {
        const TARGET_CLUSTERS: usize = 250;

        let clusters = self.capacity_clusters();
        let sampled = TARGET_CLUSTERS.min(clusters);
        // A power-of-two stride over a power-of-two cluster count visits `sampled` distinct
        // clusters spread evenly across the table.
        let stride = clusters / sampled;

        let mut occupied = 0_usize;
        for i in 0..sampled {
            for slot in &self.clusters[i * stride].slots {
                if (slot.data.load(Ordering::Relaxed) & OCCUPIED) != 0 {
                    occupied += 1;
                }
            }
        }

        ((occupied * 1000) / (sampled * CLUSTER_SLOTS)) as u16
    }
}

/// Build a data word. The inverse of [`Snapshot::from_data`].
#[inline(always)]
fn pack(mov: PackedMove, score: Score, depth: u8, bound: Bound, age: u8) -> u64 {
    OCCUPIED
        | ((mov.0 as u64) << MOVE_SHIFT)
        | (((score.to_i16() as u16) as u64) << SCORE_SHIFT)
        | ((depth as u64) << DEPTH_SHIFT)
        | ((bound as u64) << BOUND_SHIFT)
        | (((age & AGE_MASK) as u64) << AGE_SHIFT)
}

/// How many ages ago `entry` was stored, relative to `current`.
///
/// Wrapping subtraction in the six-bit age space, so an age that has wrapped past `current` reads
/// as very old rather than as negative.
#[inline(always)]
fn relative_age(current: u8, entry: u8) -> u8 {
    (current.wrapping_add(AGE_MODULUS).wrapping_sub(entry)) & AGE_MASK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    /// A key with the same cluster index as `key` in a table with `mask`, differing in every bit
    /// the index does not constrain.
    fn sibling_key(key: u64, mask: usize) -> u64 {
        key ^ !(mask as u64)
    }

    fn store(table: &Table, key: u64, score: i16, depth: u8, bound: Bound) {
        table.store(key, Score::cp(score), depth, bound, &Move::null());
    }

    #[test]
    fn cluster_is_one_cache_line_and_slots_fill_it() {
        assert_eq!(std::mem::size_of::<Cluster>(), 64);
        assert_eq!(std::mem::align_of::<Cluster>(), 64);
        assert_eq!(std::mem::size_of::<Slot>(), 16);
        assert_eq!(std::mem::size_of::<Slot>() * CLUSTER_SLOTS, 64);
    }

    #[test]
    fn clusters_are_cache_line_aligned_in_the_allocation() {
        let table = Table::new(1);
        for cluster in table.clusters.iter() {
            assert_eq!(cluster as *const Cluster as usize % 64, 0);
        }
    }

    #[test]
    fn table_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Table>();
        assert_send_sync::<Arc<Table>>();
        assert_send_sync::<Snapshot>();
    }

    #[test]
    fn sizing_rounds_down_and_never_exceeds_the_request() {
        for mb in [1_usize, 2, 4, 7, 16, 100, 128, 300, 512, 1000, 1024] {
            let clusters = Table::clusters_from_mb(mb);
            assert!(
                clusters.is_power_of_two(),
                "{mb}MB gave {clusters} clusters"
            );
            assert!(
                clusters * std::mem::size_of::<Cluster>() <= mb * 1_024 * 1_024,
                "{mb}MB request allocated more than it asked for"
            );
            // And it is the *largest* such power of two.
            assert!(clusters * 2 * std::mem::size_of::<Cluster>() > mb * 1_024 * 1_024);
        }

        // Explicit boundaries: 1MB is 16,384 clusters of 64 bytes, and rounding down means a
        // request between two powers of two gets the lower one.
        assert_eq!(Table::clusters_from_mb(1), 16_384);
        assert_eq!(Table::clusters_from_mb(100), 1_048_576);
        assert_eq!(Table::clusters_from_mb(128), 2_097_152);
    }

    #[test]
    fn sizing_boundaries_degrade_to_one_cluster_and_saturate() {
        assert_eq!(Table::clusters_from_mb(0), 1);
        assert_eq!(Table::new(0).capacity_clusters(), 1);
        assert_eq!(Table::new(0).capacity_entries(), CLUSTER_SLOTS);
        assert_eq!(Table::new(0).capacity_mb(), 0);

        // A nonsensical request saturates rather than wrapping into a tiny allocation.
        let huge = Table::clusters_from_mb(usize::MAX);
        assert!(huge.is_power_of_two());
        assert!(huge > Table::clusters_from_mb(1024));
    }

    #[test]
    fn capacity_reports_agree() {
        let table = Table::new(4);
        assert_eq!(table.capacity_clusters(), 65_536);
        assert_eq!(table.capacity_entries(), 65_536 * CLUSTER_SLOTS);
        assert_eq!(table.capacity_bytes(), 4 * 1_024 * 1_024);
        assert_eq!(table.capacity_mb(), 4);
    }

    #[test]
    fn round_trips_every_packed_field() {
        core::init::init_globals();

        let table = Table::new(1);
        let key = 0x0123_4567_89ab_cdef;
        let mov = Move::build(
            Square::E7,
            Square::E8,
            Some(PieceType::Knight),
            MoveType::PROMOTION,
        );

        table.store(key, Score::mate(-7), 250, Bound::Lower, &mov);

        let snapshot = table.probe(key).expect("stored entry should be found");
        assert_eq!(snapshot.key(), key);
        assert_eq!(snapshot.score(), Score::mate(-7));
        assert_eq!(snapshot.depth(), 250);
        assert_eq!(snapshot.bound(), Bound::Lower);
        assert_eq!(snapshot.age(), 0);
        assert_eq!(snapshot.mov(), Some(PackedMove::from_move(&mov)));
    }

    #[test]
    fn stores_mate_scores_position_relative() {
        let table = Table::new(1);

        for score in [
            Score::mate(7),
            Score::mate(-7),
            Score::cp(42),
            Score::zero(),
        ] {
            table.store(1, score, 8, Bound::Exact, &Move::null());
            assert_eq!(table.probe(1).unwrap().score(), score);
        }
    }

    #[test]
    fn reserved_bits_stay_zero_and_empty_slots_are_all_zero() {
        let table = Table::new(1);
        store(&table, 7, 100, 9, Bound::Upper);

        let (key_xor_data, data) = table.clusters[table.cluster_index(7)].slots[0].load();
        assert_eq!(data & RESERVED_MASK, 0);
        assert_ne!(data & OCCUPIED, 0);
        assert_eq!(key_xor_data ^ data, 7);

        // Untouched slots are exactly zero, which is what makes zeroed memory empty.
        for slot in &table.clusters[table.cluster_index(7)].slots[1..] {
            assert_eq!(slot.load(), (0, 0));
        }
    }

    #[test]
    fn a_move_less_entry_is_stored_and_found() {
        let table = Table::new(1);
        store(&table, 99, 12, 3, Bound::Upper);

        let snapshot = table.probe(99).unwrap();
        assert_eq!(snapshot.mov(), None);
        assert_eq!(snapshot.score(), Score::cp(12));
    }

    #[test]
    fn a_key_of_zero_is_not_confused_with_an_empty_slot() {
        let table = Table::new(1);

        // Every slot is zeroed, and an empty slot's `key_xor_data ^ data` is also zero. The
        // occupied flag is what keeps that from reading as a hit for key zero.
        assert_eq!(table.probe(0), None);

        store(&table, 0, 55, 4, Bound::Exact);
        assert_eq!(table.probe(0).unwrap().score(), Score::cp(55));
    }

    #[test]
    fn a_different_key_sharing_a_cluster_index_is_never_accepted() {
        let table = Table::new(1);
        let key = 0xdead_beef_1234_5678;
        let other = sibling_key(key, table.mask);

        assert_ne!(key, other);
        assert_eq!(table.cluster_index(key), table.cluster_index(other));

        store(&table, key, 321, 11, Bound::Exact);

        assert_eq!(table.probe(key).unwrap().score(), Score::cp(321));
        assert_eq!(
            table.probe(other),
            None,
            "a key sharing only the cluster index was accepted"
        );
    }

    /// The full key is stored, so there is no separate signature that a distinct key could share.
    /// This enumerates every single-bit variation of a key, all of which a truncated signature
    /// scheme would have to distinguish by luck, and confirms none is accepted.
    #[test]
    fn no_single_bit_key_variation_is_accepted() {
        let table = Table::new(1);
        let key = 0x5555_aaaa_3333_cccc;
        store(&table, key, 7, 5, Bound::Exact);

        for bit in 0..64 {
            let variant = key ^ (1 << bit);
            assert_eq!(
                table.probe(variant),
                None,
                "key differing in bit {bit} was accepted"
            );
        }
    }

    #[test]
    fn a_cluster_holds_several_distinct_keys_at_once() {
        let table = Table::new(1);
        let index = 0x2a_u64;
        let keys: Vec<u64> = (0..CLUSTER_SLOTS as u64)
            .map(|i| (i << 40) | (index & table.mask as u64))
            .collect();

        for (i, &key) in keys.iter().enumerate() {
            assert_eq!(table.cluster_index(key), table.cluster_index(keys[0]));
            store(&table, key, 100 + i as i16, 5, Bound::Exact);
        }

        for (i, &key) in keys.iter().enumerate() {
            assert_eq!(
                table.probe(key).map(|s| s.score()),
                Some(Score::cp(100 + i as i16)),
                "slot {i} was evicted while the cluster still had room"
            );
        }
    }

    #[test]
    fn empty_slots_are_filled_before_anything_is_evicted() {
        let table = Table::new(1);
        let base = 0x11_u64;
        for i in 0..CLUSTER_SLOTS as u64 {
            store(&table, (i << 40) | base, 1, 200, Bound::Exact);
        }

        let occupied = table.clusters[table.cluster_index(base)]
            .slots
            .iter()
            .filter(|s| s.load().1 & OCCUPIED != 0)
            .count();
        assert_eq!(occupied, CLUSTER_SLOTS);
    }

    #[test]
    fn a_shallow_entry_does_not_evict_a_deeper_exact_one() {
        let table = Table::new(1);
        let base = 0x33_u64;
        let deep = base;
        let shallow: Vec<u64> = (1..CLUSTER_SLOTS as u64)
            .map(|i| (i << 40) | base)
            .collect();

        store(&table, deep, 500, 30, Bound::Exact);
        for &key in &shallow {
            store(&table, key, 1, 2, Bound::Upper);
        }

        // The cluster is now full: one deep exact entry and three shallow upper bounds. A new
        // shallow entry must take one of the weak slots.
        let newcomer = (9_u64 << 40) | base;
        store(&table, newcomer, 2, 3, Bound::Upper);

        assert!(
            table.probe(deep).is_some(),
            "the deeper exact entry was evicted by a shallow one"
        );
        assert!(table.probe(newcomer).is_some());
    }

    #[test]
    fn an_exact_bound_outranks_an_equally_deep_inexact_one() {
        let table = Table::new(1);
        let base = 0x44_u64;

        // Two candidates at the same depth, one exact, plus two much deeper fillers.
        store(&table, base, 10, 6, Bound::Exact);
        store(&table, (1_u64 << 40) | base, 10, 6, Bound::Upper);
        store(&table, (2_u64 << 40) | base, 10, 40, Bound::Exact);
        store(&table, (3_u64 << 40) | base, 10, 40, Bound::Exact);

        store(&table, (9_u64 << 40) | base, 10, 6, Bound::Upper);

        assert!(
            table.probe(base).is_some(),
            "the exact entry was evicted ahead of an equally deep inexact one"
        );
        assert_eq!(table.probe((1_u64 << 40) | base), None);
    }

    #[test]
    fn an_older_entry_is_evicted_before_a_deeper_current_one() {
        let table = Table::new(1);
        let base = 0x55_u64;

        // A deep entry from a much earlier search, then three current shallow ones.
        store(&table, base, 10, 40, Bound::Exact);
        for _ in 0..5 {
            table.advance_age();
        }
        for i in 1..CLUSTER_SLOTS as u64 {
            store(&table, (i << 40) | base, 10, 12, Bound::Exact);
        }

        store(&table, (9_u64 << 40) | base, 10, 12, Bound::Exact);

        assert_eq!(
            table.probe(base),
            None,
            "a five-searches-old entry survived ahead of current ones"
        );
    }

    #[test]
    fn a_same_key_store_updates_in_place_rather_than_consuming_a_slot() {
        let table = Table::new(1);
        let key = 0x77_u64;

        store(&table, key, 10, 5, Bound::Upper);
        store(&table, key, 20, 9, Bound::Exact);

        let snapshot = table.probe(key).unwrap();
        assert_eq!(snapshot.score(), Score::cp(20));
        assert_eq!(snapshot.depth(), 9);
        assert_eq!(snapshot.bound(), Bound::Exact);

        let occupied = table.clusters[table.cluster_index(key)]
            .slots
            .iter()
            .filter(|s| s.load().1 & OCCUPIED != 0)
            .count();
        assert_eq!(occupied, 1, "a same-key update consumed a second slot");
    }

    #[test]
    fn a_shallower_same_key_store_does_not_erase_a_deeper_result() {
        let table = Table::new(1);
        let key = 0x88_u64;

        store(&table, key, 400, 20, Bound::Exact);
        store(&table, key, 1, 4, Bound::Upper);

        let snapshot = table.probe(key).unwrap();
        assert_eq!(snapshot.depth(), 20);
        assert_eq!(snapshot.score(), Score::cp(400));
    }

    #[test]
    fn a_shallower_same_key_store_still_lands_when_it_upgrades_the_bound() {
        let table = Table::new(1);
        let key = 0x89_u64;

        store(&table, key, 400, 20, Bound::Upper);
        store(&table, key, 1, 4, Bound::Exact);

        let snapshot = table.probe(key).unwrap();
        assert_eq!(snapshot.depth(), 4);
        assert_eq!(snapshot.bound(), Bound::Exact);
    }

    #[test]
    fn a_stale_deeper_same_key_entry_is_refreshed_by_a_new_search() {
        let table = Table::new(1);
        let key = 0x8a_u64;

        store(&table, key, 400, 20, Bound::Exact);
        table.advance_age();
        store(&table, key, 1, 4, Bound::Exact);

        let snapshot = table.probe(key).unwrap();
        assert_eq!(
            snapshot.depth(),
            4,
            "a stale entry blocked the current search"
        );
        assert_eq!(snapshot.age(), 1);
    }

    #[test]
    fn a_move_less_update_keeps_the_move_already_recorded() {
        core::init::init_globals();

        let table = Table::new(1);
        let key = 0x8b_u64;
        let mov = Move::build(Square::G1, Square::F3, None, MoveType::QUIET);

        table.store(key, Score::cp(10), 5, Bound::Exact, &mov);
        table.store(key, Score::cp(-10), 6, Bound::Upper, &Move::null());

        let snapshot = table.probe(key).unwrap();
        assert_eq!(snapshot.depth(), 6);
        assert_eq!(
            snapshot.mov(),
            Some(PackedMove::from_move(&mov)),
            "a move-less re-search erased a usable ordering hint"
        );
    }

    /// AC#2/AC#5. The probe result is a value, so replacement between the probe and the point where
    /// the caller uses the result cannot change what the caller uses. This drives the adverse
    /// schedule deterministically rather than hoping to hit it with threads.
    #[test]
    fn a_replacement_between_probe_and_consumption_cannot_change_the_snapshot() {
        let table = Table::new(1);
        let key = 0x99_u64;

        store(&table, key, 123, 7, Bound::Exact);
        let snapshot = table.probe(key).unwrap();

        // Overwrite the entire cluster with other keys, which is the worst case: the slot the
        // snapshot came from now holds another position's data.
        for i in 1..=CLUSTER_SLOTS as u64 {
            let other = sibling_key(key, table.mask) ^ (i << 20);
            assert_eq!(table.cluster_index(other), table.cluster_index(key));
            store(&table, other, 456, 30, Bound::Lower);
        }
        assert_eq!(table.probe(key), None, "the entry was not actually evicted");

        assert_eq!(snapshot.key(), key);
        assert_eq!(snapshot.score(), Score::cp(123));
        assert_eq!(snapshot.depth(), 7);
        assert_eq!(snapshot.bound(), Bound::Exact);
    }

    /// AC#13. A reader that observes one write's key word paired with another write's data word
    /// must reject the pair. Both halves of the tear are constructed by hand so the schedule is
    /// exact rather than incidental.
    #[test]
    fn a_hand_constructed_torn_pair_is_rejected() {
        let table = Table::new(1);
        let key_a = 0xaaaa_0000_0000_0010;
        let key_b = 0xbbbb_0000_0000_0010;
        assert_eq!(table.cluster_index(key_a), table.cluster_index(key_b));

        store(&table, key_a, 10, 5, Bound::Exact);
        let (key_word_a, data_a) = table.clusters[table.cluster_index(key_a)].slots[0].load();

        store(&table, key_b, 20, 6, Bound::Lower);
        // Force the tear: slot 0 keeps A's key word but is given B's data word.
        let data_b = table.clusters[table.cluster_index(key_b)].slots[1]
            .data
            .load(Ordering::Relaxed);
        assert_ne!(data_a, data_b);
        let slot = &table.clusters[table.cluster_index(key_a)].slots[0];
        slot.key_xor_data.store(key_word_a, Ordering::Relaxed);
        slot.data.store(data_b, Ordering::Relaxed);

        assert_eq!(
            slot.probe(key_a),
            None,
            "a hybrid of two writes was accepted for the first key"
        );
        assert_eq!(
            slot.probe(key_b),
            None,
            "a hybrid of two writes was accepted for the second key"
        );
    }

    #[test]
    fn clear_discards_every_entry_and_resets_the_age() {
        let mut table = Table::new(1);
        for i in 0..20_000_u64 {
            store(
                &table,
                i.wrapping_mul(0x9e37_79b9_7f4a_7c15),
                5,
                5,
                Bound::Exact,
            );
        }
        table.advance_age();
        assert!(table.hashfull() > 0);

        table.clear();

        assert_eq!(table.hashfull(), 0);
        assert_eq!(table.current_age(), 0);
        for i in 0..20_000_u64 {
            assert_eq!(table.probe(i.wrapping_mul(0x9e37_79b9_7f4a_7c15)), None);
        }
    }

    #[test]
    fn age_wraps_without_invalidating_entries() {
        let table = Table::new(1);
        let key = 0xabc_u64;
        store(&table, key, 42, 9, Bound::Exact);

        for _ in 0..AGE_MODULUS {
            table.advance_age();
        }

        assert_eq!(table.current_age(), 0, "age is a six-bit wrapping counter");
        let snapshot = table
            .probe(key)
            .expect("a wrapped age must not invalidate an entry");
        assert_eq!(snapshot.score(), Score::cp(42));
        assert_eq!(snapshot.age(), 0);
    }

    #[test]
    fn relative_age_is_wrapping_and_never_negative() {
        assert_eq!(relative_age(0, 0), 0);
        assert_eq!(relative_age(5, 3), 2);
        // The entry was stored just before the counter wrapped past `current`.
        assert_eq!(relative_age(0, 63), 1);
        assert_eq!(relative_age(2, 60), 6);
        // Sixty-four searches on, an entry looks current again. That is the documented cost of a
        // six-bit counter, and it only ever misprices a victim.
        assert_eq!(relative_age(10, 10), 0);
    }

    #[test]
    fn hashfull_is_total_for_every_supported_capacity() {
        for mb in [0_usize, 1, 2, 8] {
            let table = Table::new(mb);
            assert_eq!(table.hashfull(), 0, "an empty {mb}MB table is not empty");
        }

        // The smallest supported table is one cluster, far below the 1000 entries a contiguous
        // prefix sample would need.
        let table = Table::new(0);
        assert_eq!(table.capacity_entries(), CLUSTER_SLOTS);
        for i in 0..CLUSTER_SLOTS as u64 {
            store(&table, (i << 40) | 1, 1, 1, Bound::Exact);
        }
        assert_eq!(table.hashfull(), 1000);
    }

    #[test]
    fn hashfull_samples_the_whole_table_not_a_prefix() {
        let table = Table::new(2);
        let clusters = table.capacity_clusters();

        // Fill only the first eighth of the table. A prefix sample would report it as full.
        for cluster in 0..clusters / 8 {
            for slot in 0..CLUSTER_SLOTS as u64 {
                store(&table, (slot << 40) | cluster as u64, 1, 1, Bound::Exact);
            }
        }

        let reported = table.hashfull();
        assert!(
            (100..=150).contains(&reported),
            "an eighth-full table reported {reported} per mille"
        );
    }

    #[test]
    fn hashfull_rises_with_occupancy() {
        let table = Table::new(1);
        assert_eq!(table.hashfull(), 0);

        let mut previous = 0;
        for round in 1..=4_u64 {
            for i in 0..(table.capacity_entries() as u64 / 4) {
                store(
                    &table,
                    i.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ round,
                    1,
                    1,
                    Bound::Exact,
                );
            }
            let reported = table.hashfull();
            assert!(reported >= previous, "occupancy estimate went backwards");
            assert!(reported <= 1000);
            previous = reported;
        }
        assert!(previous > 500, "a heavily loaded table reported {previous}");
    }

    /// AC#14. Every writer stores a score that is a known function of its key, so any snapshot a
    /// reader accepts whose score does not match that function is information the table invented.
    /// Races are allowed to lose entries; they are not allowed to fabricate one.
    #[test]
    fn concurrent_writers_and_readers_never_invent_an_entry() {
        fn expected(key: u64) -> i16 {
            ((key.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 40) % 20_001) as i16 - 10_000
        }

        // Deliberately small, so every writer contends for the same handful of clusters.
        let table = Arc::new(Table::new(0));
        let accepted = Arc::new(AtomicUsize::new(0));
        const KEYS: u64 = 64;

        std::thread::scope(|scope| {
            for thread in 0..8_u64 {
                let table = Arc::clone(&table);
                let accepted = Arc::clone(&accepted);
                scope.spawn(move || {
                    let mut hits = 0;
                    for round in 0..20_000_u64 {
                        let key = (round.wrapping_add(thread * 7) % KEYS) | 1;
                        if round % 2 == 0 {
                            table.store(
                                key,
                                Score::cp(expected(key)),
                                (round % 200) as u8,
                                Bound::Exact,
                                &Move::null(),
                            );
                        } else if let Some(snapshot) = table.probe(key) {
                            assert_eq!(snapshot.key(), key);
                            assert_eq!(
                                snapshot.score(),
                                Score::cp(expected(key)),
                                "a snapshot carried data belonging to another key"
                            );
                            hits += 1;
                        }
                    }
                    accepted.fetch_add(hits, Ordering::Relaxed);
                });
            }
        });

        assert!(
            accepted.load(Ordering::Relaxed) > 0,
            "the concurrency test never observed a hit, so it verified nothing"
        );
    }

    /// AC#11/AC#12. Probes and stores go through `&Table` from many threads with no coordination,
    /// and every worker can consume every other worker's entries: nothing is partitioned by worker.
    #[test]
    fn every_worker_can_consume_every_other_workers_entries() {
        let table = Arc::new(Table::new(1));
        const PER_THREAD: u64 = 500;

        std::thread::scope(|scope| {
            for thread in 0..4_u64 {
                let table = Arc::clone(&table);
                scope.spawn(move || {
                    for i in 0..PER_THREAD {
                        let key = (thread << 32) | (i + 1);
                        table.store(
                            key,
                            Score::cp(thread as i16),
                            20,
                            Bound::Exact,
                            &Move::null(),
                        );
                    }
                });
            }
        });

        // A single reader now finds entries written by threads it has no relationship with.
        let mut found = [0_usize; 4];
        for thread in 0..4_u64 {
            for i in 0..PER_THREAD {
                let key = (thread << 32) | (i + 1);
                if let Some(snapshot) = table.probe(key) {
                    assert_eq!(snapshot.score(), Score::cp(thread as i16));
                    found[thread as usize] += 1;
                }
            }
        }

        for (thread, count) in found.iter().enumerate() {
            assert!(
                *count > 0,
                "worker {thread}'s entries were unreachable from another worker"
            );
        }
    }

    #[test]
    fn concurrent_age_advances_never_invalidate_entries() {
        let table = Arc::new(Table::new(1));
        for i in 1..=1_000_u64 {
            store(&table, i, 5, 5, Bound::Exact);
        }

        std::thread::scope(|scope| {
            for _ in 0..4 {
                let table = Arc::clone(&table);
                scope.spawn(move || {
                    for _ in 0..1_000 {
                        table.advance_age();
                    }
                });
            }
            let table = Arc::clone(&table);
            scope.spawn(move || {
                for _ in 0..1_000 {
                    for i in 1..=100_u64 {
                        if let Some(snapshot) = table.probe(i) {
                            assert_eq!(snapshot.score(), Score::cp(5));
                        }
                    }
                }
            });
        });

        // Ages only order replacement, so entries nothing has overwritten are still there.
        assert!((1..=1_000_u64).any(|i| table.probe(i).is_some()));
    }
}
