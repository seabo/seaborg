//! Exact forced-mate proving over the move generator.
//!
//! This module answers one question with the rules alone: from a given position,
//! can the side to move force checkmate, and in how few plies? It is a complete
//! minimax over legal moves — the attacker (side to move) is credited with a mate
//! only when *every* defender reply still loses, so a positive answer is a
//! game-theoretic proof, not a heuristic estimate.
//!
//! It deliberately shares nothing with the heuristic search (`crate::search`): no
//! evaluation, no transposition table, no pruning that could discard a defence.
//! That independence is the point. A diagnostic that asks whether the *search*
//! finds a mate cannot use the search to decide whether the mate is real; the
//! ground truth has to come from the rules. The move generator this is built on
//! is the same perft-verified generator the engine plays with, so "a mate exists
//! here" means exactly "checkmate is forced under the rules of chess".
//!
//! Distances are counted in plies (half-moves) and are always odd: a forced mate
//! ends on one of the attacker's own moves, so the sequence has one more attacker
//! move than defender move. A mate "in N moves" is `2 * N - 1` plies.

use chess::mono_traits::{All, Legal};
use chess::mov::Move;
use chess::movelist::BasicMoveList;
use chess::position::Position;

/// The result of a bounded forced-mate search.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MateResult {
    /// The side to move forces checkmate; the value is the minimal length of the
    /// forced sequence in plies (always odd).
    Mate(u32),
    /// No forced mate exists within the ply bound. The search ran to completion,
    /// so this is a proof of absence *within the bound*, not merely a failure to
    /// find one.
    NoMate,
    /// The node budget was exhausted before the search finished. Nothing is
    /// proven either way; a caller building a verified set must discard the
    /// position rather than treat this as `NoMate`.
    Budget,
}

/// A bounded exact-mate search over the legal-move tree.
///
/// The node budget is the only thing standing between a caller and an
/// exponential tree on a position with no short mate. Hitting it yields
/// [`MateResult::Budget`]; it never causes a false `Mate` or a false `NoMate`,
/// because the budget is checked before a node expands, so any answer actually
/// returned was computed from a fully expanded subtree.
pub struct MateSolver {
    nodes: u64,
    budget: u64,
    over_budget: bool,
}

impl MateSolver {
    /// Create a solver that expands at most `node_budget` interior nodes.
    pub fn new(node_budget: u64) -> Self {
        Self {
            nodes: 0,
            budget: node_budget,
            over_budget: false,
        }
    }

    /// Number of interior nodes expanded so far. Useful when reusing one solver
    /// across many positions to bound total work.
    pub fn nodes(&self) -> u64 {
        self.nodes
    }

    fn tick(&mut self) -> bool {
        self.nodes += 1;
        if self.nodes > self.budget {
            self.over_budget = true;
        }
        self.over_budget
    }

    /// Prove the minimal forced-mate distance for the side to move, searching no
    /// deeper than `max_plies` plies.
    ///
    /// The returned distance is the game-theoretic optimum: the attacker plays
    /// the fastest mate, the defender the longest resistance.
    pub fn solve(&mut self, pos: &mut Position, max_plies: u32) -> MateResult {
        let dist = self.attack(pos, max_plies);
        if self.over_budget {
            MateResult::Budget
        } else {
            match dist {
                Some(d) => MateResult::Mate(d),
                None => MateResult::NoMate,
            }
        }
    }

    /// Every legal move for the side to move that keeps a forced mate within
    /// `mate_plies` plies. On a position whose proven distance is `mate_plies`
    /// these are exactly the optimal mating moves; a defender never appears here
    /// because the caller only asks this on attacker-to-move positions.
    ///
    /// Returns an empty vector if the budget was exhausted, so a caller must
    /// treat "empty" as inconclusive rather than "no winning move".
    pub fn winning_first_moves(&mut self, pos: &mut Position, mate_plies: u32) -> Vec<Move> {
        let mut winning = Vec::new();
        for mv in legal_moves(pos) {
            pos.make_move(&mv);
            let mates = if pos.in_checkmate() {
                mate_plies >= 1
            } else if mate_plies >= 3 {
                self.defend(pos, mate_plies - 1).is_some()
            } else {
                false
            };
            pos.unmake_move();
            if self.over_budget {
                return Vec::new();
            }
            if mates {
                winning.push(mv);
            }
        }
        winning
    }

