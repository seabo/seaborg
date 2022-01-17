use crate::position::Position;
use crate::tables::TranspoTable;

#[derive(Clone)]
pub struct TTData {
    depth: usize,
    nodes: usize,
}

pub struct PerftWithTT<'a> {
    position: &'a mut Position,
    tt: TranspoTable<TTData>,
}

impl<'a> PerftWithTT<'a> {
    fn new(position: &'a mut Position) -> Self {
        Self {
            position,
            tt: TranspoTable::with_capacity(27),
        }
    }

    fn perft_inner(&mut self, depth: usize) -> usize {
        if depth == 0 {
            return 1;
        }

        let moves = self.position.generate_moves();

        if depth == 1 {
            let mut n = 0;
            for _ in &moves {
                n += 1;
            }
            return n;
        }

        match self.tt.get(self.position) {
            Some(data) => {
                if data.depth == depth {
                    return data.nodes;
                } else {
                    let mut n = 0;
                    for mov in &moves {
                        self.position.make_move(*mov);
                        n += self.perft_inner(depth - 1);
                        self.position.unmake_move();
                    }

                    self.tt.insert(self.position, TTData { depth, nodes: n });
                    return n;
                }
            }
            None => {
                let mut n = 0;
                for mov in &moves {
                    self.position.make_move(*mov);
                    n += self.perft_inner(depth - 1);
                    self.position.unmake_move();
                }
                self.tt.insert(self.position, TTData { depth, nodes: n });
                return n;
            }
        }
    }

    pub fn perft(position: &'a mut Position, depth: usize) -> usize {
        let mut perft = Self::new(position);
        let result = perft.perft_inner(depth);
        println!("COMPLETED PERFT. DISPLAYING TRANSPOSITION TABLE TRACE.");
        perft.tt.display_trace();
        result
    }
}
