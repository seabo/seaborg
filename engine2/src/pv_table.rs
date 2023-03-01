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
    pub fn update_at(&mut self, d: u8, mov: Move) {
        match d {
            1 => self.update_leaf(mov),
            n => self.update_internal(n, mov),
        }
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

    pub fn print_pv(&self) {
        println!(
            "pv: {}",
            self.data[0..self.depth]
                .iter()
                .rev()
                .map(|m| m.to_uci_string())
                .collect::<Vec<String>>()
                .join(" ")
        );
    }
}
