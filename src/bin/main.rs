use rchess::bb::Bitboard;

use rchess::mov::Move;
use rchess::position::{Position, Square};
use std::time::Instant;

fn main() {
    // let start_position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    // let other_position = "rn3rk1/1bq2ppp/p3p3/1pnp2B1/3N1P2/2b3Q1/PPP3PP/2KRRB2 w - - 0 17";

    // let now = Instant::now();
    // let pos = Position::from_fen(other_position);
    // let elapsed = now.elapsed().as_micros();

    // match pos {
    //     Ok(pos) => {
    //         println!("{:?}", pos);
    //         println!("{:?}", pos.generate_moves());
    //     }
    //     Err(fen_error) => {
    //         println!("{}", fen_error.msg);
    //     }
    // }
    // println!("FEN string took {}μs to parse", elapsed);

    let bb = Bitboard::new(0xAB878DE7787627F8);
    for idx in bb {
        println!("popped idx: {}", idx);
        println!("bsf: {}", bb);
    }
}
