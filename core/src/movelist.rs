/// Data structure to store a collection `Move`s. This is used for move generation
/// and exists to avoid using `Vec<Move>`, which would live on the heap. `ArrayVec`
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
use std::fmt::Debug;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut, Index, IndexMut, Range};
use std::slice;
use std::slice::Iter;

use crate::mov::Move;

/// 254 is chosen so that the total size of the `ArrayVec` struct is exactly 1024
/// bytes, taking account of the `len` field.
pub const MAX_MOVES: usize = 254;

/// Trait to generalize operations on structures containing a collection of `Move`s.
pub trait MoveList: Debug {
    /// Create an empty move list.
    fn empty() -> Self;
    /// Add a `Move` to the end of the list.
    fn push(&mut self, mv: Move);
    /// The length of the move list.
    fn len(&self) -> usize;
    /// Clear the list.
    fn clear(&mut self);
}

pub type BasicMoveList = ArrayVec<Move, MAX_MOVES>;

/// A container for `T: Copy` which lives on the stack and has a fixed maximum size (default 254).
///
/// If you attempt to push more than 254 items into the list, the `push` will fail silently rather
/// than panic. The idea is that no chess position should ever have so many moves, and if by some
/// miracle it does, the best move is hopefully already in the first 254(!) so there's no need for
/// the program to die.
///
/// This approach seems to be the fastest way of working with lists of `Move`s because it doesn't
/// allocate and it doesn't have any handling for
///
/// Note: we do not have any `Drop` implementation, but if `Move` needed to be dropped we would
/// need to think about this.
#[derive(Debug)]
pub struct ArrayVec<T, const N: usize> {
    inner: [MaybeUninit<T>; N],
    len: usize,
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
            inner: MaybeUninit::uninit_array(),
            len: 0,
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

impl<T, const N: usize> Into<Vec<T>> for ArrayVec<T, N>
where
    T: Copy,
{
    #[inline]
    fn into(self) -> Vec<T> {
        self.vec()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a ArrayVec<T, N> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        unsafe { MaybeUninit::slice_assume_init_ref(self.inner.get_unchecked(0..self.len)).iter() }
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a mut ArrayVec<T, N> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        unsafe {
            MaybeUninit::slice_assume_init_mut(self.inner.get_unchecked_mut(0..self.len)).iter_mut()
        }
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
        self.len == 0
    }

    /// Return the number of elements in the list.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Unsafely set the length to `len`. This is unsafe because no invariants are checked. The
    /// caller must ensure that any use of this function maintains the invariant that all values
    /// up to `len` are properly initialized.
    #[inline(always)]
    pub unsafe fn set_len(&mut self, len: usize) {
        self.len = len;
    }

    /// Get the `ArrayVec` as a slice, `&[T]`.
    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        self
    }

    /// Return a random element from the list.
    #[inline]
    pub fn random(&self) -> Option<&T> {
        let mut rng = thread_rng();
        rng.choose(self.as_slice())
    }

    /// Add a `T` to the end of the list, without checking bounds.
    #[inline(always)]
    pub unsafe fn unchecked_push_mv(&mut self, t: T) {
        let end = self.inner.get_unchecked_mut(self.len);
        end.write(t);
        self.len += 1;
    }

    /// Return a mut pointer to the first (0th index) element in the list.
    #[inline(always)]
    pub unsafe fn list_ptr_mut(&mut self) -> *mut T {
        self.as_mut_ptr()
    }

    /// Return a const pointer to the first (0th index) element in the list.
    #[inline(always)]
    pub unsafe fn list_ptr(&self) -> *const T {
        self.as_ptr()
    }

    /// Return a mut pointer to the element next to the last element in the list.
    #[inline(always)]
    pub unsafe fn over_bounds_ptr_mut(&mut self) -> *mut T {
        MaybeUninit::slice_as_mut_ptr(&mut self.inner).add(self.len)
        // self.as_mut_ptr().add(self.len)
    }

    /// Return a const pointer to the element next to the last element in the list.
    #[inline(always)]
    pub unsafe fn over_bounds_ptr(&self) -> *const T {
        MaybeUninit::slice_as_ptr(&self.inner).add(self.len)
        // self.inner.as_slice().as_ptr().add(self.len)
    }

