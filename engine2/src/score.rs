//! Type and trait implementations for representing the score of a chess position. It's limiting to
//! use just a single i32 to represent scores for positions, since we want to also track depth to
//! mate in provably mate-in-N positions. This is so that we can pick the move with most resistance
//! if we are defending, or the move with fastest mate when we are attacking.
//!
//! Essentially, we want an order that looks like the following:
//! -∞, -#1, -#2, ..., -#100, ..., -9,999, -9,998, ..., -2, -1, 0, 1, 2, ..., 9,998, 9,999, ...,
//! #100, #99, ..., #2, #1, ∞
//!
//! In other words:
//! * There is a negative infinity, for use comparing against everything else during search
//! * Negative infinity is smaller than everything else
//! * There are negative mates, meaning the player to move is getting mated
//! * A smaller distance to mate is worse for the player getting mated
//! * There are centipawn evaluations, which are i32 integers
//! * Negative centipawn evaluations are bad for the player to move, positive are good; 0 is
//! drawish
//! * There are positive mates, meaning the player to move is mating the opponent
//! * Shorter distance to mate is better in positive mates, because we want to get on and win asap

use std::cmp::{Eq, Ord, Ordering, PartialEq, PartialOrd};
use std::ops::Neg;

/// Represents the score of a position.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Score {
    /// Negative infinity. This is only included to be used as a starting point for when we loop
    /// over the legal moves in a position. We want every move to have a value greater than this,
    /// so that it can be progressively increased.
    InfN,
    /// Positive infinity. This is need so that we can negate `InfN`, particularly in places like
    /// recursive alpha-beta calls.
    InfP,
    /// A centipawn evaluation.
    Cp(i32),
    /// A mate-in-N position. Here, N refers to _ply_ depth to mate, not _full move_ depth to mate,
    /// as is common in chess literature.
    Mate(i8),
}

impl Score {
    /// Increment the depth to mate if this is a mate score. Otherwise, leave.
    ///
    /// This is useful in search routines where we recursively call and need to increment the depth
    /// to mate from the parent position.
    pub fn inc_mate(self) -> Self {
        match self {
            Score::Mate(n) if n >= 0 => Score::Mate(n + 1),
            Score::Mate(n) if n < 0 => Score::Mate(n - 1),
            Score::Mate(_) => unreachable!("all variants covered above"),
            Score::Cp(v) => Score::Cp(v),
            Score::InfN => Score::InfN,
            Score::InfP => Score::InfP,
        }
    }
}

impl PartialOrd for Score {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Score {
    fn cmp(&self, other: &Self) -> Ordering {
        match (*self, *other) {
            (Score::InfN, Score::InfN) => Ordering::Equal,
            (Score::InfN, _) => Ordering::Less, // InfN is less than everything but itself
            (Score::InfP, Score::InfP) => Ordering::Equal,
            (Score::InfP, _) => Ordering::Greater, // InfP is greater than everything but itself
            (Score::Cp(_), Score::InfN) => Ordering::Greater,
            (Score::Cp(_), Score::InfP) => Ordering::Less,
            (Score::Cp(v1), Score::Cp(v2)) => v1.cmp(&v2),
            (Score::Cp(_), Score::Mate(m)) if m > 0 => Ordering::Less,
            (Score::Cp(_), Score::Mate(m)) if m < 0 => Ordering::Greater,
            (Score::Cp(_), Score::Mate(_)) => unreachable!("should never have a Score::Mate(0)"),
            (Score::Mate(_), Score::InfN) => Ordering::Greater,
            (Score::Mate(_), Score::InfP) => Ordering::Less,
            (Score::Mate(m), Score::Cp(_)) if m > 0 => Ordering::Greater,
            (Score::Mate(m), Score::Cp(_)) if m < 0 => Ordering::Less,
            (Score::Mate(_), Score::Cp(_)) => unreachable!("should never have a Score::Mate(0)"),
            (Score::Mate(m1), Score::Mate(m2)) if m1 > 0 && m2 < 0 => Ordering::Greater,
            (Score::Mate(m1), Score::Mate(m2)) if m1 > 0 && m2 > 0 => m2.cmp(&m1),
            (Score::Mate(m1), Score::Mate(m2)) if m1 < 0 && m2 < 0 => m2.cmp(&m1),
            (Score::Mate(m1), Score::Mate(m2)) if m1 < 0 && m2 > 0 => Ordering::Less,
            (Score::Mate(_), Score::Mate(_)) => unreachable!("should never have a Score::Mate(0)"),
        }
    }
}

impl Neg for Score {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            Score::InfN => Score::InfP,
            Score::InfP => Score::InfN,
            Score::Cp(v) => Score::Cp(-v),
            Score::Mate(n) => Score::Mate(-n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert!(Score::InfN < Score::Cp(-3));
        assert!(Score::InfN < Score::Cp(0));
        assert!(Score::InfN < Score::Cp(999));
        assert!(Score::Cp(999) > Score::InfN);
        assert!(Score::Mate(-3) < Score::Mate(3));
        assert!(Score::InfN == Score::InfN);
        assert!(Score::Mate(3) == Score::Mate(3));
        assert!(Score::Mate(3) > Score::Mate(4));
        assert!(Score::Mate(-44) > Score::Mate(-2)); // "If we must get mated, it's better for it
                                                     // to take a long time."
        assert!(Score::Cp(-10) > Score::Mate(-4));
        assert!(Score::Cp(-10) < Score::Mate(4));
        assert!(Score::Mate(1) > Score::Cp(300));
    }
}
