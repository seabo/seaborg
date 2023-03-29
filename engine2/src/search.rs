use crate::history::HistoryTable;

use super::eval::Evaluation;
use super::info::{CurrMoveInfo, Info, PvInfo};
use super::killer::KillerTable;
use super::ordering::{Loader, OrderedMoves, ScoredMoveList, Scorer};
use super::pv_table::PVTable;
use super::score::Score;
use super::trace::Tracer;
use super::tt::{Bound, Table};

use core::mono_traits::{All as AllGen, Captures, Legal, QueenPromotions, Quiets};
use core::mov::Move;
use core::movelist::{BasicMoveList, MoveList};
use core::position::{Player, Position};

use separator::Separatable;

use std::ops::Neg;
use std::sync::atomic::{AtomicBool, Ordering};

/// Trait to monomorphize search functionality over different thread types: master and worker.
///
/// The master thread will perform slightly different functionality, such as printing UCI info
/// reports.
pub trait Thread {
    fn is_master() -> bool;
}

/// Dummy type representing the master search thread.
pub struct Master;
impl Thread for Master {
    fn is_master() -> bool {
        true
    }
}

/// Dummy type representing a worker thread.
pub struct Worker;
impl Thread for Worker {
    fn is_master() -> bool {
        false
    }
}

/// Trait to monomorphize search routine over the node type.
///
/// The three node types are PV, ALL and CUT.
///
/// * The root node is a PV node.
/// * The first child of a PV node is a PV node.
/// * Children of PV nodes that are searched with a zero-window are Cut nodes.
/// * Children of PV nodes that have to be re-search because the scout search failed high are PV
/// nodes.
/// * The first child of a Cut node and other candidate cutoff moves (nullmove, killers, captures,
/// checks) is an All node.
/// * A Cut node becomes an All node once all the candidate cutoff moves are searched.
/// * Children of All nodes are Cut nodes.
pub trait NodeType {
    fn pv() -> bool;
    fn cut() -> bool;
    fn all() -> bool;
    fn root() -> bool;
}

/// Dummy type representing a PV node.
pub struct Pv;
impl NodeType for Pv {
    fn pv() -> bool {
        true
    }
    fn cut() -> bool {
        false
    }
    fn all() -> bool {
        false
    }
    fn root() -> bool {
        false
    }
}

/// Dummy type representing a non-PV node.
pub struct NonPv;
impl NodeType for NonPv {
    fn pv() -> bool {
        false
    }
    fn cut() -> bool {
        false
    }
    fn all() -> bool {
        false
    }
    fn root() -> bool {
        false
    }
}

/// Dummy type representing a CUT node.
pub struct Cut;
impl NodeType for Cut {
    fn pv() -> bool {
        false
    }
    fn cut() -> bool {
        true
    }
    fn all() -> bool {
        false
    }
    fn root() -> bool {
        false
    }
}

/// Dummy type representing an ALL node.
pub struct All;
impl NodeType for All {
    fn pv() -> bool {
        false
    }
    fn cut() -> bool {
        false
    }
    fn all() -> bool {
        true
    }
    fn root() -> bool {
        false
    }
}

/// Dummy type representing the root node. This is also a PV node.
pub struct Root;
impl NodeType for Root {
    fn pv() -> bool {
        true
    }
    fn cut() -> bool {
        false
    }
    fn all() -> bool {
        false
    }
    fn root() -> bool {
        true
    }
}

/// Manages the search.
pub struct Search<'engine> {
    /// The internal board position.
    pub(super) pos: Position,
    /// Table for tracking the principal variation of the search.
    pvt: PVTable,
    /// Tracer to track search stats.
    trace: Tracer,
    /// The transposition table.
    tt: &'engine Table,
    /// The killer move table.
    kt: KillerTable,
    /// The history table.
    history: HistoryTable,
    /// Flag to indicate when the search should start unwinding due to user intervention.
    stopping: &'engine AtomicBool,
    /// Time to at which to end search.
    stop_time: Option<std::time::Instant>,
    search_depth: u8,
    depth_reached: u8,
}

