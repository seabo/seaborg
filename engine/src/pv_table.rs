//! A simple implementation of a PV table for maintaining the principal variation during search.
//!
//! There are some possible implementation optimisations we could play with - this version is
//! written quickly for correctness. It's unclear if optimisations are worth it here, because the
//! PV will be updated rarely when the search is good. This means that the naive implementation
//! with copies won't really impact performance.
//!
//! For future reference, possible enhancements are:
//! * Use linked-lists, and only ever swap pointers around as the PV builds up. It should be
//!   possible to accomplish this but the code is gnarly. In theory, it should knock out loads of
//!   copies and make things much faster, but as above, it might not make an overall difference.
//!
//! PV tables are often not used when the search uses a transpostion table, since the principal
//! variation can usually be recovered by inspecting this directly as needed.

use chess::mov::Move;

/// Table for storing the principal variation during search.
///
/// The table is a triangular array of rows indexed by *ply from the root*. Row `p` holds the
/// variation the search intends to play starting from ply `p`, in the order the moves are played,
/// so row 0 is the principal variation itself.
///
/// Rows are indexed by ply rather than by remaining depth because remaining depth is not a
/// property of the node: two siblings at the same ply may be searched to different depths once
/// reductions exist, and a node may be searched deeper than the nominal horizon once extensions
/// exist. Either would make depth-derived rows collide or run off the end of the table. Ply is
/// unambiguous, so each node writes exactly the row belonging to its own position on the path.
pub struct PVTable {
    data: Vec<Move>,
    plies: usize,
}

impl PVTable {
    /// Build a table able to report a variation `plies` moves long.
    pub fn new(plies: u8) -> Self {
        let p = plies as usize;
        Self {
            data: vec![Move::null(); p * p],
            plies: p,
        }
    }

    /// The greatest number of moves the row for `ply` can hold: everything from `ply` down to the
    /// last ply the table covers.
    #[inline(always)]
    fn row_len(&self, ply: usize) -> usize {
        self.plies - ply
    }

    /// Records that the node at `ply` intends to play `mov`, followed by whatever line the child
    /// at `ply + 1` established.
    ///
    /// The child's row is read as it stands, so this must be called after the child returned and
    /// before any sibling overwrites it.
    ///
    /// A node deeper than the table can report is ignored rather than truncating some other ply's
    /// row: a subtree extended past the nominal horizon simply does not contribute to the reported
    /// line.
    pub fn copy_to(&mut self, ply: usize, mov: Move) {
        if ply >= self.plies {
            return;
        }

        let stride = self.plies;
        let len = self.row_len(ply);
        let row = ply * stride;

        self.data[row] = mov;
        if len > 1 {
            let child = row + stride;
            self.data.copy_within(child..child + len - 1, row + 1);
        }
    }

    /// Empties the row belonging to `ply`, so the row no longer describes any variation.
    ///
    /// Search calls this on entry to every node. A node that returns without establishing an exact
    /// line — a transposition cutoff, an immediate draw, mate-distance pruning, razoring, an abort,
    /// checkmate or stalemate, or an all-node fail-low — then leaves an empty row instead of the
    /// line left behind by a previously searched sibling. Without this, the parent's `copy_to`
    /// would splice that unrelated sibling line into its own PV, producing a reported variation
    /// that does not chain legally.
    ///
    /// A ply beyond the table's reach has no row to clear and is a no-op, on the same terms as
    /// [`PVTable::copy_to`].
    pub fn clear_at(&mut self, ply: usize) {
        if ply >= self.plies {
            return;
        }

        let row = ply * self.plies;
        let len = self.row_len(ply);
        self.data[row..row + len].fill(Move::null());
    }

    /// Get an iterator over the principal variation.
    pub fn pv(&self) -> PVIter<'_> {
        PVIter {
            iter: self.data[0..self.plies].iter(),
        }
    }
}

/// An iterator over the principal variation.
///
/// The line ends at the first empty slot: a node that stopped short of the horizon left the rest
/// of its row null, and the moves beyond it belong to no established variation.
pub struct PVIter<'a> {
    iter: std::slice::Iter<'a, Move>,
}

impl<'a> Iterator for PVIter<'a> {
    type Item = &'a Move;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().filter(|&m| !m.is_null())
    }
}

