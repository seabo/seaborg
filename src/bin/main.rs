use rchess::position::Position;
use rchess::precalc::boards::init_boards;
use rchess::precalc::magic::init_magics;
use rchess::search::perft::Perft;

use separator::Separatable;

use std::sync::Once;
use std::time::Instant;

static INITALIZED: Once = Once::new();

fn init_globals() {
    INITALIZED.call_once(|| {
        init_magics();
        init_boards();
    })
}

fn main() {
    init_globals();
    let start_position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    // let other_position = "rn3rk1/1bq2ppp/p3p3/1pnp2B1/3N1P2/2b3Q1/PPP3PP/2KRRB2 w - - 0 17";
    // let position3 = "2r1b2k/3P4/8/8/8/8/8/7K w - - 0 1";
    // let position4 = "7k/8/8/1PpP4/8/8/8/7K w - c6 0 2";
    // let position5 = "7k/8/8/3Rnr2/3pKb2/3rpp2/8/8 w - - 0 1";
    // let position6 = "rnb1kb1r/1pqp1ppp/p3pn2/8/3NP3/2PB4/PP3PPP/RNBQK2R w KQkq - 3 7";
    // let position7 = "8/4k3/4b3/8/4Q3/8/6K1/8 w - - 0 1";
    // let position8 = "8/p7/4k3/1p5p/1P1r1K1P/P4P2/8/8 w - - 0 40";
    // let position9 = "r1bqkb1r/ppp2ppp/2n5/4p3/2p5/5NN1/PPPPQPPP/R1B1K2R b KQkq - 1 7";
    // let position10 = "8/3k4/3q4/8/3B4/3K4/8/8 w - - 0 1";
    // let position11 = "3r1rk1/pp3ppp/1qb1pn2/8/1PPb1B2/2N2B2/P1Q2PPP/3R1RK1 w - - 1 16";
    // let kiwipete = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
    // let cpw_position3 = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
    // let cpw_position4 = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
    // let cpw_position5 = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
    let cpw_position6 = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

    let mut pos = Position::from_fen(cpw_position6);

    match pos {
        Ok(ref mut pos) => {
            let depth = 6;
            let now = Instant::now();
            let perft_result = Perft::divide(pos, depth);
            let elapsed = now.elapsed();

            println!(
                "{}µs to calculate perft {}",
                elapsed.as_micros().separated_string(),
                depth
            );
            println!(
                "{} nodes/sec",
                ((perft_result.nodes * 1_000_000_000) / (elapsed.as_nanos() as usize))
                    .separated_string()
            );
            //============
        }
        Err(fen_error) => {
            println!("{}", fen_error.msg);
        }
    }
}
