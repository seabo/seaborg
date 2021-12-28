use rchess::bb::{
    Bitboard, WHITE_LEFTWARD_PROMOTION_MASK, WHITE_LEFT_PAWN_CAPTURE_MASK,
    WHITE_RIGHT_PAWN_CAPTURE_MASK,
};

use rchess::mov::Move;
use rchess::movegen::MoveGen;
use rchess::position::{Position, Square};
use rchess::precalc::magic::{bishop_attacks, init_magics};

use std::sync::{Once, ONCE_INIT};
use std::time::Instant;

use rchess::precalc::boards::init_boards;

static INITALIZED: Once = ONCE_INIT;

fn init_globals() {
    INITALIZED.call_once(|| {
        init_boards();
        init_magics();
    })
}

fn main() {
    init_globals();
    let start_position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    let other_position = "rn3rk1/1bq2ppp/p3p3/1pnp2B1/3N1P2/2b3Q1/PPP3PP/2KRRB2 w - - 0 17";
    let position3 = "2r1b2k/3P4/8/8/8/8/8/7K w - - 0 1";
    let position4 = "7k/8/8/1PpP4/8/8/8/7K w - c6 0 2";
    let position5 = "7k/8/8/3Rnr2/3pKb2/3rpp2/8/8 w - - 0 1";
    let position6 = "rnb1kb1r/1pqp1ppp/p3pn2/8/3NP3/2PB4/PP3PPP/RNBQK2R w KQkq - 3 7";

    let now = Instant::now();
    let pos = Position::from_fen(start_position);
    let elapsed = now.elapsed().as_micros();

    println!("{}", WHITE_LEFTWARD_PROMOTION_MASK);

    match pos {
        Ok(pos) => {
            println!("{:?}", pos);

            let now = Instant::now();
            let movelist = MoveGen::generate(&pos);
            let elapsed = now.elapsed().as_micros();

            for mov in movelist.iter() {
                println!("{}", mov);
            }

            println!("# of moves: {}", movelist.len());
            println!("Took {}us to gen moves", elapsed);
        }
        Err(fen_error) => {
            println!("{}", fen_error.msg);
        }
    }

    // println!("FEN string took {}μs to parse", elapsed);
}
