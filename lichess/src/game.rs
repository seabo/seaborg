//! Handoff from an accepted challenge to game play.
//!
//! When a game starts, the event loop builds a [`GameHandoff`] describing what a
//! game runner needs to play it: the game id to stream and send moves to, the
//! engine options to apply, and the position to start from. Driving the game to
//! completion — streaming its moves and replying with the engine's — is not
//! implemented yet; this type is the seam that stage plugs into.

use core::position::Position;
use engine::options::EngineOpt;

use crate::config::EngineSettings;

/// Everything a game runner needs to start playing one game.
pub struct GameHandoff {
    /// The game's Lichess id.
    pub game_id: String,
    /// Engine options derived from the bot configuration.
    pub engine_options: Vec<EngineOpt>,
    /// The position the game begins from.
    pub initial_position: Position,
}

impl GameHandoff {
    /// Build the handoff for a game with the given id using `settings`.
    ///
    /// The initial position is the standard starting position; variant start
    /// positions arrive with variant play support and are out of scope here.
    pub fn new(game_id: impl Into<String>, settings: &EngineSettings) -> GameHandoff {
        GameHandoff {
            game_id: game_id.into(),
            engine_options: settings.engine_options(),
            initial_position: Position::start_pos(),
        }
    }
}
