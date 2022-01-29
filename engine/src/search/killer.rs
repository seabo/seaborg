use core::mov::Move;
use core::position::Position;
use core::position::Square;

#[derive(Copy, Clone, Debug)]
pub struct KillerMove {
    orig: Square,
    dest: Square,
    is_castle: bool,
}

impl KillerMove {
    pub fn new(mov: Move, is_castle: bool) -> Self {
        KillerMove {
            orig: mov.orig(),
            dest: mov.dest(),
            is_castle,
        }
    }
}
