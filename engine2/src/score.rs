use std::cmp::{Eq, Ord, PartialEq, PartialOrd};
use std::ops::{Add, Neg, Sub};

/// Represents the score of a position.
///
/// Our original naive implementation used an enum with 4 variants:
/// * InfN - representing negative infinity
/// * Mate(i8) - representing mate-in-N positions
/// * Cp(i32) - representing centipawn evaluations
/// * InfP - representing positive infinity
///
/// We had to write a rather complex custom implementation of `std::cmp::Ord` for this data
/// structure, in order to use it effectively as a score. This involved a match statement with lots
/// of arms to handle the full cartesian product of variants. This is too expensive for a structure
/// on the hot path. We can instead implement the whole thing with a single i16 and some judicious
/// choices of value.
///
/// * -10_000 - 10_000 -> centipawn evaluations
/// * 20_000 - 20_100 -> positive mate-in-N (i.e. the player to move is mating the opponent)
///   * 20_100 represents mate-in-0, 20_099 represents mate-in-1 etc. This is so that shorter depth
///   to mate is better.
/// * -20_100 - -20_000 -> negative mate-in-N (i.e. the player to move is being mated)
///   * -20_100 represents mate-in-0, -20_099 represents mate-in-1 etc. This is so that longer depth
///   to mate is better.
/// * -30_000 -> negative infinity
/// * 30_000 -> positive infinity
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Score(i16);

impl Score {
    /// Represents negative infinity.
    pub const INF_N: Score = Score(-30_000);

    /// Represents positive infinity.
    pub const INF_P: Score = Score(30_000);

    /// Increment the depth to mate if this is a mate score. Otherwise, leave.
    ///
    /// This is useful in search routines where we recursively call and need to increment the depth
    /// to mate from the parent position.
    pub fn inc_mate(self) -> Self {
        // We can't increment mate further when we are at mate-in-100 (represented by +/- 20_000).
        // But of course, this is never going to happen.
        debug_assert!(self.0 != 20_000);
        debug_assert!(self.0 != -20_000);

        if self.0.abs() >= 30_000 {
            self
        } else if self.0 < -20_000 {
            Score(self.0 + 1)
        } else if self.0 > 20_000 {
            Score(self.0 - 1)
        } else {
            self
        }
    }

    /// Construct a score representing a mate-in-`n`.
    pub fn mate(n: i8) -> Self {
        debug_assert!(n.abs() <= 100);

        if n > 0 {
            Score(20_100 - n as i16)
        } else {
            Score(-20_100 - n as i16)
        }
    }

    /// Construct a score representing a `x` centipawns.
    pub fn cp(x: i16) -> Self {
        debug_assert!(x < 10_000);
        debug_assert!(x > -10_000);
        Score(x)
    }

    /// Convenience for `Score::cp(0)`.
    pub fn zero() -> Self {
        Self::cp(0)
    }
}

impl Neg for Score {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl Add for Score {
    type Output = Self;

    #[inline(always)]
    fn add(self, other: Self) -> Self::Output {
        Self(self.0 + other.0)
    }
}

impl Sub for Score {
    type Output = Self;

    #[inline(always)]
    fn sub(self, other: Self) -> Self::Output {
        Self(self.0 - other.0)
    }
}

impl std::fmt::Debug for Score {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 30_000 {
            write!(f, "InfP")
        } else if self.0 == -30_000 {
            write!(f, "InfN")
        } else if self.0 < -20_000 {
            write!(f, "Mate(-{})", self.0 + 20_100)
        } else if self.0 > 20_000 {
            write!(f, "Mate({})", 20_100 - self.0)
        } else {
            write!(f, "Cp({})", self.0)
        }
    }
}

impl std::fmt::Display for Score {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 30_000 {
            write!(f, "+∞")
        } else if self.0 == -30_000 {
            write!(f, "-∞")
        } else if self.0 < -20_000 {
            let plies_to_mate = self.0 + 20_100;
            let moves_to_mate = plies_to_mate / 2;

            debug_assert!(plies_to_mate % 2 == 0); // When negative, the side to move is getting mated,
                                                   // so this should always be an even number of plies.

            write!(f, "mate -{}", moves_to_mate)
        } else if self.0 > 20_000 {
            let plies_to_mate = 20_100 - self.0;
            let moves_to_mate = (plies_to_mate + 1) / 2;

            debug_assert!(plies_to_mate % 2 == 1); // When positive, the opponent is getting mated,
                                                   // so this should always be an odd number of plies.

            write!(f, "mate {}", moves_to_mate)
        } else {
            write!(f, "cp {}", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert!(Score::INF_N < Score::cp(-3));
        assert!(Score::INF_N < Score::cp(0));
        assert!(Score::INF_N < Score::cp(999));
        assert!(Score::cp(999) > Score::INF_N);
        assert!(Score::mate(-3) < Score::mate(3));
        assert!(Score::INF_N == Score::INF_N);
        assert!(Score::mate(3) == Score::mate(3));
        assert!(Score::mate(3) > Score::mate(4)); // "It's better to mate the opponent in fewer
                                                  // moves"
        assert!(Score::mate(-44) > Score::mate(-2)); // "If we must get mated, it's better for it
                                                     // to take a long time."
        assert!(Score::cp(-10) > Score::mate(-4));
        assert!(Score::cp(-10) < Score::mate(4));
        assert!(Score::mate(1) > Score::cp(300));
        assert!(Score::cp(0) > Score::INF_N);
        assert!(Score::cp(0) < Score::INF_P);
    }
}
