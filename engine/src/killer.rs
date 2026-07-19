//! Killer moves table.

use core::mov::Move;
use core::position::Position;

#[derive(Debug)]
pub struct KillerTable {
    data: Vec<Entry>,
}

#[derive(Clone, Debug)]
struct Entry {
    mov_a: (Move, usize),
    mov_b: (Move, usize),
}

impl Default for Entry {
    fn default() -> Self {
        Entry {
            mov_a: (Move::null(), 0),
            mov_b: (Move::null(), 0),
        }
    }
}

impl KillerTable {
    /// Create a new `KillerTable` covering plies `0..size` from the root.
    ///
    /// Slot 0 is the root, which never records a killer: the root is searched with an infinite
    /// beta, so no root move can fail high. It is kept so that the ply a node is at is also the
    /// index it uses, with no offset to get wrong.
    pub fn new(size: usize) -> Self {
        let mut data = Vec::with_capacity(size);
        data.resize(size, Default::default());

        Self { data }
    }

    /// Probe the killer table for moves stored at `ply` from the root. Only returns moves which
    /// are valid and legal in the given position.
    ///
    /// Killers are keyed by ply rather than by remaining depth because two siblings at the same
    /// ply may be searched to different depths, and a quiet refutation found in one is just as
    /// likely to refute the other. Keying by depth would file them separately and neither would
    /// see the other's killer.
    pub fn probe(&mut self, ply: usize, pos: &Position) -> (Option<Move>, Option<Move>) {
        if ply == 0 || ply >= self.data.len() {
            return (None, None);
        }

        let entry = &mut self.data[ply];
        let mut ret1 = (None, 0);
        let mut ret2 = (None, 0);

        if pos.valid_move(&entry.mov_a.0) {
            ret1 = (Some(entry.mov_a.0), entry.mov_a.1);
            entry.mov_a.1 += 1;
        }

        if pos.valid_move(&entry.mov_b.0) {
            ret2 = (Some(entry.mov_b.0), entry.mov_b.1);
            entry.mov_b.1 += 1;
        }

        if ret1.0.is_some() && ret2.0.is_some() && ret1.1 < ret2.1 {
            std::mem::swap(&mut ret1, &mut ret2);
        }

        (ret1.0, ret2.0)
    }

    /// Store a killer move found at `ply` from the root. This function does not accept `ply == 0`,
    /// since we do not have killer moves at the root node.
    pub fn store(&mut self, killer: Move, ply: usize) {
        debug_assert!(ply > 0);

        if ply >= self.data.len() {
            return;
        }

        let entry = &mut self.data[ply];

        if entry.mov_a.0 == killer || entry.mov_b.0 == killer {
            // This killer move is already included at this ply.
            return;
        }

        if entry.mov_a.1 < entry.mov_b.1 {
            entry.mov_a = (killer, 0);
        } else {
            entry.mov_b = (killer, 0);
        }
    }
}

impl std::fmt::Display for KillerTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "KillerTable {{")?;
        for (i, e) in self.data.iter().enumerate() {
            writeln!(
                f,
                "  {:>2} | ({}, {}) - ({}, {})",
                i,
                e.mov_a.0.to_uci_string(),
                e.mov_a.1,
                e.mov_b.0.to_uci_string(),
                e.mov_b.1
            )?;
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

    /// Siblings at the same ply share killers even when they were searched to different depths.
    /// Reductions make that the normal case, and keying the table by remaining depth would file
    /// the two subtrees apart so that neither ever saw the other's refutation.
    #[test]
    fn a_killer_is_visible_to_every_node_at_the_same_ply() {
        core::init::init_globals();
        let pos = Position::start_pos();
        let first = quiet(Square::E2, Square::E4);
        let second = quiet(Square::D2, Square::D4);

        let mut kt = KillerTable::new(8);

        // Interleaved as the search does it: a node stores its refutation, and the next node at
        // the same ply probes for it before contributing one of its own. Both nodes are at ply 3,
        // whatever depth either was searched to.
        kt.store(first, 3);
        assert!(offered(&mut kt, 3, &pos).contains(&first));

        kt.store(second, 3);
        let offered = offered(&mut kt, 3, &pos);
        assert!(offered.contains(&first), "got {offered:?}");
        assert!(offered.contains(&second), "got {offered:?}");
    }

    /// The killers a node would actually be given, in no particular order. Which of the two slots
    /// a move occupies is a detail of the replacement policy, not something a caller relies on.
    fn offered(kt: &mut KillerTable, ply: usize, pos: &Position) -> Vec<Move> {
        let (a, b) = kt.probe(ply, pos);
        [a, b].into_iter().flatten().collect()
    }

    /// Killers belong to one ply only. A refutation found deeper in the tree must not be offered
    /// to a shallower node, whose position it has no relationship to.
    #[test]
    fn a_killer_does_not_leak_to_a_neighbouring_ply() {
        core::init::init_globals();
        let pos = Position::start_pos();
        let killer = quiet(Square::E2, Square::E4);

        let mut kt = KillerTable::new(8);
        kt.store(killer, 3);

        assert_eq!(kt.probe(2, &pos), (None, None));
        assert_eq!(kt.probe(4, &pos), (None, None));
    }

    /// The root records no killer, and a ply past the table's reach is dropped rather than
    /// wrapping onto some other ply's entry.
    #[test]
    fn the_root_and_plies_beyond_the_table_hold_nothing() {
        core::init::init_globals();
        let pos = Position::start_pos();

        let mut kt = KillerTable::new(4);
        kt.store(quiet(Square::E2, Square::E4), 4);
        kt.store(quiet(Square::D2, Square::D4), 99);

        assert_eq!(kt.probe(0, &pos), (None, None));
        for ply in 0..4 {
            assert_eq!(kt.probe(ply, &pos), (None, None));
        }
    }
}
