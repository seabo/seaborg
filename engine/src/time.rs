use core::position::Player;

static AVERAGE_GAME_LENGTH: u64 = 40;
static MINIMUM_REMAINING_MOVES: u64 = 20;
static PER_MOVE_BUFFER_TIME: u64 = 150;

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

        // Estimate for how much base time we can afford to use.
        // This is an integer in milliseconds, and so can be 0, if we have very little time
        // remaining.
        let base_time_per_move = base_time / est_remaining_moves;

        base_time_per_move
            .saturating_add(inc)
            .saturating_sub(PER_MOVE_BUFFER_TIME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_game_uses_minimum_remaining_move_estimate() {
        let control = TimeControl::new(20_000, 20_000, 0, 0, None);

        assert_eq!(control.to_move_time(41, Player::WHITE), 850);
        assert_eq!(control.to_move_time(100, Player::BLACK), 850);
    }

    #[test]
    fn sub_buffer_allocation_saturates_at_zero() {
        let control = TimeControl::new(100, 100, 0, 0, Some(20));

        assert_eq!(control.to_move_time(1, Player::WHITE), 0);
    }

    #[test]
    fn increment_contributes_to_allocation_before_buffer() {
        let control = TimeControl::new(1_000, 1_000, 200, 300, Some(10));

        assert_eq!(control.to_move_time(1, Player::WHITE), 150);
        assert_eq!(control.to_move_time(1, Player::BLACK), 250);
    }

    #[test]
    fn explicit_moves_to_go_controls_allocation_and_zero_is_safe() {
        let ten_moves = TimeControl::new(10_000, 10_000, 0, 0, Some(10));
        let zero_moves = TimeControl::new(10_000, 10_000, 0, 0, Some(0));

        assert_eq!(ten_moves.to_move_time(80, Player::WHITE), 850);
        assert_eq!(zero_moves.to_move_time(80, Player::WHITE), 9_850);
    }

    #[test]
    fn allocation_preserves_values_above_u32_max() {
        let control = TimeControl::new(u64::from(u32::MAX) * 40, 0, 0, 0, Some(20));

        assert_eq!(
            control.to_move_time(1, Player::WHITE),
            u64::from(u32::MAX) * 2 - PER_MOVE_BUFFER_TIME
        );
    }
}
