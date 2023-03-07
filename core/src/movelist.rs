/// Data structure to store a collection `Move`s. This is used for move generation
/// and exists to avoid using `Vec<Move>`, which would live on the heap. `BasicMoveList`
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
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::slice;
use std::slice::Iter;

use crate::mov::Move;

/// 254 is chosen so that the total size of the `BasicMoveList` struct is exactly 1024
/// bytes, taking account of the `len` field.
pub const MAX_MOVES: usize = 254;

/// Trait to generalize operations on structures containing a collection of `Move`s.
// pub trait MoveList: Index<usize, Output = Move> + IndexMut<usize, Output = Move> {
pub trait MoveList {
    /// Create an empty move list.
    fn empty() -> Self;
    /// Add a `Move` to the end of the list.
    fn push(&mut self, mv: Move);
    /// The length of the move list.
    fn len(&self) -> usize;
}

#[derive(Clone)]
pub struct BasicMoveList {
    inner: [Move; MAX_MOVES],
    len: usize,
}

impl fmt::Display for BasicMoveList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for i in 0..self.len() {
            write!(f, "{}, ", self.inner[i])?;
        }
        writeln!(f, "]")
    }
}

impl Default for BasicMoveList {
    #[inline]
    fn default() -> Self {
        BasicMoveList {
            inner: [Move::null(); MAX_MOVES],
            len: 0,
        }
    }
}

impl From<Vec<Move>> for BasicMoveList {
    fn from(vec: Vec<Move>) -> Self {
        let mut list = BasicMoveList::default();
        vec.iter().for_each(|m| list.push(*m));
        list
    }
}

impl Into<Vec<Move>> for BasicMoveList {
    #[inline]
    fn into(self) -> Vec<Move> {
        self.vec()
    }
}

impl<'a> IntoIterator for &'a BasicMoveList {
    type Item = &'a Move;
    type IntoIter = std::slice::Iter<'a, Move>;

    fn into_iter(self) -> Self::IntoIter {
        unsafe { self.inner.get_unchecked(0..self.len).iter() }
    }
}

impl BasicMoveList {
    /// Returns true if empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Create a `Vec<Move>` from this `MoveList`.
    pub fn vec(&self) -> Vec<Move> {
        self.into_iter().map(|m| *m).collect()
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

impl Deref for BasicMoveList {
    type Target = [Move];

    #[inline]
    fn deref(&self) -> &[Move] {
        unsafe {
            let p = self.inner.as_ptr();
            slice::from_raw_parts(p, self.len)
        }
    }
}

impl DerefMut for BasicMoveList {
    #[inline]
    fn deref_mut(&mut self) -> &mut [Move] {
        unsafe {
            let p = self.inner.as_mut_ptr();
            slice::from_raw_parts_mut(p, self.len)
        }
    }
}

impl Index<usize> for BasicMoveList {
    type Output = Move;

    #[inline(always)]
    fn index(&self, index: usize) -> &Move {
        &(**self)[index]
    }
}

impl IndexMut<usize> for BasicMoveList {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut Move {
        &mut (**self)[index]
    }
}

impl MoveList for BasicMoveList {
    #[inline(always)]
    fn empty() -> Self {
        Default::default()
    }

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
    fn len(&self) -> usize {
        self.len
    }
}

/// A type implementing `MoveList` which based on a `Vec`.
pub struct VecMoveList(Vec<Move>);

impl MoveList for VecMoveList {
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
}

impl Index<usize> for VecMoveList {
    type Output = Move;

    #[inline(always)]
    fn index(&self, index: usize) -> &Move {
        &*self.0.index(index)
    }
}

impl IndexMut<usize> for VecMoveList {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut Move {
        &mut *self.0.index_mut(index)
    }
}

impl<'a> IntoIterator for &'a VecMoveList {
    type Item = &'a Move;
    type IntoIter = std::slice::Iter<'a, Move>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

const MAX_MOVES_FAST: usize = 54;

/// An attempt at a fast `MoveList` implementation which doesn't take too much space.
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
    fn empty() -> Self {
        FastMoveList {
            moves: [MaybeUninit::uninit(); MAX_MOVES_FAST],
            len: 0,
            overflow: MaybeUninit::uninit(),
        }
    }

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

    fn len(&self) -> usize {
        self.len
    }
}

// impl Index<usize> for FastMoveList {
//     type Output = Move;
//
//     #[inline(always)]
//     fn index(&self, index: usize) -> &Move {
//         if index + 1 > self.len {
//             panic!(
//                 "index out of bounds; the index is {}, but the length is {}",
//                 index,
//                 self.len()
//             );
//         } else {
//             // SAFETY: we have done bounds checking above.
//             unsafe { (*self.moves.get_unchecked(index)).assume_init_ref() }
//         }
//     }
// }
//
// impl IndexMut<usize> for FastMoveList {
//     #[inline(always)]
//     fn index_mut(&mut self, index: usize) -> &mut Move {
//         if index >= MAX_MOVES_FAST {
//             &mut self.overflow[index - MAX_MOVES_FAST]
//         } else if index + 1 > self.len {
//             panic!(
//                 "index out of bounds; the index is {}, but the length is {}",
//                 index,
//                 self.len()
//             )
//         } else {
//             // SAFETY: we have done bounds checking above.
//             unsafe { (*self.moves.get_unchecked_mut(index)).assume_init_mut() }
//         }
//     }
// }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::Position;

    use std::mem;

    #[test]
    fn basic_move_list_is_1024_bytes() {
        assert_eq!(mem::size_of::<BasicMoveList>(), 1024);
    }

    #[test]
    fn fast_move_list() {
        crate::init::init_globals();

        let pos = Position::start_pos();

        let moves = pos.generate_moves::<FastMoveList>();

        for mov in &moves {
            println!("{}", mov);
        }
        println!("len: {}", moves.len());
    }
}
