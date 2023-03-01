use core::position::Player;

use std::cmp::max;

static AVERAGE_GAME_LENGTH: u32 = 40;
static MINIMUM_REMAINING_MOVES: u32 = 20;
static PER_MOVE_BUFFER_TIME: u32 = 150;

#[derive(Clone, Debug)]
pub enum TimingMode {
    Timed(TimeControl),
    MoveTime(usize),
    Depth(u8),
    Infinite,
}

#[derive(Clone, Debug)]
pub struct TimeControl {
    /// Time remaning on white's clock, in milliseconds.
    wtime: usize,
    /// Time remaning on black's clock, in milliseconds.
    btime: usize,
    /// White's increment per move.
    winc: usize,
    /// Black's increment per move.
    binc: usize,
    /// Number of moves until the time control changes / is reset. If `None`, there no further time
    /// controls.
    moves_to_go: Option<usize>,
}

impl TimeControl {
    pub fn new(
        wtime: usize,
        btime: usize,
        winc: usize,
        binc: usize,
        moves_to_go: Option<usize>,
    ) -> Self {
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
    pub fn to_move_time(&self, curr_move_number: u32, turn: Player) -> u32 {
        // An estimate for how many moves we expect to have to play with the time remaining on our
        // clock.
        let est_remaining_moves = match self.moves_to_go {
            Some(n) => n as u32,
            None => max(
                MINIMUM_REMAINING_MOVES,
                AVERAGE_GAME_LENGTH - curr_move_number,
            ),
        };

        let base_time = if turn.is_white() {
            self.wtime
        } else {
            self.btime
        };

        let inc = if turn.is_white() {
            self.winc as u32
        } else {
            self.binc as u32
        };

        // Estimate for how much base time we can afford to use.
        // This is an integer in milliseconds, and so can be 0, if we have very little time
        // remaining.
        let base_time_per_move = base_time as u32 / est_remaining_moves;

        base_time_per_move + inc - PER_MOVE_BUFFER_TIME
    }
}
