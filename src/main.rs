mod bb;
mod position;

use bb::Bitboard;
use position::Position;
use position::Square;
use std::time::Instant;

fn main() {
    let start_position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    let now = Instant::now();
    let pos = Position::from_fen(start_position);
    let elapsed = now.elapsed().as_micros();

    match pos {
        Ok(pos) => {
            println!("{:?}", pos);
        }
        Err(fen_error) => {
            println!("{}", fen_error.msg);
        }
    }
    println!("FEN string took {}μs to parse", elapsed);
}