    /// One optimal forced-mate line: the attacker's fastest mating move followed
    /// by the defender's longest defence, recursively. Returns `None` if no
    /// forced mate exists within `max_plies` or the budget was exhausted.
    ///
    /// The line is a concrete artefact a caller can replay independently with
    /// [`playout_is_mate`] as a second, structurally different check on the same
    /// claim.
    pub fn mate_line(&mut self, pos: &mut Position, max_plies: u32) -> Option<Vec<Move>> {
        let dist = match self.solve(pos, max_plies) {
            MateResult::Mate(d) => d,
            _ => return None,
        };
        // Reconstructing the line re-searches subtrees, and the initial proof
        // above has already spent part of the node budget. Give the build phase a
        // fresh budget: the proof completed within budget, so the subtrees it
        // revisits fit within budget too. Without this, a proof that consumed most
        // of the budget would starve the build and yield a truncated, non-mating
        // line.
        self.nodes = 0;
        self.over_budget = false;
        let mut line = Vec::new();
        self.build_line(pos, dist, &mut line);
        // A complete line has exactly `dist` plies and ends in mate. If the budget
        // was exhausted mid-build the line is incomplete; report that as no line
        // rather than hand back a sequence that does not mate.
        if self.over_budget || line.len() != dist as usize {
            return None;
        }
        Some(line)
    }

    /// Fill `line` with an optimal mating sequence of exactly `dist` plies.
    fn build_line(&mut self, pos: &mut Position, dist: u32, line: &mut Vec<Move>) {
        if dist == 0 {
            return;
        }
        // Attacker to move: pick a move that mates in exactly `dist`.
        for mv in legal_moves(pos) {
            pos.make_move(&mv);
            let achieved = if pos.in_checkmate() {
                dist == 1
            } else if dist >= 3 {
                self.defend(pos, dist - 1) == Some(dist - 1)
            } else {
                false
            };
            if achieved {
                line.push(mv);
                if dist > 1 {
                    self.build_defender_reply(pos, dist - 1, line);
                }
                pos.unmake_move();
                return;
            }
            pos.unmake_move();
        }
    }

    /// Append the defender's longest-resisting reply and the attacker's follow-up.
    fn build_defender_reply(&mut self, pos: &mut Position, budget: u32, line: &mut Vec<Move>) {
        // Defender to move: pick the reply that drags the mate out to the full
        // `budget`, matching the distance the minimax already proved.
        for mv in legal_moves(pos) {
            pos.make_move(&mv);
            let resists = self.attack(pos, budget - 1) == Some(budget - 1);
            if resists {
                line.push(mv);
                self.build_line(pos, budget - 1, line);
                pos.unmake_move();
                return;
            }
            pos.unmake_move();
        }
    }

    /// Attacker (side to move) node: minimal plies to force mate within `budget`,
    /// or `None` if the attacker cannot force mate that fast.
    fn attack(&mut self, pos: &mut Position, budget: u32) -> Option<u32> {
        if budget == 0 || self.tick() {
            return None;
        }
        let mut best: Option<u32> = None;
        for mv in legal_moves(pos) {
            // Once a mate in `b` is known, only a strictly shorter mate can
            // improve it, so later moves are searched with a tighter budget.
            // This preserves the minimax optimum while pruning most of the tree.
            let search_budget = match best {
                Some(1) => break,
                Some(b) => (b - 2).min(budget),
                None => budget,
            };
            pos.make_move(&mv);
            let dist = if pos.in_checkmate() {
                Some(1)
            } else if search_budget >= 3 {
                self.defend(pos, search_budget - 1).map(|k| k + 1)
            } else {
                None
            };
            pos.unmake_move();
            if self.over_budget {
                return None;
            }
            if let Some(d) = dist {
                best = Some(best.map_or(d, |b| b.min(d)));
            }
        }
        best
    }

    /// Defender (side to move) node: `Some(plies)` only if *every* legal reply
    /// still loses within `budget` plies, where `plies` is the longest such
    /// resistance. `None` the moment one reply escapes — including a stalemate,
    /// which is a legal escape from being mated, not a mate.
    fn defend(&mut self, pos: &mut Position, budget: u32) -> Option<u32> {
        if self.tick() {
            return None;
        }
        let moves = legal_moves(pos);
        if moves.is_empty() {
            // No legal move for the defender. Checkmate is impossible here: it is
            // caught by `in_checkmate` on the attacker's move before this node is
            // ever reached. So this is stalemate — the defender has escaped, and
            // the attacker's previous move does not force mate.
            return None;
        }
        let mut worst = 0;
        for mv in moves {
            pos.make_move(&mv);
            let dist = self.attack(pos, budget.saturating_sub(1));
            pos.unmake_move();
            if self.over_budget {
                return None;
            }
            // A reply the attacker cannot answer means the defence escapes: the
            // whole node fails, so propagate the `None` immediately.
            worst = worst.max(dist?);
        }
        Some(worst + 1)
    }
}