impl<'engine> Search<'engine> {
    pub fn new(
        pos: Position,
        flag: &'engine AtomicBool,
        stop_time: Option<std::time::Instant>,
        tt: &'engine Table,
    ) -> Self {
        Self {
            pos,
            tt,
            kt: KillerTable::new(20),
            history: HistoryTable::new(),
            pvt: PVTable::new(8),
            trace: Tracer::new(),
            stopping: flag,
            stop_time,
            search_depth: 0,
            depth_reached: 0,
        }
    }

    pub fn start_search<T: Thread>(&mut self, d: u8) -> Score {
        self.trace = Tracer::new();

        // TODO: turn this into a proper use error.
        assert!(d > 0);

        // Some bookeeping and prep.
        self.tt.clear(); // TODO: we shouldn't have to do this. There is a bug somewhere.
        self.trace.commence_search();
        self.search_depth = d;

        let (score, best_move) = self.iterative_deepening::<T>(d);
        self.trace.end_search();

        if T::is_master() {
            self.report_telemetry(d, score);
            println!("bestmove {}", best_move);
        }

        self.history.reset();

        score
    }

    fn iterative_deepening<T: Thread>(&mut self, depth: u8) -> (Score, Move) {
        let mut score = Score::INF_N;
        let mut best_move = Move::null();

        for d in 1..=depth {
            if self.stopping() {
                break;
            }

            self.pvt = PVTable::new(d);
            self.search_depth = d;
            let value = self.alphabeta::<T, Root>(Score::INF_N, Score::INF_P, d);

            if !self.stopping() {
                score = value;
                best_move = match self.pvt.pv().into_iter().next() {
                    Some(mov) => *mov,
                    None => {
                        let entry = self.tt.probe(&self.pos).into_inner();
                        let tt_entry = entry.read();
                        assert!(!tt_entry.is_empty());
                        tt_entry.mov.to_move(&self.pos)
                    }
                };
                self.depth_reached = d;
            }

            if T::is_master() && !self.stopping() {
                self.report_pv(self.depth_reached, score);
            }
        }

        (score, best_move)
    }

