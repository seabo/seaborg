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
///
/// The reserve caps how fast we may drain, rather than being deducted from the pool we divide.
/// Deducting it up front also works, but it pays for the reserve in the opening and midgame,
/// where the clock is nowhere near it and the time buys real strength; a 1711-game self-play
/// match measured that at -7.9 Elo. Capping the drain leaves every allocation above the reserve
/// exactly as it was and only binds on the approach.
static RESERVE_INCREMENT_MOVES: u64 = 10;

/// Extra moves added to the divisor under a periodic (`movestogo`) control.
///
/// Dividing the period's budget by exactly the moves that remain plans to arrive at the boundary
/// with nothing, which leaves no room for a search that overshoots its allotment. Dividing by
/// `n + 1` instead holds a cushion back, and the arithmetic of doing so every move is unusually
/// tidy: spending `budget / (n + 1)` and then re-dividing what is left over `n - 1` moves yields
/// the same figure again, so the allocation is flat across the period and the clock reaches the
/// boundary holding exactly one move's worth. Unspent time carries across the boundary rather than
/// being forfeited, so that cushion is not wasted.
///
/// One move is also enough to keep [`MAX_CLOCK_SHARE_DIVISOR`] out of the way. On the boundary
/// move, where the cap used to be the whole of the policy, the divisor is 2 and no increment is
/// yet counted, so the ask is exactly half the usable clock — comfortably inside the three
/// quarters the cap allows. The cap goes back to being a backstop against pathological input,
/// which it can still be: an increment several times the size of the clock will overrun it.
static BOUNDARY_CUSHION_MOVES: u64 = 1;

