/// Data structure to store a collection `Move`s. This is used for move generation
/// and exists to avoid using `Vec<Move>`, which would live on the heap. `ArrayVec`
/// uses a fixed size array structure, and so lives on the stack. We can do this
/// because it is (believed to be) the case that no chess position has more than
/// 218 legal moves, so we will never overflow the bounds if used correctly.
///
use rand::{thread_rng, Rng};

use std::fmt;
use std::fmt::Debug;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::slice;

use crate::mov::Move;

/// 254 is chosen so that the total size of the `ArrayVec` struct is exactly 1024
/// bytes, taking account of the `len` field.
pub const MAX_MOVES: usize = 254;

/// Trait to generalize operations on structures containing a collection of `Move`s.
pub trait MoveList: Debug {
    /// Create an empty move list.
    fn empty() -> Self;
    /// Add a `Move` to the end of the list. Fixed-capacity implementations ignore the move when
    /// the list is full.
    fn push(&mut self, mv: Move);
    /// The length of the move list.
    fn len(&self) -> usize;
    /// Clear the list.
    fn clear(&mut self);
}

/// Move-generation storage specialized for the hot perft path. This intentionally avoids both
/// initialization of the 1 KiB backing array and a destructor on every node. Pushes beyond the
/// capacity are ignored.
#[derive(Debug)]
pub struct HotArrayVec<T, const N: usize> {
    inner: [MaybeUninit<T>; N],
    len: usize,
}

pub type BasicMoveList = HotArrayVec<Move, MAX_MOVES>;

impl<T, const N: usize> HotArrayVec<T, N> {
    #[inline]
    pub fn random(&self) -> Option<&T> {
        let mut rng = thread_rng();
        rng.choose(self)
    }
}

impl<T, const N: usize> Default for HotArrayVec<T, N> {
    #[inline(always)]
    fn default() -> Self {
        Self {
            inner: [const { MaybeUninit::uninit() }; N],
            len: 0,
        }
    }
}

impl<T, const N: usize> Deref for HotArrayVec<T, N> {
    type Target = [T];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        // SAFETY: `MoveList::push` initializes every element below `len`, and `len` never exceeds
        // the backing array's capacity.
        unsafe { slice::from_raw_parts(self.inner.as_ptr().cast::<T>(), self.len) }
    }
}

impl<T, const N: usize> DerefMut for HotArrayVec<T, N> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: the same initialized-prefix invariant applies, and `&mut self` guarantees unique
        // access to the returned slice.
        unsafe { slice::from_raw_parts_mut(self.inner.as_mut_ptr().cast::<T>(), self.len) }
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a HotArrayVec<T, N> {
    type Item = &'a T;
    type IntoIter = slice::Iter<'a, T>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        // SAFETY: the initialized-prefix invariant guarantees the entire range contains moves.
        unsafe {
            self.inner
                .get_unchecked(0..self.len)
                .assume_init_ref()
                .iter()
        }
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a mut HotArrayVec<T, N> {
    type Item = &'a mut T;
    type IntoIter = slice::IterMut<'a, T>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        // SAFETY: the initialized-prefix invariant guarantees the entire range contains moves.
        unsafe {
            self.inner
                .get_unchecked_mut(0..self.len)
                .assume_init_mut()
                .iter_mut()
        }
    }
}

/// A stack-allocated vector with fixed capacity. Pushes beyond the capacity are ignored, matching
/// the engine's historical behavior for the impossible-in-practice case of more than 254 moves.
#[derive(Debug)]
#[repr(align(8))]
pub struct ArrayVec<T, const N: usize> {
    inner: arrayvec::ArrayVec<T, N>,
}

impl<T, const N: usize> fmt::Display for ArrayVec<T, N>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for item in self {
            write!(f, "{}, ", item)?;
        }
        writeln!(f, "]")
    }
}

impl<T, const N: usize> Default for ArrayVec<T, N> {
    #[inline]
    fn default() -> Self {
        ArrayVec {
            inner: arrayvec::ArrayVec::new(),
        }
    }
}

impl<T, const N: usize> From<Vec<T>> for ArrayVec<T, N> {
    fn from(vec: Vec<T>) -> Self {
        let mut list = ArrayVec::<T, N>::default();
        vec.into_iter().for_each(|v| list.push_val(v));
        list
    }
}

