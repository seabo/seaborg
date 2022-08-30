use core::init::init_globals;
use core::position::{Position, START_POSITION};
use engine::search::perft::Perft;

fn run_perft(fen: &str, depth: usize) -> usize {
    let mut pos = Position::from_fen(fen).unwrap();
    let res = Perft::perft(&mut pos, depth, false, false, false);
    res.nodes.unwrap()
}

#[test]
#[rustfmt::skip]
fn perft_suite() {
    init_globals();

    let tests = [
        // The following positions are taken from https://www.chessprogramming.org/Perft_Results
        (START_POSITION, 5, 4_865_609),
        ("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", 5, 193_690_690),
        ("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 6, 11_030_083),
        ("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1", 5, 15_833_292),
        ("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8", 5, 89_941_194),
        ("r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10", 5, 164_075_551),

        // The following positions are taken from the interesting positions section at the bottom
        // of https://www.codeproject.com/Articles/5313417/Worlds-Fastest-Bitboard-Chess-Movegenerator.
        // Reference results were calculated with Stockfish.
        ("rnb1kb1r/pp1p2pp/2p5/q7/8/3P4/PPP1PPPP/RN2KBNR w - - 0 1", 6, 97_149_646),
        ("1q6/8/8/3pP3/8/6K1/8/k7 w - d6 0 1", 6, 4_133_671),
        ("8/8/8/1q1pP1K1/8/8/8/k7 w - d6 0 1", 6, 4_305_206)
    ];

    for (p, d, r) in tests {
        assert_eq!(run_perft(p, d), r);
    }
}
