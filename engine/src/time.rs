use core::position::Player;

static AVERAGE_GAME_LENGTH: u64 = 40;
static MINIMUM_REMAINING_MOVES: u64 = 20;

/// Fixed safety margin held back from the clock, in milliseconds.
///
/// This covers the round trip that is not search: writing `bestmove`, the GUI or match runner
/// reading it, and scheduling jitter on the way back in. It is a property of the connection
/// rather than of the move, so it is deducted from the clock once, before any per-move slice is
/// taken. Deducting it per move instead made its relative cost grow without bound as the time
/// control shortened, which starved fast controls entirely.
static MOVE_OVERHEAD: u64 = 30;

/// Largest share of the usable clock a single move may be allotted, as a divisor.
///
/// A value of 4 caps one move at three quarters of the usable clock. This is what stops a large
/// increment or a `movestogo` of 1 from allotting more time than we actually hold, and it is the
/// only thing standing between us and a forfeit once the fixed buffer is no longer doing that job
/// by accident.
static MAX_CLOCK_SHARE_DIVISOR: u64 = 4;

#[derive(Clone, Debug)]
pub enum TimingMode {
    Timed(TimeControl),
    MoveTime(u64),
    Depth(u8),
    Infinite,
}

#[derive(Clone, Debug)]
pub struct TimeControl {
    /// Time remaning on white's clock, in milliseconds.
    wtime: u64,
    /// Time remaning on black's clock, in milliseconds.
    btime: u64,
    /// White's increment per move.
    winc: u64,
    /// Black's increment per move.
    binc: u64,
    /// Number of moves until the time control changes / is reset. If `None`, there no further time
    /// controls.
    moves_to_go: Option<u64>,
}

impl TimeControl {
    pub fn new(wtime: u64, btime: u64, winc: u64, binc: u64, moves_to_go: Option<u64>) -> Self {
        Self {
            wtime,
            btime,
            winc,
            binc,
            moves_to_go,
        }
    }