    /// Push a `T` to the end of the list.
    #[inline(always)]
    pub fn push_val(&mut self, val: T) {
        if self.len() < MAX_MOVES {
            unsafe { self.unchecked_push_mv(val) }
        }
    }

    /// Clear the `ArrayVec`.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Return a slice over a specified range, without performing bounds checking. Undefined
    /// behaviour will result if the range is out of bounds, so the caller must ensure it is valid.
    ///
    /// This is a faster alternative to using `Index<Range<usize>>` because the latter is a safe
    /// API and would require bounds checking.
    #[inline(always)]
    pub unsafe fn get_slice_unchecked(&self, rng: Range<usize>) -> &[T] {
        let p = self.inner.as_ptr();
        let start = p.add(rng.start);
        MaybeUninit::slice_assume_init_ref(slice::from_raw_parts(start, rng.len()))
    }

    /// Return a mutable slice over a specified range, without performing bounds checking. Undefined
    /// behaviour will result if the range is out of bounds, so the caller must ensure it is valid.
    ///
    /// This is a faster alternative to using `Index<Range<usize>>` because the latter is a safe
    /// API and would require bounds checking.
    #[inline(always)]
    pub unsafe fn get_slice_mut_unchecked(&mut self, rng: Range<usize>) -> &mut [T] {
        let p = self.inner.as_mut_ptr();
        let start = p.add(rng.start);
        MaybeUninit::slice_assume_init_mut(slice::from_raw_parts_mut(start, rng.len()))
    }
}

impl<T, const N: usize> ArrayVec<T, N>
where
    T: Copy,
{
    /// Create a `Vec<T>` from this `ArrayVec`.
    pub fn vec(&self) -> Vec<T> {
        self.into_iter().map(|v| *v).collect()
    }
}

impl<T, const N: usize> Deref for ArrayVec<T, N> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &[T] {
        unsafe {
            let p = self.inner.as_ptr();
            MaybeUninit::slice_assume_init_ref(slice::from_raw_parts(p, self.len))
        }
    }
}

impl<T, const N: usize> DerefMut for ArrayVec<T, N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [T] {
        unsafe {
            let p = self.inner.as_mut_ptr();
            MaybeUninit::slice_assume_init_mut(slice::from_raw_parts_mut(p, self.len))
        }
    }
}

impl<T, const N: usize> Index<usize> for ArrayVec<T, N> {
    type Output = T;

    #[inline(always)]
    fn index(&self, index: usize) -> &T {
        if index >= self.len {
            panic!(
                "index out of bounds; the len is {} but the index is {}",
                self.len, index
            );
        }

        // SAFETY: this is initialized and `Some` given the bounds check above.
        unsafe { self.inner.get(index).unwrap().assume_init_ref() }
    }
}

impl<T, const N: usize> IndexMut<usize> for ArrayVec<T, N> {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut T {
        if index >= self.len {
            panic!(
                "index out of bounds; the len is {} but the index is {}",
                self.len, index
            );
        }

        // SAFETY: this is initialized and `Some` given the bounds check above.
        unsafe { self.inner.get_mut(index).unwrap().assume_init_mut() }
    }
}

impl MoveList for ArrayVec<Move, MAX_MOVES> {
    #[inline(always)]
    fn empty() -> Self {
        Self::new()
    }

