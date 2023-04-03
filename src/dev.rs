use core::init::init_globals;
use core::position::Position;

pub fn dev() {
    init_globals();
    do_threefold_detect();
}

fn do_threefold_detect() {
    let mut pos =
        Position::from_fen("2rqkb1r/1p1n1ppp/p7/3NpP2/4n3/1P2B2P/1PP3P1/R2QKB1R w KQk - 0 13")
            .unwrap();
    pos.make_uci_move("d1g4");
    pos.make_uci_move("e4f6");
    pos.make_uci_move("g4d1");
    pos.make_uci_move("f6e4");
    pos.make_uci_move("d1g4");
    pos.make_uci_move("e4f6");
    pos.make_uci_move("g4d1");
    pos.make_uci_move("f6e4");

    let s = std::time::Instant::now();
    assert!(pos.in_threefold());
    let t = s.elapsed().as_nanos();
    println!("took {}ns to test for threefold", t);
}