    pub fn alphabeta<T: Thread, Node: NodeType>(
        &mut self,
        mut alpha: Score,
        mut beta: Score,
        depth: u8,
    ) -> Score {
        self.trace.visit_node();

        let draft = self.search_depth - depth;
        let mut tt_move = false;

        debug_assert!(Score::INF_N <= alpha);
        debug_assert!(alpha < beta);
        debug_assert!(beta <= Score::INF_P);
        debug_assert!(Node::pv() || alpha + Score::cp(1) == beta);

        // Step 1. Check for aborted search and immediate draw.
        if self.stopping() {
            // TODO: is this robust?
            return Score::zero();
        }
        // TODO: check for immediate draw.

        // Step 2. Mate distance pruning.
        if !Node::root() {
            // If we mate at the next move, the value at the root would be Mate(draft). If we
            // already have alpha greater than this, then we had a quicker mate elsewhere in the
            // tree. So we can prune here.
            alpha = std::cmp::max(Score::mate(draft as i8).neg(), alpha);
            beta = std::cmp::min(Score::mate(draft as i8 + 1), beta);
            if alpha >= beta {
                return alpha;
            }
        }

        // Step 3. Load transposition table entry.
        let (tt_entry, tt_mov) = {
            use super::tt::Probe::*;
            match self.tt.probe(&self.pos) {
                Hit(entry) => {
                    let e = entry.read();
                    if e.mov.is_null() {
                        (entry, None)
                    } else {
                        let mov = e.mov.to_move(&self.pos);
                        if self.pos.valid_move(&mov) {
                            self.trace.hash_hit();
                            tt_move = true;
                            (entry, Some(mov))
                        } else {
                            self.trace.hash_collision();
                            (entry, None)
                        }
                    }
                }
                Clash(entry) => {
                    self.trace.hash_clash();
                    (entry, None)
                }
                Empty(entry) => (entry, None),
            }
        };

        // Step 4. In non-PV nodes, check for early cutoff.
        if !Node::pv() {
            let entry = tt_entry.read();

            if !entry.is_empty() && entry.depth >= depth {
                match entry.bound() {
                    Bound::Exact => {
                        return entry.score;
                    }
                    Bound::Lower => {
                        if entry.score > beta {
                            return entry.score;
                        } else if entry.score > alpha {
                            alpha = entry.score
                        }
                    }
                    Bound::Upper => {
                        if entry.score < alpha {
                            return entry.score;
                        } else if entry.score < beta {
                            beta = entry.score
                        }
                    }
                }
            }

            if alpha == beta {
                return alpha;
            }
        }

        // Step 5. Straight to quiescence search if depth <= 0.
        if depth == 0 {
            let score = self.quiesce::<T, Node>(alpha, beta);
            if score == Score::mate(0) {
                self.pvt.pv_leaf_at(0);
            }

            return score;
        }

        // Step 6. Static evaluation.
        let eval = self.evaluate();

        // Step 7. Razoring.
        // When eval is very low, check with quiescence whether has any hope of raising alpha. If
        // not, return a fail low.
        //
        // TODO: this doesn't work because of overflowing subtraction. Perhaps we need to switch to
        // representing scores with an i64 so there's plenty of space.
        //
        // if eval < alpha - Score::cp(426) - Score::cp(252 * depth as i16 * depth as i16) {
        //     let value = self.quiesce::<Master, NonPv>(alpha - Score::cp(1), alpha);
        //     if value < alpha {
        //         return value;
        //     }
        // }

        // Step 8. Futility pruning.
        //         TODO

        // Step 9. Null move search with verification (non-PV only).
        //         TODO

        // Step 10. ProbCut.
        //         TODO

        // Step 11. In PV nodes, if the move is not in TT, decrease depth by 3.
        //          TODO

        // Step 12. If depth <= 0, run quiescence search.
        // if depth == 0 {
        //     let score = self.quiesce::<T>(alpha, beta);
        //     if score == Score::mate(0) {
        //         self.pvt.pv_leaf_at(0);
        //     }
        //     return score;
        // }

        // Step 13. In non-PV nodes with depth >= 7 and not in TT, decrease depth by 2.
        //          TODO

        // Step 14. If PV move and TT move failed low, this is a likely fail-low.
        //          TODO

        // Step 15. Iterate moves.
        let mut best_value = Score::INF_N;
        let mut best_move = Move::null();
        let mut moves = OrderedMoves::new();
        let mut move_count = 0;
        let mut did_raise_alpha = false;

        'move_loop: while moves.load_next_phase(MoveLoader::from(self, tt_mov, draft)) {
            for mov in &moves {
                if self.stopping() {
                    break 'move_loop;
                }

                move_count += 1;
                let mut value = Score::INF_N;

                // Start reporting which move we're considering after 3 seconds have elapsed.
                if T::is_master() && Node::root() && self.trace.live_elapsed().as_millis() > 3000 {
                    self.report_curr_move(depth, &mov, move_count);
                }

                // Step 16. Reductions & extensions.
                //          TODO

                // Step 17. Late move reduction.
                //          TODO

                // Step 18. Make the move.
                self.pos.make_move(mov);

                // Step 19. Search non-PV move with null window.
                if !Node::pv() || move_count > 1 {
                    value = self
                        .alphabeta::<T, NonPv>(-(alpha + Score::cp(1)), -alpha, depth - 1)
                        .neg()
                        .inc_mate();
                }

                // Step 20. Search PV move, or perform re-search if null window search failed high.
                //
                // If this is a PV node, do a full search on the first move and any move for which
                // the null-window search failed to produce a cutoff.
                if Node::pv()
                    && (move_count == 1 || (value > alpha && (Node::root() || value < beta)))
                {
                    value = self
                        .alphabeta::<T, Pv>(-beta, -alpha, depth - 1)
                        .neg()
                        .inc_mate();
                }

                debug_assert!(Node::pv() || !(value > alpha && (Node::root() || value < beta)));

                // Step 21. Undo move.
                self.pos.unmake_move();

                debug_assert!(value > Score::INF_N);
                debug_assert!(value < Score::INF_P);

                // Step 22. Check for new best move.
                if value > best_value {
                    best_value = value;

                    if value > alpha {
                        best_move = *mov;

                        self.pvt.copy_to(depth, *mov);

                        if Node::pv() && value < beta {
                            alpha = value;
                            did_raise_alpha = true;
                            // TODO: reduce depth on remaining moves.
                        } else {
                            debug_assert!(value >= beta);
                            break 'move_loop;
                        }
                    }
                }
            }
        }