    #[cfg(debug_assertions)]
    #[inline(always)]
    fn push(&mut self, mv: Move) {
        self.push_val(mv);
    }
    #[cfg(not(debug_assertions))]
    #[inline(always)]
    fn push(&mut self, mv: Move) {
        unsafe {
            self.unchecked_push_mv(mv);
        }
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.clear();
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

const MAX_MOVES_FAST: usize = 54;

/// An attempt at a fast `MoveList` implementation which doesn't take too much space.
#[derive(Debug)]
pub struct FastMoveList {
    /// The main move storage. This can hold up to 54 moves, which is almost always enough. In very
    /// rare case, we need more than this and overflow into an allocated `Vec`.
    moves: [MaybeUninit<Move>; MAX_MOVES_FAST],
    /// The size of the total move list, including overflow.
    len: usize,
    /// Overflow storage, initialized to zero capacity and rarely used.
    overflow: MaybeUninit<Vec<Move>>,
}

impl MoveList for FastMoveList {
    #[inline(always)]
    fn empty() -> Self {
        FastMoveList {
            moves: [MaybeUninit::uninit(); MAX_MOVES_FAST],
            len: 0,
            overflow: MaybeUninit::uninit(),
        }
    }

    #[inline(always)]
    fn push(&mut self, mv: Move) {
        if self.len >= MAX_MOVES_FAST {
            if self.len == MAX_MOVES_FAST {
                self.overflow = MaybeUninit::new(Vec::with_capacity(16));
            } else {
                // SAFETY: we initialized the `Vec` on the previous `push` in the branch above.
                unsafe {
                    (*self.overflow.assume_init_mut()).push(mv);
                }
            }

            self.len += 1;
        } else {
            // SAFETY: We have already checked that `self.len` is in bounds.
            unsafe {
                *self.moves.get_unchecked_mut(self.len) = MaybeUninit::new(mv);
                self.len += 1;
            }
        }
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    fn clear(&mut self) {
        if self.len() >= MAX_MOVES_FAST {
            // SAFETY: the bounds check implies that the `Vec` has been initialized.
            unsafe {
                self.overflow.assume_init_mut().clear();
            }
        }
        self.len = 0;
    }
}

pub struct FastMoveIter<'a> {
    movelist: &'a FastMoveList,
    cursor: *const MaybeUninit<Move>,
    end: *const MaybeUninit<Move>,
    overflow_iter: MaybeUninit<Iter<'a, Move>>,
    in_overflow: bool,
}

impl<'a> IntoIterator for &'a FastMoveList {
    type Item = &'a Move;
    type IntoIter = FastMoveIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        let cursor = (&self.moves).as_ptr();

        let end = if self.len() > MAX_MOVES_FAST {
            // SAFETY: since the overall move list is longer than the main storage, we can offset
            // to the end of main storage. We are offsetting one past the end of the array, which
            // is a bit dangerous, but we will never dereference this.
            unsafe { cursor.offset(MAX_MOVES_FAST as isize) }
        } else {
            // SAFETY: we did a bounds check to ensure that the end is inside the main array
            // storage, so this pointer offset is valid.
            unsafe { cursor.offset(self.len() as isize) }
        };

        FastMoveIter {
            movelist: &self,
            cursor,
            end,
            overflow_iter: MaybeUninit::uninit(),
            in_overflow: false,
        }
    }
}

impl<'a> Iterator for FastMoveIter<'a> {
    type Item = &'a Move;

    fn next(&mut self) -> Option<Self::Item> {
        if self.in_overflow {
            unsafe { self.overflow_iter.assume_init_mut().next() }
        } else {
            if self.cursor == self.end {
                if self.movelist.len() > MAX_MOVES_FAST {
                    // SAFETY: given the bounds check, we know the overflow has been initialized
                    unsafe {
                        self.overflow_iter =
                            MaybeUninit::new(self.movelist.overflow.assume_init_ref().iter());
                    }

                    self.in_overflow = true;

                    // SAFETY: we have just initialized this above
                    unsafe { self.overflow_iter.assume_init_mut().next() }
                } else {
                    None
                }
            } else {
                // SAFETY: given the bounds check, we can dereference
                unsafe {
                    let m = (*self.cursor).assume_init_ref();
                    self.cursor = self.cursor.offset(1);
                    Some(m)
                }
            }
        }
    }
}

/// A structure maintaining contiguous memory for many `MoveList`s. Used to provide fast and
/// reliable move iteration at each ply in a search process, without allocating each time or having
/// a very large fixed-length array on the program stack.
#[derive(Debug)]
pub struct MoveStack {
    /// Raw storage for the moves.
    data: Vec<Move>,
}

impl MoveStack {
    /// Create a new `MoveStack`.
    pub fn new() -> Self {
        Self {
            data: Vec::with_capacity(1_024),
        }
    }

