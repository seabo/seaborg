//! Stores additional state about a position which is often reused across many
//! methods on `Position`. We keep track of here in a dedicated struct.
//!
//! Contains things like `checkers` (which pieces are currently checking the moving
//! player's king), `zobrist` (the efficiently updateable hash key for the transposition
//! table).

use super::{Player, Position};
use crate::bb::Bitboard;
use crate::masks::PLAYER_CNT;

use std::fmt;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct State {
    /// A `Bitboard` containing the pieces which are currently checking
    /// the player to move.
    pub checkers: Bitboard,
    /// One `Bitboard` for each player, tracking the pieces which are
    /// currently blocking attacks to that player's king. Used to quickly
    /// evaluate if a move creates a discovered check, or is pinned.
    pub blockers: [Bitboard; PLAYER_CNT],
    /// One `Bitboard` for each player, tracking the pieces which are
    /// currently pinning some other piece to the opponent's king.
    pub pinners: [Bitboard; PLAYER_CNT],
}

impl State {
    /// Returns a blank `State`.
    pub const fn blank() -> Self {
        Self {
            checkers: Bitboard(0),
            blockers: [Bitboard(0); PLAYER_CNT],
            pinners: [Bitboard(0); PLAYER_CNT],
        }
    }

    /// Set the `State` data based on the associated `Position`.
    pub(crate) fn from_position(position: &Position) -> Self {
        let mut state = Self::blank();
        let us = position.turn();
        let them = !us;
        let ksq = position.king_sq(us);

        state.checkers = position.attackers_to(ksq) & position.get_occupied_player_runtime(them);
        state.set_check_info(position);

        state
    }

    /// Used after a move is made to build the information concerning checking,
    /// blocking, pinners etc.
    pub(crate) fn set_check_info(&mut self, position: &Position) {
        let (white_blockers, white_pinners) =
            position.slider_blockers(position.occupied_black(), position.king_sq(Player::WHITE));

        self.blockers[Player::WHITE.inner() as usize] = white_blockers;
        self.pinners[Player::WHITE.inner() as usize] = white_pinners;

        let (black_blockers, black_pinners) =
            position.slider_blockers(position.occupied_white(), position.king_sq(Player::BLACK));

        self.blockers[Player::BLACK.inner() as usize] = black_blockers;
        self.pinners[Player::BLACK.inner() as usize] = black_pinners;
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Checkers:\n {}", self.checkers)?;
        writeln!(
            f,
            "Blockers - white:\n {}",
            self.blockers[Player::WHITE.inner() as usize]
        )?;
        writeln!(
            f,
            "Blockers - black:\n {}",
            self.blockers[Player::BLACK.inner() as usize]
        )?;
        writeln!(
            f,
            "Pinners - white:\n {}",
            self.pinners[Player::WHITE.inner() as usize]
        )?;
        writeln!(
            f,
            "Pinners - black:\n {}",
            self.pinners[Player::BLACK.inner() as usize]
        )
    }
}
