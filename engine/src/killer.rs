//! Killer moves table.
//!
//! Each ply from the root keeps a small, fixed set of *recency* slots holding the most recent quiet
//! moves that produced a beta cutoff there. On a distinct quiet cutoff the newest killer is installed
//! in slot one and the previous distinct slot-one move shifts down; the oldest killer falls off the
//! end. Slot order is therefore purely a record of recency, and a probe reads it without modifying it.
//!
//! The table deliberately measures nothing about how *useful* a killer is: it does not count how
//! often a move was legal, offered or successful. Ordering by usefulness is the job of the history,
//! counter-move and continuation-history tables, which accumulate that evidence over the whole
//! search. Killers only supply "this exact quiet just refuted a sibling at this ply", which recency
//! captures directly and cheaply.

use core::mov::Move;
use core::position::Position;

/// The greatest number of recency slots a ply can hold.
///
/// Two is the conventional killer width: a ply usually has at most a couple of distinct quiet
/// refutations worth trying ahead of the general quiet moves, and a wider table mostly re-surfaces
/// moves the history tables already rank. The active width is chosen per table at construction and
/// may be smaller (including zero, which disables killers entirely) for measurement.
pub const MAX_KILLER_SLOTS: usize = 2;

/// A fixed-width, per-ply recency table of quiet killer moves.
///
/// The table is keyed by ply from the root and owns one row per ply. It is search-local: a worker
/// owns its own table, retains it across iterative-deepening iterations, and clears it between
/// separate searches. Nothing here is shared between workers.
#[derive(Debug)]
pub struct KillerTable {
    /// Active recency slots per ply. `0` disables killers: probe returns nothing and store is a
    /// no-op. Never exceeds [`MAX_KILLER_SLOTS`].
    slots: usize,
    /// One row of [`MAX_KILLER_SLOTS`] moves per ply, indexed by ply from the root. Only the leading
    /// `slots` entries of each row are ever read or written; the rest stay null.
    rows: Vec<[Move; MAX_KILLER_SLOTS]>,
}

impl KillerTable {
    /// Create a table covering plies `0..plies` from the root with `slots` active recency slots.
    ///
    /// Row 0 is the root, which never records a killer: the root is searched with an infinite beta,
    /// so no root move can fail high. The row is kept so that the ply a node is at is also the index
    /// it uses, with no offset to get wrong.
    ///
    /// `slots` must not exceed [`MAX_KILLER_SLOTS`]. A `slots` of zero produces a table that stores
    /// and returns nothing, which is how killers are disabled for an ablation without a separate code
    /// path in the search.
    pub fn new(plies: usize, slots: usize) -> Self {
        assert!(
            slots <= MAX_KILLER_SLOTS,
            "killer table cannot have more than {MAX_KILLER_SLOTS} slots"
        );

        Self {
            slots,
            rows: vec![[Move::null(); MAX_KILLER_SLOTS]; plies],
        }
    }

    /// The number of active recency slots per ply.
    pub fn slots(&self) -> usize {
        self.slots
    }

    /// Probe the killer table for moves stored at `ply` from the root, returning them in slot order:
    /// the newer killer first, then the older. Only moves that are pseudo-legal in `pos` are
    /// returned; a null or now-illegal slot yields `None` in that position.
    ///
    /// The probe is observationally read-only. A killer is learned at a ply in one position and may
    /// be probed at the same ply in a different one, so legality must still be validated here — but
    /// legality, and how often a move is offered, are exposure signals and are deliberately not fed
    /// back into slot order or replacement. That keeps returned order a deterministic function of the
    /// cutoff history alone.
    ///
    /// Killers are keyed by ply rather than by remaining depth because two siblings at the same ply
    /// may be searched to different depths, and a quiet refutation found in one is just as likely to
    /// refute the other. Keying by depth would file them separately and neither would see the other's
    /// killer.
    pub fn probe(&self, ply: usize, pos: &Position) -> (Option<Move>, Option<Move>) {
        if ply == 0 || ply >= self.rows.len() {
            return (None, None);
        }

        let row = &self.rows[ply];
        let first = (self.slots >= 1 && pos.valid_move(&row[0])).then_some(row[0]);
        let second = (self.slots >= 2 && pos.valid_move(&row[1])).then_some(row[1]);

        (first, second)
    }

    /// The recency slot `mov` currently occupies at `ply`, or `None` if it is not a killer there.
    ///
    /// Used by telemetry to attribute a searched move or a beta cutoff to the slot it came from,
    /// after staged ordering has already suppressed any killer that duplicated the hash move. It does
    /// not consider legality and does not mutate the table.
    pub fn slot_of(&self, ply: usize, mov: Move) -> Option<usize> {
        if ply == 0 || ply >= self.rows.len() {
            return None;
        }

        let row = &self.rows[ply];
        (0..self.slots).find(|&i| row[i] == mov)
    }

