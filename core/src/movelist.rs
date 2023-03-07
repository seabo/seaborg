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
use rand::{thread_rng, Rng};

use std::fmt;
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
}

#[derive(Clone)]
pub struct MoveList {
    inner: [Move; MAX_MOVES],
    len: usize,
}

impl fmt::Display for MoveList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for i in 0..self.len() {
            write!(f, "{}, ", self.inner[i])?;
        }
        writeln!(f, "]")
    }
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

impl<'a> IntoIterator for &'a MoveList {
    type Item = &'a Move;
    type IntoIter = std::slice::Iter<'a, Move>;

    fn into_iter(self) -> Self::IntoIter {
        unsafe { self.inner.get_unchecked(0..self.len).iter() }
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

    /// Get the `MoveList` as a slice, `&[Move]`.
    #[inline(always)]
    pub fn as_slice(&self) -> &[Move] {
        self
    }

    /// Return a random move from the list.
    #[inline]
    pub fn random(&self) -> Option<Move> {
        let mut rng = thread_rng();
        rng.choose(self.as_slice()).copied()
    }

    /// Add a `Move` to the end of the list, without checking bounds.
    #[inline(always)]
    pub unsafe fn unchecked_push_mv(&mut self, mv: Move) {
        let end = self.inner.get_unchecked_mut(self.len);
        *end = mv;
        self.len += 1;
    }

    /// Return a pointer to the first (0th index) element in the list.
    #[inline(always)]
    pub unsafe fn list_ptr(&mut self) -> *mut Move {
        self.as_mut_ptr()
    }

    /// Return a pointer to the element next to the last element in the list.
    #[inline(always)]
    pub unsafe fn over_bounds_ptr(&mut self) -> *mut Move {
        self.as_mut_ptr().add(self.len)
    }

    /// Add a `Move` to the end of the list.
    #[inline(always)]
    fn push_mv(&mut self, mv: Move) {
        if self.len() < MAX_MOVES {
            unsafe { self.unchecked_push_mv(mv) }
        }
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