        debug_assert!(
            move_count > 0 || self.pos.generate::<BasicMoveList, AllGen, Legal>().len() == 0
        );

        if self.stopping() {
            return Score::zero();
        }

        // Step 23. Check for mate and stalemate.
        if move_count == 0 {
            self.pvt.pv_leaf_at(depth);

            best_value = if self.pos.in_check() {
                Score::mate(0)
            } else {
                Score::cp(0)
            };
        }

        debug_assert!(best_value > Score::INF_N);

        // Step 24. Write node information to the transposition table.
        tt_entry.write(
            &self.pos,
            best_value,
            depth,
            if best_value >= beta {
                debug_assert!(!best_move.is_null());
                Bound::Lower
            } else if Node::pv() && !best_move.is_null() {
                debug_assert!(did_raise_alpha);
                Bound::Exact
            } else {
                debug_assert!(!did_raise_alpha);
                Bound::Upper
            },
            &best_move,
        );

        // Step 25. Return best value.
        best_value
    }

    #[inline(always)]
    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::Relaxed)
            || self
                .stop_time
                .map(|s| s <= std::time::Instant::now())
                .unwrap_or(false)
    }

    /// Returns the static evaluation, from the perspective of the side to move.
    #[inline(always)]
    fn evaluate(&mut self) -> Score {
        Score::cp(self.pos.material_eval() * self.pov())
    }

    /// Returns 1 if the player to move is White, -1 if Black. Useful wherever we are using
    /// evaluation functions in a negamax framework, and have to return the evaluation from the
    /// perspective of the side to move.
    #[inline(always)]
    fn pov(&self) -> i16 {
        match self.pos.turn() {
            Player::WHITE => 1,
            Player::BLACK => -1,
        }
    }

    /// The quiescence search.
    fn quiesce<T: Thread, Node: NodeType>(&mut self, mut alpha: Score, mut beta: Score) -> Score {
        self.trace.visit_q_node();

        debug_assert!(!Node::root());
        debug_assert!(Score::INF_N <= alpha);
        debug_assert!(alpha < beta);
        debug_assert!(beta <= Score::INF_P);
        debug_assert!(Node::pv() || alpha + Score::cp(1) == beta);

        if self.stopping() {
            // TODO: is this robust?
            return Score::zero();
        }

        // Step 1. Check for an immediate draw or max ply reached.
        //         TODO

        // Step 2. Load transposition table entry.
        let (tt_entry, tt_mov, tt_value) = {
            use super::tt::Probe::*;
            match self.tt.probe(&self.pos) {
                Hit(entry) => {
                    let e = entry.read();
                    if e.mov.is_null() {
                        (entry, None, None)
                    } else {
                        let mov = e.mov.to_move(&self.pos);
                        let val = e.score;
                        if self.pos.valid_move(&mov) {
                            self.trace.hash_hit();
                            (entry, Some(mov), Some(val))
                        } else {
                            self.trace.hash_collision();
                            (entry, None, None)
                        }
                    }
                }
                Clash(entry) => {
                    self.trace.hash_clash();
                    (entry, None, None)
                }
                Empty(entry) => (entry, None, None),
            }
        };

        // Step 3. Check for early TT cutoff.
        if !Node::pv() {
            let entry = tt_entry.read();

            if !entry.is_empty() && entry.depth >= 0 {
                match entry.bound() {
                    Bound::Exact => {
                        return entry.score;
                    }
                    Bound::Lower => {
                        if entry.score > beta {
                            return entry.score;
                        } else if entry.score > alpha {
                            alpha = entry.score
                        }
                    }
                    Bound::Upper => {
                        if entry.score < alpha {
                            return entry.score;
                        } else if entry.score < beta {
                            beta = entry.score
                        }
                    }
                }
            }

            if alpha == beta {
                return alpha;
            }
        }

        // Step 4. Static evaluation.
        let stand_pat = match tt_value {
            Some(s) => s,
            None => self.evaluate(),
        };

        if stand_pat >= beta {
            return beta;
        }

        if alpha < stand_pat {
            alpha = stand_pat;
        }

        // TODO: deal with this by looking at quiet check evasions, rather than going back to main
        // search.
        if self.pos.in_check() {
            // A one move search extension. The main alphabeta function will tell us if we are in
            // checkmate or stalemate, and if not, it will try the possible evasions.

            // Commenting out as this is currently causing stack overflows. We need to generate
            // evasions when in check, instead of dropping back to main search.
            // return self.alphabeta::<T, Pv>(alpha, beta, 1);
        }

        // TODO: use the ordered move system. We can make a different move loader for quiescence
        // which generates check evasions when necessary, and also queen promotions.
        let captures = self.pos.generate::<BasicMoveList, Captures, Legal>();
        let mut score: Score;

        // Step 5. Loop through all the moves until no moves remain or a beta cutoff occurs.
        for mov in &captures {
            // TODO: this now goes in the move loader.
            // Evaluate whether the capture is likely to be favourable with SEE.
            let see_eval = self.see(
                mov.orig(),
                mov.dest(),
                self.pos.piece_at_sq(mov.dest()).type_of(),
                self.pos.piece_at_sq(mov.orig()).type_of(),
            );

            if see_eval < Score::cp(0) {
                self.trace.see_skip_node();
                continue;
            }

            self.pos.make_move(mov);
            score = self.quiesce::<T, Node>(-beta, -alpha).neg().inc_mate();
            self.pos.unmake_move();

            if score >= beta {
                return beta;
            }

            if score > alpha {
                alpha = score;
            }
        }

        alpha
    }

    fn report_pv(&self, depth: u8, score: Score) {
        println!(
            "{}",
            Info::Pv(PvInfo {
                depth,
                score,
                time: self.trace.live_elapsed().as_millis() as usize,
                nodes: self.trace.nodes_visited(),
                pv: self
                    .pvt
                    .pv()
                    .map(|m| format!("{}", m))
                    .intersperse(" ".to_string())
                    .collect::<String>(),
                hashfull: self.tt.hashfull(),
                nps: self.trace.live_nps() as u32,
            })
        );
    }

    fn report_curr_move(&self, depth: u8, mov: &Move, num: u8) {
        println!(
            "{}",
            Info::CurrMove(CurrMoveInfo {
                depth,
                currmove: *mov,
                number: num,
            })
        );
    }

    /// Detailed debug info about the search, printed after the end of search in debug mode.
    fn report_telemetry(&self, depth: u8, score: Score) {
        if false {
            println!(
                "nodes:     {}",
                self.trace.all_nodes_visited().separated_string()
            );
            println!(
                "% q_nodes: {:.2}%",
                self.trace.q_nodes_visited() as f32 / self.trace.all_nodes_visited() as f32 * 100.0
            );
            println!(
                "nps:       {}",
                self.trace
                    .nps()
                    .expect("`end_search` was called, so this should always work")
                    .separated_string()
            );
            println!(
                "see skips: {}",
                self.trace.see_skipped_nodes().separated_string()
            );
            println!(
                "time:      {}ms",
                self.trace
                    .elapsed()
                    .expect("we called `end_search`")
                    .as_millis()
                    .separated_string()
            );
            println!(
                "eff. bf:   {}",
                self.trace.eff_branching(depth).separated_string()
            );
            println!("tt stats ----------------");
            println!(
                " size: {}MB, slots: {}",
                self.tt.capacity_mb(),
                self.tt.capacity_entries().separated_string()
            );
            println!(
                " hits:       {:>8} ({:.1}%)",
                self.trace.hash_hits().separated_string(),
                self.trace.hash_hits() as f64 / self.trace.hash_probes() as f64 * 100.
            );
            println!(
                " collisions: {:>8} ({:.1}%)",
                self.trace.hash_collisions().separated_string(),
                self.trace.hash_collisions() as f64 / self.trace.hash_probes() as f64 * 100.
            );
            println!(
                " clashes:    {:>8} ({:.1}%)",
                self.trace.hash_clashes().separated_string(),
                self.trace.hash_clashes() as f64 / self.trace.hash_probes() as f64 * 100.
            );
            println!(" hashfull: {:.2}%", self.tt.hashfull() as f64 / 10.);
            println!("-------------------------");
            println!(
                "pv:        {}",
                self.pvt
                    .pv()
                    .map(|m| m.to_uci_string())
                    .collect::<Vec<String>>()
                    .join(" ")
            );
            println!("score:     {:?}", score);
            println!(
                "tt move found at {:.2}% of nodes",
                self.trace.hash_found.avg() * 100_f64
            );
            println!(
                "killers found per node: {:.2}",
                self.trace.killers_per_node.avg() * 2_f64
            );
        }
    }

    fn report_best_move(&self) {
        // Get TT entry.
        let entry = self.tt.probe(&self.pos).into_inner();
        let tt_entry = entry.read();
        assert!(!tt_entry.is_empty());
        println!("bestmove {}", tt_entry.mov.to_move(&self.pos));
    }
}