    /// Record a killer move found at a distinct quiet beta cutoff at `ply` from the root.
    ///
    /// Replacement is pure recency: if `killer` is not already the slot-one move, every slot shifts
    /// down by one (dropping the oldest) and `killer` takes slot one. Re-storing the current slot-one
    /// move is a no-op, so a move that keeps cutting off is not needlessly reshuffled. A `killer`
    /// equal to a *lower* slot is promoted to slot one by the same shift, which cannot leave it
    /// duplicated because the shift overwrites its old position.
    ///
    /// This function does not accept `ply == 0`; the root records no killer. Plies past the table's
    /// reach, and any store into a zero-slot table, are dropped rather than wrapping onto another
    /// ply's row.
    pub fn store(&mut self, killer: Move, ply: usize) {
        debug_assert!(ply > 0);

        if self.slots == 0 || ply == 0 || ply >= self.rows.len() {
            return;
        }

        let row = &mut self.rows[ply];
        if row[0] == killer {
            return;
        }

        for i in (1..self.slots).rev() {
            row[i] = row[i - 1];
        }
        row[0] = killer;
    }

    /// Clear every slot at every ply.
    ///
    /// Killers are retained across the iterations of one search but are not carried into the next
    /// search: the caller resets the table when a search ends so a later search on this worker starts
    /// from an empty table rather than inheriting refutations learned for an unrelated position.
    pub fn reset(&mut self) {
        for row in &mut self.rows {
            *row = [Move::null(); MAX_KILLER_SLOTS];
        }
    }
}

