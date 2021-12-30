use crate::mov::Move;
use crate::movegen::MoveGen;
use crate::position::Position;

use separator::Separatable;

use std::fmt;

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct PerftData {
    pub nodes: usize,
    pub captures: usize,
    pub en_passant: usize,
    pub castles: usize,
}

impl fmt::Display for PerftData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f);
        writeln!(f, "Nodes: {}", self.nodes.separated_string())?;
        writeln!(f, "Ep: {}", self.en_passant.separated_string())?;
        writeln!(
            f,
            "Captures: {}",
            (self.captures + self.en_passant).separated_string()
        )?;
        writeln!(f, "Castles: {}", self.castles.separated_string())
    }
}

impl PerftData {
    pub fn new() -> Self {
        Self {
            nodes: 0,
            captures: 0,
            en_passant: 0,
            castles: 0,
        }
    }
}

pub struct Perft<'a> {
    position: &'a mut Position,
    pub data: PerftData,
}

impl<'a> Perft<'a> {
    fn new(position: &'a mut Position) -> Self {
        Self {
            position,
            data: PerftData::new(),
        }
    }

    fn perft_inner(&mut self, depth: usize) {
        if depth == 0 {
            self.data.nodes += 1;
        }

        let moves = MoveGen::generate(&self.position);

        for mov in moves {
            if !self.position.legal_move(mov) {
                continue;
            }

            if depth == 1 {
                self.handle_leaf(mov);
            } else {
                self.recurse(mov, depth - 1);
            }
        }
    }

    pub fn perft(position: &'a mut Position, depth: usize, print_data: bool) -> PerftData {
        let mut perft = Self::new(position);
        perft.perft_inner(depth);

        if print_data {
            println!("{}", perft.data);
        }

        perft.data
    }

    pub fn divide(position: &'a mut Position, depth: usize) -> PerftData {
        assert!(depth >= 1);
        let mut perft = Self::new(position);
        let moves = MoveGen::generate(&perft.position);
        for mov in moves {
            if !perft.position.legal_move(mov) {
                continue;
            }
            if depth == 1 {
                println!("{}: 1", mov);
                perft.handle_leaf(mov);
            } else {
                perft.position.make_move(mov);
                let node_perft_data = Perft::perft(perft.position, depth - 1, false);
                perft.data += &node_perft_data;
                println!("{}: {}", mov, node_perft_data.nodes.separated_string());
                perft.position.unmake_move();
            }
        }
        println!("{}", perft.data);
        perft.data
    }

    #[inline(always)]
    fn handle_leaf(&mut self, mov: Move) {
        self.data.nodes += 1;
        if mov.is_en_passant() {
            self.data.en_passant += 1;
        }

        if mov.is_capture() {
            self.data.captures += 1;
        }

        if mov.is_castle() {
            self.data.castles += 1;
        }
    }

    #[inline(always)]
    fn recurse(&mut self, mov: Move, depth: usize) {
        self.position.make_move(mov);
        self.perft_inner(depth);
        self.position.unmake_move();
    }
}

impl std::ops::AddAssign<&PerftData> for PerftData {
    fn add_assign(&mut self, rhs: &PerftData) {
        self.nodes += rhs.nodes;
        self.captures += rhs.captures;
        self.en_passant += rhs.en_passant;
        self.castles += rhs.castles;
    }
}
