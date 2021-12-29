use crate::movegen::MoveGen;
use crate::position::Position;

pub fn perft(position: &mut Position, depth: usize) -> usize {
    assert!(depth >= 1);
    let moves = MoveGen::generate_legal(&position);
    let mut node_count = 0;
    if depth == 1 {
        moves.len()
    } else {
        for mov in moves {
            let pre_move = position.clone();
            position.make_move(mov);
            node_count += perft(position, depth - 1);
            position.unmake_move();
            let post_move = position.clone();
            if pre_move != post_move {
                println!("{}", mov);
                println!("{:?}", position);
                println!("PRE======================\n{:?}", pre_move);
                println!("POST=====================\n{:?}", post_move);
                panic!();
            }
        }
        node_count
    }
}

pub fn divide(position: &mut Position, depth: usize) -> usize {
    assert!(depth >= 1);

    let moves = MoveGen::generate_legal(&position);
    let mut node_count = 0;

    if depth == 1 {
        for mov in moves {
            println!("{}: 1", mov);
            node_count += 1;
        }
    } else {
        for mov in moves {
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
