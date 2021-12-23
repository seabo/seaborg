use rchess::bb::{
    Bitboard, WHITE_LEFTWARD_PROMOTION_MASK, WHITE_LEFT_PAWN_CAPTURE_MASK,
    WHITE_RIGHT_PAWN_CAPTURE_MASK,
};

use rchess::mov::Move;
use rchess::position::{Position, Square};
use std::sync::{Once, ONCE_INIT};
use std::time::Instant;

use rchess::precalc::boards::{init_boards, king_moves, knight_moves};

static INITALIZED: Once = ONCE_INIT;

fn init_globals() {
    INITALIZED.call_once(|| {
        init_boards();
    })
}

fn main() {
    init_globals();
    println!("{}", knight_moves(Square(29)));

    // let start_position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    // let other_position = "rn3rk1/1bq2ppp/p3p3/1pnp2B1/3N1P2/2b3Q1/PPP3PP/2KRRB2 w - - 0 17";
    // let position3 = "2r1b2k/3P4/8/8/8/8/8/7K w - - 0 1";
    // let position4 = "7k/8/8/1PpP4/8/8/8/7K w - c6 0 2";

    // let now = Instant::now();
    // let pos = Position::from_fen(other_position);
    // let elapsed = now.elapsed().as_micros();

    // println!("{}", WHITE_LEFTWARD_PROMOTION_MASK);

    // match pos {
    //     Ok(pos) => {
    //         println!("{:?}", pos);

    //         for mov in pos.generate_moves() {
    //             println!("{}", mov);
    //         }
    //     }
    //     Err(fen_error) => {
    //         println!("{}", fen_error.msg);
    //     }
    // }

    // println!("FEN string took {}μs to parse", elapsed);
}
