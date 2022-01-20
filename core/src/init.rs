use crate::precalc::boards::init_boards;
use crate::precalc::magic::init_magics;
use crate::precalc::zobrist::init_zobrist;
use std::sync::Once;

static INITALIZED: Once = Once::new();

/// Initialises global variables used by the engine and internal
/// board representation, such as:
/// - magic bitboard tables
/// - precalculated piece movements
/// - zobrist hash keys.
///
/// Any subsequent calls to this function after the first have no
/// effect and should return instantly.
pub fn init_globals() {
    // The closure inside `call_once()` is only ever invoked on the first call,
    // so this function will return instantly on further calls.
    INITALIZED.call_once(|| {
        init_magics();
        init_boards();
        init_zobrist();
    })
}
