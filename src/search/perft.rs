use crate::mov::Move;
use crate::movegen::MoveGen;
use crate::position::Position;

use separator::Separatable;

use std::fmt;

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct PerftDataInternal {
    pub nodes: usize,
    pub captures: usize,
    pub en_passant: usize,
    pub castles: usize,
    pub promotions: usize,
    pub checkmate: usize,
    pub check: usize,
}

pub struct PerftData {
    pub nodes: Option<usize>,
    pub captures: Option<usize>,
    pub en_passant: Option<usize>,
    pub castles: Option<usize>,
    pub promotions: Option<usize>,
    pub checkmate: Option<usize>,
    pub check: Option<usize>,
}

impl PerftData {
    pub fn print_nodes(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(n) = self.nodes {
            writeln!(f, "Nodes:      {}", n.separated_string())
        } else {
            write!(f, "")
        }
    }

    pub fn print_captures(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(n) = self.captures {
            writeln!(f, "Captures:   {}", n.separated_string())
        } else {
            write!(f, "")
        }
    }

    pub fn print_en_passant(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(n) = self.en_passant {
            writeln!(f, "Ep:         {}", n.separated_string())
        } else {
            write!(f, "")
        }
    }

    pub fn print_castles(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(n) = self.castles {
            writeln!(f, "Castles:    {}", n.separated_string())
        } else {
            write!(f, "")
        }
    }

    pub fn print_promotions(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(n) = self.promotions {
            writeln!(f, "Promotions: {}", n.separated_string())
        } else {
            write!(f, "")
        }
    }

    pub fn print_checkmate(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(n) = self.checkmate {
            writeln!(f, "Checkmate: {}", n.separated_string())
        } else {
            write!(f, "")
        }
    }

    pub fn print_check(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(n) = self.check {
            writeln!(f, "Check:      {}", n.separated_string())
        } else {
            write!(f, "")
        }
    }
}

pub struct PerftOptions {
    /// Flag for whether this perft run should collect information about both
    /// checks and checkmates.
    pub collect_check_data: bool,
}

impl PerftOptions {
    pub fn new(collect_check_data: bool) -> Self {
        Self { collect_check_data }
    }
}

impl fmt::Display for Perft<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let out = self.output();
        writeln!(f)?;
        out.print_nodes(f)?;
        out.print_captures(f)?;
        out.print_en_passant(f)?;
        out.print_castles(f)?;
        out.print_promotions(f)?;
        out.print_checkmate(f)?;
        out.print_check(f)
    }
}

impl PerftDataInternal {
    pub fn new() -> Self {
        Self {
            nodes: 0,
            captures: 0,
            en_passant: 0,
            castles: 0,
            promotions: 0,
            checkmate: 0,
            check: 0,
        }
    }
}

pub struct Perft<'a> {
    options: PerftOptions,
    position: &'a mut Position,
    pub data: PerftDataInternal,
}

impl<'a> Perft<'a> {
    fn new(position: &'a mut Position, options: PerftOptions) -> Self {
        Self {
            options,
            position,
            data: PerftDataInternal::new(),
        }
    }

    fn output(&self) -> PerftData {
        let check: Option<usize>;
        let checkmate: Option<usize>;

        if self.options.collect_check_data {
            check = Some(self.data.check);
            checkmate = Some(self.data.checkmate);
        } else {
            check = None;
            checkmate = None;
        };

        PerftData {
            nodes: Some(self.data.nodes),
            captures: Some(self.data.captures),
            en_passant: Some(self.data.en_passant),
            castles: Some(self.data.castles),
            promotions: Some(self.data.promotions),
            checkmate,
            check,
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

    pub fn perft(
        position: &'a mut Position,
        depth: usize,
        collect_check_data: bool,
        print_data: bool,
    ) -> PerftData {
        let perft_options = PerftOptions::new(collect_check_data);
        let mut perft = Self::new(position, perft_options);
        perft.perft_inner(depth);

        if print_data {
            println!("{}", perft);
        }

        perft.output()
    }

    /// Runs the "divide" perft routine on the given position and to the given
    /// depth. The parameter `collect_check_data` determines whether to collect
    /// data about checks and checkmates in the leaf nodes (see tables at
    /// `https://www.chessprogramming.org/Perft_Results`). Collecting this information
    /// requires determining whether the leaf nodes are in checkmate, which is
    /// expensive (causes an extra movegen at each leaf node), so `divide()`
    /// becomes c.5-6x slower when running in this mode.  
    pub fn divide(position: &'a mut Position, depth: usize, collect_check_data: bool) -> PerftData {
        assert!(depth >= 1);
        let perft_options = PerftOptions::new(collect_check_data);
        let mut perft = Self::new(position, perft_options);
        let mut cumulative_nodes: usize = 0;
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
                perft.perft_inner(depth - 1);
                perft.position.unmake_move();
                let new_nodes_for_mov = perft.data.nodes - cumulative_nodes;
                println!("{}: {}", mov, new_nodes_for_mov.separated_string());
                cumulative_nodes += new_nodes_for_mov;
            }
        }
        println!("{}", perft);
        perft.output()
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

        if mov.is_promo() {
            self.data.promotions += 1;
        }

        if self.options.collect_check_data {
            self.position.make_move(mov);
            if self.position.in_checkmate() {
                self.data.checkmate += 1;
            }
            if self.position.in_double_check() {
                self.data.check += 1;
            }
            self.position.unmake_move();
        }
    }

    #[inline(always)]
    fn recurse(&mut self, mov: Move, depth: usize) {
        self.position.make_move(mov);
        self.perft_inner(depth);
        self.position.unmake_move();
    }
}
