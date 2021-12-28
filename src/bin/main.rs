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
        init_magics();
        init_boards();
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
    let position7 = "8/4k3/4b3/8/4Q3/8/6K1/8 w - - 0 1";
    let position8 = "8/p7/4k3/1p5p/1P1r1K1P/P4P2/8/8 w - - 0 40";
    let position9 = "r1bqkb1r/ppp2ppp/2n5/4p3/2p5/5NN1/PPPPQPPP/R1B1K2R b KQkq - 1 7";
    let position10 = "8/3k4/3q4/8/3B4/3K4/8/8 w - - 0 1";

    let pos = Position::from_fen(position5);

    match pos {
        Ok(pos) => {
            println!("{:?}", pos);

            let now = Instant::now();
            let movelist = MoveGen::generate_legal(&pos);
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