impl std::fmt::Debug for PVTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let d = self.plies;

        write!(f, "     │ ")?;
        for col in 0..d {
            write!(f, "{:^5} │ ", col)?;
        }
        writeln!(f)?;

        write!(f, "─────┼")?;
        for _ in 0..(d.saturating_sub(1)) {
            write!(f, "───────┼")?;
        }
        writeln!(f, "───────┤")?;

        for ply in 0..d {
            write!(f, " {:>3} │ ", ply)?;
            for col in 0..d {
                let mov = self.data[ply * d + col];
                if mov.is_null() {
                    write!(f, "  *   │ ")?;
                } else {
                    write!(f, " {:>5} │ ", mov)?;
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess::mov::MoveType;
    use chess::position::Square;

    fn mov(orig: Square, dest: Square) -> Move {
        Move::build(orig, dest, None, MoveType::QUIET)
    }

    fn pv_of(table: &PVTable) -> Vec<Move> {
        table.pv().copied().collect()
    }

    /// A PV is assembled from the leaf upwards, one row per ply, and each row is spliced onto the
    /// row belonging to the ply below it.
    #[test]
    fn copy_to_chains_each_ply_onto_the_line_below_it() {
        let a = mov(Square::E2, Square::E4);
        let b = mov(Square::E7, Square::E5);
        let c = mov(Square::G1, Square::F3);

        let mut table = PVTable::new(3);
        table.copy_to(2, c);
        table.copy_to(1, b);
        table.copy_to(0, a);

        assert_eq!(pv_of(&table), vec![a, b, c]);
    }

    /// A node that stops short of the search horizon reports only as far as it actually looked.
    #[test]
    fn pv_terminates_at_the_first_empty_slot() {
        let a = mov(Square::E2, Square::E4);
        let b = mov(Square::E7, Square::E5);

        let mut table = PVTable::new(3);
        table.copy_to(1, b);
        table.copy_to(0, a);

        assert_eq!(pv_of(&table), vec![a, b]);
    }

    /// The regression that produced illegal reported PVs: a node that returns without establishing
    /// a line must not leave its sibling's continuation behind for the parent to splice up.
    #[test]
    fn clear_at_prevents_a_stale_sibling_line_from_being_spliced_up() {
        let sibling_reply = mov(Square::E7, Square::E5);
        let sibling = mov(Square::E2, Square::E4);
        let chosen = mov(Square::D2, Square::D4);

        let mut table = PVTable::new(3);

        // A sibling subtree searched to the horizon, leaving rows for plies 1 and 2 populated.
        table.copy_to(2, mov(Square::G1, Square::F3));
        table.copy_to(1, sibling_reply);
        table.copy_to(0, sibling);
        assert_eq!(pv_of(&table).len(), 3);

        // The next root move's subtree returns early, so its ply-1 row is only cleared, never
        // written. The root must then report just its own move.
        table.clear_at(1);
        table.copy_to(0, chosen);

        assert_eq!(pv_of(&table), vec![chosen]);
    }

    /// A subtree extended past the nominal horizon reaches plies the table has no row for. Those
    /// nodes must be ignored rather than corrupting a row that belongs to a shallower ply.
    #[test]
    fn plies_beyond_the_table_neither_panic_nor_disturb_the_reported_line() {
        let a = mov(Square::E2, Square::E4);
        let b = mov(Square::E7, Square::E5);

        let mut table = PVTable::new(2);
        table.copy_to(1, b);
        table.copy_to(0, a);
        let before = pv_of(&table);

        table.clear_at(2);
        table.clear_at(7);
        table.copy_to(2, mov(Square::G1, Square::F3));
        table.copy_to(7, mov(Square::B1, Square::C3));

        assert_eq!(pv_of(&table), before);
    }

    /// The deepest row the table covers holds a single move, so a node there reports its own move
    /// and no continuation, without reading a child row that does not exist.
    #[test]
    fn the_deepest_row_holds_only_its_own_move() {
        let a = mov(Square::E2, Square::E4);
        let b = mov(Square::E7, Square::E5);

        let mut table = PVTable::new(2);
        table.copy_to(1, b);
        table.copy_to(0, a);

        assert_eq!(pv_of(&table), vec![a, b]);
    }
}
