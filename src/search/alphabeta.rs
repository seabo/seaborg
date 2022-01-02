use crate::position::Position;
use std::cmp::{max, min};

pub fn alphabeta(
    pos: &mut Position,
    depth: usize,
    mut alpha: i64,
    mut beta: i64,
    is_white: bool,
) -> i64 {
    if depth == 0 {
        if pos.in_checkmate() {
            return if is_white { -10000 } else { 10000 };
        } else {
            return 0;
        }
    }

    if is_white {
        let mut val = -10000;
        let moves = pos.generate_moves();
        for mov in moves {
            pos.make_move(mov);
            val = max(val, alphabeta(pos, depth - 1, alpha, beta, false));
            pos.unmake_move();
            alpha = max(alpha, val);
            if val >= beta {
                break;
            }
        }

        return val;
    } else {
        let mut val = 10000;
        let moves = pos.generate_moves();
        for mov in moves {
            pos.make_move(mov);
            val = min(val, alphabeta(pos, depth - 1, alpha, beta, true));
            pos.unmake_move();
            beta = min(beta, val);
            if val <= alpha {
                break;
            }
        }
        return val;
    }
}