impl<T, const N: usize> From<ArrayVec<T, N>> for Vec<T> {
    #[inline]
    fn from(value: ArrayVec<T, N>) -> Self {
        value.inner.into_iter().collect()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a ArrayVec<T, N> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a mut ArrayVec<T, N> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter_mut()
    }
}

impl<T, const N: usize> ArrayVec<T, N> {
    /// Create an empty `ArrayVec`.
    #[inline(always)]
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns true if empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Return the number of elements in the list.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Get the `ArrayVec` as a slice, `&[T]`.
    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        self.inner.as_slice()
    }

    /// Get the `ArrayVec` as a mutable slice, `&mut [T]`.
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.inner.as_mut_slice()
    }

    /// Return a random element from the list.
    #[inline]
    pub fn random(&self) -> Option<&T> {
        let mut rng = thread_rng();
        rng.choose(self.as_slice())
    }

    /// Push a `T` to the end of the list.
    #[inline(always)]
    pub fn push_val(&mut self, val: T) {
        if self.inner.len() < N {
            // SAFETY: capacity was checked immediately above. Using `try_push` here materially
            // slows scored move insertion because its error path returns the rejected value.
            unsafe { self.inner.push_unchecked(val) }
        }
    }

    /// Clear the `ArrayVec`.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<T, const N: usize> ArrayVec<T, N>
where
    T: Copy,
{
    /// Create a `Vec<T>` from this `ArrayVec`.
    pub fn vec(&self) -> Vec<T> {
        self.as_slice().to_vec()
    }
}

impl<T, const N: usize> Deref for ArrayVec<T, N> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T, const N: usize> DerefMut for ArrayVec<T, N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl MoveList for HotArrayVec<Move, MAX_MOVES> {
    #[inline(always)]
    fn empty() -> Self {
        Self::default()
    }

    #[inline(always)]
    fn push(&mut self, mv: Move) {
        if self.len < MAX_MOVES {
            // SAFETY: capacity was checked immediately above. Avoiding initialization and checked
            // push/drop overhead is measurable in perft.
            unsafe { self.inner.get_unchecked_mut(self.len).write(mv) };
            self.len += 1;
        }
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.len = 0;
    }
}

/// A type implementing `MoveList` which based on a `Vec`.
#[derive(Debug)]
pub struct OverflowingMoveList(Vec<Move>);

impl MoveList for OverflowingMoveList {
    #[inline(always)]
    fn empty() -> Self {
        Self(Vec::new())
    }

    #[inline(always)]
    fn push(&mut self, mv: Move) {
        self.0.push(mv);
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.0.len()
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.0.clear()
    }
}

impl Index<usize> for OverflowingMoveList {
    type Output = Move;

    #[inline(always)]
    fn index(&self, index: usize) -> &Move {
        &*self.0.index(index)
    }
}

impl IndexMut<usize> for OverflowingMoveList {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut Move {
        &mut *self.0.index_mut(index)
    }
}

impl<'a> IntoIterator for &'a OverflowingMoveList {
    type Item = &'a Move;
    type IntoIter = std::slice::Iter<'a, Move>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::init::init_globals;
    use crate::mono_traits::{All, Legal};
    use crate::position::Position;
    use std::mem;

    #[test]
    fn basic_move_list_is_1024_bytes() {
        assert_eq!(mem::size_of::<BasicMoveList>(), 1024);
    }

    #[test]
    fn arrayvec_ignores_pushes_beyond_capacity() {
        let mut list = ArrayVec::<u8, 1>::new();
        list.push_val(1);
        list.push_val(2);

        assert_eq!(list.as_slice(), &[1]);
    }

    #[test]
    fn hot_arrayvec_ignores_pushes_beyond_capacity() {
        let mut list = BasicMoveList::empty();

        for _ in 0..MAX_MOVES {
            list.push(Move::null());
        }
        assert_eq!(list.len(), MAX_MOVES);

        list.push(Move::null());
        assert_eq!(list.len(), MAX_MOVES);
    }

    #[test]
    fn start_position_retains_all_legal_moves() {
        init_globals();
        let moves = Position::start_pos().generate::<BasicMoveList, All, Legal>();

        assert_eq!(moves.len(), 20);
    }
}
