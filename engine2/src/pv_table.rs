//! A simple implementation of a PV table for maintaining the principal variation during search.
//!
//! There are some possible implementation optimisations we could play with - this version is
//! written quickly for correctness. It's unclear if optimisations are worth it here, because the
//! PV will be updated rarely when the search is good. This means that the naive implementation
//! with copies won't really impact performance.
//!
//! For future reference, possible enhancements are:
//! * Use linked-lists, and only ever swap pointers around as the PV builds up. It should be
//! possible to accomplish this but the code is gnarly. In theory, it should knock out loads of
//! copies and make things much faster, but as above, it might not make an overall difference.
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

    /// Called when a move searched at depth `d` turns out to be a checkmate or stalemate position,
    /// meaning that there is no variation following this point, and no move to include.
    pub fn pv_leaf_at(&mut self, d: u8) {
        let m = self.depth;
        let d = d as usize;
        let k = m - d;
        let _ = &mut self.data[(k * m)..(k * m + d)].fill(Move::null());
    }

    #[inline(always)]
    fn update_leaf(&mut self, mov: Move) {
        // Safety: we never mutate `self.depth` after the `PVTable` struct is created, so this
        // index is guaranteed to exist.
        let m = self.depth;
        unsafe { *self.data.get_unchecked_mut(m * (m - 1)) = mov };
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
        self.iter
            .next()
            .and_then(|m| if m.is_null() { None } else { Some(m) })
    }
}
