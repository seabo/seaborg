use crate::movegen::MoveGen;
use crate::position::{Position, Square};

pub fn perft(position: &mut Position, depth: usize) -> usize {
    if depth == 0 {
        return 1;
    }
    assert!(depth >= 1);
    let moves = MoveGen::generate(&position);
    let mut node_count = 0;

    for mov in moves {
        if !position.legal_move(mov) {
            continue;
        }

        if depth == 1 {
            node_count += 1
        } else {
            position.make_move(mov);
            node_count += perft(position, depth - 1);
            position.unmake_move();
        }
    }
    node_count
}

pub fn divide(position: &mut Position, depth: usize) -> usize {
    assert!(depth >= 1);

    let moves = MoveGen::generate(&position);
    let mut node_count = 0;

    for mov in moves {
        if !position.legal_move(mov) {
            continue;
        }

        if depth == 1 {
            println!("{}: 1", mov);
            node_count += 1;
        } else {
            position.make_move(mov);
            let node_perft = perft(position, depth - 1);
            println!("{}: {}", mov, node_perft);
            node_count += node_perft;
            position.unmake_move();
        }
    }
    println!("Total nodes searched: {}", node_count);
    node_count
}