    /// Get a new `Frame` for a `MoveStack`. This represents an empty `MoveList`.
    ///
    /// This is the only way to get a `Frame`.
    pub fn new_frame<'a, 's>(&'s mut self) -> Frame<'a> {
        let end = self.data.as_mut_ptr_range().end;

        Frame {
            movestack: self as *mut Self,
            rng: Range { start: end, end },
            _marker: PhantomData,
        }
    }
}

/// A window into the `MoveStack`, representing a single iterable collection of moves.
#[derive(Debug)]
pub struct Frame<'a> {
    /// Raw pointer to the underlying `MoveStack`.
    movestack: *mut MoveStack,
    /// The raw pointers defining the range of `Move`s in this `Frame`.
    rng: Range<*mut Move>,
    _marker: PhantomData<&'a ()>,
}

impl Frame<'_> {
    #[inline(always)]
    fn movestack(&self) -> &mut MoveStack {
        // SAFETY: well, strictly this isn't safe. It's possible that the MoveStack has gone.
        // TODO: make sure it is properly safe. This basically means that we have to tie the
        // lifetime of the `Frame` to the lifetime of the `MoveStack`. In `Search`, we have given
        // the whole search process a lifetime `'search`. We would like to have the compiler know
        // that the `MoveStack` field on `Search` will never be reassigned during `'search`. What's
        // the way to do that?
        //
        // I think the way to do it involves having a second lifetime parameter `'ms` on `Frame`
        // which refers to the lifetime of the underlying movestack. Therefore `'ms: 'a` is a bound.
        // I think we could then build the `Search` struct by consuming a `&'ms mut MoveStack` from
        // outside. But this gets a bit ugly since we have no real need to do make the `MoveStack`
        // separately from the search process. Can we use an `Inner` struct to manage that problem?
        //
        // Even if we don't do all the above, for practical purposes in Seaborg, this should never
        // actually crash because we use `MoveStack` in a limited way here. But it would be nice to
        // be guaranteed safe so things can be reused freely in the future without remembering
        // about all this..!
        unsafe { &mut *self.movestack }
    }
}

impl<'a> MoveList for Frame<'a> {
    #[inline(always)]
    fn empty() -> Self {
        // TODO: it would be nice to remove the requirement for this method from the trait
        // interface so that we don't need to panic.
        panic!(
            "not allowed to create an empty `Frame` without reference to an underlying `MoveStack`"
        );
    }

    #[inline(always)]
    fn push(&mut self, mv: Move) {
        // This pushes the move into the underlying `MoveStack`.
        // We want an invariant like:
        //   "this has to be the top frame of the stack, otherwise we couldn't have a mutable
        //   reference to it"
        //
        // So when we iterate, we need to iterate over `&Frame`. The iterator's existence means
        // that we can't push onto that `Frame` until it's dropped. Creating an iterator pushes a
        // new `Frame` onto the `MoveStack`.
        self.movestack().data.push(mv);
        self.rng.end = unsafe { self.rng.end.add(1) };
    }

    #[inline(always)]
    fn len(&self) -> usize {
        // Look at the start and end pointers on the `MoveStack`
        unsafe {
            self.rng
                .end
                .offset_from(self.rng.start)
                .try_into()
                .expect("we shouldn't have long move lists")
        }
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.rng.end = self.rng.start.clone();
    }
}

#[derive(Debug)]
pub struct FrameIter<'a> {
    cursor: *mut Move,
    frame: &'a Frame<'a>,
    _marker: PhantomData<&'a ()>,
}

impl<'a, 'b: 'a> IntoIterator for &'b Frame<'a> {
    type Item = &'a Move;
    type IntoIter = FrameIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        FrameIter {
            cursor: self.rng.start,
            frame: &self,
            _marker: PhantomData,
        }
    }
}

impl<'a, 'b> Iterator for FrameIter<'a> {
    type Item = &'a Move;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.frame.rng.end {
            None
        } else {
            let m = unsafe { &*self.cursor };
            self.cursor = unsafe { self.cursor.add(1) };
            Some(m)
        }
    }
}

impl<'a> Drop for Frame<'a> {
    fn drop(&mut self) {
        // move the internal vec end cursor to `self.rng.start` and adjust its len
        let frame_len = self.len();
        let data_len = self.movestack().data.len();
        self.movestack().data.truncate(data_len - frame_len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::mem;

    #[test]
    fn basic_move_list_is_1024_bytes() {
        assert_eq!(mem::size_of::<ArrayVec<Move, 254>>(), 1024);
    }
}
