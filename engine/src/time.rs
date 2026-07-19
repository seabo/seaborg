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

/// Size of the reserve we refuse to spend down, measured in moves' worth of increment.
///
/// In an increment game the increment funds the steady state and the base clock is a separate
/// pool. Spending a fixed fraction of the whole clock every move drains that pool geometrically,
/// so the allocation converges onto the bare increment and the engine ends up playing
/// hand-to-mouth from roughly move 60 at fast controls. Holding back this reserve makes the
/// converged state an explicit choice: the clock settles at `MOVE_OVERHEAD + reserve` rather than
/// at `MOVE_OVERHEAD` plus whatever rounding leaves behind.
///
/// Ten moves of increment is enough to absorb a run of moves that overshoot their allotment, and
/// to leave something worth spending if the game reaches a critical late position. It is
/// deliberately expressed in increments rather than milliseconds: a flat reserve would be the same
/// mistake as the flat per-move buffer that starved fast controls before, and it would penalise a
/// sudden-death control, where spending the clock down to nothing is the correct policy.
static RESERVE_INCREMENT_MOVES: u64 = 10;

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

        // The reserve we intend to still be holding when the game ends. Zero without an
        // increment, where there is no steady state to fund and spending down is correct.
        let reserve = inc.saturating_mul(RESERVE_INCREMENT_MOVES);

        let allocation = match usable_time.checked_sub(reserve) {
            // Above the reserve: the increment we will earn back by playing this move, plus a
            // share of the surplus only. Both terms scale with the time control, so the
            // allocation degrades proportionally as the clock shrinks rather than collapsing at
            // a fixed threshold, and the surplus is spent down deliberately instead of the whole
            // clock decaying geometrically towards the increment.
            Some(surplus) => inc.saturating_add(surplus / est_remaining_moves),
            // Below the reserve, because the opponent's play or our own overshoot took us there.
            // Spend a tenth of what we hold, which is strictly less than the increment down here
            // (`usable_time < reserve` means `usable_time / RESERVE_INCREMENT_MOVES < inc`), so
            // the clock climbs back towards the reserve instead of creeping further past it.
            None => usable_time / RESERVE_INCREMENT_MOVES,
        };

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
        // A clock comfortably above both sides' reserves, so the surplus term is what is being
        // divided and the increment is what distinguishes the two colours.
        let control = TimeControl::new(10_000, 10_000, 200, 400, Some(20));

        // The side's increment, plus its share of whatever sits above its own reserve:
        // (10_000 - 30 - 200 * 10) / 20 for white, (10_000 - 30 - 400 * 10) / 20 for black.
        assert_eq!(control.to_move_time(1, Player::WHITE), 598);
        assert_eq!(control.to_move_time(1, Player::BLACK), 698);
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
        // A 5_000ms increment against a 1_000ms clock. This once allotted 728ms, the share cap
        // trimming a 1_000 + 5_000 allocation. The reserve policy now binds first and harder: we
        // are far below a 50_000ms reserve, so we spend a tenth of what we hold and let the
        // increment refill the clock over the next few moves.
        let huge_increment = TimeControl::new(1_000, 1_000, 5_000, 5_000, Some(1));
        assert_eq!(huge_increment.to_move_time(1, Player::WHITE), 97);

        // The share cap is still the binding constraint where the reserve is not. Here the
        // surplus above a 1_000ms reserve, divided over a single remaining move, would allot
        // 9_070ms of a 10_000ms clock; three quarters of the usable clock is the most we commit.
        let one_move_left = TimeControl::new(10_000, 10_000, 100, 100, Some(1));
        assert_eq!(one_move_left.to_move_time(1, Player::WHITE), 7_478);
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
        // The 2+0.05 opening position from TASK-38: 50 + (2_000 - 30 - 50 * 10) / 39. This
        // allotted 0ms before that fix, which is what had the engine playing its opening at
        // depth 1. TASK-42 holds a reserve back from the surplus, so this is now 87ms rather
        // than 100ms; the point of the test is that it stays a large multiple of a depth-1
        // search rather than any particular number.
        let two_plus_005 = TimeControl::new(2_000, 2_000, 50, 50, None);
        assert_eq!(two_plus_005.to_move_time(1, Player::WHITE), 87);

        // 1+0.01, faster still, and a bare 1-second control with no increment at all.
        let one_plus_001 = TimeControl::new(1_000, 1_000, 10, 10, None);
        assert_eq!(one_plus_001.to_move_time(1, Player::WHITE), 32);

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

    /// Play a self-play game against the allocation policy alone, spending exactly what is
    /// allotted and earning the increment back, and report the clock after each move number.
    ///
    /// Nothing here models search overshoot or transport delay; the question is whether the
    /// policy itself drains the clock, which it did before TASK-42.
    fn simulate_game(base: u64, inc: u64, moves: u32) -> Vec<(u32, u64, u64)> {
        let mut clock = base;
        let mut history = Vec::new();

        for move_number in 1..=moves {
            let control = TimeControl::new(clock, clock, inc, inc, None);
            let allotted = control.to_move_time(move_number, Player::WHITE);

            assert!(
                allotted < clock,
                "move {move_number} allotted {allotted}ms of a {clock}ms clock"
            );

            clock = clock - allotted + inc;
            history.push((move_number, allotted, clock));
        }

        history
    }

    #[test]
    fn an_increment_game_settles_on_the_reserve_rather_than_the_increment() {
        // Before TASK-42 these clocks converged to 49ms, 96ms and 163ms respectively: a reserve
        // of tens of milliseconds above the fixed overhead, whatever the time control. The
        // converged clock is now the reserve the policy asks for, plus the overhead it holds
        // back once.
        for (base, inc) in [(1_000, 10), (2_000, 50), (10_000, 100)] {
            let reserve = inc * RESERVE_INCREMENT_MOVES;
            let history = simulate_game(base, inc, 140);

            for &(move_number, _, clock) in &history {
                assert!(
                    clock > reserve,
                    "{base}+{inc}: clock fell to {clock}ms at move {move_number}, \
                     below the {reserve}ms reserve"
                );
            }

            for probe in [60, 100, 140] {
                let (_, allotted, clock) = history[probe - 1];

                assert!(
                    clock >= reserve + MOVE_OVERHEAD,
                    "{base}+{inc}: clock was {clock}ms at move {probe}, below the \
                     {reserve}ms reserve plus the {MOVE_OVERHEAD}ms overhead"
                );
                assert!(
                    allotted >= inc,
                    "{base}+{inc}: move {probe} allotted {allotted}ms, below the {inc}ms \
                     increment"
                );
            }
        }
    }

    #[test]
    fn a_late_game_move_can_still_be_allotted_far_more_than_the_increment() {
        // Move 100 of a 1+0.01 game, holding a clock that the pre-TASK-42 policy would never
        // have reached: the surplus above the reserve is still spendable, so a critical late
        // position gets a real think rather than the bare increment.
        let control = TimeControl::new(2_000, 2_000, 10, 10, None);
        let allotted = control.to_move_time(100, Player::WHITE);

        // 10 + (2_000 - 30 - 100) / 20.
        assert_eq!(allotted, 103);
        assert!(allotted > 10 * 10);
    }

    #[test]
    fn a_clock_below_the_reserve_spends_less_than_the_increment_and_recovers() {
        // Dropping below the reserve must be self-correcting, or the reserve is a label rather
        // than a floor. Start well under it and check the clock climbs back.
        let inc = 100;
        let reserve = inc * RESERVE_INCREMENT_MOVES;
        let mut clock = 400;

        for _ in 0..40 {
            let control = TimeControl::new(clock, clock, inc, inc, None);
            let allotted = control.to_move_time(100, Player::WHITE);

            assert!(
                allotted < inc,
                "allotted {allotted}ms below the reserve, at or above the {inc}ms increment"
            );

            clock = clock - allotted + inc;
        }

        assert!(
            clock > reserve,
            "clock recovered only to {clock}ms, still below the {reserve}ms reserve"
        );
    }

    #[test]
    fn sudden_death_holds_no_reserve() {
        // Without an increment there is no steady state to protect, so the reserve is zero and
        // the clock is spent down as before. This is what keeps the reserve from behaving like
        // the flat per-move buffer TASK-38 removed.
        let history = simulate_game(60_000, 0, 100);
        let (_, _, final_clock) = history[99];

        assert!(
            final_clock < 1_000,
            "sudden death held {final_clock}ms back instead of spending the clock down"
        );
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
