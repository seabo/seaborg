use crate::mov::Move;
use crate::movegen::MoveGen;
use crate::position::Position;

pub struct Perft<'a> {
    position: &'a mut Position,
    pub nodes: usize,
    pub captures: usize,
    pub en_passant: usize,
    pub castles: usize,
}

impl<'a> Perft<'a> {
    fn new(position: &'a mut Position) -> Self {
        Self {
            position,
            nodes: 0,
            captures: 0,
            en_passant: 0,
            castles: 0,
        }
    }

    fn perft(&mut self, depth: usize) {
        if depth == 0 {
            self.nodes += 1;
        }

        let moves = MoveGen::generate(&self.position);

        for mov in moves {
            if !self.position.legal_move(mov) {
                continue;
            }

            if depth == 1 {
                self.nodes += 1;
                if mov.is_en_passant() {
                    self.en_passant += 1;
                }

                if mov.is_capture() {
                    self.captures += 1;
                }

                if mov.is_castle() {
                    self.castles += 1;
                }
            } else {
                self.position.make_move(mov);
                self.perft(depth - 1);
                self.position.unmake_move();
            }
        }
    }

    pub fn run_perft(position: &'a mut Position, depth: usize) -> Perft {
        let mut perft = Self::new(position);
        perft.perft(depth);
        perft
    }
}

pub fn perft(position: &mut Position, depth: usize) -> Perft {
    Perft::run_perft(position, depth)
}

pub fn divide(position: &mut Position, depth: usize) {
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
            println!("{}: {}", mov, node_perft.nodes);
            node_count += node_perft.nodes;
            position.unmake_move();
        }
    }
    println!("Total nodes searched: {}", node_count);
}
