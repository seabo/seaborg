use core::init::init_globals;
use core::position::Position;
use engine::search::perft::Perft;

use separator::Separatable;

use std::time::Instant;

pub fn perft(depth: u8) {
    init_globals();

    let start_position = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    // let kiwipete = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
    // let cpw_position3 = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
    // let cpw_position4 = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
    // let cpw_position5 = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
    // let cpw_position6 = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

    let mut pos = Position::from_fen(start_position);

    match pos {
        Ok(ref mut pos) => {
            let start_zob = pos.zobrist().clone();
            let now = Instant::now();
            let perft_result = Perft::divide(pos, depth as usize, false);
            let elapsed = now.elapsed();

            println!(
                "{}µs to calculate perft {}",
                elapsed.as_micros().separated_string(),
                depth
            );
            println!(
                "{} nodes/sec",
                ((perft_result.nodes.unwrap() * 1_000_000_000) / (elapsed.as_nanos() as usize))
                    .separated_string()
            );
            let end_zob = pos.zobrist().clone();
            println!();
            println!("Start zob: {}", start_zob);
            println!("End zob:   {}", end_zob);
            println!(
                "Zobrist keys {}differ",
                if start_zob != end_zob { "" } else { "do not " }
            );
            //============
        }
        Err(fen_error) => {
            println!("{}", fen_error.msg);
        }
    }
}