/// Collect the legal moves of `pos` into an owned vector, so the caller can make
/// and unmake moves on `pos` while iterating.
fn legal_moves(pos: &Position) -> Vec<Move> {
    pos.generate::<BasicMoveList, All, Legal>()
        .into_iter()
        .copied()
        .collect()
}

/// Find the legal move of `pos` whose UCI encoding is `uci`, if any.
///
/// Matching against generated legal moves both parses the string and confirms
/// the move is legal in this position, so a caller never has to trust an
/// externally supplied move.
pub fn find_legal_move(pos: &Position, uci: &str) -> Option<Move> {
    legal_moves(pos)
        .into_iter()
        .find(|mv| mv.to_uci_string() == uci)
}

/// Replay `line` from `pos` and report whether it is a legal sequence ending in
/// checkmate, with no earlier move ending the game.
///
/// This is an independent, purely mechanical check on a mate claim: it never
/// enumerates alternatives or reasons about forcing, it just plays the given
/// moves and looks at the final board. A caller that already has a minimax proof
/// can use it as a structurally different confirmation that the proven line
/// really does deliver mate.
pub fn playout_is_mate(pos: &mut Position, line: &[Move]) -> bool {
    if line.is_empty() {
        return false;
    }
    let mut made = 0;
    let mut ok = true;
    for (i, mv) in line.iter().enumerate() {
        let legal = legal_moves(pos);
        let matched = legal
            .iter()
            .find(|m| m.to_uci_string() == mv.to_uci_string());
        let Some(matched) = matched else {
            ok = false;
            break;
        };
        pos.make_move(matched);
        made += 1;
        let last = i + 1 == line.len();
        if !last && (pos.in_checkmate() || legal_moves(pos).is_empty()) {
            // The game ended before the final move, so the supplied line is not
            // a single coherent mating sequence.
            ok = false;
            break;
        }
        if last {
            ok = pos.in_checkmate();
        }
    }
    for _ in 0..made {
        pos.unmake_move();
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(fen: &str) -> Position {
        Position::from_fen(fen).expect("valid FEN")
    }

    /// An independent, deliberately naive reference solver for short mates: a
    /// fixed-depth minimax over the move generator with no pruning, no budget,
    /// and no shared code with [`MateSolver`]. Two independent implementations
    /// agreeing on many positions is a far stronger correctness signal than
    /// either checked against hand-computed distances. Depth is capped at
    /// mate-in-two (3 plies) to keep the unpruned tree small.
    fn ref_mate_distance(p: &mut Position, max_plies: u32) -> Option<u32> {
        ref_attack(p, max_plies)
    }

    fn ref_attack(p: &mut Position, budget: u32) -> Option<u32> {
        if budget == 0 {
            return None;
        }
        let mut best: Option<u32> = None;
        for mv in legal_moves(p) {
            p.make_move(&mv);
            let d = if p.in_checkmate() {
                Some(1)
            } else if budget >= 3 {
                ref_defend(p, budget - 1).map(|k| k + 1)
            } else {
                None
            };
            p.unmake_move();
            if let Some(d) = d {
                best = Some(best.map_or(d, |b| b.min(d)));
            }
        }
        best
    }

    fn ref_defend(p: &mut Position, budget: u32) -> Option<u32> {
        let moves = legal_moves(p);
        if moves.is_empty() {
            return None;
        }
        let mut worst = 0;
        for mv in moves {
            p.make_move(&mv);
            let d = ref_attack(p, budget.saturating_sub(1));
            p.unmake_move();
            worst = worst.max(d?);
        }
        Some(worst + 1)
    }

    #[test]
    fn accepts_a_mate_in_one() {
        // Black to move; Qh4# is the fool's mate, the only move that mates.
        let mut p = pos("rnbqkbnr/pppp1ppp/8/4p3/6P1/5P2/PPPPP2P/RNBQKBNR b KQkq g3 0 2");
        assert_eq!(
            MateSolver::new(1_000_000).solve(&mut p, 7),
            MateResult::Mate(1)
        );
        let winners = MateSolver::new(1_000_000).winning_first_moves(&mut p, 1);
        let uci: Vec<String> = winners.iter().map(|m| m.to_uci_string()).collect();
        assert_eq!(uci, vec!["d8h4".to_string()]);
    }

    #[test]
    fn accepts_a_back_rank_mate_in_one() {
        // White to move; Ra8# — the king on g8 is boxed in by its own pawns.
        let mut p = pos("6k1/5ppp/8/8/8/8/8/R6K w - - 0 1");
        assert_eq!(
            MateSolver::new(1_000_000).solve(&mut p, 7),
            MateResult::Mate(1)
        );
    }

    #[test]
    fn rejects_a_stalemate() {
        // Black to move, not in check, with no legal move: a draw, not a mate for
        // either side.
        let mut p = pos("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1");
        assert_eq!(
            MateSolver::new(1_000_000).solve(&mut p, 7),
            MateResult::NoMate
        );
    }

    #[test]
    fn rejects_the_opening_position() {
        // No side forces mate out of the gate.
        let mut p = Position::start_pos();
        assert_eq!(
            MateSolver::new(2_000_000).solve(&mut p, 5),
            MateResult::NoMate
        );
    }

    #[test]
    fn agrees_with_the_reference_solver_across_random_play() {
        // Walk a deterministic pseudo-random game and, at every position, require
        // the pruned production solver and the naive reference to return the same
        // mate distance for distances up to mate-in-two. This exercises the
        // minimax and the budget-narrowing prune on a wide variety of real trees.
        let mut state: u64 = 0x5EAB_0698_1234_5678;
        let mut next = || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545_f491_4f6c_dd1d)
        };

        let mut checked = 0;
        for _ in 0..40 {
            let mut p = Position::start_pos();
            for _ in 0..30 {
                let moves = legal_moves(&p);
                if moves.is_empty() {
                    break;
                }
                let expected = ref_mate_distance(&mut p, 3);
                let got = match MateSolver::new(2_000_000).solve(&mut p, 3) {
                    MateResult::Mate(d) => Some(d),
                    MateResult::NoMate => None,
                    MateResult::Budget => {
                        panic!("reference-scale search should not run out of budget")
                    }
                };
                assert_eq!(got, expected, "disagreement on {}", p.to_fen());
                checked += 1;
                let pick = (next() as usize) % moves.len();
                p.make_move(&moves[pick]);
            }
        }
        assert!(
            checked > 100,
            "expected to check many positions, got {checked}"
        );
    }

    #[test]
    fn playout_confirms_and_rejects_lines() {
        let mut p = pos("6k1/5ppp/8/8/8/8/8/R6K w - - 0 1");
        let line = MateSolver::new(1_000_000)
            .mate_line(&mut p, 7)
            .expect("a mate line exists");
        assert!(playout_is_mate(&mut p, &line));

        // A single non-mating move is not a mating line.
        let quiet = find_legal_move(&p, "h1g1").expect("Kg1 is legal");
        assert!(!playout_is_mate(&mut p, &[quiet]));
    }

    #[test]
    fn budget_exhaustion_is_reported_not_guessed() {
        // A tiny budget on the opening position cannot finish the search, and the
        // solver must say so rather than claim NoMate.
        let mut p = Position::start_pos();
        assert_eq!(MateSolver::new(10).solve(&mut p, 7), MateResult::Budget);
    }

    #[test]
    fn mate_line_survives_a_budget_the_proof_alone_exhausts() {
        // Proving the mate and reconstructing its line both cost nodes, and the
        // proof runs first. Give `mate_line` a budget of exactly what the proof
        // costs: the proof spends all of it, so the line only reconstructs if the
        // build phase is handed a fresh budget rather than the proof's leftovers.
        // Without that reset the build starves and returns a truncated, non-mating
        // line — a defect that surfaced on dense self-play positions. A small
        // king-and-queen mate keeps the tree tiny so this stays fast.
        let fen = "1k6/4Q3/8/8/4K3/8/8/8 w - - 95 115";
        let mut p = pos(fen);
        let cost = {
            let mut solver = MateSolver::new(50_000_000);
            assert_eq!(solver.solve(&mut p, 7), MateResult::Mate(5));
            solver.nodes()
        };
        let line = MateSolver::new(cost)
            .mate_line(&mut p, 7)
            .expect("the line must reconstruct on a fresh budget");
        assert_eq!(line.len(), 5, "a mate in three is five plies");
        assert!(playout_is_mate(&mut p, &line));
    }
}
