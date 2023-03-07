use core::init::init_globals;
use core::position::Position;
use engine::search::perft::Perft;
use engine2::perft::TESTS;

fn run_perft(fen: &str, depth: usize) -> usize {
    let mut pos = Position::from_fen(fen).unwrap();
    let res = Perft::perft(&mut pos, depth, false, false, false);
    res.nodes.unwrap()
}

#[test]
#[rustfmt::skip]
fn perft_suite() {
    init_globals();

    for (p, d, r) in TESTS {
        assert_eq!(run_perft(p, d), r);
    }
}
