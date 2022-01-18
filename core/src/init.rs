use crate::precalc::boards::init_boards;
use crate::precalc::magic::init_magics;
use crate::precalc::zobrist::init_zobrist;
use std::sync::Once;

static INITALIZED: Once = Once::new();

pub fn init_globals() {
    INITALIZED.call_once(|| {
        init_magics();
        init_boards();
        init_zobrist();
    })
}