impl std::fmt::Display for KillerTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "KillerTable {{")?;
        for (i, row) in self.rows.iter().enumerate() {
            write!(f, "  {i:>3} |")?;
            for slot in &row[..self.slots] {
                write!(f, " {}", slot.to_uci_string())?;
            }
            writeln!(f)?;
        }
        writeln!(f, "}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mov::MoveType;
    use core::position::Square;

    fn quiet(orig: Square, dest: Square) -> Move {
        Move::build(orig, dest, None, MoveType::QUIET)
    }

    /// The killers a node would actually be given, in slot order.
    fn offered(kt: &KillerTable, ply: usize, pos: &Position) -> Vec<Move> {
        let (a, b) = kt.probe(ply, pos);
        [a, b].into_iter().flatten().collect()
    }

    /// Siblings at the same ply share killers even when they were searched to different depths.
    /// Reductions make that the normal case, and keying the table by remaining depth would file the
    /// two subtrees apart so that neither ever saw the other's refutation.
    #[test]
    fn a_killer_is_visible_to_every_node_at_the_same_ply() {
        core::init::init_globals();
        let pos = Position::start_pos();
        let first = quiet(Square::E2, Square::E4);
        let second = quiet(Square::D2, Square::D4);

        let mut kt = KillerTable::new(8, MAX_KILLER_SLOTS);

        kt.store(first, 3);
        assert!(offered(&kt, 3, &pos).contains(&first));

        kt.store(second, 3);
        let offered = offered(&kt, 3, &pos);
        assert!(offered.contains(&first), "got {offered:?}");
        assert!(offered.contains(&second), "got {offered:?}");
    }

    /// Killers belong to one ply only. A refutation found deeper in the tree must not be offered to a
    /// shallower node, whose position it has no relationship to.
    #[test]
    fn a_killer_does_not_leak_to_a_neighbouring_ply() {
        core::init::init_globals();
        let pos = Position::start_pos();
        let killer = quiet(Square::E2, Square::E4);

        let mut kt = KillerTable::new(8, MAX_KILLER_SLOTS);
        kt.store(killer, 3);

        assert_eq!(kt.probe(2, &pos), (None, None));
        assert_eq!(kt.probe(4, &pos), (None, None));
    }

    /// The root records no killer, and a ply past the table's reach is dropped rather than wrapping
    /// onto some other ply's entry.
    #[test]
    fn the_root_and_plies_beyond_the_table_hold_nothing() {
        core::init::init_globals();
        let pos = Position::start_pos();

        let mut kt = KillerTable::new(4, MAX_KILLER_SLOTS);
        kt.store(quiet(Square::E2, Square::E4), 4);
        kt.store(quiet(Square::D2, Square::D4), 99);

        assert_eq!(kt.probe(0, &pos), (None, None));
        for ply in 0..4 {
            assert_eq!(kt.probe(ply, &pos), (None, None));
        }
    }

    /// The last row of the table stores and retrieves a killer, and the first index past it is
    /// dropped. Probe and store must agree on the same boundary so a move stored at the deepest
    /// supported ply is actually retrievable there.
    #[test]
    fn the_last_supported_ply_stores_and_the_next_is_dropped() {
        core::init::init_globals();
        let pos = Position::start_pos();
        let killer = quiet(Square::E2, Square::E4);

        let mut kt = KillerTable::new(8, MAX_KILLER_SLOTS);

        kt.store(killer, 7);
        assert_eq!(offered(&kt, 7, &pos), vec![killer]);

        kt.store(quiet(Square::D2, Square::D4), 8);
        assert_eq!(kt.probe(8, &pos), (None, None));
    }

    /// Slot order is a deterministic record of recency: the most recent distinct cutoff is slot one,
    /// the previous distinct move is slot two, and probe returns them newest-first regardless of the
    /// position they are read in.
    #[test]
    fn returned_order_is_newest_first() {
        core::init::init_globals();
        let pos = Position::start_pos();
        let older = quiet(Square::E2, Square::E4);
        let newer = quiet(Square::D2, Square::D4);

        let mut kt = KillerTable::new(8, MAX_KILLER_SLOTS);
        kt.store(older, 3);
        kt.store(newer, 3);

        assert_eq!(kt.probe(3, &pos), (Some(newer), Some(older)));
    }

    /// Storing the current slot-one move again is a no-op: it does not evict slot two or otherwise
    /// disturb the ordering.
    #[test]
    fn restoring_the_first_slot_changes_nothing() {
        core::init::init_globals();
        let pos = Position::start_pos();
        let first = quiet(Square::E2, Square::E4);
        let second = quiet(Square::D2, Square::D4);

        let mut kt = KillerTable::new(8, MAX_KILLER_SLOTS);
        kt.store(second, 3);
        kt.store(first, 3);
        assert_eq!(kt.probe(3, &pos), (Some(first), Some(second)));

        kt.store(first, 3);
        assert_eq!(kt.probe(3, &pos), (Some(first), Some(second)));
    }

    /// A move already held in slot two that cuts off again is promoted to slot one, not duplicated
    /// across both slots.
    #[test]
    fn promoting_the_second_slot_does_not_duplicate_it() {
        core::init::init_globals();
        let pos = Position::start_pos();
        let a = quiet(Square::E2, Square::E4);
        let b = quiet(Square::D2, Square::D4);

        let mut kt = KillerTable::new(8, MAX_KILLER_SLOTS);
        kt.store(a, 3);
        kt.store(b, 3); // slots: [b, a]
        kt.store(a, 3); // a promoted back to slot one
        assert_eq!(kt.probe(3, &pos), (Some(a), Some(b)));
    }

    /// After three distinct cutoffs the two slots hold the two most recent moves and the oldest has
    /// fallen off the end.
    #[test]
    fn a_third_distinct_cutoff_evicts_the_oldest() {
        core::init::init_globals();
        let pos = Position::start_pos();
        let first = quiet(Square::E2, Square::E4);
        let second = quiet(Square::D2, Square::D4);
        let third = quiet(Square::G1, Square::F3);

        let mut kt = KillerTable::new(8, MAX_KILLER_SLOTS);
        kt.store(first, 3);
        kt.store(second, 3);
        kt.store(third, 3);

        assert_eq!(kt.probe(3, &pos), (Some(third), Some(second)));
    }

    /// `slot_of` reports which recency slot a move occupies, ignoring legality, and `None` for a move
    /// that is not a killer at that ply.
    #[test]
    fn slot_of_reports_the_recency_slot() {
        core::init::init_globals();
        let first = quiet(Square::E2, Square::E4);
        let second = quiet(Square::D2, Square::D4);
        let absent = quiet(Square::G1, Square::F3);

        let mut kt = KillerTable::new(8, MAX_KILLER_SLOTS);
        kt.store(second, 3);
        kt.store(first, 3);

        assert_eq!(kt.slot_of(3, first), Some(0));
        assert_eq!(kt.slot_of(3, second), Some(1));
        assert_eq!(kt.slot_of(3, absent), None);
        assert_eq!(kt.slot_of(2, first), None);
    }

    /// A single-slot table keeps only the most recent killer, and probe never returns a second.
    #[test]
    fn one_slot_keeps_only_the_newest() {
        core::init::init_globals();
        let pos = Position::start_pos();
        let older = quiet(Square::E2, Square::E4);
        let newer = quiet(Square::D2, Square::D4);

        let mut kt = KillerTable::new(8, 1);
        kt.store(older, 3);
        kt.store(newer, 3);

        assert_eq!(kt.probe(3, &pos), (Some(newer), None));
        assert_eq!(kt.slot_of(3, older), None);
    }

    /// A zero-slot table disables killers: it stores nothing and returns nothing, which is how an
    /// ablation runs the search with killers off through the same code path.
    #[test]
    fn zero_slots_disables_killers() {
        core::init::init_globals();
        let pos = Position::start_pos();

        let mut kt = KillerTable::new(8, 0);
        kt.store(quiet(Square::E2, Square::E4), 3);

        assert_eq!(kt.probe(3, &pos), (None, None));
        assert_eq!(kt.slot_of(3, quiet(Square::E2, Square::E4)), None);
    }

    /// Reset clears every slot so a later search starts from an empty table rather than inheriting an
    /// earlier search's refutations.
    #[test]
    fn reset_clears_every_ply() {
        core::init::init_globals();
        let pos = Position::start_pos();
        let killer = quiet(Square::E2, Square::E4);

        let mut kt = KillerTable::new(8, MAX_KILLER_SLOTS);
        kt.store(killer, 3);
        kt.store(killer, 5);
        assert_eq!(offered(&kt, 3, &pos), vec![killer]);

        kt.reset();

        assert_eq!(kt.probe(3, &pos), (None, None));
        assert_eq!(kt.probe(5, &pos), (None, None));
    }
}