    /// Convert this time control into a fixed number of milliseconds we should allow searching
    /// for.
    pub fn to_move_time(&self, curr_move_number: u32, turn: Player) -> u64 {
        // An estimate for how many moves we expect to have to play with the time remaining on our
        // clock.
        let est_remaining_moves = match self.moves_to_go {
            Some(n) => n.max(1),
            None => AVERAGE_GAME_LENGTH
                .saturating_sub(curr_move_number.into())
                .max(MINIMUM_REMAINING_MOVES),
        };

        let base_time = if turn.is_white() {
            self.wtime
        } else {
            self.btime
        };

        let inc = if turn.is_white() {
            self.winc
        } else {
            self.binc
        };

        // Hold the safety margin back from the clock as a whole, once, before slicing it up. If
        // that exhausts the clock there is genuinely nothing left to spend; the search still
        // guarantees a completed first ply, so returning 0 here is safe.
        let usable_time = base_time.saturating_sub(MOVE_OVERHEAD);
        if usable_time == 0 {
            return 0;
        }

        // Our share of the clock for this move, plus the increment we will earn back by playing
        // it. Both terms scale with the time control, so the allocation degrades proportionally
        // as the clock shrinks rather than collapsing at a fixed threshold.
        let allocation = (usable_time / est_remaining_moves).saturating_add(inc);

        // Refuse to commit more than a fixed share of what we actually hold, however generous the
        // increment or `movestogo` estimate is. Written as a subtraction so it cannot overflow for
        // very large clocks.
        let max_allocation = usable_time - usable_time / MAX_CLOCK_SHARE_DIVISOR;

        // With time on the clock we always search for at least a moment; the clamp above keeps
        // this within `usable_time`.
        allocation.min(max_allocation).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_game_uses_minimum_remaining_move_estimate() {
        let control = TimeControl::new(20_000, 20_000, 0, 0, None);

        // (20_000 - 30) / 20, the minimum remaining-move estimate applying in both cases.
        assert_eq!(control.to_move_time(41, Player::WHITE), 998);
        assert_eq!(control.to_move_time(100, Player::BLACK), 998);
    }

    #[test]
    fn increment_contributes_to_allocation() {
        let control = TimeControl::new(1_000, 1_000, 200, 300, Some(10));

        // (1_000 - 30) / 10, plus the side's increment.
        assert_eq!(control.to_move_time(1, Player::WHITE), 297);
        assert_eq!(control.to_move_time(1, Player::BLACK), 397);
    }

    #[test]
    fn explicit_moves_to_go_controls_allocation_and_zero_is_safe() {
        let ten_moves = TimeControl::new(10_000, 10_000, 0, 0, Some(10));
        let zero_moves = TimeControl::new(10_000, 10_000, 0, 0, Some(0));

        assert_eq!(ten_moves.to_move_time(80, Player::WHITE), 997);
        // `movestogo 0` is treated as one move, so the share cap is what binds: three quarters of
        // the usable clock rather than the whole of it.
        assert_eq!(zero_moves.to_move_time(80, Player::WHITE), 7_478);
    }

    #[test]
    fn allocation_preserves_values_above_u32_max() {
        let control = TimeControl::new(u64::from(u32::MAX) * 40, 0, 0, 0, Some(20));

        // (u32::MAX * 40 - 30) / 20. The point is that nothing narrows to u32 on the way.
        let move_time = control.to_move_time(1, Player::WHITE);
        assert_eq!(move_time, (u64::from(u32::MAX) * 40 - MOVE_OVERHEAD) / 20);
        assert!(move_time > u64::from(u32::MAX));
    }

    #[test]
    fn huge_increment_cannot_allocate_more_than_the_clock_holds() {
        // Without the share cap this would allot 1_000 + 5_000 against a 1_000ms clock.
        let control = TimeControl::new(1_000, 1_000, 5_000, 5_000, Some(1));

        assert_eq!(control.to_move_time(1, Player::WHITE), 728);
    }

    #[test]
    fn allocation_never_exceeds_the_remaining_clock() {
        let clocks = [1, 2, 5, 10, 29, 30, 31, 50, 100, 500, 2_000, 60_000];
        let increments = [0, 10, 50, 100, 5_000];
        let moves_to_go = [None, Some(0), Some(1), Some(5), Some(20), Some(60)];

        for &clock in &clocks {
            for &inc in &increments {
                for &mtg in &moves_to_go {
                    let control = TimeControl::new(clock, clock, inc, inc, mtg);

                    for move_number in [1, 20, 41, 200] {
                        let move_time = control.to_move_time(move_number, Player::WHITE);

                        assert!(
                            move_time < clock,
                            "allotted {move_time}ms of a {clock}ms clock \
                             (inc {inc}, movestogo {mtg:?}, move {move_number})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn fast_time_controls_receive_a_positive_proportional_allocation() {
        // A 2+0.05 opening position: (2_000 - 30) / 39 + 50. Integer division of the residual
        // once truncated this to 0ms, which had the engine playing its opening at depth 1.
        let two_plus_005 = TimeControl::new(2_000, 2_000, 50, 50, None);
        assert_eq!(two_plus_005.to_move_time(1, Player::WHITE), 100);

        // 1+0.01, faster still, and a bare 1-second control with no increment at all.
        let one_plus_001 = TimeControl::new(1_000, 1_000, 10, 10, None);
        assert_eq!(one_plus_001.to_move_time(1, Player::WHITE), 34);

        let one_second = TimeControl::new(1_000, 1_000, 0, 0, None);
        assert_eq!(one_second.to_move_time(1, Player::WHITE), 24);
    }

    #[test]
    fn allocation_degrades_proportionally_as_the_clock_shrinks() {
        // Halving the clock should roughly halve the allocation, all the way down, rather than
        // collapsing to zero once a flat buffer overtakes the per-move slice.
        let clocks = [64_000, 32_000, 16_000, 8_000, 4_000, 2_000, 1_000, 500, 250];

        let mut previous: Option<u64> = None;
        for &clock in &clocks {
            let control = TimeControl::new(clock, clock, 0, 0, None);
            let move_time = control.to_move_time(1, Player::WHITE);

            assert!(move_time > 0, "{clock}ms clock allotted no time at all");

            if let Some(previous) = previous {
                let halved = previous / 2;
                // Within a few ms of exactly half, the slack absorbing the fixed overhead.
                assert!(
                    move_time <= halved && move_time + 5 >= halved,
                    "{clock}ms clock allotted {move_time}ms, not close to half of {previous}ms"
                );
            }

            previous = Some(move_time);
        }
    }

    #[test]
    fn a_clock_at_or_below_the_overhead_allots_no_time() {
        // Nothing to spend; the search still guarantees a legal move under a zero budget.
        for clock in [0, 1, 15, MOVE_OVERHEAD] {
            let control = TimeControl::new(clock, clock, 0, 0, Some(20));

            assert_eq!(control.to_move_time(1, Player::WHITE), 0);
        }
    }

    #[test]
    fn a_clock_just_above_the_overhead_still_allots_time() {
        let control = TimeControl::new(MOVE_OVERHEAD + 1, MOVE_OVERHEAD + 1, 0, 0, Some(20));

        assert_eq!(control.to_move_time(1, Player::WHITE), 1);
    }
}
