use core::position::Player;

use std::cmp::max;

/// Following Lichess, we naively assume games are 40 moves long, or 80 ply, on average.
static AVERAGE_GAME_LENGTH: u32 = 40;
/// Minimum remaining moves to assume.
static MINIMUM_REMAINING_MOVES: u32 = 20;
/// Buffer time (in ms) to allocate to each move for executing non-search code (like
/// parsing commands, setting up the board position, calculating time management etc.)
/// TODO: experiment / run tests to work out appropriate time for this.
static PER_MOVE_BUFFER_TIME: u32 = 50;
/// A minimum time to allocate to each move, in ms, in case the algorithm comes up with
/// something unreasonably short.
static MINIMUM_TIME_PER_MOVE: u32 = 50;

/// A struct to hold information about the time control for a search.
#[derive(Copy, Clone, Debug)]
pub struct TimeControl {
    /// The amount of time on white's clock, in milliseconds.
    wtime: u32,
    /// The amount of time on black's clock, in milliseconds.
    btime: u32,
    /// The increment applied to white's clock after every white move, in milliseconds.
    winc: u32,
    /// The increment applied to black's clock after every black move, in milliseconds.
    binc: u32,
    /// The number of moves until the next time control, when more time will be added to
    /// the main clocks. If `None`, then there is no further time control to reach, so
    /// the current readings of `wtime` and `btime` are for playing the rest of the game
    /// to completion.
    moves_to_go: Option<u8>,
}

impl TimeControl {
    /// Build a new `TimeControl` struct.
    pub fn new(wtime: u32, btime: u32, winc: u32, binc: u32, moves_to_go: Option<u8>) -> Self {
        Self {
            wtime,
            btime,
            winc,
            binc,
            moves_to_go,
        }
    }

    /// Calculate how long we should be willing to search a position given this `TimeControl`.
    /// Returns a fixed amount of time, expressed in milliseconds.
    pub fn to_fixed_time(&self, curr_move_number: u32, turn: Player) -> u32 {
        // TODO: it should be possible to (optionally) pass in some more parameters
        // which would help us get a better estimate of the number of remaining moves in the
        // game. This might be something like the 'game phase' (e.g. opening / middlegame /
        // endgame) as determined by the evaluation functions etc.
        // An estimate of the number of remaining moves in the game.
        let remaining_moves = match self.moves_to_go {
            Some(n) => u32::from(n),
            None => max(
                MINIMUM_REMAINING_MOVES,
                AVERAGE_GAME_LENGTH - curr_move_number,
            ),
        };
        // An estimate of the time remaining for the current `Player`.
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
        let remaining_time = base_time + remaining_moves * inc;
        let buffer_time = PER_MOVE_BUFFER_TIME * remaining_moves;

        if remaining_time < buffer_time {
            MINIMUM_TIME_PER_MOVE
        } else {
            (remaining_time - buffer_time) / remaining_moves
        }
    }
}