pub struct MoveLoader<'a, 'search> {
    search: &'a mut Search<'search>,
    hash_move: Option<Move>,
    draft: u8,
}

impl<'a, 'engine> MoveLoader<'a, 'engine> {
    /// Create a `MoveLoader` from the passed `Search`.
    #[inline(always)]
    pub fn from(search: &'a mut Search<'engine>, hash_move: Option<Move>, draft: u8) -> Self {
        MoveLoader {
            search,
            hash_move,
            draft,
        }
    }
}

impl<'a, 'search> Loader for MoveLoader<'a, 'search> {
    #[inline]
    fn load_hash(&mut self, movelist: &mut ScoredMoveList) {
        match self.hash_move {
            Some(mv) => {
                self.search.trace.hash_found.push(1);
                movelist.push(mv)
            }
            None => {
                self.search.trace.hash_found.push(0);
            }
        }
    }

    fn load_promotions(&mut self, movelist: &mut ScoredMoveList) {
        self.search
            .pos
            .generate_in::<_, QueenPromotions, Legal>(movelist);
    }

    fn load_captures(&mut self, movelist: &mut ScoredMoveList) {
        self.search.pos.generate_in::<_, Captures, Legal>(movelist);
    }

    fn load_killers(&mut self, movelist: &mut ScoredMoveList) {
        let (km1, km2) = self.search.kt.probe(self.draft, &self.search.pos);
        let mut cnt = 0;

        if km1.is_some() {
            cnt += 1;
            movelist.push(km1.unwrap());
        }
        if km2.is_some() {
            cnt += 1;
            movelist.push(km2.unwrap());
        }
        self.search.trace.killers_per_node.push_many(cnt, 2);
    }