/// Milliseconds to spend on a `go movetime` search of the requested duration.
///
/// The requested figure is what the caller expects to elapse between the command and `bestmove`,
/// not what we may spend thinking, so the same round-trip margin the clock-based path holds back
/// applies here too. A GUI that enforces `movetime` strictly will flag a search that spends the
/// whole of it. Requests at or below the margin saturate to a zero budget; the search still
/// guarantees a completed first ply, so it returns a legal move regardless.
pub fn move_time_budget(requested: u64) -> u64 {
    requested.saturating_sub(MOVE_OVERHEAD)
}

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
        // Moves left until the next time grant, if we are under a periodic control at all.
        //
        // UCI does not define `movestogo 0`, and GUIs emit it loosely: sometimes for "no periodic
        // control", sometimes for the boundary move itself. Reading it as one move left would
        // commit most of the clock to a single move on the strength of a value that carries no
        // information, so it is treated as an unknown horizon and falls back to the estimate below.
        let moves_to_boundary = self.moves_to_go.filter(|&n| n > 0);

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

        let allocation = match moves_to_boundary {
            // A periodic control funds itself: a fresh grant arrives at the boundary, and in
            // standard implementations whatever is unspent carries across rather than being lost.
            // So the pool to divide is everything this period has left to spend, and the goal is
            // to reach the boundary having spent it, keeping only the cushion.
            //
            // That pool is the clock we hold now plus the increments the period's remaining moves
            // will earn — but only `n - 1` of them. The increment for the final move of the period
            // arrives after that move has been played, so it carries across the boundary rather
            // than funding anything on this side of it. Counting it would have the policy plan to
            // spend time it does not yet hold, which on the boundary move itself is exactly when
            // the clock is least able to absorb the mistake.
            //
            // The increment reserve deliberately plays no part here. It is calibrated on the
            // premise that the increment funds the steady state, which is false when a grant does,
            // and it would bind hardest exactly where it matters least: near a boundary the clock
            // is at its smallest, but new time is imminent, so holding a reserve back at that
            // moment is precisely backwards.
            Some(n) => {
                let period_budget = usable_time.saturating_add(inc.saturating_mul(n - 1));
                period_budget / (n + BOUNDARY_CUSHION_MOVES)
            }
            // No boundary in sight: this clock is all we are going to get, so it is spread over an
            // estimate of the moves left in the game.
            None => {
                let est_remaining_moves = AVERAGE_GAME_LENGTH
                    .saturating_sub(curr_move_number.into())
                    .max(MINIMUM_REMAINING_MOVES);

                // The reserve we intend to still be holding when the game ends. Zero without an
                // increment, where there is no steady state to fund and spending down is correct.
                let reserve = inc.saturating_mul(RESERVE_INCREMENT_MOVES);

                match usable_time.checked_sub(reserve) {
                    // Above the reserve: the increment we will earn back by playing this move,
                    // plus our share of the clock. Both terms scale with the time control, so the
                    // allocation degrades proportionally as the clock shrinks rather than
                    // collapsing at a fixed threshold.
                    //
                    // Spending `inc + x` and earning `inc` back drains the clock by exactly `x`,
                    // so holding `x` to the headroom above the reserve is precisely the statement
                    // that this move will not take us below it. Far from the reserve this never
                    // binds and the allocation is unchanged; on the approach it is what arrests
                    // the decay, leaving the clock at the reserve instead of asymptoting onto the
                    // bare increment.
                    Some(headroom) => {
                        inc.saturating_add((usable_time / est_remaining_moves).min(headroom))
                    }
                    // Below the reserve, because the opponent's play or our own overshoot took us
                    // there. Spend a tenth of what we hold, which is strictly less than the
                    // increment down here (`usable_time < reserve` means
                    // `usable_time / RESERVE_INCREMENT_MOVES < inc`), so the clock climbs back
                    // towards the reserve instead of creeping further past it.
                    None => usable_time / RESERVE_INCREMENT_MOVES,
                }
            }
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

        // The period holds the usable clock plus the nineteen increments it can still spend,
        // spread over its twenty moves and the cushion. The increment is the whole of the
        // difference between the two colours.
        assert_eq!(control.to_move_time(1, Player::WHITE), 655);
        assert_eq!(control.to_move_time(1, Player::BLACK), 836);
    }

    #[test]
    fn explicit_moves_to_go_divides_the_period_budget() {
        let ten_moves = TimeControl::new(10_000, 10_000, 0, 0, Some(10));

        // (10_000 - 30) / 11: ten moves to play and one move's cushion to arrive with.
        assert_eq!(ten_moves.to_move_time(80, Player::WHITE), 906);
    }

    /// `movestogo 0` carries no information about the horizon, so it must not be read as "one move
    /// left" — that once committed three quarters of the clock to a single move on the strength of
    /// a value GUIs emit loosely.
    #[test]
    fn moves_to_go_of_zero_falls_back_to_the_game_length_heuristic() {
        let zero_moves = TimeControl::new(10_000, 10_000, 0, 0, Some(0));
        let unknown = TimeControl::new(10_000, 10_000, 0, 0, None);

        // Indistinguishable from no periodic control at all, at any stage of the game.
        for move_number in [1, 20, 41, 80] {
            assert_eq!(
                zero_moves.to_move_time(move_number, Player::WHITE),
                unknown.to_move_time(move_number, Player::WHITE)
            );
        }

        // (10_000 - 30) / 20, the minimum remaining-move estimate, rather than the 7_478ms
        // share-cap maximum a horizon of one move would have asked for.
        assert_eq!(zero_moves.to_move_time(80, Player::WHITE), 498);
    }

    #[test]
    fn allocation_preserves_values_above_u32_max() {
        let control = TimeControl::new(u64::from(u32::MAX) * 40, 0, 0, 0, Some(20));

        // (u32::MAX * 40 - 30) / 21, the twenty moves of the period plus its cushion. The point is
        // that nothing narrows to u32 on the way.
        let move_time = control.to_move_time(1, Player::WHITE);
        assert_eq!(
            move_time,
            (u64::from(u32::MAX) * 40 - MOVE_OVERHEAD) / (20 + BOUNDARY_CUSHION_MOVES)
        );
        assert!(move_time > u64::from(u32::MAX));
    }

    #[test]
    fn huge_increment_cannot_allocate_more_than_the_clock_holds() {
        // A 5_000ms increment against a 1_000ms clock, two moves from the boundary: the period is
        // credited with an increment five times the size of the clock, so it plans to spend
        // 1_990ms it does not hold. Nothing in the allocation policy can help here, so the share
        // cap earns its keep as the backstop against pathological input it is meant to be.
        let huge_increment = TimeControl::new(1_000, 1_000, 5_000, 5_000, Some(2));
        assert_eq!(huge_increment.to_move_time(1, Player::WHITE), 728);

        // On the boundary move itself no increment is credited at all, so however large it is the
        // policy asks for half the usable clock and the cap has nothing to trim. The other half
        // carries across the boundary to join the incoming grant.
        let one_move_left = TimeControl::new(10_000, 10_000, 100, 100, Some(1));
        assert_eq!(one_move_left.to_move_time(1, Player::WHITE), 4_985);
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
        // The 2+0.05 opening: (2_000 - 30) / 39 + 50. A flat per-move buffer once made this 0ms,
        // which had the engine playing its whole opening at depth 1. The reserve caps how fast the
        // clock may drain rather than shrinking the pool being divided, and an opening clock is
        // nowhere near the reserve, so these allocations are untouched by it.
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

    /// Play a self-play game against the allocation policy alone, spending exactly what is
    /// allotted and earning the increment back, and report the clock after each move number.
    ///
    /// Nothing here models search overshoot or transport delay; the question is whether the
    /// policy itself drains the clock, which it once did all the way down to the increment.
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
        // Dividing the whole clock every move converged these to 49ms, 96ms and 163ms: a reserve
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
        // Move 100 of a 1+0.01 game, holding a clock that a policy decaying onto the increment
        // would never still have: whatever sits above the reserve stays spendable, so a critical
        // late position gets a real think rather than the bare increment.
        let control = TimeControl::new(2_000, 2_000, 10, 10, None);
        let allotted = control.to_move_time(100, Player::WHITE);

        // 10 + (2_000 - 30) / 20; the reserve is far away, so it does not bind here.
        assert_eq!(allotted, 108);
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
        // a flat per-move buffer, which starved fast controls by taking a share that grew
        // without bound as the time control shortened.
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

    /// Near a control boundary the clock is at its smallest, but a fresh grant is imminent, so the
    /// increment reserve is at its least relevant precisely where it would otherwise bite hardest.
    /// Applying it there once cut a 1_000ms clock with one move to go from a 728ms allotment to
    /// 97ms — a tenth of the clock spent on the last move before more time arrives.
    #[test]
    fn the_increment_reserve_does_not_bind_as_a_boundary_approaches() {
        let clock = 1_000;
        let inc = 100;

        // What the below-reserve branch of the sudden-death path would have allotted: a tenth of
        // the usable clock, because 1_000ms sits far below a ten-increment reserve.
        let below_reserve_share = (clock - MOVE_OVERHEAD) / RESERVE_INCREMENT_MOVES;

        // Walking in towards the boundary, the allotment must grow, not shrink.
        let mut previous = 0;
        for moves_to_go in (1..=10).rev() {
            let control = TimeControl::new(clock, clock, inc, inc, Some(moves_to_go));
            let allotted = control.to_move_time(1, Player::WHITE);

            assert!(
                allotted >= previous,
                "movestogo {moves_to_go} allotted {allotted}ms, less than the {previous}ms \
                 allotted further from the boundary"
            );
            assert!(
                allotted > below_reserve_share,
                "movestogo {moves_to_go} allotted {allotted}ms, no more than the \
                 {below_reserve_share}ms the increment reserve would have allowed"
            );

            previous = allotted;
        }

        // On the boundary move itself the period budget, not the reserve, is what governs: half of
        // (1_000 - 30), rather than the 97ms a ten-increment reserve would have left.
        let boundary = TimeControl::new(clock, clock, inc, inc, Some(1));
        assert_eq!(boundary.to_move_time(1, Player::WHITE), 485);
    }

    /// Play out a whole period, spending exactly what is allotted, earning the increment, and
    /// counting down to the boundary. Returns the allotments and the clock on arrival.
    fn simulate_period(base: u64, inc: u64, moves_to_go: u64) -> (Vec<u64>, u64) {
        let mut clock = base;
        let mut allotments = Vec::new();

        for remaining in (1..=moves_to_go).rev() {
            let control = TimeControl::new(clock, clock, inc, inc, Some(remaining));
            let allotted = control.to_move_time(1, Player::WHITE);

            assert!(
                allotted < clock,
                "movestogo {remaining} allotted {allotted}ms of a {clock}ms clock"
            );

            clock = clock - allotted + inc;
            allotments.push(allotted);
        }

        (allotments, clock)
    }

    /// Dividing by exactly the moves that remain plans to arrive at the boundary with nothing,
    /// which leaves no room for a search that overshoots. The cushion in the divisor is what buys
    /// that room, and because unspent time carries across the boundary it is not thrown away.
    #[test]
    fn a_period_arrives_at_its_boundary_with_time_still_on_the_clock() {
        for (base, inc, moves_to_go) in [
            (300_000, 0, 40),
            (180_000, 2_000, 40),
            (60_000, 0, 20),
            (10_000, 100, 10),
            (5_000, 0, 5),
        ] {
            let (allotments, arrival) = simulate_period(base, inc, moves_to_go);

            // A flat allocation across the period, so the last move of a period is no poorer than
            // the first and the cushion is not consumed on the way.
            let first = allotments[0];
            for (index, &allotted) in allotments.iter().enumerate() {
                assert!(
                    allotted.abs_diff(first) <= 2,
                    "{base}+{inc}/{moves_to_go}: move {index} allotted {allotted}ms against \
                     {first}ms at the start of the period"
                );
            }

            // Arrival holds roughly the one move's worth the cushion asks for, and never nothing.
            assert!(
                arrival >= first,
                "{base}+{inc}/{moves_to_go}: arrived at the boundary with {arrival}ms, less than \
                 the {first}ms a single move of the period was allotted"
            );
        }
    }

    /// The share cap exists to stop pathological input from committing more time than we hold. It
    /// is not the allocation policy, and at `movestogo 1` it once was: the policy asked for the
    /// whole clock and the cap trimmed it to three quarters. A well-formed periodic control should
    /// land inside the cap on its own.
    #[test]
    fn the_share_cap_never_binds_for_a_well_formed_periodic_control() {
        let assert_cap_is_slack = |clock: u64, inc: u64| {
            for moves_to_go in 1..=60 {
                let control = TimeControl::new(clock, clock, inc, inc, Some(moves_to_go));
                let allotted = control.to_move_time(1, Player::WHITE);

                let usable = clock - MOVE_OVERHEAD;
                let cap = usable - usable / MAX_CLOCK_SHARE_DIVISOR;

                assert!(
                    allotted < cap,
                    "movestogo {moves_to_go} on a {clock}ms clock (inc {inc}) allotted \
                     {allotted}ms, at or above the {cap}ms share cap"
                );
            }
        };

        // Periodic controls a GUI actually emits, from classical down through blitz. The last of
        // these has an increment two thirds the size of its clock and still leaves the cap slack.
        for (clock, inc) in [
            (5_400_000, 30_000),
            (300_000, 3_000),
            (180_000, 2_000),
            (60_000, 1_000),
            (10_000, 0),
            (3_000, 2_000),
        ] {
            assert_cap_is_slack(clock, inc);
        }

        // And a synthetic sweep. The policy credits a period with up to `n - 1` increments and
        // then divides by `n + 1`, so for a long period it can ask for something approaching the
        // increment itself; an increment beyond about three quarters of the usable clock is what
        // it takes to reach the cap. Half the usable clock stays clear of that with room to spare,
        // and no real control comes near it.
        let clocks = [500, 2_000, 10_000, 60_000, 300_000];
        let increments = [0, 10, 100, 1_000, 30_000];

        for &clock in &clocks {
            for &inc in &increments {
                if 2 * inc > clock - MOVE_OVERHEAD {
                    continue;
                }

                assert_cap_is_slack(clock, inc);
            }
        }
    }

    /// `go movetime` names the time the caller expects to elapse before `bestmove`, so it owes the
    /// same round-trip margin the clock-based path holds back. Spending the whole of it risks a
    /// flag under a GUI that enforces `movetime` strictly.
    #[test]
    fn movetime_holds_back_the_same_overhead_as_the_clock() {
        assert_eq!(move_time_budget(1_000), 1_000 - MOVE_OVERHEAD);
        assert_eq!(move_time_budget(MOVE_OVERHEAD + 1), 1);

        // Below the overhead there is nothing to spend. The budget saturates at zero rather than
        // wrapping, and the search still returns a legal move under a zero budget.
        for requested in [0, 1, MOVE_OVERHEAD - 1, MOVE_OVERHEAD] {
            assert_eq!(move_time_budget(requested), 0);
        }

        // Nothing narrows on the way through, for a `movetime` beyond a 32-bit range.
        let huge = u64::from(u32::MAX) + 1_000;
        assert_eq!(move_time_budget(huge), huge - MOVE_OVERHEAD);
    }
}
