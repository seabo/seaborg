use core::mov::Move;
use core::movelist::BasicMoveList;
use core::position::Position;

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
    /// Should this perft run collect detailed data on captures, en passant, castles and
    /// promotions.
    ///
    /// This does not include information on cheks and checkmates, as they are considerably more
    /// expensive to calculate. These are enabled with the `collect_check_date` option.
    pub detailed: bool,
    /// Should this perft run collect information about checks and checkmates.
    pub checks: bool,
}

impl PerftOptions {
    pub fn new(detailed: bool, checks: bool) -> Self {
        Self { detailed, checks }
    }
}

impl fmt::Display for Perft<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let out = self.output();
        writeln!(f)?;
        out.print_nodes(f)?;

        if self.options.detailed {
            out.print_captures(f)?;
            out.print_en_passant(f)?;
            out.print_castles(f)?;
            out.print_promotions(f)?;
            if self.options.checks {
                out.print_checkmate(f)?;
                out.print_check(f)?;
            }
        }
        write!(f, "")
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
    data: PerftDataInternal,
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

        if self.options.checks {
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

        let moves = self.position.generate_moves();

        if depth == 1 {
            self.handle_leaf(&moves);
        } else {
            for mov in &moves {
                self.recurse(mov, depth - 1);
            }
        }
    }

    pub fn perft(
        position: &'a mut Position,
        depth: usize,
        collect_detailed_data: bool,
        collect_check_data: bool,
        print_data: bool,
    ) -> PerftData {
        let perft_options = PerftOptions::new(collect_detailed_data, collect_check_data);
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
    pub fn divide(
        position: &'a mut Position,
        depth: usize,
        collect_detailed_data: bool,
        collect_check_data: bool,
    ) -> PerftData {
        assert!(depth >= 1);
        let perft_options = PerftOptions::new(collect_detailed_data, collect_check_data);
        let mut perft = Self::new(position, perft_options);
        let mut cumulative_nodes: usize = 0;
        let moves = perft.position.generate_moves();
        if depth == 1 {
            perft.handle_leaf(&moves);
            for mov in &moves {
                println!("{}: 1", mov);
            }
        } else {
            for mov in &moves {
                perft.recurse(mov, depth - 1);
                let new_nodes_for_mov = perft.data.nodes - cumulative_nodes;
                println!("{}: {}", mov, new_nodes_for_mov.separated_string());
                cumulative_nodes += new_nodes_for_mov;
            }
        }
        println!("{}", perft);
        perft.output()
    }

    #[inline(always)]
    fn handle_leaf(&mut self, moves: &BasicMoveList) {
        if !self.options.detailed && !self.options.checks {
            self.data.nodes += moves.len();
        } else {
            for mov in moves {
                if self.options.detailed {
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
                }

                if self.options.checks {
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
        }
    }

    #[inline(always)]
    fn recurse(&mut self, mov: &Move, depth: usize) {
        self.position.make_move(mov);
        self.perft_inner(depth);
        self.position.unmake_move();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::init::init_globals;
    use core::position::START_POSITION;

    fn setup() {
        init_globals();
    }

    fn run_perft(fen: &'static str, depth: usize) -> usize {
        let mut pos = Position::from_fen(fen).unwrap();
        let res = Perft::perft(&mut pos, depth, false, false, false);
        res.nodes.unwrap()
    }

    /// Run a comprehensive perft suite based on the position found at 
    /// the [chess programming wiki](https://www.chessprogramming.org/Perft_Results) 
    /// to test for any movegen, make move or unmake move regressions.
    #[rustfmt::skip]
    #[test]
    fn perft_suite() {
        setup();
        assert_eq!(run_perft(START_POSITION, 6), 119_060_324);
        assert_eq!(run_perft("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", 4), 4_085_603);
        assert_eq!(run_perft("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 6), 11_030_083);
        assert_eq!(run_perft("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1", 5), 15_833_292);
        assert_eq!(run_perft("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8", 5), 89_941_194);
        assert_eq!(run_perft("r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10", 5), 164_075_551);
    }
}
