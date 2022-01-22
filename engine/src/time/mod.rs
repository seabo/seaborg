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
    pub fn new(wtime: u32, btime: u32, winc: u32, binc: u32, moves_to_go: Option<u8>) -> Self {
        Self {
            wtime,
            btime,
            winc,
            binc,
            moves_to_go,
        }
    }
}
