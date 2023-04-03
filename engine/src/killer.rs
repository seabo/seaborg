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
    // Create a new `KillerTable`, initialized to `size` plies.
    pub fn new(size: usize) -> Self {
        let mut data = Vec::with_capacity(size);
        data.resize(size, Default::default());

        Self { data }
    }

    /// Probe the killer table for moves at `draft` distance from the root. Only returns moves
    /// which are valid and legal in the given position.
    pub fn probe(&mut self, draft: u8, pos: &Position) -> (Option<Move>, Option<Move>) {
        if draft == 0 || draft as usize > self.data.len() {
            return (None, None);
        }

        let entry = &mut self.data[draft as usize - 1];
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

    /// Store a killer move for a given draft (number of ply down from the root). This function
    /// does not accept `draft == 0`, since we do not have killer moves at the root node.
    pub fn store(&mut self, killer: Move, draft: u8) {
        debug_assert!(draft > 0);

        if draft as usize > self.data.len() {
            return;
        }

        let entry = &mut self.data[draft as usize - 1];

        if entry.mov_a.0 == killer || entry.mov_b.0 == killer {
            // This killer move is already included at this draft.
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