    fn load_quiets(&mut self, movelist: &mut ScoredMoveList) {
        self.search.pos.generate_in::<_, Quiets, Legal>(movelist);
    }

    fn score_captures(&mut self, captures: Scorer) {
        for (mov, score) in captures {
            if mov.is_capture() {
                *score = self
                    .search
                    .see(
                        mov.orig(),
                        mov.dest(),
                        self.search.pos.piece_at_sq(mov.dest()).type_of(),
                        self.search.pos.piece_at_sq(mov.orig()).type_of(),
                    )
                    .to_i16();
            }
        }
    }

    fn score_quiets(&mut self, quiets: Scorer) {
        let turn = self.search.pos.turn();
        for (mov, score) in quiets {
            // SAFETY: these are legal moves, so the squares must be valid.
            unsafe {
                *score = self
                    .search
                    .history
                    .get_unchecked(mov.orig(), mov.dest(), turn) as i16;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suite() -> Vec<(&'static str, u8, Score)> {
        // Test position tuples have the form:
        // (fen, depth, value from perpsective of side to move)

        #[rustfmt::skip]
        {
            vec![
                // Mates
                ("8/2R2pp1/k3p3/8/5Bn1/6P1/5r1r/1R4K1 w - - 4 3", 6, Score::mate(5)),
                ("5R2/1p1r2pk/p1n1B2p/2P1q3/2Pp4/P6b/1B1P4/2K3R1 w - - 5 3", 6, Score::mate(5)),
                ("1r6/p5pk/1q1p2pp/3P3P/4Q1P1/3p4/PP6/3KR3 w - - 0 36", 6, Score::mate(5)),
                ("1r4k1/p3p1bp/5P1r/3p2Q1/5R2/3Bq3/P1P2RP1/6K1 b - - 0 33", 6, Score::mate(5)),
                ("2q4k/3r3p/2p2P2/p7/2P5/P2Q2P1/5bK1/1R6 w - - 0 36", 6, Score::mate(5)),
                ("5rk1/rb3ppp/p7/1pn1q3/8/1BP2Q2/PP3PPP/3R1RK1 w - - 7 21", 6, Score::mate(5)),
                ("6rk/p7/1pq1p2p/4P3/5BrP/P3Qp2/1P1R1K1P/5R2 b - - 0 34", 8, Score::mate(7)),
                ("6k1/1p2qppp/4p3/8/p2PN3/P5QP/1r4PK/8 w - - 0 40", 6, Score::mate(5)),
                ("2R1bk2/p5pp/5p2/8/3n4/3p1B1P/PP1q1PP1/4R1K1 w - - 0 27", 6, Score::mate(5)),
                ("8/7R/r4pr1/5pkp/1R6/P5P1/5PK1/8 w - - 0 42", 6, Score::mate(5)),
                ("r5k1/2qn2pp/2nN1p2/3pP2Q/3P1p2/5N2/4B1PP/1b4K1 w - - 0 25", 8, Score::mate(7)),

                // Winning material
                ("rn1q1rk1/5pp1/pppb4/5Q1p/3P4/3BPP1P/PP3PK1/R1B2R2 b - - 1 15", 7, Score::cp(300)),
                ("4k3/8/8/4q3/8/8/7P/3K2R1 w - - 0 1", 3, Score::cp(100)), 
                ("6k1/8/3q4/8/8/3B4/2P5/1K1R4 w - - 0 1", 3, Score::cp(900)),
                ("r5k1/p1P5/8/8/8/8/3RK3/8 w - - 0 1", 6, Score::cp(900)),
                ("6k1/8/8/3q4/8/8/P7/1KNB4 w - - 0 1", 4, Score::cp(400)),
                ("2kr3r/ppp1qpb1/5n2/5b1p/6p1/1PNP4/PBPQBPPP/2KRR3 b - - 6 14", 5, Score::cp(400)),
                ("7k/2R5/8/8/6q1/7p/7P/7K w - - 0 1", 6, Score::cp(0)),

                // Pawn race
                // ("8/6pk/8/8/8/8/P7/K7 w - - 0 1", 22, Score::cp(800)),
            ]
        }
    }

    /// A regression test to ensure that our search routine produces the expected results for a
    /// range of positions.
    #[test]
    fn gives_correct_answers() {
        core::init::init_globals();

        let suite = suite();

        for (fen, depth, score) in suite {
            let pos = Position::from_fen(fen).unwrap();
            let flag = AtomicBool::new(false);
            let tt = Table::new(16);
            let mut search = Search::new(pos, &flag, None, &tt);
            let s = search.start_search::<Master>(depth);

            assert_eq!(s, score);
        }
    }
}
