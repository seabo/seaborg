use crate::position::Position;

pub struct TranspoTable {
    /// Data
    data: Vec<Option<TranspoEntry>>,
    // TODO: we want to keep some kind of struct containing info on
    // accesses, collisions, updates, etc. Ideally this can be turned off
    // for efficiency in production, but would that need a macro?
}

#[derive(Clone, Debug)]
pub struct TranspoEntry {
    pub key_check: u32,
    pub x: u8,
}

impl TranspoTable {
    pub fn with_capacity(c: u32) -> Self {
        let capacity = usize::pow(2, c);
        let mut data: Vec<Option<TranspoEntry>> = Vec::with_capacity(capacity);
        data.resize(capacity, None);

        TranspoTable { data }
    }

    pub fn byte_size(&self) -> usize {
        std::mem::size_of_val(&self.data)
    }

    pub fn insert(&mut self, pos: Position) {
        let idx = self.pos_to_idx(pos);

        // Safety: `idx` is guaranteed to be smaller than the capacity
        // of `self.data` by construction (the modulus / mask trick).
        // We have also pre-initialized the entire Vec to `None`.
        let current_entry = unsafe { self.data.get_unchecked_mut(idx) };

        match current_entry {
            Some(entry) => {
                // decide whether to replace or keep
                println!("there was a collision");
                println!("{:?}", entry);
            }
            None => {
                *current_entry = Some(TranspoEntry { key_check: 0, x: 0 });
            }
        }
    }

    pub fn get(&self, pos: Position) -> &Option<TranspoEntry> {
        let idx = self.pos_to_idx(pos);

        // Safety: `idx` is guaranteed to be smaller than the capacity
        // of `self.data` by construction (the modulus / mask trick).
        // We have also pre-initialized the entire Vec to `None`.
        unsafe { self.data.get_unchecked(idx) }
    }

    #[inline(always)]
    fn pos_to_idx(&self, pos: Position) -> usize {
        let zob = pos.zobrist();
        let zob_left_bits = (zob.0 >> 32) as usize;
        // Store the modulus mask in the TranspoTable rather than calculating every time
        let modulus_mask = self.data.capacity() - 1;
        let idx = zob_left_bits & modulus_mask;
        idx
    }
}

// We want:
//
// 1. with_capacity() -> [note: new() function calls this with a default capacity]
//    -- this takes a exponent for a power of two table-size
// 2. insert(pos: Position, data: Data) -> puts a position in the transpo table; this involves:
//    -- modulus the (first half) of the zobrist key by the capacity (use the bitmask trick for modulus 2^n)
//    -- insert at that index
//    -- build an entry with:
//       - second half of zobrist key
//       - data struct (so that we can make this generic later)
//       - data struct initially contains: search depth
//    -- if there is already an entry at this index, we need to apply a replacement strategy; possibilities are:
//       - always replace
//       - priority by move ordering position
//       - depth-preferred
//    -- perhaps we can define a ReplacementStrategy trait
// 3. get(pos: Position) -> Option<Data>
//    -- under the hood, we will be doing a modulus on the capacity of the Vec, so can use get_unchecked() for
//         faster access
// 4. get_or_insert(pos: Position) -> Data
//
//
