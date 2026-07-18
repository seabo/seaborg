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

use core::mov::Move;

/// Table for storing the principal variation during search.
pub struct PVTable {
    data: Vec<Move>,
    depth: usize,
}

impl PVTable {
    pub fn new(depth: u8) -> Self {
        let d = depth as usize;
        Self {
            data: vec![Move::null(); d * d],
            depth: d,
        }
    }

    /// Called when a move searched at depth `d` improves the score. Copies the current principal
    /// variation from depth `d-1` and copies it into the depth `d` column of the table, appending
    /// the new move to the end.
    pub fn copy_to(&mut self, d: u8, mov: Move) {
        match d {
            1 => self.update_leaf(mov),
            n => self.update_internal(n, mov),
        }
    }

    /// Empties the row a node searching at remaining depth `d` would write, so the row no longer
    /// describes any variation.
    ///
    /// Search calls this on entry to every node. A node that returns without establishing an exact
    /// line — a transposition cutoff, an immediate draw, mate-distance pruning, razoring, an abort,
    /// checkmate or stalemate, or an all-node fail-low — then leaves an empty row instead of the
    /// line left behind by a previously searched sibling. Without this, the parent's `copy_to`
    /// would splice that unrelated sibling line into its own PV, producing a reported variation
    /// that does not chain legally.
    ///
    /// `d == 0` is a no-op: quiescence nodes have no row of their own.
    pub fn clear_at(&mut self, d: u8) {
        let m = self.depth;
        let d = d as usize;
        let k = m - d;
        let _ = &mut self.data[(k * m)..(k * m + d)].fill(Move::null());
    }

    #[inline(always)]
    fn update_leaf(&mut self, mov: Move) {
        let m = self.depth;
        self.data[m * (m - 1)] = mov;
    }

    #[inline(always)]
    fn update_internal(&mut self, d: u8, mov: Move) {
        let m = self.depth;
        let d = d as usize;
        let k = m - d;

        let _ = &mut self
            .data
            .copy_within(((k + 1) * m)..((k + 1) * m + d - 1), k * m);

        self.data[k * m + d - 1] = mov;
    }

    /// Get an iterator over the principal variation.
    pub fn pv(&self) -> PVIter<'_> {
        PVIter {
            iter: self.data[0..self.depth].iter().rev(),
        }
    }
}

/// An iterator over the principal variation.
pub struct PVIter<'a> {
    iter: std::iter::Rev<std::slice::Iter<'a, Move>>,
}

impl<'a> Iterator for PVIter<'a> {
    type Item = &'a Move;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().filter(|&m| !m.is_null())
    }
}

impl std::fmt::Debug for PVTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let d = self.depth;

        write!(f, "    │ ")?;
        for col in 0..d {
            write!(f, "{:^5} │ ", col)?;
        }
        writeln!(f)?;

        write!(f, "    ├")?;
        for _ in 0..(d - 1) {
            write!(f, "───────┼")?;
        }
        writeln!(f, "───────┤")?;

        for row in 0..d {
            write!(f, " {:>2} │ ", row)?;
            for col in 0..d {
                let mov = self.data[col * d + (d - row - 1)];
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
    use core::mov::MoveType;
    use core::position::Square;

    fn mov(orig: Square, dest: Square) -> Move {
        Move::build(orig, dest, None, MoveType::QUIET)
    }

    fn pv_of(table: &PVTable) -> Vec<Move> {
        table.pv().copied().collect()
    }

    /// A PV is assembled from the leaf upwards, one row per ply.
    #[test]
    fn copy_to_chains_each_ply_onto_the_line_below_it() {
        let a = mov(Square::E2, Square::E4);
        let b = mov(Square::E7, Square::E5);
        let c = mov(Square::G1, Square::F3);

        let mut table = PVTable::new(3);
        table.copy_to(1, c);
        table.copy_to(2, b);
        table.copy_to(3, a);

        assert_eq!(pv_of(&table), vec![a, b, c]);
    }

    /// A node that stops short of the search horizon reports only as far as it actually looked.
    #[test]
    fn pv_terminates_at_the_first_empty_slot() {
        let a = mov(Square::E2, Square::E4);
        let b = mov(Square::E7, Square::E5);

        let mut table = PVTable::new(3);
        table.copy_to(2, b);
        table.copy_to(3, a);

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

        // A sibling subtree searched to the horizon, leaving rows for plies 2 and 3 populated.
        table.copy_to(1, mov(Square::G1, Square::F3));
        table.copy_to(2, sibling_reply);
        table.copy_to(3, sibling);
        assert_eq!(pv_of(&table).len(), 3);

        // The next root move's subtree returns early, so its ply-2 row is only cleared, never
        // written. The root must then report just its own move.
        table.clear_at(2);
        table.copy_to(3, chosen);

        assert_eq!(pv_of(&table), vec![chosen]);
    }

    /// Quiescence nodes sit below the table, so clearing at remaining depth zero does nothing.
    #[test]
    fn clear_at_zero_depth_is_a_no_op() {
        let a = mov(Square::E2, Square::E4);

        let mut table = PVTable::new(2);
        table.copy_to(1, mov(Square::E7, Square::E5));
        table.copy_to(2, a);
        let before = pv_of(&table);

        table.clear_at(0);

        assert_eq!(pv_of(&table), before);
    }
}
