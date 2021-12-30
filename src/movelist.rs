/// Data structure to store a collection `Move`s. This is used for move generation
/// and exists to avoid using `Vec<Move>`, which would live on the heap. `MoveList`
/// uses a fixed size array structure, and so lives on the stack. We can do this
/// because it is (believed to be) the case that no chess position has more than
/// 218 legal moves, so we will never overflow the bounds if used correctly.
///
/// # Safety
///
/// `MoveList` contains both a `push_mv` method and a `unchecked_push_mv` method.
/// The intention is to the use the latter in production builds for max speed, but
/// the former in debug builds. We can then stress test by running a variety of
/// perft tests and check there are no bound overflow panics. Clearly this is still
/// not a cast-iron guarantee of safety but should be good enough.
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::slice;

use crate::mov::Move;

/// 254 is chosen so that the total size of the `MoveList` struct is exactly 1024
/// bytes, taking account of the `len` field.
pub const MAX_MOVES: usize = 254;

/// Trait to generalize operations on structures containing a collection of `Move`s.
pub trait MVPushable: Sized + IndexMut<usize> + Index<usize> + DerefMut {
    /// Add a `Move` to the end of the list. Wraps `push_mv` and `unchecked_push_mv`
    /// and use `cfg(debug_assertions)` to choose which to use at compile-time.
    fn push(&mut self, mv: Move);

    /// Add a `Move` to the end of the list.
    fn push_mv(&mut self, mv: Move);

    /// Add a `Move` to the end of the list, without checking bounds.
    unsafe fn unchecked_push_mv(&mut self, mv: Move);

    /// Set the length of the list.
    unsafe fn unchecked_set_len(&mut self, len: usize);

    /// Return a pointer to the first (0th index) element in the list.
    unsafe fn list_ptr(&mut self) -> *mut Self::Output;

    /// Return a pointer to the element next to the last element in the list.
    unsafe fn over_bounds_ptr(&mut self) -> *mut Self::Output;
}

#[derive(Clone)]
pub struct MoveList {
    inner: [Move; MAX_MOVES],
    len: usize,
}

impl Default for MoveList {
    #[inline]
    fn default() -> Self {
        MoveList {
            inner: [Move::null(); MAX_MOVES],
            len: 0,
        }
    }
}

impl From<Vec<Move>> for MoveList {
    fn from(vec: Vec<Move>) -> Self {
        let mut list = MoveList::default();
        vec.iter().for_each(|m| list.push(*m));
        list
    }
}

impl Into<Vec<Move>> for MoveList {
    #[inline]
    fn into(self) -> Vec<Move> {
        self.vec()
    }
}

pub struct MoveListIterator {
    movelist: MoveList,
    cursor: usize,
}

impl IntoIterator for MoveList {
    type Item = Move;
    type IntoIter = MoveListIterator;

    fn into_iter(self) -> Self::IntoIter {
        Self::IntoIter {
            movelist: self,
            cursor: 0,
        }
    }
}

impl Iterator for MoveListIterator {
    type Item = Move;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor < self.movelist.len {
            unsafe {
                let mov = self.movelist.inner.get_unchecked(self.cursor);
                self.cursor += 1;
                Some(*mov)
            }
        } else {
            None
        }
    }
}

impl MoveList {
    /// Returns true if empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Create a `Vec<Move>` from this `MoveList`.
    pub fn vec(&self) -> Vec<Move> {
        let mut vec = Vec::with_capacity(self.len);
        for mov in self.iter() {
            vec.push(*mov);
        }
        assert_eq!(vec.len(), self.len);
        vec
    }

    /// Return the number of moves inside the list.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[Move] {
        self
    }
}

impl Deref for MoveList {
    type Target = [Move];

    #[inline]
    fn deref(&self) -> &[Move] {
        unsafe {
            let p = self.inner.as_ptr();
            slice::from_raw_parts(p, self.len)
        }
    }
}

impl DerefMut for MoveList {
    #[inline]
    fn deref_mut(&mut self) -> &mut [Move] {
        unsafe {
            let p = self.inner.as_mut_ptr();
            slice::from_raw_parts_mut(p, self.len)
        }
    }
}

impl Index<usize> for MoveList {
    type Output = Move;

    #[inline(always)]
    fn index(&self, index: usize) -> &Move {
        &(**self)[index]
    }
}

impl IndexMut<usize> for MoveList {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut Move {
        &mut (**self)[index]
    }
}

impl MVPushable for MoveList {
    #[cfg(debug_assertions)]
    #[inline(always)]
    fn push(&mut self, mv: Move) {
        self.push_mv(mv);
    }
    #[cfg(not(debug_assertions))]
    #[inline(always)]
    fn push(&mut self, mv: Move) {
        unsafe {
            self.unchecked_push_mv(mv);
        }
    }
    #[inline(always)]
    fn push_mv(&mut self, mv: Move) {
        if self.len() < MAX_MOVES {
            unsafe { self.unchecked_push_mv(mv) }
        }
    }

    #[inline(always)]
    unsafe fn unchecked_push_mv(&mut self, mv: Move) {
        let end = self.inner.get_unchecked_mut(self.len);
        *end = mv;
        self.len += 1;
    }

    #[inline(always)]
    unsafe fn unchecked_set_len(&mut self, len: usize) {
        self.len = len
    }

    #[inline(always)]
    unsafe fn list_ptr(&mut self) -> *mut Move {
        self.as_mut_ptr()
    }

    #[inline(always)]
    unsafe fn over_bounds_ptr(&mut self) -> *mut Move {
        self.as_mut_ptr().add(self.len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn move_list_is_1024_bytes() {
        assert_eq!(mem::size_of::<MoveList>(), 1024);
    }
}
