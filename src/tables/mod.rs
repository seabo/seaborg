use crate::position::Position;
use separator::Separatable;
use std::cell::RefCell;

pub struct TranspoTable<T> {
    /// Data
    data: Vec<Option<Slot<T>>>,
    /// Pre-calculated mask to help with indexing into the table
    modulus_mask: usize,
    /// Contains tracking info to help debug perfomance
    trace: RefCell<Tracer>,
}

#[derive(Clone, Debug)]
struct Slot<T> {
    pub signature: u32,
    pub data: T,
}

impl<T: Clone> TranspoTable<T> {
    pub fn with_capacity(c: u32) -> Self {
        let capacity = usize::pow(2, c);
        let mut data: Vec<Option<Slot<T>>> = Vec::with_capacity(capacity);
        data.resize(capacity, None);
        let modulus_mask = data.capacity() - 1;

        TranspoTable {
            data,
            modulus_mask,
            trace: RefCell::new(Tracer::new()),
        }
    }

    pub fn byte_size(&self) -> usize {
        std::mem::size_of_val(&self.data)
    }

    pub fn insert(&mut self, pos: &Position, data: T) {
        let (idx, signature) = self.pos_to_idx_and_sig(pos);

        // Safety: `idx` is guaranteed to be smaller than the capacity
        // of `self.data` by construction (the modulus / mask trick).
        // We have also pre-initialized the entire Vec to `None`.
        let entry = self.get_entry_mut(idx);
        match entry {
            Some(slot) => {
                if slot.signature == signature {
                    // we have a true hit (or possibly a hash collision, but there's no way to know)
                    // TODO: currently, we just always replace - need a better thought through approach
                    // to replacement strategy
                    *entry = Some(Slot { signature, data });
                    self.record_replacement();
                    self.record_hit();
                } else {
                    // we had an index collision (i.e. the Zobrist hash led to an entry in the
                    // transposition table that was already populated by a different position
                    self.record_collision();
                }
            }
            None => {
                *entry = Some(Slot { signature, data });
                self.record_miss();
            }
        }
    }

    pub fn get(&self, pos: &Position) -> Option<T> {
        let (idx, signature) = self.pos_to_idx_and_sig(pos);
        let entry = self.get_entry(idx);
        match entry {
            Some(ts) => {
                if ts.signature == signature {
                    self.record_hit();
                    let data = ts.data.clone();
                    Some(data)
                } else {
                    self.record_collision();
                    None
                }
            }
            None => {
                self.record_miss();
                None
            }
        }
    }

    fn record_hit(&self) {
        let mut trace = self.trace.borrow_mut();
        trace.hit();
    }

    fn record_collision(&self) {
        let mut trace = self.trace.borrow_mut();
        trace.idx_collision();
    }

    fn record_miss(&self) {
        let mut trace = self.trace.borrow_mut();
        trace.miss();
    }

    fn record_replacement(&self) {
        let mut trace = self.trace.borrow_mut();
        trace.replacement();
    }

    fn get_entry(&self, idx: usize) -> &Option<Slot<T>> {
        // SAFETY
        // Should only be called with an idx derived from `pos_to_idx_and_sig()`
        // This function is private to the module, so this is fine.
        unsafe { self.data.get_unchecked(idx) }
    }

    fn get_entry_mut(&mut self, idx: usize) -> &mut Option<Slot<T>> {
        // SAFETY
        // Should only be called with an idx derived from `pos_to_idx_and_sig()`
        // This function is private to the module, so this is fine.
        unsafe { self.data.get_unchecked_mut(idx) }
    }

    #[inline(always)]
    fn pos_to_idx_and_sig(&self, pos: &Position) -> (usize, u32) {
        let zob = pos.zobrist();
        let zob_left_bits = (zob.0 >> 32) as usize;
        let check_bits = zob.0 as u32 & u32::MAX;
        // Store the modulus mask in the TranspoTable rather than calculating every time
        let idx = zob_left_bits & self.modulus_mask;
        (idx, check_bits)
    }

    pub fn display_trace(&self) {
        println!("{}", self.trace.borrow());
    }
}

#[derive(Clone, Debug)]
struct Tracer {
    accesses: usize,
    /// Represents an idx collision where the entry was occupied, but the
    /// position did not match (when checked against the stored check bits).
    idx_collisions: usize,
    /// Represents a table hit, where the entry was occupied and the posistion
    /// did match the stored check bits.
    hits: usize,
    /// Represents a table miss, where the entry was not not occupied.
    misses: usize,
    /// Counts the number of times we executed a replacement on finding an
    /// entry occupied.
    replacements: usize,
}

impl Tracer {
    fn new() -> Self {
        Self {
            accesses: 0,
            idx_collisions: 0,
            hits: 0,
            misses: 0,
            replacements: 0,
        }
    }

    fn idx_collision(&mut self) {
        self.accesses += 1;
        self.idx_collisions += 1;
    }

    fn hit(&mut self) {
        self.accesses += 1;
        self.hits += 1;
    }

    fn miss(&mut self) {
        self.accesses += 1;
        self.misses += 1;
    }

    fn replacement(&mut self) {
        self.replacements += 1;
    }
}

impl std::fmt::Display for Tracer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Accesses:       {:>14}",
            self.accesses.separated_string()
        )?;
        writeln!(
            f,
            "Idx collisions: {:>14} ({:.2})",
            self.idx_collisions.separated_string(),
            self.idx_collisions as f64 / self.accesses as f64 * 100 as f64
        )?;
        writeln!(
            f,
            "Hits:           {:>14} ({:.2}%)",
            self.hits.separated_string(),
            self.hits as f64 / self.accesses as f64 * 100 as f64
        )?;
        writeln!(
            f,
            "Misses:         {:>14} ({:.2}%)",
            self.misses.separated_string(),
            self.misses as f64 / self.accesses as f64 * 100 as f64
        )?;
        writeln!(
            f,
            "Replacements:   {:>14} ({:.2}%)",
            self.replacements.separated_string(),
            self.replacements as f64 / self.accesses as f64 * 100 as f64
        )
    }
}
