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
///     to mate is better.
/// * -20_100 - -20_000 -> negative mate-in-N (i.e. the player to move is being mated)
///   * -20_100 represents mate-in-0, -20_099 represents mate-in-1 etc. This is so that longer depth
///     to mate is better.
/// * -30_000 -> negative infinity
/// * 30_000 -> positive infinity
///
/// Values between 10_000 and 20_000 in either direction, and anything beyond 30_000, name no
/// variant. `Debug` prints any value outside the bands above in raw `Score(n)` form rather than
/// guessing at a variant, so an unexpected one is legible as itself.
///
/// Two narrower ranges matter when reading search code, because they are not the same range:
///
/// * *Node scores* — what a searched node returns — occupy `mate(0) ..= mate(1)`, that is
///   -20_100 to 20_099. Scores are position-relative, so the worst a node can do is be mated now
///   and the best is to mate on the next ply; no larger mate distance can be reported from the
///   position it is measured against. [`Score::is_node_score`] tests this band, and both `search`
///   and `quiesce` clamp their windows into it before use.
/// * *Window bounds* range more widely, since a bound only has to be a threshold to compare
///   against, not a value anything can hold. They span the infinities, and in transit they reach
///   `Score(20_101)`: [`Score::child_bound`] is exact, so converting the parent bound `mate(0)`
///   yields one step past the top of the mate band. That value is consumed by the child's entry
///   clamp and never becomes a score.
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

    /// Convert a bound at the current position into the equivalent bound for a child position.
    ///
    /// Search scores mates relative to the position being searched. Returning from a child
    /// negates its score and increments its mate distance, so a window passed in the other
    /// direction must apply the inverse operation. Plain centipawn and infinity bounds only need
    /// negation.
    ///
    /// The transformation is exact, and deliberately so: it is the inverse of `neg().inc_mate()`,
    /// and callers rely on that to keep a null window null. Exactness means the result can sit one
    /// step outside the mate band — `child_bound(mate(0))` is `Score(20_101)`, asking for a value
    /// one better than mating on the next ply, which nothing can attain. Such a bound is
    /// meaningful as a threshold but is not a score. Callers clamp their windows into the node
    /// band on entry (see [`Self::is_node_score`]), which is what keeps the excursion from
    /// reaching a returned score or compounding across plies.
    pub fn child_bound(self) -> Self {
        if self.0 < -20_000 && self.0 > -30_000 {
            Score(-self.0 + 1)
        } else if self.0 > 20_000 && self.0 < 30_000 {
            Score(-self.0 - 1)
        } else {
            -self
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

    /// Construct a score representing `x` centipawns.
    pub fn cp(x: i16) -> Self {
        debug_assert!(x <= 10_000);
        debug_assert!(x >= -10_000);
        Score(x)
    }

    /// Convenience for `Score::cp(0)`.
    pub fn zero() -> Self {
        Self::cp(0)
    }

    pub fn to_i16(&self) -> i16 {
        self.0
    }

    /// Reconstruct a score from its compact transposition-table representation.
    ///
    /// Mate scores are stored in the transposition table without any ply adjustment. This engine
    /// scores mate *position-relative*: the checkmate leaf returns a constant `Score::mate(0)` and
    /// `inc_mate` accumulates the distance-to-mate on unwind, so a node's mate score is the
    /// distance from *that position* to mate and is independent of the ply at which the position
    /// is reached. A transposed position therefore carries the same intrinsic mate distance no
    /// matter its ply, so the stored value round-trips unchanged and needs no encode/decode step.
    pub(crate) fn from_i16(value: i16) -> Self {
        Self(value)
    }

    /// True if this `Score` represents a forced mate-in-n.
    pub fn is_mate(&self) -> bool {
        self.0 < -20_000 || self.0 > 20_000
    }

    /// True if this `Score` lies in the band a searched node can actually hold.
    ///
    /// Scores are position-relative, so at every node the worst attainable value is being mated
    /// now (`mate(0)`) and the best is mating on the next ply (`mate(1)`); no deeper mate can be
    /// reported from the position it is measured against. Window bounds are a different quantity
    /// and may sit outside this band, up to and including the infinities.
    pub fn is_node_score(&self) -> bool {
        Self::mate(0) <= *self && *self <= Self::mate(1)
    }

    /// True if this `Score` represents a centipawn evaluation.
    pub fn is_cp(&self) -> bool {
        -10_000 <= self.0 && self.0 <= 10_000
    }

    /// Increment the value of the score by 1, regardless of whether it is a centipawn score or a
    /// mate-in-N score. This is useful in search, where we have to create null windows by
    /// increasing alpha by 1.
    pub fn inc_one(&self) -> Self {
        Self(self.0 + 1)
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
        if self.is_cp() && !other.is_cp() || self.is_mate() && !other.is_mate() {
            // Incompatible score variants. Return self.
            self
        } else if self.is_cp() {
            Self((self.0 + other.0).clamp(-10_000, 10_000))
        } else if self.0 > 10_000 {
            Self((self.0 + other.0).clamp(10_000, 20_000))
        } else if self.0 < -10_000 {
            Self((self.0 + other.0).clamp(-20_000, -10_000))
        } else {
            unreachable!()
        }
    }
}

impl Sub for Score {
    type Output = Self;

    #[inline(always)]
    fn sub(self, other: Self) -> Self::Output {
        self + -other
    }
}

impl std::fmt::Debug for Score {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Every arm is guarded on both sides so that a value outside the encoding is reported as
        // such. Rendering it as the nearest plausible-looking variant is worse than useless: an
        // out-of-band `Score(20_101)` once printed as `Mate(-1)`, which reads as a perfectly
        // ordinary score and hides the very defect the reader is looking for.
        if self.0 == 30_000 {
            write!(f, "InfP")
        } else if self.0 == -30_000 {
            write!(f, "InfN")
        } else if (-20_100..=-20_000).contains(&self.0) {
            write!(f, "Mate(-{})", self.0 + 20_100)
        } else if (20_000..=20_100).contains(&self.0) {
            write!(f, "Mate({})", 20_100 - self.0)
        } else if self.is_cp() {
            write!(f, "Cp({})", self.0)
        } else {
            write!(f, "Score({})", self.0)
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

    #[test]
    fn child_bounds_invert_parent_mate_distance_conversion() {
        for score in [
            Score::mate(-8),
            Score::mate(-1),
            Score::cp(-42),
            Score::cp(42),
            Score::mate(1),
            Score::mate(9),
            Score::INF_N,
            Score::INF_P,
        ] {
            assert_eq!(score.child_bound().neg().inc_mate(), score);
        }
    }

    /// Every score a searched node can hold, in the parity the encoding actually admits: negative
    /// mates are reached at an even ply count and positive mates at an odd one, because the side
    /// being mated is the side to move only on alternate plies.
    fn node_scores() -> Vec<Score> {
        let mut scores: Vec<Score> = (-10_000..=10_000).map(Score::cp).collect();
        scores.extend((0..=100).step_by(2).map(|plies| Score::mate(-plies)));
        scores.extend((1..=99).step_by(2).map(Score::mate));
        scores
    }

    #[test]
    fn child_bounds_at_the_mate_band_boundaries_stay_exact() {
        // `mate(0)` is the bottom of the node score band, so its child-relative inverse asks for a
        // value one better than mating on the next ply. Nothing attains that, but the conversion
        // stays exact rather than saturating, because callers rely on it to map a null window to a
        // null window. Clamping the excursion is the entry clamp's job, not this function's.
        let bound = Score::mate(0).child_bound();
        assert_eq!(bound, Score::from_i16(20_101));
        assert!(!bound.is_node_score());
        assert_eq!(bound.neg().inc_mate(), Score::mate(0));
        assert!(bound > Score::mate(1));

        // The other boundary is one step inside the band and needs no special treatment.
        assert_eq!(Score::mate(1).child_bound(), Score::mate(0));
        assert_eq!(
            Score::mate(1).child_bound().neg().inc_mate(),
            Score::mate(1)
        );

        // A null window at the bottom of the band maps to a window entirely above the band. Both
        // ends are out of range, which is why the entry clamp has to clamp both ends inwards and
        // outwards rather than only towards the middle.
        let alpha = Score::mate(0);
        let beta = alpha.inc_one();
        assert_eq!(beta.child_bound(), Score::from_i16(20_100));
        assert_eq!(alpha.child_bound(), Score::from_i16(20_101));
        assert_eq!(
            beta.child_bound()
                .clamp(Score::mate(0), Score::mate(1))
                .child_bound(),
            Score::mate(0),
        );
    }

    #[test]
    fn every_node_score_is_in_band_and_formats_sensibly() {
        for score in node_scores() {
            assert!(score.is_node_score(), "{score:?} is outside the node band");

            // `Debug` falls back to raw form for anything it cannot name, so a non-raw rendering
            // is itself the assertion that the value sits in a documented band.
            let debug = format!("{score:?}");
            assert!(
                !debug.starts_with("Score("),
                "{debug} was not recognised as a score variant",
            );

            // `Display` carries the parity assertions that panicked the UCI driver thread in
            // TASK-54, so formatting each value is the check.
            let display = format!("{score}");
            assert!(
                display.starts_with("cp ") || display.starts_with("mate "),
                "unexpected UCI rendering {display}",
            );
        }
    }

    #[test]
    fn boundary_scores_render_with_the_right_mate_distance() {
        assert_eq!(format!("{:?}", Score::mate(0)), "Mate(-0)");
        assert_eq!(format!("{}", Score::mate(0)), "mate -0");
        assert_eq!(format!("{:?}", Score::mate(1)), "Mate(1)");
        assert_eq!(format!("{}", Score::mate(1)), "mate 1");
    }

    #[test]
    fn debug_reports_values_outside_the_encoding_in_raw_form() {
        // `Score(20_101)` used to render as `Mate(-1)`, which reads as an ordinary score and hides
        // exactly the excursion a reader would be looking for. The gap between the centipawn and
        // mate bands was likewise reported as a centipawn value.
        assert_eq!(
            format!("{:?}", Score::mate(0).child_bound()),
            "Score(20101)"
        );
        assert_eq!(format!("{:?}", Score::from_i16(-20_101)), "Score(-20101)");
        assert_eq!(format!("{:?}", Score::from_i16(15_000)), "Score(15000)");
        assert_eq!(format!("{:?}", Score::from_i16(-15_000)), "Score(-15000)");

        // Values the encoding does name are unaffected.
        assert_eq!(format!("{:?}", Score::INF_P), "InfP");
        assert_eq!(format!("{:?}", Score::INF_N), "InfN");
        assert_eq!(format!("{:?}", Score::cp(-42)), "Cp(-42)");
    }
}
