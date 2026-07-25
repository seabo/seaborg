use crate::continuation::{ContinuationHistory, CounterMoveTable, CONTINUATION_DISTANCES};
use crate::history::{CaptureHistory, HistoryTable, HISTORY_MAX};

use super::eval::{piece_value, EvalState, Evaluation, PAWN_VALUE};
use super::killer::KillerTable;
use super::nnue::{self, Accumulator, Network};
use super::ordering::{Loader, OrderedMoves, Phase, ScoredMoveList, Scorer};

// Re-exported so the ordering-ablation harness can label each build with the quiet-ordering design
// it was compiled against, exactly as [`KILLER_SLOTS`] labels the killer ablation.
pub use super::ordering::{EQUAL_CAPTURES_AFTER_REFUTATIONS, FOLD_COUNTER_INTO_QUIETS};
use super::pv_table::PVTable;
use super::score::Score;
use super::trace::Tracer;
use super::tt::{Bound, Snapshot, Table};

use chess::mono_traits::{All as AllGen, Captures, Legal, QueenPromotions, Quiets};
use chess::mov::Move;
use chess::movelist::{BasicMoveList, MoveList};
use chess::position::{Piece, PieceType, Player, Position, Square};

use separator::Separatable;

use std::ops::Neg;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::LazyLock;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

// The durable specification for a multi-worker (Lazy SMP) root search. The module's own `//!` docs
// carry the contract; keep it here (not as an outer doc on this line) so its intra-doc links
// resolve in the module's own scope.
pub mod team;

const MAX_DEPTH: u8 = 255;

/// Remaining search depth at a node.
///
/// Signed, and allowed to fall to or below zero, at which point the node hands over to quiescence.
/// An unsigned depth makes every reduction an underflow hazard: `depth - 1 - r` has no
/// representation once `r` exceeds the remaining depth, so each reduction would need its own
/// saturating guard, and one missed guard wraps to a near-infinite depth rather than failing
/// loudly. Letting depth go negative removes the hazard at the type level.
pub type Depth = i16;

/// Depth-squared history evidence, capped to the gravity table's representable range.
fn history_bonus(depth: Depth) -> i32 {
    i32::from(depth.max(1)).pow(2).min(HISTORY_MAX)
}

/// Preserve history ordering when the combined contextual score extends beyond the move list's
/// compact `i16`. The sum of plain and continuation history can exceed `i16`, so it is clamped
/// rather than wrapped; the clamp only bites when several tables agree strongly, where the exact
/// magnitude no longer changes which move is tried first.
fn history_ordering_score(value: i32) -> i16 {
    value.clamp(i16::MIN.into(), i16::MAX.into()) as i16
}

/// Largest magnitude the capture-history term may add to a capture's static-exchange ordering score.
///
/// Static exchange outcomes are whole multiples of the pawn value, so two captures of different
/// material outcome differ by at least one pawn. Keeping this bound strictly below half a pawn means
/// the full swing between two captures — at most `2 * CAPTURE_HISTORY_ORDER_MAX` — can never bridge a
/// one-pawn gap. Learned history therefore only ever reorders captures of *identical* material
/// outcome; it cannot promote a capture past one that wins more material, nor across the
/// static-exchange boundary that separates the good, equal and bad capture phases. That is what makes
/// capture history an intra-phase tie-break rather than a re-classification.
const CAPTURE_HISTORY_ORDER_MAX: i32 = PAWN_VALUE as i32 / 2 - 1;

/// Map a capture-history score in `-HISTORY_MAX..=HISTORY_MAX` onto the bounded ordering term added
/// to a capture's static-exchange score. The linear scaling keeps the sign and relative magnitude of
/// the evidence while confining it to `[-CAPTURE_HISTORY_ORDER_MAX, CAPTURE_HISTORY_ORDER_MAX]`.
fn capture_history_order_term(history: i32) -> i16 {
    (history * CAPTURE_HISTORY_ORDER_MAX / HISTORY_MAX) as i16
}

/// Score bonus added to the counter move when it is folded into the combined quiet ranking rather
/// than staged (see [`FOLD_COUNTER_INTO_QUIETS`]). Sized to a fully trained history entry so the
/// counter ranks highly among quiets without being placed unconditionally first — the distinction
/// the dedicated [`Phase::Counter`](super::ordering::Phase::Counter) stage makes and this fold does
/// not. Applied only in the folded measurement build; the shipped staged design never reads it.
const COUNTER_FOLD_BONUS: i32 = HISTORY_MAX;

/// The greatest ply from the root that per-ply state is kept for.
///
/// Ply is bounded so that the search stack, the killer table and the recursion itself have a
/// static limit. The main search hands over to quiescence on reaching it, which bounds the main
/// tree; quiescence has no cap of its own yet, and capping it is separate work.
pub const MAX_PLY: usize = 256;

/// Active recency slots per ply in the killer table.
///
/// Two is the shipped policy: the newest quiet refutation at a ply occupies slot one and the
/// previous distinct one shifts to slot two. Setting this to `1` keeps only the newest killer and
/// `0` disables killers entirely, which is how the disabled/one-slot/two-slot ablation is built
/// without a separate search path. It must not exceed
/// [`MAX_KILLER_SLOTS`](super::killer::MAX_KILLER_SLOTS). Public so the ablation harness in
/// `examples/killer_ablation.rs` can label its output with the width it was built against.
pub const KILLER_SLOTS: usize = 2;

/// Lowest iteration depth at which the root is searched with an aspiration window rather than the
/// full `(-inf, +inf)` window.
///
/// Two things set the floor. A shallow iteration searches a tiny tree, so a mispredicted narrow
/// window costs more in re-searches than the window saves; and iteration 1 must complete as a
/// single search to honour the guaranteed-first-ply contract (`min_search_complete`), which an
/// aspiration re-search loop would break. Keeping the floor above 1 covers both: the guaranteed
/// ply is always a single full-window search, and aspiration only engages once a previous score
/// exists to centre the window on and the tree is large enough to profit.
const ASPIRATION_MIN_DEPTH: u8 = 4;

/// Half-width, in centipawns, of the first aspiration window tried at each iteration.
///
/// The window is centred on the previous iteration's score. A score that moves by less than this
/// between iterations lands inside the window and needs no re-search; a larger swing fails a bound
/// and widens. The value trades the node savings of a tight window against the re-search cost of
/// guessing too tight.
///
/// Half a pawn is deliberately wide: the current evaluation is material-only, so a root score
/// routinely jumps by a whole pawn or more between iterations, and a tighter window would fail and
/// re-search more often than it would save. This wants revisiting once a finer-grained evaluation
/// makes successive scores move in smaller steps.
const ASPIRATION_INITIAL_DELTA: i16 = 50;

/// Growth factor applied to the failing side's half-width after each fail-high or fail-low.
///
/// Geometric growth bounds the number of re-searches to a logarithm of the eventual window width,
/// so a badly mispredicted score reaches a full window in a handful of steps rather than crawling
/// outward centipawn by centipawn.
const ASPIRATION_WIDEN_FACTOR: i16 = 2;

/// Half-width beyond which a widened aspiration bound is opened all the way to infinity.
///
/// Once the window is this wide, the odds that the true score sits just outside it are low enough
/// that another bounded re-search is not worth its cost, and snapping to infinity guarantees the
/// side can never fail again. That, with the mate short-circuit in [`Search::aspiration_search`],
/// is what makes the re-search loop terminate in a bounded number of steps.
const ASPIRATION_MAX_DELTA: i16 = 2_000;

/// Offset a node score outward by a centipawn half-width to form one edge of an aspiration window.
///
/// The result is a *window bound*, not a node score: it only has to be a threshold to compare
/// against, so it may sit at an infinity. Two cases open the bound fully. A mate (or any
/// non-centipawn) score cannot be nudged by a centipawn amount — mates and centipawns occupy
/// different bands and [`Score`] arithmetic would return the mate unchanged — so a mate collapses
/// the bound straight to the matching infinity, which both widens correctly and keeps the window
/// strictly ordered. A half-width past [`ASPIRATION_MAX_DELTA`] does the same, bounding the
/// re-search count. Otherwise the centre is a centipawn score and the offset stays inside the
/// centipawn band, where it is a valid, strictly ordered window edge.
fn aspiration_bound(centre: Score, delta: i16) -> Score {
    if !centre.is_cp() || delta.abs() > ASPIRATION_MAX_DELTA {
        return if delta < 0 {
            Score::INF_N
        } else {
            Score::INF_P
        };
    }
    let raw = i32::from(centre.to_i16()) + i32::from(delta);
    Score::cp(raw.clamp(-10_000, 10_000) as i16)
}

/// A node either completed with a usable score or aborted before establishing one.
type NodeResult = Option<Score>;

/// Extra razoring margin, in centipawns, demanded when the side to move is improving.
///
/// Razoring gives up on a node whose static evaluation sits so far below alpha that a quiescence
/// check is unlikely to rescue it. When the side to move is *improving* — doing better than it was
/// two plies ago — that verdict is less trustworthy, because the trend is upward, so the margin is
/// widened and the node is razored less readily. Razoring only fires at `depth <= 6`, where the
/// base margin is still small enough for this adjustment to change the decision; at higher draft
/// the depth-squared term dominates and razoring effectively never triggers anyway.
const RAZOR_IMPROVING_MARGIN: i16 = 64;

fn should_razor(depth: Depth, eval: Score, alpha: Score, improving: bool) -> bool {
    // The `depth <= 6` guard must be evaluated first: the depth-squared term overflows an `i16` at
    // the drafts a real search reaches, and only the short-circuit keeps it from being computed
    // there. The improving margin widens the threshold, razoring the node less readily.
    depth <= 6
        && alpha.is_cp()
        && eval
            + Score::cp(
                426 + 252 * depth * depth + if improving { RAZOR_IMPROVING_MARGIN } else { 0 },
            )
            < alpha
}

/// Largest remaining depth at which reverse futility pruning is considered.
///
/// Reverse futility pruning — also called static null move pruning — is the beta-side mirror of
/// razoring: when the static evaluation already stands a depth-scaled margin *above* beta, it bets
/// that no quiet reply will drag the score back below beta before the horizon, so the whole node
/// fails high without a move ever being generated. Like razoring the bet is only safe close to the
/// leaves; with more depth remaining the opponent has room to build a threat the static evaluation
/// cannot see, so the technique is switched off above this draft.
///
/// The bound is set lower than razoring's because this prune has no safety net. Razoring confirms
/// its verdict with a quiescence search and null-move pruning re-searches deep cutoffs, so both
/// refuse to fire on a node that actually holds a forced mate for the side to move: a free move
/// there lets the opponent escape and the verification fails. Reverse futility pruning searches
/// nothing — it returns on the static evaluation alone — so it cannot tell a quiet won position from
/// one hiding a mate, and above this draft it masks forced wins the full search would prove. The
/// shallow forced wins in the search regression suite are what pin it here: at depth three a
/// king-and-pawn win and the suite's short mates begin reporting a bare material score instead.
const REVERSE_FUTILITY_MAX_DEPTH: Depth = 2;

/// The reverse futility margin at a given remaining depth, in centipawns.
///
/// How far above beta the static evaluation must stand before the node is discarded unsearched. The
/// allowance grows with remaining depth because a deeper subtree gives the opponent more room to
/// claw the score back below beta, keeping the prune conservative exactly where the horizon is
/// further away.
///
/// The fixed base term matters as much as the slope. `evaluate` is material-only, so the signal
/// being compared ignores king safety, activity and pawn structure entirely; a thin margin would
/// prune whenever the evaluation drifts a little above beta, including where beta is low only
/// because the parent is probing with a pessimistic window rather than because the side to move
/// truly stands better. Demanding a multi-pawn surplus keeps the prune to positions where the
/// material edge is large enough to survive what the evaluation cannot see.
#[inline]
fn reverse_futility_margin(depth: Depth) -> Score {
    debug_assert!(depth >= 1);
    Score::cp(300 + 100 * depth)
}

/// Largest remaining depth at which futility pruning is considered.
///
/// Futility pruning bets that a quiet move cannot lift a static evaluation that already sits a
/// margin below alpha into a useful score before the horizon. That bet is only safe close to the
/// leaves: with more depth remaining, a quiet move has room to set up a threat the static
/// evaluation cannot see, so the technique is switched off entirely above this draft.
const FUTILITY_MAX_DEPTH: Depth = 6;

/// The futility margin at a given remaining depth, in centipawns.
///
/// This is how much a single quiet move is generously assumed to be able to improve the static
/// evaluation before the horizon. A quiet move whose node evaluates at `eval` can raise the score
/// to at most about `eval + margin`; if that still does not reach alpha, searching the move cannot
/// change this node's result and it is skipped. The allowance grows with remaining depth because a
/// deeper subtree has more room to realise a latent gain, keeping the pruning conservative exactly
/// where the horizon is further away.
#[inline]
fn futility_margin(depth: Depth) -> Score {
    debug_assert!(depth >= 1);
    Score::cp(100 + 100 * depth)
}

/// Threshold, in centipawns, at or below which a capture's static exchange evaluation marks it as
/// losing material and disqualifies it from a quiescence search.
///
/// Quiescence exists to resolve the material swings a fixed-depth search stops in the middle of, so
/// a capture the swing-off already scores as losing has nothing to resolve: playing it hands the
/// opponent a favourable recapture the static evaluation would then reward. SEE is a pure material
/// calculation and does not depend on evaluation quality, which is what makes discarding these
/// captures reliable rather than a gamble on the evaluation. Zero is the natural cut: a capture is
/// kept only when the exchange sequence is at worst even. A capture that gives check is exempt — the
/// in-check evasion path never reaches this cut, so a side genuinely in check keeps every reply, and
/// a checking capture at the horizon can still force mate.
const QUIESCENCE_SEE_THRESHOLD: i16 = 0;

/// Delta-pruning margin for quiescence captures, in centipawns.
///
/// Even a capture that wins material is not worth searching if the most it could add — the value of
/// the piece it takes, plus a promotion's gain — still leaves the side to move short of alpha by
/// more than this cushion. The cushion absorbs what a bare material count omits: a recapture may
/// open a file, a passed pawn may be created, positional compensation may exist. It is deliberately
/// generous so the cut only fires when no plausible tactic could bridge the gap. Like every static
/// margin it is meaningless against a mate-distance alpha and is not applied there.
const QUIESCENCE_DELTA_MARGIN: i16 = 200;

/// Smallest remaining depth at which a null-move search is attempted.
///
/// A null cutoff is a fail-high bound, not an exact score, so where it fires it replaces whatever a
/// full search of the node would have returned — including a forced mate the node was about to
/// prove. Close to the horizon the reduced null search cannot see such a mate, so the side about to
/// be mated appears to hold and the mate drops out of the line. Gating the technique above this
/// remaining depth leaves the last few plies before the horizon — where a shallow mate is proved —
/// searched in full, while still pruning the large subtrees higher up where null move earns almost
/// all of its saving. The bound is set where the engine stops losing exact-mate detection; the
/// shallow forced mates in the search regression suite are what pin it here.
const NULL_MOVE_MIN_DEPTH: Depth = 5;

/// Remaining depth at or above which a fail-high null search is confirmed by a verification search.
///
/// A shallow null cutoff is cheap and its blunders are bounded, so it is trusted directly. A deep
/// one commits a large subtree, and zugzwang — where passing is better than any legal move — makes
/// the null result unsound in exactly the endgames where the stakes are highest. From this draft
/// up, the cutoff is re-checked by a normal reduced-depth search with null moves disabled, and only
/// taken if that search also fails high.
const NULL_MOVE_VERIFY_DEPTH: Depth = 10;

/// The depth reduction applied to a null-move search at a given remaining depth.
///
/// The opponent is handed a free move and the resulting position is searched this much shallower
/// than the node itself. A larger reduction is cheaper but trusts the null result over a coarser
/// search; the depth term lets deeper nodes reduce a little more without collapsing shallow ones.
#[inline]
fn null_move_reduction(depth: Depth) -> Depth {
    3 + depth / 4
}

/// Smallest remaining depth at which late-move reduction is attempted.
///
/// A reduced move is re-searched at full depth if its shallow search unexpectedly beats alpha, so
/// the technique is self-correcting; but below this draft the subtree it would trim is already tiny
/// and the reduced-then-re-searched pair costs more than the single full search it replaces. Set at
/// three so a reduced scout still has at least one ply of its own — `new_depth` is at least two here
/// — leaving the depth-at-or-below-zero handover to quiescence untouched.
const LMR_MIN_DEPTH: Depth = 3;

/// Number of moves searched at full depth before late-move reduction engages.
///
/// Move ordering puts the moves most likely to be best first — the hash move, winning captures,
/// killers — so the opening moves of the list are the ones a reduction would most often have to undo
/// with a re-search. Reducing only from the move after this count keeps those full-depth while still
/// trimming the long tail of quiet moves that almost never raise alpha. The count is bypassed once a
/// move has already raised alpha at this node: the remaining moves are then scouted against a proven
/// bound and are reducible immediately.
const LMR_MOVE_THRESHOLD: u8 = 3;

/// Whether late-move reduction draws its base reduction from the log-shaped [`LmrTable`] rather than
/// the older hand-tuned step function.
///
/// Flip to `false` and rebuild to measure the table's own strength contribution against the step
/// function it replaced; the modulation toggles below stack on top of whichever base this selects.
pub const LMR_LOG_TABLE: bool = true;

/// Whether the base reduction is eased for quiet moves the ordering tables already trust and deepened
/// for those they distrust: a move with strong accumulated main-plus-continuation history is reduced
/// less, a poorly scored one more. Flip to `false` and rebuild to isolate this refinement's effect.
pub const LMR_HISTORY_MODULATION: bool = true;

/// Whether a side to move that is not improving takes one extra ply of reduction while an improving
/// side keeps the base. When the position is deteriorating the moves matter less and can be trimmed
/// harder. Flip to `false` and rebuild to isolate this refinement's effect.
pub const LMR_IMPROVING_MODULATION: bool = true;

/// Whether PV nodes and killer/counter moves are reduced less than a plain late quiet at the same
/// depth and move count, so the ordering prefix keeps its depth. Flip to `false` and rebuild to
/// isolate this refinement's effect.
pub const LMR_FAVOURED_MODULATION: bool = true;

/// Milliplies per whole ply. The base reduction and every modulation accumulate in this fixed-point
/// unit so fractional adjustments compose smoothly, and the sum is divided down to whole plies only
/// once, at the end of [`lmr_reduction`].
const LMR_PLY: i32 = 1024;

/// Constant term of the base reduction curve, in plies: the reduction a barely-late, barely-deep
/// move receives before the logarithmic growth term adds to it. Kept low so shallow, near-forcing
/// lines — where a mate or tactic can sit just past a one-ply cut — are reduced only a single ply,
/// matching the older step function there; the growth term is what makes deep late moves aggressive.
const LMR_BASE: f64 = 0.5;

/// Divisor of the logarithmic growth term. A smaller value steepens the curve, concentrating the
/// reduction on deep moves far down the ordering — the ones least likely to repay a full search —
/// where cuts of three or four plies pay off, while leaving shallow moves lightly reduced.
const LMR_DIVISOR: f64 = 2.0;

/// Side length of the square [`LmrTable`], covering every remaining depth and move count the search
/// can present. `MAX_DEPTH` is 255 and the move count is a `u8`, so both indices fit; the table
/// clamps anything at the boundary, where the reduction has long since saturated.
const MAX_LMR_DIM: usize = 256;

/// Divisor mapping a move's accumulated quiet history to a reduction adjustment in milliplies. A
/// smaller value lets history move the reduction more per unit of evidence; the result is clamped by
/// [`LMR_HISTORY_MAX_ADJUST`] so one extreme table entry cannot swamp the base reduction.
const LMR_HISTORY_DIVISOR: i32 = 40;

/// Largest reduction adjustment, in milliplies, the history term may contribute in either direction.
const LMR_HISTORY_MAX_ADJUST: i32 = 2 * LMR_PLY;

/// Base late-move reduction, in milliplies, as a function of remaining depth and move count.
///
/// The reduction grows like `LMR_BASE + ln(depth) * ln(move_count) / LMR_DIVISOR`: a shallow or early
/// move is barely reduced, while a move buried deep in the ordering far from the horizon — the one
/// least likely to repay a full search — is cut by several plies. Both logarithms are non-decreasing
/// and non-negative over the covered range, so the curve is monotonically non-decreasing in each
/// argument: a later or deeper move is never assigned a smaller base reduction than an earlier or
/// shallower one.
struct LmrTable {
    /// `MAX_LMR_DIM * MAX_LMR_DIM` milliplies values, row-major in remaining depth then move count.
    reductions: Box<[i32]>,
}

impl LmrTable {
    fn new() -> Self {
        let mut reductions = vec![0i32; MAX_LMR_DIM * MAX_LMR_DIM].into_boxed_slice();
        for depth in 0..MAX_LMR_DIM {
            for move_count in 0..MAX_LMR_DIM {
                // The logarithm is undefined at zero and negative just above it; a move at depth or
                // count zero never reaches reduction anyway, so the corner is pinned to no reduction.
                let milliplies = if depth == 0 || move_count == 0 {
                    0
                } else {
                    let growth = (depth as f64).ln() * (move_count as f64).ln() / LMR_DIVISOR;
                    ((LMR_BASE + growth) * LMR_PLY as f64).round().max(0.0) as i32
                };
                reductions[depth * MAX_LMR_DIM + move_count] = milliplies;
            }
        }
        Self { reductions }
    }

    /// Base reduction in milliplies for a remaining depth and move count, clamped to the table bounds.
    #[inline]
    fn base(&self, depth: Depth, move_count: u8) -> i32 {
        let d = (depth.max(0) as usize).min(MAX_LMR_DIM - 1);
        let m = (move_count as usize).min(MAX_LMR_DIM - 1);
        self.reductions[d * MAX_LMR_DIM + m]
    }
}

/// The reduction curve is a pure function of depth and move count, identical for every search, so it
/// is built once for the process rather than per search — a search is reconstructed every move and a
/// per-move rebuild of a 64K-entry table would be pure waste.
static LMR_TABLE: LazyLock<LmrTable> = LazyLock::new(LmrTable::new);

/// Plies removed from a late quiet move's zero-window scout search.
///
/// The base cut comes from the log-shaped [`LmrTable`] (or, when [`LMR_LOG_TABLE`] is off, the older
/// step function): it grows with both remaining depth and how far down the ordering the move sits,
/// because a move ordered late and searched deep is the one least likely to repay a full search. That
/// base is then modulated by signals the search has already computed, each behind its own toggle so a
/// strength match can isolate it:
///
/// * a move with strong accumulated quiet history (main plus continuation) is reduced less and a
///   poorly scored one more, since history is the engine's own estimate of how promising the move is;
/// * a non-improving side to move takes an extra ply, trimming harder in a deteriorating position;
/// * PV nodes and killer/counter moves are reduced less, keeping the trusted ordering prefix deep.
///
/// The growth is bounded and the result never negative: a modulation sum below zero means "do not
/// reduce", not "extend" — late-move *extensions* are out of scope. The caller keeps the safety
/// properties the reduction depends on: it is applied only to a quiet, non-checking, unextended move
/// past the first, it caps the result so the scout keeps at least one ply of its own, and any reduced
/// scout that raises alpha is re-searched at full depth before it can enter the PV.
///
/// `quiet_history` is the move's combined main-plus-continuation history for the side to move, and
/// `favoured` is true when the move is a killer or the counter move; both are meaningful only for the
/// quiet moves this reduction applies to and are ignored otherwise.
#[inline]
fn lmr_reduction(
    depth: Depth,
    move_count: u8,
    pv: bool,
    improving: bool,
    favoured: bool,
    quiet_history: i32,
) -> Depth {
    let mut r: i32 = if LMR_LOG_TABLE {
        LMR_TABLE.base(depth, move_count)
    } else {
        // The historical step function: one ply, and a second only for a move both deep in the tree
        // and well down the ordering.
        if depth >= 8 && move_count >= 8 {
            2 * LMR_PLY
        } else {
            LMR_PLY
        }
    };

    if LMR_HISTORY_MODULATION {
        let adjust = (quiet_history / LMR_HISTORY_DIVISOR)
            .clamp(-LMR_HISTORY_MAX_ADJUST, LMR_HISTORY_MAX_ADJUST);
        r -= adjust;
    }

    if LMR_IMPROVING_MODULATION && !improving {
        r += LMR_PLY;
    }

    if LMR_FAVOURED_MODULATION {
        if pv {
            r -= LMR_PLY;
        }
        if favoured {
            r -= LMR_PLY;
        }
    }

    // Whole plies, never negative. The caller further caps this so the reduced scout keeps a ply.
    (r.max(0) / LMR_PLY) as Depth
}

/// Largest remaining depth at which late-move (move-count) pruning is attempted.
///
/// Move-count pruning discards the tail of the quiet-move list outright, without any verifying
/// re-search — unlike late-move *reduction*, a move dropped here is never looked at again. That is
/// only safe close to the horizon, where a quiet move buried deep in the ordering is very unlikely to
/// be the one that changes the node's value and the subtree saved is small enough that the occasional
/// missed resource costs little. Higher up the tree the discarded subtrees are large: a single
/// overlooked quiet can be the first move of a forced mate whose remaining plies branch below this
/// node, and dropping it delays or hides that mate. The cap is deliberately low for a second reason
/// beyond safety — a search tree is overwhelmingly leaf-heavy, so nodes within three plies of the
/// horizon are the vast majority, and pruning only there already captures almost all of the node
/// saving while leaving the deeper mating and tactical lines fully searched. Raising the cap trims
/// comparatively few extra nodes yet starts to defer short forced mates the deeper nodes would
/// otherwise prove.
const LMP_MAX_DEPTH: Depth = 3;

/// Number of moves to search at a node before the remaining quiet moves are pruned by move count.
///
/// Once this many moves have been searched, every further move drawn from the history-ordered quiet
/// phase is discarded (a quiet move that gives check excepted; see the move loop). The count grows
/// with remaining depth: nearer the horizon a shallow search is coarse and the ordering less to be
/// trusted, so fewer moves are kept; a little deeper — still within [`LMP_MAX_DEPTH`] — the quiets are
/// ordered more reliably and the node can afford to look wider before giving up on the tail. Across
/// the pruned band this yields 3, 5 and 7 moves at remaining depths 1, 2 and 3. The constant term
/// keeps a floor of three moves searched at every depth so the promising prefix (hash move, captures,
/// killers, counter, and the best quiets) is never pruned.
///
/// The threshold counts *all* moves searched so far, not only quiets, because the quiet phase always
/// follows the hash move, captures and refutations in the ordering; a node with many captures has
/// therefore already spent part of its allowance before the first quiet, which is the intended
/// behaviour — a noisy position warrants pruning its quiet tail sooner.
#[inline]
fn late_move_count(depth: Depth) -> u8 {
    // Quadratic growth kept modest across the pruned band. `depth` is positive here — the caller
    // gates on `depth >= 1` and never calls past `LMP_MAX_DEPTH` — so the arithmetic cannot underflow
    // and the result is small enough to cast exactly.
    (3 + depth * depth / 2) as u8
}

/// Whether the side to move is doing better than the last time it was on move.
///
/// The *improving* signal compares this node's static evaluation against the static evaluation two
/// plies earlier — the same side's previous turn. A rising evaluation means the position is
/// consolidating and margin-based pruning can afford to be more cautious; a falling one means it is
/// deteriorating. Margin-based techniques are conventionally widened or narrowed on this basis.
///
/// It is deliberately conservative when the comparison cannot be made. The root and its immediate
/// child have no ply two steps back, and a node in check computes no static evaluation, so either
/// operand may be absent; in every such case the position is treated as *not* improving, which
/// applies the tighter margins rather than the more generous ones.
#[inline]
fn is_improving(current: Option<Score>, two_plies_ago: Option<Score>) -> bool {
    match (current, two_plies_ago) {
        (Some(now), Some(then)) => now > then,
        _ => false,
    }
}

/// Per-ply state for the node currently occupying that ply of the search path.
///
/// Search features routinely need to know something about an ancestor, or need somewhere to record
/// a decision that only makes sense for one node on the current path. Threading each such value
/// through the recursion separately does not scale and does not let a node inspect its parent at
/// all, so they live here, indexed by ply.
///
/// A slot holds whatever the node at that ply last wrote. Entries are not cleared between visits:
/// every field is written before it is read within a single node's lifetime, and a stale value
/// from a previously searched sibling is meaningless rather than dangerous.
#[derive(Clone, Copy, Debug)]
pub struct StackEntry {
    /// The static evaluation of the position at this ply, where one was computed. `None` at a node
    /// that returned before evaluating — a transposition cutoff, an immediate draw, or a node that
    /// went straight to quiescence.
    pub eval: Option<Score>,
    /// The move this node is currently searching, i.e. the move played to reach the child at
    /// `ply + 1`. Null before the move loop starts.
    pub mov: Move,
    /// The piece that made [`mov`](Self::mov), captured before the move is played so that the child
    /// and grandchild can key their continuation history on this move's *(piece, to)* context. The
    /// piece includes its colour. `Piece::None` when `mov` is null or unset, which reads as "no
    /// continuation context" and suppresses any continuation update or lookup keyed on this ply.
    pub moved_piece: Piece,
    /// A move this node must not search.
    ///
    /// Singular extensions establish that a move is the only good one by re-searching the node
    /// with that move excluded and checking that everything else fails low. Nothing sets this yet.
    /// A future user must also keep the excluded re-search out of the transposition table: its
    /// value describes a restricted move list, not the position, so publishing it under the
    /// position's key would hand a wrong value to every ordinary visit.
    pub excluded: Option<Move>,
}

impl Default for StackEntry {
    fn default() -> Self {
        Self {
            eval: None,
            mov: Move::null(),
            moved_piece: Piece::None,
            excluded: None,
        }
    }
}

/// The pair of wall-clock durations a timed search runs under.
///
/// A single deadline forces one figure to answer two different questions: how much this move is
/// worth, and how much it may cost. Those diverge exactly where it matters — a position whose root
/// best move is still changing is worth more than its share, and one whose answer settled three
/// iterations ago is worth less. Carrying both lets the search spend against the first and be
/// bounded by the second.
///
/// The `soft` figure is advisory and consulted only between iterations, so nothing about it can
/// abort a search mid-tree. The `hard` figure is the deadline proper: it is what
/// [`Search::stopping`] tests, and no search exceeds it beyond the guaranteed first ply that a
/// legal `bestmove` depends on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeBudget {
    soft: Duration,
    hard: Duration,
}

impl TimeBudget {
    /// A budget the search may extend into, up to `hard`, when the position turns out to warrant
    /// it. A `hard` below `soft` is raised to it rather than rejected, so the invariant
    /// `soft <= hard` holds by construction for every caller.
    pub fn new(soft: Duration, hard: Duration) -> Self {
        Self {
            soft,
            hard: hard.max(soft),
        }
    }

    /// A budget with no room to extend: the search plans to spend exactly this and may not exceed
    /// it. This is what an exact request such as `go movetime` asks for.
    pub fn fixed(duration: Duration) -> Self {
        Self {
            soft: duration,
            hard: duration,
        }
    }

    /// The time the search plans to spend, and will not start a new iteration past.
    pub fn soft(&self) -> Duration {
        self.soft
    }

    /// The time the search may not exceed under any circumstances.
    pub fn hard(&self) -> Duration {
        self.hard
    }
}

/// A limit controlling how long a search may run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchLimit {
    /// Search through the given depth.
    Depth(u8),
    /// Search under the given wall-clock budget.
    Time(TimeBudget),
    /// Search until the given number of nodes has been visited.
    ///
    /// Unlike a time or depth budget this is reproducible: the same position under the same
    /// budget on the same build visits the same nodes and returns the same move, because the count
    /// does not depend on machine speed, concurrent load, or the debug/release split. That is why
    /// it is the conventional budget for self-play data generation and for A/B testing search
    /// changes, and it is the meaning of the UCI `go nodes` parameter.
    Nodes(u64),
    /// Search until explicitly cancelled.
    Infinite,
}

/// A snapshot produced after an iterative-deepening iteration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchProgress {
    pub depth: u8,
    pub score: Score,
    pub elapsed: Duration,
    pub nodes: usize,
    pub nps: u32,
    pub hashfull: u16,
    pub principal_variation: Vec<Move>,
}

/// The move currently being considered at the root of the search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentMove {
    pub depth: u8,
    pub current_move: Move,
    pub number: u8,
}

/// A typed update emitted while a search is running.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchEvent {
    Progress(SearchProgress),
    CurrentMove(CurrentMove),
}

/// The final result from a completed iterative-deepening iteration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub score: Score,
    pub best_move: Option<Move>,
    pub depth: u8,
}

/// The reason a search stopped, together with its latest completed result, if any.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchOutcome {
    Completed(Option<SearchResult>),
    Cancelled(Option<SearchResult>),
}

impl SearchOutcome {
    pub fn result(&self) -> Option<&SearchResult> {
        match self {
            Self::Completed(result) | Self::Cancelled(result) => result.as_ref(),
        }
    }

    pub fn was_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled(_))
    }
}

/// Bounds placed on the branching factor measured between two consecutive iterations.
///
/// The raw ratio is not trustworthy on its own. A fail-high at the root can make one iteration
/// several times more expensive than the tree alone justifies, and a transposition table warmed by
/// the previous iteration can make the next one barely more expensive at all. Left unbounded,
/// either would be extrapolated into a prediction that stops the search far too early or far too
/// late. The bounds bracket the range real iterations actually occupy, so an outlier degrades the
/// prediction rather than dominating it.
const MIN_BRANCHING_FACTOR: f64 = 1.5;
const MAX_BRANCHING_FACTOR: f64 = 8.0;

/// The shortest iteration whose cost is taken as a measurement rather than as noise.
///
/// The early iterations of a search finish in microseconds, where scheduling jitter and the
/// clock's own resolution are the same size as the quantity being measured. Dividing by such a
/// figure produces a ratio that says nothing about the tree. Below this the prediction is
/// withheld entirely and the loop runs ungated, which is what it did before there was a
/// prediction at all.
const MIN_MEASURABLE_ITERATION: Duration = Duration::from_micros(500);

/// How much of its planned spend a search may add for a root best move that just changed.
///
/// A changed root move is the strongest evidence available that the previous iteration's answer
/// was wrong, and it arrives exactly when stopping would commit to the move being abandoned.
const BEST_MOVE_CHANGE_EXTENSION: f64 = 0.6;

/// The root score drop, in centipawns, that buys one whole extra planned spend.
///
/// A falling score means the position is worse than the last iteration believed and the search has
/// not yet found what to do about it. Scaling with the size of the drop rather than triggering on
/// any drop keeps the extension proportionate: a two-centipawn wobble between iterations is
/// ordinary and buys almost nothing.
const SCORE_DROP_PER_EXTENSION: f64 = 150.0;

/// The most a score drop alone may add, so that a single collapsing evaluation cannot ask for an
/// unbounded extension. The hard deadline bounds the total regardless; this bounds the request.
const MAX_SCORE_DROP_EXTENSION: f64 = 1.0;

/// The floor the soft-limit multiplier may be contracted to.
///
/// A settled position spends less than its planned share, but never so little that a genuine
/// resolution is starved of the depth to find it: a position that merely *looks* quiet for a few
/// iterations can still hide a reply that only surfaces deeper, and half the planned spend leaves
/// ample room to find it while still handing the rest of the clock to later moves. The hard
/// deadline is unaffected either way; this bounds only the planned spend.
const MIN_STABILITY_SCALE: f64 = 0.5;

/// The largest inter-iteration score swing, in centipawns, a position may show and still count as
/// flat.
///
/// A settled search returns very nearly the same score iteration to iteration; a couple of
/// centipawns is ordinary aspiration-window noise. A swing past this in *either* direction — not
/// only a drop — is evidence the answer is still moving, which resets the contraction rather than
/// deepening it.
const STABILITY_FLAT_MARGIN: i32 = 8;

/// How many consecutive settled iterations must accumulate before contraction begins.
///
/// One quiet iteration is not evidence the answer has stopped moving; a position often looks
/// settled for a ply just before the search finds the reply that unsettles it. Requiring a streak
/// keeps the guaranteed early plies at their full planned spend and only contracts once stability
/// is established.
const STABILITY_CONTRACTION_ONSET: u32 = 3;

/// How much of the planned spend each settled iteration past the onset removes, until the floor.
const STABILITY_CONTRACTION_PER_ITER: f64 = 0.1;

/// The multiplier applied to the planned spend, given what the last completed iteration revealed
/// about the position — in both directions.
///
/// Above 1 for an *unsettled* iteration — a root move that just changed, or a score that fell past
/// the flat margin — which is worth more than its planned share and may run into the extension
/// budget up to the hard deadline. Below 1 for a *settled* one — the same root move and a score
/// within the flat margin, held across enough consecutive iterations — which is worth less than its
/// share and hands the unspent clock to later moves. Exactly 1 in between.
///
/// `stable_iterations` is the caller's count of consecutive settled iterations, and it is nonzero
/// *if and only if* this iteration is settled: the caller resets it to zero the moment the root
/// move changes or the score leaves the flat margin (see the iterative-deepening loop). That is the
/// contract this function relies on to keep the two directions from competing. It matters because a
/// genuinely flat search does not hold its score perfectly still — it wobbles a few centipawns
/// either way between iterations — and treating any such wobble as a "fall" (as an unconditional
/// `score_drop > 0` extension would) vetoes the contraction on half the iterations of every flat
/// position, which is the whole set this lever exists to speed up. A sub-margin wobble is the flat
/// case, not a fall.
///
/// The result is only ever a *request*. The hard deadline still bounds an extension, so a large
/// scale on a short clock simply resolves to the hard deadline; and [`MIN_STABILITY_SCALE`] bounds
/// how far a settled position may pull its spend below optimum.
fn stability_scale(best_move_changed: bool, score_drop: i32, stable_iterations: u32) -> f64 {
    if stable_iterations == 0 {
        // Unsettled: extend exactly as before contraction existed. A changed root move is the
        // strongest evidence the previous answer was wrong; a fall past the flat margin says the
        // position is worse than believed and the search has not yet found what to do about it.
        // Only a drop counts — a rising score is the search finding more than it expected, not a
        // reason to distrust the move it is about to play.
        let mut scale = 1.0;
        if best_move_changed {
            scale += BEST_MOVE_CHANGE_EXTENSION;
        }
        if score_drop > 0 {
            scale +=
                (f64::from(score_drop) / SCORE_DROP_PER_EXTENSION).min(MAX_SCORE_DROP_EXTENSION);
        }
        return scale;
    }

    // Settled: contract, one step per iteration held past the onset, down to the floor.
    let steps = stable_iterations.saturating_sub(STABILITY_CONTRACTION_ONSET);
    let contraction = f64::from(steps) * STABILITY_CONTRACTION_PER_ITER;
    (1.0 - contraction).max(MIN_STABILITY_SCALE)
}

/// What the last two iterative-deepening iterations cost, and what that implies the next one will.
///
/// The estimate is measured rather than assumed because the branching factor is a property of the
/// position and the move ordering, not of the engine: a forcing position with one reasonable reply
/// per node grows far more slowly than an open middlegame, and a fixed constant chosen for one is
/// wrong for the other in whichever direction is more expensive.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct IterationCost {
    /// The cost of the iteration before `latest`, or `None` until two have completed.
    previous: Option<Duration>,
    latest: Option<Duration>,
}

impl IterationCost {
    fn record(&mut self, cost: Duration) {
        self.previous = self.latest;
        self.latest = Some(cost);
    }

    /// What the next iteration is expected to cost, or `None` where no honest estimate exists yet
    /// — fewer than two completed iterations, or a previous iteration too short to measure.
    ///
    /// A `None` means the caller must not gate on the prediction. That is the safe direction: it
    /// leaves the loop behaving as though there were no prediction at all, which is what the first
    /// couple of iterations of every search do.
    fn predict_next(&self) -> Option<Duration> {
        let (previous, latest) = (self.previous?, self.latest?);
        if previous < MIN_MEASURABLE_ITERATION {
            return None;
        }

        let branching_factor = (latest.as_secs_f64() / previous.as_secs_f64())
            .clamp(MIN_BRANCHING_FACTOR, MAX_BRANCHING_FACTOR);
        Some(latest.mul_f64(branching_factor))
    }
}

/// The planned spend of a timed search, kept as an origin and a duration rather than a single
/// instant so that the stability multiplier can be applied to it as a multiple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SoftLimit {
    /// The instant the search's clock started. Shared with the hard deadline, so both describe the
    /// same zero.
    start: Instant,
    /// What the search plans to spend, measured from `start`.
    budget: Duration,
}

impl SoftLimit {
    /// The instant this limit falls at once scaled by a stability factor.
    ///
    /// `scale` above 1 is the search's report that this position is worth more than its planned
    /// share; below 1, that it is worth less and the unspent clock should go to later moves. The
    /// factor is applied verbatim — [`stability_scale`] has already bounded it below by
    /// [`MIN_STABILITY_SCALE`], so there is no floor to reapply here. The result is not clamped to
    /// the hard deadline either: the caller compares against both, and conflating them would hide
    /// which one bound.
    fn deadline(&self, scale: f64) -> Instant {
        self.start + self.budget.mul_f64(scale)
    }
}

/// A [`TimeBudget`] resolved against the clock, as what a running search compares itself to.
///
/// Both are `None` for an untimed search — a depth, node, or infinite limit — which is what makes
/// every clock-related check in the search a no-op there rather than a branch on the limit kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Deadlines {
    /// When the search intends to have stopped. Advisory: consulted only between iterations.
    soft: Option<SoftLimit>,
    /// When the search must have stopped. This is the deadline [`Search::stopping`] enforces.
    hard: Option<Instant>,
}

impl Deadlines {
    /// The deadlines of a search that is not bounded by the clock at all.
    fn none() -> Self {
        Self {
            soft: None,
            hard: None,
        }
    }

    /// A search bounded only by a hard deadline, which therefore never declines to start an
    /// iteration and simply runs until the deadline aborts it.
    fn hard_only(hard: Option<Instant>) -> Self {
        Self { soft: None, hard }
    }

    fn from_budget(budget: TimeBudget) -> Self {
        let start = Instant::now();
        Self {
            soft: Some(SoftLimit {
                start,
                budget: budget.soft(),
            }),
            hard: Some(start + budget.hard()),
        }
    }
}

/// A clonable token used to cancel a running search.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A reusable owner of search resources.
pub struct SearchEngine {
    table: Arc<Table>,
    /// The NNUE network every started search evaluates with, or `None` for the hand-crafted
    /// evaluation.
    ///
    /// Held here so the choice outlives any single search: each move builds a fresh [`Search`] in
    /// [`SearchEngine::start_inner`], which clones this reference-counted handle rather than the
    /// weights.
    network: Option<Arc<Network>>,
}

impl SearchEngine {
    /// A fresh engine evaluating with the network built into this binary, or with the hand-crafted
    /// evaluation in a build that has none.
    ///
    /// Full strength is the default rather than something a caller opts into: every consumer that
    /// forgot to select a network would otherwise silently play weaker than the engine can. A
    /// caller that genuinely wants the hand-crafted evaluation — self-play generating the
    /// bootstrap generation's data, for one — says so with [`SearchEngine::set_network`].
    pub fn new(hash_size_mb: usize) -> Self {
        Self {
            table: Arc::new(Table::new(hash_size_mb)),
            network: nnue::built_in_network(),
        }
    }

    /// The network every subsequently started search will evaluate with, or `None` for the
    /// hand-crafted evaluation. Reported to the operator so a running process can be attributed to
    /// a specific evaluator.
    pub fn network(&self) -> Option<&Network> {
        self.network.as_deref()
    }

    /// Select the network every subsequently started search evaluates with, or `None` to restore
    /// the hand-crafted evaluation. Searches already running keep the handle they were started with.
    pub fn set_network(&mut self, network: Option<Arc<Network>>) {
        self.network = network;
    }

    /// The static evaluation of `pos` produced by exactly the evaluator a search would use — the
    /// selected NNUE forward pass, or the hand-crafted tapered evaluation when no network is set.
    ///
    /// This is the search leaf value with no search performed: it lets one position's evaluation be
    /// inspected in isolation, independent of tree shape, so the evaluation function can be measured
    /// apart from the search that normally consumes it. The sign convention matches the search leaf
    /// value in [`Search::evaluate`]: the score is from the side to move's perspective, with a
    /// positive value favouring the side to move.
    pub fn static_eval(&self, pos: &Position) -> i16 {
        match self.network.as_deref() {
            // The forward pass already returns the score from the side to move's perspective, so it
            // takes no perspective flip; the accumulator is rebuilt from the position here rather
            // than maintained incrementally. This mirrors the network branch of `Search::evaluate`.
            Some(network) => {
                let accumulator = Accumulator::from_position(network, pos);
                nnue::forward(network, &accumulator, pos.turn()) as i16
            }
            // The hand-crafted evaluation is from White's perspective, so it is flipped to the side
            // to move to match the network branch's convention.
            None => {
                let pov = match pos.turn() {
                    Player::WHITE => 1,
                    Player::BLACK => -1,
                };
                pos.static_eval() * pov
            }
        }
    }

    /// Invalidate the shared hash at an explicit administrative boundary.
    ///
    /// The ownership boundary is enforced rather than merely documented. [`Table::clear`] needs an
    /// exclusive reference, and `Arc::get_mut` only yields one once no worker holds a clone of the
    /// table — that is, once every search that could still be relying on its contents has finished.
    /// A caller that has not stopped its searches gets a panic here rather than silently pulling the
    /// table out from under a running worker.
    pub fn clear_hash(&mut self) {
        Arc::get_mut(&mut self.table)
            .expect("the hash cannot be cleared while a search still holds the table")
            .clear();
    }

    /// Reallocate the shared hash to `hash_mb` megabytes at an owner-controlled quiescent boundary.
    ///
    /// The replacement table is built before the live one is touched, so a failure to allocate it
    /// leaves the existing table — and the configuration that describes it — in place rather than
    /// dropping the engine into a state with no table. The swap is then gated on exclusivity exactly
    /// as [`clear_hash`](Self::clear_hash) is: `Arc::get_mut` only yields once every worker has
    /// released its clone, so a caller that has not stopped and joined its search first panics here
    /// rather than replacing an allocation a running worker is still probing.
    pub fn set_hash_size(&mut self, hash_mb: usize) {
        let replacement = Table::new(hash_mb);
        assert!(
            Arc::get_mut(&mut self.table).is_some(),
            "the hash cannot be resized while a search still holds the table"
        );
        self.table = Arc::new(replacement);
    }

    /// Begin a new game with an empty transposition table.
    ///
    /// Normal searches reuse the existing contents; only the session owner discards them.
    pub fn new_game(&mut self) {
        self.clear_hash();
    }

    /// Start searching a cloned position on a background thread.
    pub fn start(&self, position: Position, limit: SearchLimit) -> SearchHandle {
        self.start_inner(position, limit).0
    }

    /// Start a search while also handing back a clone of the worker's event `Sender`.
    ///
    /// Production callers use [`SearchEngine::start`] and drop the extra sender
    /// immediately. Tests retain it to hold the events channel open, which lets them
    /// assert that completion is observed through the explicit signal rather than
    /// through a channel disconnect.
    fn start_inner(
        &self,
        position: Position,
        limit: SearchLimit,
    ) -> (SearchHandle, Sender<SearchEvent>) {
        if let SearchLimit::Depth(depth) = limit {
            assert!(depth > 0, "search depth must be greater than zero");
        }

        let cancellation = CancellationToken::new();
        let thread_cancellation = cancellation.clone();
        // Stamp entries written from now on with a fresh age, so that when this search competes for
        // a slot with results left by earlier ones, the earlier ones are the cheaper thing to give
        // up. Ages never invalidate: everything already in the table stays readable.
        self.table.advance_age();
        let table = Arc::clone(&self.table);
        // Clone the reference-counted handle, not the weights, and hand it to the worker so the
        // network outlives this call on the search thread.
        let network = self.network.clone();
        let (events, receiver) = unbounded();
        let events_probe = events.clone();
        // Capacity 1 and a single send per worker, so signalling completion can never
        // block the worker thread on its way out.
        let (finished_tx, finished_rx) = bounded(1);
        let join = std::thread::spawn(move || {
            // Both deadlines are anchored to a single clock read taken here, on the worker, so the
            // soft and hard limits describe the same instant zero however long the spawn took.
            let (depth, deadlines, node_limit) = match limit {
                SearchLimit::Depth(depth) => (depth, Deadlines::none(), None),
                SearchLimit::Time(budget) => (MAX_DEPTH, Deadlines::from_budget(budget), None),
                SearchLimit::Nodes(nodes) => (MAX_DEPTH, Deadlines::none(), Some(nodes)),
                SearchLimit::Infinite => (MAX_DEPTH, Deadlines::none(), None),
            };
            let mut search = Search::with_events(
                position,
                &thread_cancellation.0,
                deadlines,
                node_limit,
                &table,
                events,
                network,
            );
            let result = search.run::<Master>(depth);
            let outcome = if thread_cancellation.is_cancelled() {
                SearchOutcome::Cancelled(result)
            } else {
                SearchOutcome::Completed(result)
            };
            // Release the event `Sender` before signalling, so a driver woken by the
            // signal finds the full event backlog already queued and terminated.
            drop(search);
            // The explicit completion signal. The driver must never have to infer that
            // this thread finished from the events channel disconnecting: that wakeup
            // has been observed to be lost, parking the driver forever.
            let _ = finished_tx.send(());
            outcome
        });

        let handle = SearchHandle {
            cancellation,
            events: receiver,
            finished: finished_rx,
            join: Some(join),
        };
        (handle, events_probe)
    }

    /// Test-only variant of [`SearchEngine::start`] that keeps the worker's event
    /// `Sender` alive, so the events channel never disconnects when the worker exits.
    #[cfg(test)]
    pub(crate) fn start_retaining_events(
        &self,
        position: Position,
        limit: SearchLimit,
    ) -> (SearchHandle, Sender<SearchEvent>) {
        self.start_inner(position, limit)
    }
}

/// Access to a running search's events, cancellation, and final outcome.
pub struct SearchHandle {
    cancellation: CancellationToken,
    events: Receiver<SearchEvent>,
    finished: Receiver<()>,
    join: Option<JoinHandle<SearchOutcome>>,
}

impl SearchHandle {
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn events(&self) -> &Receiver<SearchEvent> {
        &self.events
    }

    /// Receives exactly one message once the worker thread has finished, whether the
    /// search completed or was cancelled.
    ///
    /// This is the authoritative completion signal. Unlike the events channel
    /// disconnecting, it is an ordinary message send on a channel the driver is
    /// already selecting over.
    pub fn finished(&self) -> &Receiver<()> {
        &self.finished
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn is_finished(&self) -> bool {
        self.join.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn wait(mut self) -> SearchOutcome {
        self.join
            .take()
            .expect("search outcome was already taken")
            .join()
            .expect("search thread panicked")
    }
}

impl Drop for SearchHandle {
    /// Cancel the worker and wait for it to exit.
    ///
    /// Joining rather than detaching is what makes "no search is running" a structural property
    /// instead of a caller convention. The worker holds a clone of the shared transposition table,
    /// and [`SearchEngine::clear_hash`] needs an exclusive reference to it, so a detached worker
    /// outliving its handle would make an otherwise correct `ucinewgame` panic — intermittently,
    /// and pointing at the clear rather than at the drop that caused it. Once every handle either
    /// joins through [`SearchHandle::wait`] or joins here, no path can leave a worker behind.
    ///
    /// The join always terminates: cancellation is checked on the search hot path, and neither
    /// channel the worker writes on its way out can block it (the events channel is unbounded, and
    /// the completion channel has capacity for the single message ever sent on it).
    ///
    /// The join result is discarded. There is no consumer for the outcome here, and a worker that
    /// panicked must not panic this thread in turn: during unwinding that would abort the process.
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            self.cancel();
            let _ = join.join();
        }
    }
}

/// Trait to monomorphize search functionality over different thread types: master and worker.
///
/// The master thread emits typed search events while workers search silently.
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
///   nodes.
/// * The first child of a Cut node and other candidate cutoff moves (nullmove, killers, captures,
///   checks) is an All node.
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
    /// The static evaluation of `pos`, maintained incrementally in step with it.
    ///
    /// Rather than rescan the board at every leaf, the search updates this accumulator by the pieces
    /// each move touches (see [`Search::make_move`]) and restores it on unmake from `eval_stack`. It
    /// is seeded from `pos` when the search is built, so it is correct whatever position — including a
    /// clone taken to start a search — the search was handed. Under debug builds every make asserts it
    /// against a from-scratch recomputation, so a divergence surfaces at the node it happens on rather
    /// than as a mysterious later misvaluation.
    eval_state: EvalState,
    /// Saved evaluation accumulators, one per made-but-not-unmade move, newest last.
    ///
    /// `make_move` pushes the pre-move accumulator here and `unmake_move` pops it, so restoring the
    /// evaluation on unmake is an O(1) copy rather than a recomputation or a reverse-delta. It grows
    /// and shrinks in lockstep with the position's own move history.
    eval_stack: Vec<EvalState>,
    /// The selected NNUE network, or `None` to use the hand-crafted evaluation.
    ///
    /// This is the evaluation selector the design contract places at the single consumption point
    /// [`Search::evaluate`]: while it is `None` the search evaluates leaves with the tapered
    /// hand-crafted score exactly as before, and when a network is set it evaluates them through the
    /// scalar quantized forward pass instead. It stays `None` by default so the hand-crafted path
    /// remains the engine's evaluation until a trained network exists and passes its strength gate.
    ///
    /// The accumulator is rebuilt from the position at each evaluated leaf rather than maintained
    /// incrementally through make and unmake; the incremental accumulator seam is a later
    /// performance concern and is not needed for the correctness this scalar path provides.
    ///
    /// The network is held behind an [`Arc`] because a fresh `Search` is constructed for every move
    /// the engine plays; sharing the weights by reference-count keeps that per-move setup from
    /// deep-copying the whole parameter blob each time.
    network: Option<Arc<Network>>,
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
    /// One recorded quiet reply per preceding move — the counter-move heuristic.
    counter: CounterMoveTable,
    /// Bounded continuation-history evidence for the moves one and two plies back.
    cont_hist: ContinuationHistory,
    /// Bounded history for captures, keyed on the moving piece, destination and captured type. It
    /// breaks ties among captures of equal static exchange value in move ordering.
    capture_history: CaptureHistory,
    /// Counts every history-sensitive draw short-circuit taken during this search.
    ///
    /// A draw claimed by repetition or by the fifty-move rule is a property of how the position was
    /// reached, not of the position itself, so it is not covered by the Zobrist key. A node samples
    /// this counter before searching its children and compares it afterwards; if it moved, the
    /// node's value depends on the current history and must not be stored as position-intrinsic
    /// exact information. See `is_history_draw`.
    ///
    /// # Transposition-table reuse policy
    ///
    /// The Zobrist key covers pieces, side to move, castling rights and the en-passant file. It does
    /// not cover the halfmove clock or the move history, so a stored search value is only reusable
    /// where those uncovered parts of the state cannot change the answer. Three rules enforce that,
    /// and one known gap remains:
    ///
    /// 1. *Writes are suppressed for history-sensitive values.* A node whose subtree claimed a
    ///    repetition or fifty-move draw is not written at all (Step 24). Downgrading `Exact` to a
    ///    bound would not do: a draw score can raise a value to a beta cutoff as readily as it can
    ///    cap it, so the resulting bound is unsound in an incompatible history too. Consequently no
    ///    entry in the table embeds a draw that depends on how the position was reached.
    ///
    /// 2. *Reads are gated on the halfmove clock.* Because of rule 1, a stored value ignores the
    ///    fifty-move rule; `clock_permits_tt_reuse` therefore only allows a cutoff where the rule is
    ///    still out of reach within the stored depth.
    ///
    /// 3. *Leaf values are position-intrinsic.* `evaluate` does not read the clock, so the only
    ///    clock dependence left in a propagated score is the one rules 1 and 2 handle.
    ///
    /// # Known gap: repetition on the read side
    ///
    /// Rules 1 and 2 make a stored value independent of the history it was *computed* in. They do
    /// not make it valid in every history it is *read* in. A value computed where no descendant
    /// repeated can still be reused on a path where a descendant now repeats a position played
    /// before the root, and there the true value is a draw. This is the graph-history-interaction
    /// problem, and closing it needs entries keyed or gated by path history, which means reworking
    /// the table's layout, replacement policy and sizing. That is deliberately out of scope here;
    /// the engine accepts the resulting rare misvaluation, as mainstream engines do.
    ///
    /// Rule 1 applies to quiescence exactly as it does to the main search: `store_quiescence`
    /// carries the same comparison, so no writer of this table publishes a history-sensitive value.
    history_draws: u64,
    /// Flag to indicate when the search should start unwinding due to user intervention.
    stopping: &'engine AtomicBool,
    /// Time at which to end search. Nothing may run past this except the guaranteed first ply.
    stop_time: Option<Instant>,
    /// Time by which the search intends to have finished.
    ///
    /// Unlike [`Self::stop_time`] this never aborts anything: it is read once per completed
    /// iteration, in [`Self::iterative_deepening`], to decide whether the *next* iteration is worth
    /// beginning. Keeping it out of [`Self::stopping`] is deliberate — an advisory limit that could
    /// abort mid-tree would throw away the iteration it was trying to protect, which is the exact
    /// waste the split exists to remove.
    soft_limit: Option<SoftLimit>,
    /// Total node count at which to end search, if a node budget was set. Honoured on the same
    /// footing as the time deadline: suppressed until the guaranteed first ply completes, so a
    /// budget too small to finish a ply still returns a searched move rather than the unsearched
    /// fallback.
    node_limit: Option<u64>,
    /// Node count at the most recent deadline sample. `usize::MAX` means that a sampled deadline
    /// expired and remains latched while the search unwinds. Only the comparatively expensive
    /// clock read is throttled; the cancellation flag is still read on every call.
    last_deadline_check_nodes: Option<usize>,
    /// Whether the guaranteed-minimum search (one full ply) has completed. The time deadline is
    /// suppressed until this is set, so a search always returns a completed legal root move even
    /// when the allotted budget is zero or already elapsed.
    min_search_complete: bool,
    /// Whether a legal root fallback has been established. The explicit cancellation flag is
    /// suppressed until this is set, and from then on it aborts immediately: the fallback
    /// guarantees a legal bestmove without waiting for the (unbounded) depth-1 quiescence tree.
    root_fallback_ready: bool,
    /// The move to report if cancellation ends the search before any iteration completes. It starts
    /// as the first generated legal root move and is upgraded to the best fully searched root move
    /// as the first ply progresses. `None` only for a terminal root position.
    root_fallback: Option<Move>,
    #[cfg(test)]
    abort_after_nodes: Option<usize>,
    /// Test hook that disables the forward-pruning steps (futility, null move) so a test can search
    /// the same position with and without them and confirm the guards leave sound positions
    /// unchanged.
    #[cfg(test)]
    forward_pruning_disabled: bool,
    /// Test hook that disables late-move reduction so a test can search the same position at full
    /// depth and confirm the reduction, with its full-depth re-search, leaves a sound result
    /// unchanged while still shrinking the tree.
    #[cfg(test)]
    lmr_disabled: bool,
    /// Test hook that disables late-move (move-count) pruning so a test can search the same position
    /// with the quiet tail kept and confirm the prune shrinks the tree without changing a sound
    /// fixed-depth result.
    #[cfg(test)]
    lmp_disabled: bool,
    /// Test hook that disables reverse futility pruning so a test can search the same position with
    /// and without it and confirm the guards leave sound positions unchanged while it still shrinks
    /// the tree where it fires.
    #[cfg(test)]
    rfp_disabled: bool,
    /// Test hook that disables the search extensions so a test can search a position at exactly its
    /// nominal depth. Used where a test pins an exact fixed-depth score to an evaluation property and
    /// the deeper effective search an extension produces would otherwise move that score.
    #[cfg(test)]
    extensions_disabled: bool,
    /// Test hook that disables the quiescence static-exchange cuts — the losing-capture cut and the
    /// delta cut — so a test can search the same position with and without them and confirm they
    /// leave a materially sound result unchanged while still shrinking the tree.
    #[cfg(test)]
    see_pruning_disabled: bool,
    /// Destination for typed search progress events.
    events: Option<Sender<SearchEvent>>,
    /// Per-ply state for the nodes on the current search path, indexed by ply from the root.
    ///
    /// Boxed because it is far too large to sit in a stack frame, and allocated once per `Search`
    /// rather than per node.
    stack: Box<[StackEntry; MAX_PLY]>,
    depth_reached: u8,
    /// Plies below which null-move pruning is suppressed.
    ///
    /// Normally zero, so null moves are allowed at every ply. While a null-move cutoff is being
    /// confirmed by a verification search, this is raised to the verifying node's ply so that the
    /// verification re-searches the position with null moves switched off — otherwise the very
    /// cutoff under test could confirm itself. A node attempts a null move only when its ply is at
    /// or above this bound. See the null-move step in [`Search::search`].
    nmp_min_ply: usize,
}

impl<'engine> Search<'engine> {
    pub fn new(
        pos: Position,
        flag: &'engine AtomicBool,
        stop_time: Option<Instant>,
        tt: &'engine Table,
    ) -> Self {
        Self::build(
            pos,
            flag,
            Deadlines::hard_only(stop_time),
            None,
            tt,
            None,
            None,
        )
    }

    fn with_events(
        pos: Position,
        flag: &'engine AtomicBool,
        deadlines: Deadlines,
        node_limit: Option<u64>,
        tt: &'engine Table,
        events: Sender<SearchEvent>,
        network: Option<Arc<Network>>,
    ) -> Self {
        Self::build(pos, flag, deadlines, node_limit, tt, Some(events), network)
    }

    fn build(
        pos: Position,
        flag: &'engine AtomicBool,
        deadlines: Deadlines,
        node_limit: Option<u64>,
        tt: &'engine Table,
        events: Option<Sender<SearchEvent>>,
        network: Option<Arc<Network>>,
    ) -> Self {
        let eval_state = EvalState::from_position(&pos);
        Self {
            pos,
            eval_state,
            eval_stack: Vec::with_capacity(MAX_PLY),
            network,
            tt,
            kt: KillerTable::new(MAX_PLY, KILLER_SLOTS),
            history: HistoryTable::new(),
            counter: CounterMoveTable::new(),
            cont_hist: ContinuationHistory::new(),
            capture_history: CaptureHistory::new(),
            pvt: PVTable::new(8),
            trace: Tracer::new(),
            history_draws: 0,
            stopping: flag,
            stop_time: deadlines.hard,
            soft_limit: deadlines.soft,
            node_limit,
            last_deadline_check_nodes: None,
            events,
            stack: Box::new([StackEntry::default(); MAX_PLY]),
            depth_reached: 0,
            nmp_min_ply: 0,
            min_search_complete: false,
            root_fallback_ready: false,
            root_fallback: None,
            #[cfg(test)]
            abort_after_nodes: None,
            #[cfg(test)]
            forward_pruning_disabled: false,
            #[cfg(test)]
            lmr_disabled: false,
            #[cfg(test)]
            lmp_disabled: false,
            #[cfg(test)]
            rfp_disabled: false,
            #[cfg(test)]
            extensions_disabled: false,
            #[cfg(test)]
            see_pruning_disabled: false,
        }
    }

    /// Selects the evaluation the search uses at its leaves: a network enables the scalar quantized
    /// NNUE forward pass, and `None` restores the default hand-crafted tapered evaluation.
    ///
    /// Selection takes effect at [`Search::evaluate`]; nothing else in the search changes, so a
    /// search configured with a network still makes and unmakes moves and maintains the hand-crafted
    /// accumulator exactly as before — the network is consulted only when a leaf is scored.
    pub fn set_network(&mut self, network: Option<Arc<Network>>) {
        self.network = network;
    }

    pub fn run<T: Thread>(&mut self, d: u8) -> Option<SearchResult> {
        self.trace = Tracer::new();
        self.last_deadline_check_nodes = None;

        assert!(d > 0);

        // Some bookeeping and prep.
        let start_zob = self.pos.zobrist();

        self.trace.commence_search();
        self.min_search_complete = false;
        self.root_fallback_ready = false;
        self.root_fallback = None;

        let result = self.iterative_deepening::<T>(d);
        self.trace.end_search();

        assert_eq!(start_zob, self.pos.zobrist());

        if let Some(result) = &result {
            self.report_telemetry(d, result.score);
        }

        // Move-ordering memory is scoped to a single search. Within this call killers and history
        // are retained across the iterative-deepening iterations, where a refutation learned at a
        // shallow depth still holds at the next; but they are cleared here so the next search on this
        // worker starts from an empty table rather than inheriting refutations learned for an
        // unrelated position. Each Lazy SMP worker owns its own tables, so this resets only this
        // worker's state.
        self.history.reset();
        self.kt.reset();
        self.counter.reset();
        self.cont_hist.reset();
        self.capture_history.reset();

        result
    }

    /// The statistics gathered by the most recent [`Search::run`].
    ///
    /// Elapsed time alone cannot explain a change in search speed. A search that finishes sooner
    /// because it visited fewer nodes got better informed; one that finishes sooner over the same
    /// nodes got cheaper per node. Node counts and probe outcomes separate the two, and unlike the
    /// timings they are exact and reproduce run to run, so a measurement harness needs them
    /// alongside the clock.
    pub fn trace(&self) -> &Tracer {
        &self.trace
    }

    fn iterative_deepening<T: Thread>(&mut self, depth: u8) -> Option<SearchResult> {
        let mut result = None;

        self.establish_root_fallback();

        // The exact score of the deepest completed iteration, used to centre the next iteration's
        // aspiration window. `None` before any iteration completes, which forces a full window.
        let mut prev_score = None;
        // What the root looked like after the previous iteration, and what the last two iterations
        // cost. Together these decide whether the next iteration is begun at all.
        let mut prev_best_move = None;
        let mut cost = IterationCost::default();
        let mut elapsed_at_last_iteration = Duration::ZERO;
        let mut stability = 1.0;
        // Consecutive iterations whose root move held and whose score barely moved. Drives the
        // contraction: a position settled for long enough spends less than its planned share.
        let mut stable_iterations: u32 = 0;

        for d in 1..=depth {
            if self.stopping() {
                break;
            }

            // An iteration that cannot finish inside the budget is worth nothing: an aborted
            // iteration is discarded whole below, so the alternative to declining it is to spend
            // the remaining clock and return the previous iteration's move anyway. Declining hands
            // the unspent time to a later move instead. The guaranteed first ply is never declined
            // — `IterationCost` has nothing measured to decline it on — so the legal-bestmove
            // contract is untouched.
            if !self.next_iteration_fits(&cost, stability) {
                break;
            }

            let completed_pvt = std::mem::replace(&mut self.pvt, PVTable::new(d));
            let Some(value) = self.aspiration_search::<T>(d, prev_score) else {
                self.pvt = completed_pvt;
                break;
            };

            self.depth_reached = d;
            let best_move = self.pvt.pv().next().copied();

            // Measure this iteration before deciding anything about the next one. `live_elapsed`
            // is monotonic within a search, so the difference is this iteration's own cost.
            let elapsed = self.trace.live_elapsed();
            cost.record(elapsed.saturating_sub(elapsed_at_last_iteration));
            elapsed_at_last_iteration = elapsed;

            let best_move_changed = prev_best_move.is_some_and(|prev| Some(prev) != best_move);
            let score_delta = prev_score.map_or(0, |prev: Score| {
                i32::from(prev.to_i16()) - i32::from(value.to_i16())
            });

            // This iteration counts as settled only when there was a previous one to compare
            // against, the root move held, and the score barely moved in either direction. A large
            // swing either way — not only a drop — means the answer is still moving, so the streak
            // restarts rather than continuing to contract.
            let settled = prev_best_move.is_some()
                && !best_move_changed
                && score_delta.abs() <= STABILITY_FLAT_MARGIN;
            stable_iterations = if settled { stable_iterations + 1 } else { 0 };

            stability = stability_scale(best_move_changed, score_delta, stable_iterations);

            prev_score = Some(value);
            prev_best_move = best_move;
            result = Some(SearchResult {
                score: value,
                best_move,
                depth: d,
            });
            if T::is_master() {
                self.emit_progress(d, value);
            }

            // The first full ply is guaranteed to run against the clock; from here on the time-based
            // deadline is honored so deeper iterations respect the allotted clock.
            self.min_search_complete = true;
        }

        // A completed iteration can carry an exact score but no move: a root already drawn by
        // repetition scores zero without any move raising alpha, leaving the principal variation
        // empty. Reporting that as-is hands back a null move — a `bestmove 0000` forfeit — even
        // though legal moves exist, so substitute the guaranteed first legal move while keeping the
        // iteration's score. A *terminal* root (checkmate or stalemate) has no fallback move, so it
        // correctly keeps its move-less result. Forward pruning makes the drawn case easy to hit by
        // racing a dead-drawn endgame to a very high depth in a sliver of time.
        result
            .map(|completed| match completed.best_move {
                None => match self.root_fallback {
                    Some(fallback) => SearchResult {
                        best_move: Some(fallback),
                        ..completed
                    },
                    None => completed,
                },
                Some(_) => completed,
            })
            // Cancellation can also end the search before any iteration completes. Report the
            // fallback so the position's legal move is still played; a terminal root has none, which
            // UCI renders as `bestmove 0000`. The score is not a search result and the depth records
            // that no iteration finished, so neither is reported as one.
            .or_else(|| {
                self.root_fallback.map(|best_move| SearchResult {
                    score: Score::zero(),
                    best_move: Some(best_move),
                    depth: 0,
                })
            })
    }

    /// Search iteration `d` at the root, narrowing the window around the previous iteration's
    /// score where that is worthwhile.
    ///
    /// A full-window root search re-derives the position's value from `(-inf, +inf)` every
    /// iteration and forfeits every cutoff a tighter window would have produced throughout the
    /// tree. Successive iterations usually return nearly the same score, so a window centred on
    /// [`prev`] and only a little wider than the expected swing lets far more of the tree fail its
    /// bounds cheaply, while a fail-high or fail-low re-search recovers the exact score whenever the
    /// guess was too tight.
    ///
    /// The returned score, when `Some`, always comes from a search whose window strictly contained
    /// it: a fail-low or fail-high loops with a widened bound rather than reporting the bound as a
    /// result. `None` propagates an aborted search unchanged, so the caller discards the iteration
    /// and restores the previous principal variation — an aborted subtree must never commit a
    /// bound as a result.
    fn aspiration_search<T: Thread>(&mut self, d: u8, prev: Option<Score>) -> NodeResult {
        // Decide whether a narrow window is worth it. Below the minimum depth, before any score
        // exists to centre on, or when the previous score is a mate — which a centipawn window
        // cannot bracket at all — fall back to the full window. Keeping the floor above depth 1
        // also makes the guaranteed first ply a single search rather than a re-search loop.
        let Some(centre) = prev.filter(|p| d >= ASPIRATION_MIN_DEPTH && !p.is_mate()) else {
            return self.search::<T, Root>(Score::INF_N, Score::INF_P, Depth::from(d), 0);
        };

        let mut lo_delta = ASPIRATION_INITIAL_DELTA;
        let mut hi_delta = ASPIRATION_INITIAL_DELTA;
        let mut alpha = aspiration_bound(centre, -lo_delta);
        let mut beta = aspiration_bound(centre, hi_delta);

        loop {
            let value = self.search::<T, Root>(alpha, beta, Depth::from(d), 0)?;

            if value <= alpha {
                // Fail low: the true score is at or below alpha. Widen downward and re-search,
                // keeping beta so a subsequent fail high is still detected. A mate return means
                // being mated; no centipawn window can bracket it, so open alpha fully at once.
                if value.is_mate() {
                    alpha = Score::INF_N;
                } else {
                    lo_delta = lo_delta.saturating_mul(ASPIRATION_WIDEN_FACTOR);
                    alpha = aspiration_bound(centre, -lo_delta);
                }
            } else if value >= beta {
                // Fail high, the mirror of the above: widen beta upward, snapping to infinity for a
                // mate score.
                if value.is_mate() {
                    beta = Score::INF_P;
                } else {
                    hi_delta = hi_delta.saturating_mul(ASPIRATION_WIDEN_FACTOR);
                    beta = aspiration_bound(centre, hi_delta);
                }
            } else {
                // The score is strictly inside the window, so it is exact and the principal
                // variation this search built is the one to report.
                return Some(value);
            }
        }
    }

    /// Whether the next iterative-deepening iteration is expected to complete within the budget.
    ///
    /// `true` whenever there is nothing to decide on: an untimed search has no soft limit, and a
    /// search too young to have measured two iterations has no prediction. Both cases leave the
    /// loop running exactly as it would with no prediction, so the decision only ever *removes*
    /// work that was going to be discarded.
    ///
    /// The deadline compared against is the planned spend scaled by `scale`, but never past the
    /// hard deadline: an extension the clock cannot fund is not an extension. A `scale` below 1
    /// contracts the deadline instead, declining a next iteration a settled position no longer
    /// needs. Note that the hard deadline binding here does not by itself stop the search — it
    /// stops the *next* iteration from starting, and [`Self::stopping`] remains the only thing that
    /// aborts one.
    fn next_iteration_fits(&self, cost: &IterationCost, scale: f64) -> bool {
        let Some(soft) = self.soft_limit else {
            return true;
        };
        let Some(predicted) = cost.predict_next() else {
            return true;
        };

        let mut deadline = soft.deadline(scale);
        if let Some(hard) = self.stop_time {
            deadline = deadline.min(hard);
        }

        // A predicted finish the clock cannot even represent is past every deadline there is.
        Instant::now()
            .checked_add(predicted)
            .is_some_and(|finish| finish <= deadline)
    }

    /// Record a legal bestmove for the root position before any node is searched.
    ///
    /// Explicit cancellation is honored only once this has run. Root move generation is finite and
    /// cheap, so the window in which cancellation is ignored is bounded by move generation rather
    /// than by the depth-1 quiescence tree, which has no practically small bound.
    fn establish_root_fallback(&mut self) {
        self.root_fallback = self
            .pos
            .generate::<BasicMoveList, AllGen, Legal>()
            .first()
            .copied();
        self.root_fallback_ready = true;
    }

    /// Wraps [`Self::search_inner`] with the same node-score check quiescence carries, so the
    /// invariant is enforced wherever a score is produced rather than only in the subtree where
    /// the excursion was first observed. Root scores reach `Display` on the UCI thread, and an
    /// out-of-band one trips its parity assertion there.
    pub fn search<T: Thread, Node: NodeType>(
        &mut self,
        alpha: Score,
        beta: Score,
        depth: Depth,
        ply: usize,
    ) -> NodeResult {
        let result = self.search_inner::<T, Node>(alpha, beta, depth, ply);

        if let Some(score) = result {
            debug_assert!(
                score.is_node_score(),
                "search returned {score:?} outside the node score band \
                 (window {alpha:?}..{beta:?}, depth {depth}, ply {ply})",
            );
        }

        result
    }

    fn search_inner<T: Thread, Node: NodeType>(
        &mut self,
        mut alpha: Score,
        mut beta: Score,
        depth: Depth,
        ply: usize,
    ) -> NodeResult {
        self.trace.visit_node();

        debug_assert!(!Node::root() || ply == 0);

        // Per-ply state and the recursion itself are bounded by `MAX_PLY`. A node with no room left
        // for a child hands over to quiescence rather than extending the path further. This is what
        // lets everything below index the stack unconditionally: any node that reaches the move
        // loop has both `ply` and `ply + 1` in range, so no extension can drive the main search
        // past the end of its own state.
        if ply + 1 >= MAX_PLY {
            return self.quiesce::<T, Node>(alpha, beta, ply);
        }

        // The PV row for this ply is rebuilt from scratch on every visit, so clear it before any
        // early return can leave a previously searched sibling's line in place for this node's
        // parent to splice into its own PV. See `PVTable::clear_at`.
        self.pvt.clear_at(ply);
        self.stack[ply].eval = None;

        debug_assert!(Score::INF_N <= alpha);
        debug_assert!(alpha < beta);
        debug_assert!(beta <= Score::INF_P);
        debug_assert!(Node::pv() || alpha.inc_one() == beta);

        // Step 1. Check for aborted search and immediate draw.
        if self.stopping() {
            return None;
        }

        // Step 2. check for immediate draw.
        if self.is_history_draw() {
            return Some(Score::zero());
        }

        // Sampled before any child is searched, and compared again at the transposition-table write
        // below. If a history-sensitive draw was claimed anywhere in this subtree, `best_value`
        // depends on the path taken to reach this node and cannot be stored as exact information
        // about the position itself.
        let history_draws_on_entry = self.history_draws;

        // Normalize search bounds into the range a node can return.
        if !Node::root() {
            // This is deliberately not mate-distance pruning. Mate scores are position-relative,
            // so the root ply does not tighten a descendant's attainable mate range: every node
            // can still be checkmated now or mate on its next ply. Bounds derived from the node's
            // distance from the root were therefore unsound, and no equivalent pruning remains.
            //
            // The clamp is still required as representation hygiene. `child_bound` is exact, so a
            // window at the very bottom of the band arrives here as
            // `(Score(20_100), Score(20_101))`: entirely above anything a node can score. Clamping
            // both ends also maps the infinity bounds used at the root into the node-score band.
            // Neither operation discards an attainable score; it only prevents a threshold from
            // escaping as a fail-soft return value.
            alpha = alpha.clamp(Score::mate(0), Score::mate(1));
            beta = beta.clamp(Score::mate(0), Score::mate(1));
            // An exact child-bound conversion can put the whole window above or below the node
            // band. Normalization then collapses it. Returning the in-band threshold is required
            // before another recursive call, whose window must be non-empty; this is bound
            // sanitation, not a mate-distance cutoff.
            if alpha >= beta {
                return Some(alpha);
            }
        }

        // Step 3. Load transposition table entry.
        //
        // The probe returns an owned snapshot, so everything below reads one atomic state of one
        // slot. A concurrent worker replacing that slot between here and Step 24 cannot change what
        // this node consumes.
        //
        // Two independent things are extracted from a hit, and neither implies the other:
        //
        // * The *score*, which is reusable whenever the entry is deep enough and the clock permits.
        // * The *move*, which is only useful if it can actually be played here.
        //
        // Coupling them costs cutoffs for no safety. A checkmated or stalemated node stores its
        // value with no move at all, and so does every fail-low node whose moves all failed to
        // raise alpha; requiring a move before trusting the score makes exactly those entries — the
        // cheapest and most certain ones in the table — permanently unusable.
        //
        // Trusting the score without a move is safe because the entry's identity is already
        // established: `Table::probe` verifies the full 64-bit key against the same write the score
        // was decoded from, so accepting a foreign position's entry requires a genuine Zobrist
        // collision. Move legality is not part of that proof and never was — it filters some wrong
        // entries by accident, but says nothing about a move-less one. See the `tt` module docs.
        let tt_entry = self.tt.probe(self.pos.zobrist().0);
        // Captured before the entry is consumed by the Step 4 cutoff filter below. The static
        // evaluation is position-intrinsic, so a full-key hit supplies it directly and Step 6 skips
        // recomputation — see [`Snapshot::eval`] for why this needs no clock gate, unlike the score.
        let tt_eval = tt_entry.as_ref().and_then(Snapshot::eval);
        let mut tt_mov = None;
        match tt_entry.as_ref() {
            Some(entry) => {
                self.trace.hash_hit();
                if let Some(packed) = entry.mov() {
                    let mov = packed.to_move(&self.pos);
                    if self.pos.valid_move(&mov) {
                        tt_mov = Some(mov);
                    } else {
                        // A verified entry whose move cannot be played here. Since the full key
                        // matched, this is a genuine Zobrist collision, and the counter measures
                        // that rather than a truncated-signature accident. The score is left alone:
                        // an unusable ordering hint is not evidence about the score's provenance.
                        self.trace.hash_collision();
                    }
                }
            }
            None => self.trace.hash_miss(),
        }

        // Step 4. Check for early cutoff.
        if !Node::pv() {
            if let Some(entry) = tt_entry.filter(|e| {
                Depth::from(e.depth()) >= depth && self.clock_permits_tt_reuse(e.depth())
            }) {
                match entry.bound() {
                    Bound::Exact => {
                        return Some(entry.score());
                    }
                    Bound::Lower => {
                        if entry.score() > beta {
                            return Some(entry.score());
                        } else if entry.score() > alpha {
                            alpha = entry.score()
                        }
                    }
                    Bound::Upper => {
                        if entry.score() < alpha {
                            return Some(entry.score());
                        } else if entry.score() < beta {
                            beta = entry.score()
                        }
                    }
                }
            }

            if alpha >= beta {
                return Some(alpha);
            }
        }

        // Step 5. Straight to quiescence search if depth <= 0.
        //
        // The test is `<= 0` rather than `== 0` because a reduction may take depth past zero in one
        // step. Quiescence still receives this node's ply, so its subtree is positioned on the path
        // rather than starting again from nothing.
        if depth <= 0 {
            return self.quiesce::<T, Node>(alpha, beta, ply);
        }

        // Step 6. Static evaluation.
        //
        // A verified hit already carries this position's static evaluation, which is intrinsic to
        // the position, so it is reused directly instead of recomputed. In debug builds the reused
        // value is checked against a fresh computation; the two can only differ under a genuine
        // Zobrist collision, which the debug assertion would surface and which no test position
        // produces.
        let eval = match tt_eval {
            Some(stored) => {
                debug_assert_eq!(
                    stored,
                    self.evaluate(),
                    "cached static evaluation disagrees with recomputation"
                );
                stored
            }
            None => self.evaluate(),
        };
        self.stack[ply].eval = Some(eval);

        // Whether the side to move is doing better than two plies ago. Read below by razoring, and
        // available to every later margin-based technique from the per-ply stack.
        let improving = is_improving(Some(eval), self.eval_two_plies_ago(ply));

        // Step 7. Razoring and reverse futility pruning.
        //
        // These are mirror images across the search window, each discarding a node from its static
        // evaluation alone near the horizon, and they share the same guards: non-PV node, not in
        // check — a forcing position has no trustworthy static evaluation — and a centipawn window
        // bound, since a mate bound makes a centipawn margin meaningless.
        //
        // Razoring works the alpha side. When the evaluation sits far below alpha, a quiescence
        // search checks whether captures can still rescue it; if not, the node fails low.
        if should_razor(depth, eval, alpha, improving) {
            let value = self.quiesce::<Master, NonPv>(alpha - Score::cp(1), alpha, ply)?;
            if value < alpha {
                return Some(value);
            }
        }

        // Reverse futility pruning works the beta side. When the evaluation stands a depth-scaled
        // margin above beta, the node is assumed to hold above beta against any quiet reply to the
        // horizon and fails high at once, without a move ever being generated. `eval` is returned as
        // the fail-soft lower bound; `evaluate` is material-only, so it is always a centipawn score
        // and never fabricates a mate distance the search has not proven.
        if self.rfp_enabled()
            && !Node::pv()
            && !self.pos.in_check()
            && depth <= REVERSE_FUTILITY_MAX_DEPTH
            && beta.is_cp()
            && eval - reverse_futility_margin(depth) >= beta
        {
            return Some(eval);
        }

        // Step 8. Futility pruning.
        //
        // Near the horizon a single quiet move rarely swings the static evaluation by more than a
        // depth-scaled margin. When even `eval + margin` cannot reach alpha, the quiet moves in the
        // loop below cannot change this node's result, so they are skipped there. The decision is
        // taken once here and applied per move in the loop. It is confined to non-PV nodes, disabled
        // while in check — a forcing position has no trustworthy static evaluation — and disabled
        // when alpha is a mate bound, where a centipawn margin is meaningless.
        let futility_pruning = self.forward_pruning_enabled()
            && !Node::pv()
            && !self.pos.in_check()
            && depth <= FUTILITY_MAX_DEPTH
            && alpha.is_cp()
            && eval + futility_margin(depth) <= alpha;

        // Step 9. Null move search with verification (non-PV only).
        //
        // Give the opponent a free move from a position that already looks at least as good as beta.
        // If even then a reduced-depth search cannot lift the opponent to beta, this node is so
        // strong that searching our own moves in full is wasted work, and it fails high at once.
        //
        // The guards keep the bet sound. It is a non-PV device: a PV node must never be pruned on a
        // null-window argument. It is disabled in check, where passing would leave the king
        // capturable. It requires the static evaluation to already reach beta, so the free move only
        // has to fail to *extend* a standing advantage. It requires the side to move to hold a piece
        // beyond king and pawns, because in king-and-pawn zugzwang passing can beat every legal move
        // and the null result is then a lie. It is not tried twice running — two passes only search
        // the same position more slowly — nor while a verification search has suppressed it through
        // `nmp_min_ply`.
        if self.forward_pruning_enabled()
            && !Node::pv()
            && !self.pos.in_check()
            && depth >= NULL_MOVE_MIN_DEPTH
            && ply >= self.nmp_min_ply
            && eval >= beta
            && self.pos.has_non_pawn_material(self.pos.turn())
            && !(ply > 0 && self.stack[ply - 1].mov.is_null())
        {
            let r = null_move_reduction(depth);

            // Record the pass as this node's current move so the child can see it arrived through a
            // null move and decline to pass straight back.
            self.stack[ply].mov = Move::null();
            self.stack[ply].moved_piece = Piece::None;
            self.make_null_move();
            self.tt.prefetch(self.pos.zobrist().0);
            let child = self.search::<T, NonPv>(
                beta.child_bound(),
                beta.dec_one().child_bound(),
                depth - 1 - r,
                ply + 1,
            );
            self.unmake_null_move();

            // The null move is already unmade, so an aborted child simply propagates upward.
            let null_value = child?.neg().inc_mate();

            if null_value >= beta {
                // A mate score returned by a search that skipped a move is not a proven mate, so it
                // is reported as the beta bound rather than as a mate distance nothing established.
                let value = if null_value.is_mate() {
                    beta
                } else {
                    null_value
                };

                if depth < NULL_MOVE_VERIFY_DEPTH {
                    return Some(value);
                }

                // Deep cutoff: confirm it with a normal reduced-depth search of this same position,
                // null moves suppressed in its subtree so the cutoff under test cannot rubber-stamp
                // itself. This is the net that catches the zugzwang the material guard only
                // approximates.
                let saved = self.nmp_min_ply;
                self.nmp_min_ply = ply + 1;
                let verify = self.search::<T, NonPv>(alpha, beta, depth - r, ply);
                self.nmp_min_ply = saved;

                if verify? >= beta {
                    return Some(value);
                }
            }
        }

        // Step 10. ProbCut.
        //         TODO

        // Step 11. In PV nodes, if the move is not in TT, decrease depth by 3.
        //          TODO

        // Step 12. If depth <= 0, run quiescence search.
        //          Handled earlier, at Step 5.

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
        let mut failed_quiets = BasicMoveList::empty();
        // Captures searched at this node that did not cause a cutoff. On a cutoff each takes a
        // capture-history malus, whether the cutoff itself was a capture or a quiet.
        let mut failed_captures = BasicMoveList::empty();

        // Whether the side to move is in check at this node, sampled once because it is the same for
        // every move played from here. It drives the check-evasion extension at Step 16 and suppresses
        // the reduction at Step 17: a move that answers a check is forced and its subtree narrow, so
        // it is searched at least as deeply as the node, never less.
        let node_in_check = self.pos.in_check();

        // Late-move (move-count) pruning is decided once per node here and applied per move in the
        // loop. It abandons the tail of the history-ordered quiet moves outright once enough moves
        // have been searched — with no verifying re-search, unlike late-move reduction. It shares the
        // forward-pruning guards: a non-PV node only, and never in check, where a forced reply must
        // not be dropped and the ordering is a poor guide. It is further confined to nodes near the
        // horizon (Step 5 sends depth <= 0 to quiescence, so depth is at least one here). Its safety
        // rests on quiet moves being ordered by history: only because the history heuristic sorts them
        // does a quiet move appearing late genuinely mean an unpromising one. Before that ordering
        // existed the quiet segment was unsorted and there was no sense in which a quiet move was
        // "late", which is why this technique depends on the history heuristic being active.
        let late_move_pruning =
            self.lmp_enabled() && !Node::pv() && !node_in_check && depth <= LMP_MAX_DEPTH;

        'move_loop: while moves.load_next_phase(MoveLoader::from(self, tt_mov, ply)) {
            // The phase is fixed for the whole batch the inner loop is about to drain, and the
            // iterator borrows `moves` for that batch, so read it once here rather than inside.
            let phase = moves.phase();

            // Underpromotions are excluded from the main search. They are the final ordering phase,
            // and each is derived from a queen promotion that has already been searched from this
            // node, so skipping them never removes the last legal move (the mate/stalemate check
            // below therefore stays sound). A rook, knight or bishop promotion is decisive so rarely
            // that resolving it is left to quiescence, whose move loop still expands the
            // queen-promotion segment into these. Dropping the phase here saves generating and
            // searching three extra moves for every promotion in the tree.
            if phase == Phase::Underpromotions {
                break 'move_loop;
            }

            for mov in &mut moves {
                if self.stopping() {
                    break 'move_loop;
                }

                move_count += 1;
                let mut value = Score::INF_N;

                // Attribute this move to the killer slot it came from, if any, for the effectiveness
                // telemetry. `phase == Killers` means staged ordering already yielded it as a
                // distinct killer — a killer that duplicated the hash move was suppressed into the
                // hash phase and is not counted here.
                let killer_slot = (phase == Phase::Killers)
                    .then(|| self.kt.slot_of(ply, mov))
                    .flatten();
                if let Some(slot) = killer_slot {
                    self.trace.killer_attempt(slot);
                }

                // Start reporting which move we're considering after 3 seconds have elapsed.
                if T::is_master() && Node::root() && self.trace.live_elapsed().as_millis() > 3000 {
                    self.emit_current_move(depth, &mov, move_count);
                }

                self.stack[ply].mov = mov;
                // Captured before the move is played, while the mover still sits on its origin, so a
                // child can key continuation history on this move's (piece, to) context.
                self.stack[ply].moved_piece = self.pos.piece_at_sq(mov.orig());

                // Step 16. Reductions & extensions.
                //
                // Extend the whole subtree by a ply when the side to move is in check. A check
                // evasion is forced — few moves answer it and the reply is constrained — so the line
                // is narrow and cheap to search a ply deeper, and that extra ply is where a mating
                // net or a decisive tactic hidden just past the horizon becomes visible. The
                // extension only ever adds depth, so it cannot truncate the principal variation.
                let extension: Depth = if node_in_check && self.extensions_enabled() {
                    1
                } else {
                    0
                };
                let new_depth = depth - 1 + extension;

                // Step 17. Late move reduction.
                //
                // The reduction is decided just below, after the move is made, where whether it gives
                // check is known: a checking move is forcing and must not be searched shallower than
                // the moves around it. See "Step 17 (applied)".
                //
                // Its history- and ordering-based inputs are sampled here, before the move is made,
                // while the mover still stands on its origin and the side to move is unflipped —
                // afterwards both reads would be wrong. Only quiet moves are ever reduced, so the
                // lookups are confined to them; anything else supplies neutral inputs it will ignore.
                let (lmr_history, lmr_favoured) = if mov.is_quiet() {
                    (
                        self.quiet_history_score(mov, ply),
                        killer_slot.is_some() || self.is_counter_move(mov, ply),
                    )
                } else {
                    (0, false)
                };

                // Step 18. Make the move.
                // SAFETY: ordered moves originate from move generation for `self.pos`.
                unsafe { self.make_move(&mov) };

                // Step 8a (applied). Late-move (move-count) pruning. Once enough moves have been
                // searched, a quiet move drawn from the history-ordered quiet phase is discarded
                // outright — with no verifying re-search, unlike late-move reduction. `move_count`
                // counts the moves already reached, so the promising prefix (hash move, captures,
                // killers, counter, and the leading quiets) is always kept before this can fire. A
                // quiet move that gives check is exempt: a checking move is forcing and can still
                // deliver mate near the horizon, and whether it checks is only known once it is on the
                // board — which is why this sits after the move is made rather than replacing it with
                // a bare counter test. The make/unmake is negligible beside the recursive search it
                // avoids. Bad captures, which the ordering places after the quiets, are deliberately
                // never pruned here: pruning losing captures by move count is in effect a
                // static-exchange prune of the main search, a combination that measured as a strong
                // strength regression, and a bad capture can be the sacrifice that forces a tactic.
                if late_move_pruning
                    && phase == Phase::Quiet
                    && move_count > late_move_count(depth)
                    && !self.pos.in_check()
                {
                    self.unmake_move();
                    continue;
                }

                // The child's first act is to probe this cluster, and the table is far larger than
                // cache, so that probe misses. Starting the fetch here overlaps the miss with the
                // recursive descent's own setup rather than stalling on it. The key is only known
                // once the move has been made, so this is the earliest point the address exists.
                self.tt.prefetch(self.pos.zobrist().0);

                // Step 8 (applied). Futility pruning skips a quiet move whose node cannot reach alpha
                // even granted the depth's margin. The move is already on the board, so its check
                // status is known: a move that gives check can still force mate near the horizon and
                // is never dropped. The first move is always searched, so the node still returns a
                // real value rather than an empty `-inf`; the futility bound the move could at best
                // have reached is folded into `best_value`, keeping the fail-low score meaningful.
                // The recursive search — the real cost — is what this avoids; the make/unmake is
                // negligible beside it and is what makes the check test exact.
                if futility_pruning && move_count > 1 && mov.is_quiet() && !self.pos.in_check() {
                    self.unmake_move();
                    let futility_value = eval + futility_margin(depth);
                    if futility_value > best_value {
                        best_value = futility_value;
                    }
                    continue;
                }

                // Step 17 (applied). Late move reduction.
                //
                // Search a late, quiet move with less depth first: ordering has already placed the
                // moves most likely to be best ahead of it, so it probably will not raise alpha, and
                // proving that cheaply at reduced depth is worth the occasional full-depth re-search
                // when the shallow search turns out wrong. The reduction is confined to moves that are
                // safe to trust a shallow verdict about: quiet moves only (a capture or promotion can
                // swing the score too far), never a move that answers or gives check (both are
                // forcing and their subtrees narrow), and never a move that was itself extended. It
                // engages once the move is past the full-depth prefix, or as soon as an earlier move
                // has raised alpha here — from that point every remaining move is scouted against a
                // proven bound and is even less likely to beat it, so it is reduced immediately (the
                // Step 20 alpha-raise records this). The first move is never reduced, so a principal
                // variation always rests on a full-depth search.
                //
                // The amount is no longer a flat step: `lmr_reduction` grows the cut with depth and
                // move count and then modulates it by the move's own quiet history, the improving
                // signal, and whether the ordering favours it, so the trusted prefix keeps its depth
                // while the unpromising tail is trimmed harder. The inputs were sampled before the
                // move was made (see Step 17).
                //
                // `self.pos.in_check()` here reports the position *after* the move, i.e. whether the
                // move gives check; the node's own in-check status is `node_in_check`.
                let mut reduction: Depth = 0;
                if self.lmr_enabled()
                    && mov.is_quiet()
                    && extension == 0
                    && !self.pos.in_check()
                    && depth >= LMR_MIN_DEPTH
                    && (move_count > LMR_MOVE_THRESHOLD || did_raise_alpha)
                {
                    reduction = lmr_reduction(
                        depth,
                        move_count,
                        Node::pv(),
                        improving,
                        lmr_favoured,
                        lmr_history,
                    );
                    // Keep at least one ply in the reduced scout. `new_depth` is at least two here
                    // (the depth gate guarantees it), so this never collapses the scout into the
                    // Step 5 quiescence handover, which must stay reserved for depth at or below zero.
                    reduction = reduction.clamp(0, new_depth - 1);
                }

                // Step 19. Search non-PV move with null window, reduced when Step 17 asked for it.
                if !Node::pv() || move_count > 1 {
                    let child = self.search::<T, NonPv>(
                        alpha.inc_one().child_bound(),
                        alpha.child_bound(),
                        new_depth - reduction,
                        ply + 1,
                    );
                    let Some(child) = child else {
                        self.unmake_move();
                        return None;
                    };
                    value = child.neg().inc_mate();

                    // Late-move-reduction re-search. A reduced scout that raised alpha may have done
                    // so only because it stopped short; the shallow result is not trusted to carry a
                    // move back into contention, so it is confirmed at full depth. A reduced scout
                    // that failed low is trusted and skipped — that is the reduction's saving, and the
                    // one case the re-search cannot second-guess. Nothing to redo when the move was
                    // not reduced.
                    if reduction > 0 && value > alpha {
                        let child = self.search::<T, NonPv>(
                            alpha.inc_one().child_bound(),
                            alpha.child_bound(),
                            new_depth,
                            ply + 1,
                        );
                        let Some(child) = child else {
                            self.unmake_move();
                            return None;
                        };
                        value = child.neg().inc_mate();
                    }
                }

                // Step 20. Search PV move, or perform re-search if null window search failed high.
                //
                // If this is a PV node, do a full search on the first move and any move for which
                // the null-window search failed to produce a cutoff.
                if Node::pv()
                    && (move_count == 1 || (value > alpha && (Node::root() || value < beta)))
                {
                    let child = self.search::<T, Pv>(
                        beta.child_bound(),
                        alpha.child_bound(),
                        new_depth,
                        ply + 1,
                    );
                    let Some(child) = child else {
                        self.unmake_move();
                        return None;
                    };
                    value = child.neg().inc_mate();
                }

                debug_assert!(Node::pv() || !(value > alpha && (Node::root() || value < beta)));

                // Step 21. Undo move.
                self.unmake_move();

                debug_assert!(value > Score::INF_N);
                debug_assert!(value < Score::INF_P);

                // Upgrade the cancellation fallback to the best fully searched root move, so a
                // cancellation during the first ply reports a searched move rather than the
                // arbitrary first generated one. An abort during this move's subtree leaves `value`
                // meaningless, so only a move searched without stopping may be adopted.
                if Node::root() && value > best_value && !self.stopping() {
                    self.root_fallback = Some(mov);
                }

                // Step 22. Check for new best move.
                if value > best_value {
                    best_value = value;

                    if value > alpha {
                        best_move = mov;

                        if Node::pv() && value < beta {
                            // Only an exact score at a PV node establishes a variation worth
                            // reporting. A fail-high returns a lower bound whose "best" move was
                            // never searched with a full window, so publishing it would splice a
                            // non-PV continuation into the reported line. Under a full-width root
                            // window (`beta == INF_P`) the root always lands here; an aspiration
                            // window gives it a finite beta, so a root fail-high now reaches the
                            // else branch and is recovered by a widening re-search.
                            self.pvt.copy_to(ply, mov);

                            alpha = value;
                            // A move has now raised alpha at this node. Every move still to come is
                            // scouted against this proven bound and is unlikely to beat it, so from
                            // here they are reduced: Step 17 reads this flag to engage the reduction
                            // on the remaining moves without waiting for the move-count threshold.
                            did_raise_alpha = true;
                        } else {
                            debug_assert!(value >= beta);
                            // beta-cutoff; record killer and history
                            if let Some(slot) = killer_slot {
                                self.trace.killer_cutoff(slot);
                            }
                            if mov.is_quiet() {
                                // The killer table reserves no slot for the root; a root cutoff is
                                // only reachable at all through an aspiration window's finite beta,
                                // and the refutation it names is relative to that artificial bound
                                // rather than a true one, so it is not recorded there.
                                if ply > 0 {
                                    self.kt.store(mov, ply);
                                }
                                self.update_quiet_histories(mov, &failed_quiets, depth, ply);
                            }
                            self.update_capture_histories(mov, &failed_captures, depth);

                            break 'move_loop;
                        }
                    }
                }

                if mov.is_quiet() {
                    failed_quiets.push(mov);
                } else if mov.is_capture() {
                    failed_captures.push(mov);
                }
            }
        }

        if self.stopping() {
            return None;
        }

        debug_assert!(
            move_count > 0
                || self
                    .pos
                    .generate::<BasicMoveList, AllGen, Legal>()
                    .is_empty()
        );

        // Step 23. Check for mate and stalemate.
        if move_count == 0 {
            // The row was already emptied on entry, so this terminal node reports no continuation.
            best_value = if self.pos.in_check() {
                Score::mate(0)
            } else {
                Score::cp(0)
            };
        }

        debug_assert!(best_value > Score::INF_N);

        // Step 24. Write node information to the transposition table.
        //
        // A subtree that claimed a draw by repetition or by the fifty-move rule produced a value
        // that depends on the moves played before the root, which the Zobrist key does not cover.
        // Storing it would let a later visit with a different history reuse a draw that does not
        // apply there. Neither is it enough to downgrade `Exact` to a bound: a draw score can raise
        // the value to a beta cutoff just as easily as it can cap it, so the resulting `Lower` or
        // `Upper` bound is unsound in an incompatible history too. The entry is therefore left
        // unwritten and the position is re-searched when it is next reached.
        //
        // Reaching here also requires `stopping()` to have been false just above, so an entry can
        // only be published by a node whose whole move loop ran to completion. An aborted subtree
        // returns `None` before this point, and every child search propagates that `None` upwards,
        // so no partially explored value ever reaches the table.
        //
        // `depth` is at least one here: a node at or below zero delegated to quiescence at Step 5.
        // That is what reserves [`Self::QUIESCENCE_DRAFT`] for quiescence alone.
        debug_assert!(depth > Depth::from(Self::QUIESCENCE_DRAFT));
        if self.history_draws == history_draws_on_entry {
            self.tt.store(
                self.pos.zobrist().0,
                best_value,
                self.stack[ply].eval,
                Self::tt_draft(depth),
                if best_value >= beta {
                    debug_assert!(
                        !best_move.is_null()
                            || best_value == Score::mate(0)
                            || best_value == Score::zero()
                    );
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
        }

        // Step 25. Return best value.
        Some(best_value)
    }

    #[inline(always)]
    fn stopping(&mut self) -> bool {
        #[cfg(test)]
        if self
            .abort_after_nodes
            .is_some_and(|limit| self.trace.all_nodes_visited() >= limit)
        {
            return true;
        }

        // The two abort signals are gated separately.
        //
        // Explicit cancellation (`stop`, `quit`, stdin EOF, or a command replacing the active
        // search) aborts as soon as the root fallback exists, which is before the first node is
        // searched. A legal bestmove is therefore always available without waiting for the depth-1
        // quiescence tree, whose size has no practically small bound. This check reads an
        // atomic bool, which is cheap enough to run on every call and must stay unthrottled so that
        // cancellation responsiveness is unaffected.
        //
        // The time deadline is still suppressed until the guaranteed-minimum search (the first full
        // ply) completes, so a zero or already-elapsed budget returns a searched move rather than
        // the unsearched fallback. The first ply is finite, so this can never hang.
        if self.stopping.load(Ordering::Relaxed) {
            return self.root_fallback_ready;
        }

        if !self.min_search_complete {
            return false;
        }

        // The node budget is gated exactly like the time deadline above: only the completed first
        // ply releases it, so a budget too small to finish a ply returns a searched move rather
        // than the unsearched fallback. Unlike the clock read it needs no throttling — the node
        // count is already read on every call — and the comparison is monotonic, so once the
        // budget is reached every later check during the unwind agrees without a latch.
        if self
            .node_limit
            .is_some_and(|limit| self.trace.all_nodes_visited() as u64 >= limit)
        {
            return true;
        }

        let Some(stop_time) = self.stop_time else {
            return false;
        };

        // Unlike the cancellation flag, the deadline needs a clock read, which is expensive enough
        // relative to a node to matter in the innermost loops. Optimized searches therefore sample
        // every eight nodes. Debug builds search orders of magnitude more slowly, so sample each
        // node there to keep wall-clock tests and developer runs responsive while still avoiding
        // repeated reads within the same node.
        const DEADLINE_CHECK_INTERVAL_NODES: usize = if cfg!(debug_assertions) { 1 } else { 8 };
        let nodes = self.trace.all_nodes_visited();
        if let Some(last) = self.last_deadline_check_nodes {
            // An expired deadline stays latched: the many stopping checks made while the search
            // unwinds must all agree, rather than the throttle letting search resume mid-unwind.
            if last == usize::MAX {
                return true;
            }
            if nodes.saturating_sub(last) < DEADLINE_CHECK_INTERVAL_NODES {
                return false;
            }
        }

        if stop_time <= Instant::now() {
            self.last_deadline_check_nodes = Some(usize::MAX);
            true
        } else {
            self.last_deadline_check_nodes = Some(nodes);
            false
        }
    }

    /// Reports whether the current position is an immediate draw by repetition or by the fifty-move
    /// rule, recording the claim so that ancestors can tell their value depends on this history.
    ///
    /// Both conditions read `Position::history`, which the Zobrist key does not cover: the same key
    /// is a draw in one line and a live position in another. Every caller must go through here so
    /// that the claim is counted, and so that both the main search and quiescence agree on the
    /// fifty-move boundary.
    #[inline(always)]
    fn is_history_draw(&mut self) -> bool {
        if self.pos.in_threefold() || self.pos.fifty_move_rule_reached() {
            self.history_draws += 1;
            true
        } else {
            false
        }
    }

    /// Plies a subtree may be searched beyond its nominal depth, through quiescence and check
    /// extensions. Used only to keep [`Self::clock_permits_tt_reuse`] on the conservative side of
    /// the fifty-move boundary.
    const HORIZON_SLACK: u32 = 16;

    /// The draft recorded for a value produced by quiescence.
    ///
    /// Quiescence and the main search share one table, so a reader has to be able to tell a
    /// capture-only value apart from a real depth-`d` search of the same position. The whole scheme
    /// rests on one reserved level: quiescence writes this draft and nothing else, and the main
    /// search never writes it, because a main-search node at depth zero delegates to quiescence
    /// before it can reach its own store. Every main-search entry therefore has a draft of at least
    /// one.
    ///
    /// That makes the ordinary `entry.depth() >= depth` test do the separation for free. A
    /// main-search node needs at least depth one, which no quiescence entry can satisfy, so a
    /// capture-only value can never masquerade as a searched one. A quiescence node needs nothing
    /// beyond this draft, so it can reuse its own results and any deeper main-search result. The
    /// only nodes that consume a quiescence entry are the ones whose own search *is* quiescence.
    const QUIESCENCE_DRAFT: u8 = 0;

    /// Narrows a searched depth to the draft the transposition table records.
    ///
    /// The table's draft field is a byte, while search depth is signed and, once extensions exist,
    /// not bounded by the nominal iteration depth. Saturating is the safe direction: an entry that
    /// understates how deeply it was searched is reused by fewer nodes than it could have been,
    /// which costs hit rate. Overstating would let a shallow value satisfy a deeper node's depth
    /// requirement, which is unsound.
    ///
    /// Callers have already established `depth >= 1`, since a node at or below zero delegated to
    /// quiescence before reaching any store. That is what keeps [`Self::QUIESCENCE_DRAFT`]
    /// reserved for quiescence alone.
    #[inline(always)]
    fn tt_draft(depth: Depth) -> u8 {
        debug_assert!(depth >= 1);
        depth.clamp(1, Depth::from(u8::MAX)) as u8
    }

    /// Reports whether a stored result of the given depth may be reused at this node, as far as the
    /// halfmove clock is concerned.
    ///
    /// Making the static evaluation position-intrinsic is not on its own enough to make reuse
    /// sound. A search value also reflects any fifty-move draw reachable inside its own subtree,
    /// and whether one is reachable depends on the clock, which the Zobrist key does not cover. The
    /// write side of this contract is enforced at Step 24: a node whose subtree claimed a fifty-move
    /// or repetition draw is never written, so a stored value never embeds such a draw. This is the
    /// matching read side: a value computed where the rule was out of reach must only be reused
    /// where it is still out of reach, or a drawn line is scored as if it played on.
    ///
    /// The horizon is bounded by the stored depth plus [`Self::HORIZON_SLACK`], since quiescence
    /// and check extensions can search past the nominal depth. That slack is a conservative
    /// allowance rather than a proof: quiescence follows captures, which reset the clock, and quiet
    /// check evasions, which do not, and the length of a forcing evasion sequence has no tight
    /// static bound. Erring high costs only hit rate, and only near the boundary.
    #[inline(always)]
    fn clock_permits_tt_reuse(&self, entry_depth: u8) -> bool {
        self.pos.half_move_clock() + entry_depth as u32 + Self::HORIZON_SLACK
            < Position::FIFTY_MOVE_RULE_PLIES
    }

    /// The static evaluation recorded two plies before `ply`, i.e. at the same side's previous
    /// turn. `None` at the root and its immediate child, which have no such ancestor, and `None`
    /// when that ancestor was in check and computed no evaluation. This is the earlier operand of
    /// the improving comparison; see [`is_improving`].
    #[inline(always)]
    fn eval_two_plies_ago(&self, ply: usize) -> Option<Score> {
        ply.checked_sub(2).and_then(|p| self.stack[p].eval)
    }

    /// The *(piece, to)* contexts of the moves preceding the node at `ply`, for each continuation
    /// distance: index 0 is the move one ply back, index 1 the move two plies back.
    ///
    /// A distance is `None` where there is no preceding move to condition on — near the root, or
    /// where that ancestor passed with a null move — which suppresses that distance's contribution
    /// to both scoring and updates. The mover is read from [`StackEntry::moved_piece`], captured at
    /// make time, rather than from the current board, because the piece two plies back may since have
    /// moved again.
    #[inline]
    fn continuation_contexts(
        &self,
        ply: usize,
    ) -> [Option<(Piece, Square)>; CONTINUATION_DISTANCES] {
        std::array::from_fn(|i| {
            let back = i + 1;
            let entry = &self.stack[ply.checked_sub(back)?];
            if entry.mov.is_null() || entry.moved_piece.is_none() {
                None
            } else {
                Some((entry.moved_piece, entry.mov.dest()))
            }
        })
    }

    /// The moving side's combined main-plus-continuation history for the quiet move `mov` at this
    /// node, summed exactly as the quiet ordering scores it. Read by late-move reduction so a
    /// well-scored quiet is reduced less and a poorly scored one more.
    ///
    /// Must be called before the move is made, while the mover still stands on its origin square and
    /// the side to move is unflipped — after the move both the origin piece and `turn` are wrong.
    #[inline]
    fn quiet_history_score(&self, mov: Move, ply: usize) -> i32 {
        let side = self.pos.turn();
        // SAFETY: `mov` is a legal quiet move for `self.pos`, so both squares are valid and a real
        // piece stands on its origin — the same invariant `score_quiets` relies on.
        let cur_piece = self.pos.piece_at_sq(mov.orig());
        let mut raw = unsafe { self.history.get_unchecked(mov.orig(), mov.dest(), side) };
        let contexts = self.continuation_contexts(ply);
        for (dist, ctx) in contexts.iter().enumerate() {
            if let Some((prev_piece, prev_to)) = *ctx {
                // SAFETY: `dist` is a tracked continuation distance and every piece is real.
                raw += unsafe {
                    self.cont_hist
                        .get_unchecked(dist, prev_piece, prev_to, cur_piece, mov.dest())
                };
            }
        }
        raw
    }

    /// Whether `mov` is the counter move recorded for the move one ply back — the quiet reply the
    /// counter-move heuristic favours here. Read at the reduction decision so a favoured move keeps
    /// more of its depth. False at the root and after a null move, where no counter context exists.
    #[inline]
    fn is_counter_move(&self, mov: Move, ply: usize) -> bool {
        let contexts = self.continuation_contexts(ply);
        match contexts[0] {
            Some((prev_piece, prev_to)) => self.counter.get(prev_piece, prev_to) == mov,
            None => false,
        }
    }

    /// Reward the quiet move that produced a beta cutoff and penalise the quiet moves tried before
    /// it, across every quiet-move table: plain from-to history, continuation history at each
    /// tracked distance, and the counter move. All updates share the bounded gravity rule, so no
    /// table accumulates an independent unbounded count.
    ///
    /// The position here is the node's own — the cutoff move has already been unmade — so the mover
    /// of each quiet is read from its origin square. Continuation updates are keyed on the preceding
    /// moves via [`Self::continuation_contexts`]; a distance with no preceding move is skipped.
    fn update_quiet_histories(
        &mut self,
        cutoff: Move,
        failed_quiets: &BasicMoveList,
        depth: Depth,
        ply: usize,
    ) {
        let bonus = history_bonus(depth);
        let side = self.pos.turn();
        let contexts = self.continuation_contexts(ply);

        self.history
            .update(cutoff.orig(), cutoff.dest(), bonus, side);
        for failed in failed_quiets {
            self.history
                .update(failed.orig(), failed.dest(), -bonus, side);
        }

        let cutoff_piece = self.pos.piece_at_sq(cutoff.orig());
        for (dist, ctx) in contexts.iter().enumerate() {
            let Some((prev_piece, prev_to)) = *ctx else {
                continue;
            };
            self.cont_hist.update(
                dist,
                prev_piece,
                prev_to,
                cutoff_piece,
                cutoff.dest(),
                bonus,
            );
            for failed in failed_quiets {
                let failed_piece = self.pos.piece_at_sq(failed.orig());
                self.cont_hist.update(
                    dist,
                    prev_piece,
                    prev_to,
                    failed_piece,
                    failed.dest(),
                    -bonus,
                );
            }
        }

        // The reply to the move one ply back is now this cutoff.
        if let Some((prev_piece, prev_to)) = contexts[0] {
            self.counter.store(prev_piece, prev_to, cutoff);
        }
    }

    /// The type of piece a capture removes, used to key the capture-history table. An en-passant
    /// capture takes a pawn that does not stand on the destination square, so it is reported as a
    /// pawn directly; every other capture removes the piece currently on the destination square. The
    /// node position is unchanged here — the move is either not yet made (scoring) or already unmade
    /// (updating) — so the destination still holds the captured piece in the non-en-passant case.
    #[inline]
    fn captured_piece_type(&self, mov: &Move) -> PieceType {
        if mov.is_en_passant() {
            PieceType::Pawn
        } else {
            self.pos.piece_at_sq(mov.dest()).type_of()
        }
    }

    /// Reward a capture that produced a beta cutoff and penalise the captures searched before it, in
    /// the capture-history table.
    ///
    /// Unlike the quiet tables this is trained even when the cutoff move is quiet: a capture that was
    /// searched and did not cut is evidence against that capture whatever ultimately refuted the
    /// node, so every searched-but-failed capture takes a malus. The matching bonus is applied only
    /// when the cutoff move is itself a capture. Both go through the bounded gravity rule the quiet
    /// tables share. The position here is the node's own — the cutoff move has already been unmade —
    /// so each mover is read from its origin square.
    fn update_capture_histories(
        &mut self,
        cutoff: Move,
        failed_captures: &BasicMoveList,
        depth: Depth,
    ) {
        let bonus = history_bonus(depth);

        if cutoff.is_capture() {
            let mover = self.pos.piece_at_sq(cutoff.orig());
            let captured = self.captured_piece_type(&cutoff);
            self.capture_history
                .update(mover, cutoff.dest(), captured, bonus);
        }

        for failed in failed_captures {
            let mover = self.pos.piece_at_sq(failed.orig());
            let captured = self.captured_piece_type(failed);
            self.capture_history
                .update(mover, failed.dest(), captured, -bonus);
        }
    }

    /// Returns the static evaluation, from the perspective of the side to move.
    ///
    /// This is deliberately *position-intrinsic*: it depends only on state that the Zobrist key
    /// covers, and in particular not on the halfmove clock. The key identifies pieces, side to
    /// move, castling rights and the en-passant file, so the value returned here is the same at
    /// every visit to a position with the same key, whatever the clock reads there.
    ///
    /// This evaluation previously scaled material towards zero as the halfmove clock approached
    /// the fifty-move threshold. That made every propagated score a function of a value the key
    /// does not cover, so a warm table could return a score computed under a materially different
    /// clock. The approach of a fifty-move draw is instead left to the draw detection in `search`
    /// and `quiesce`, which the search discovers within its own horizon.
    ///
    /// Note that this makes the *leaf* value clock-independent, which is necessary for sound
    /// transposition-table reuse but not sufficient: a propagated value still reflects any
    /// fifty-move draw reachable inside its own subtree. That residual dependence is what
    /// [`Self::clock_permits_tt_reuse`] and the write suppression at Step 24 exist to contain.
    #[inline(always)]
    fn evaluate(&mut self) -> Score {
        // The evaluation selector lives here, at the single point a leaf value is produced. With a
        // network selected the leaf is scored by the scalar quantized forward pass; otherwise the
        // hand-crafted tapered evaluation runs, unchanged.
        if let Some(network) = self.network.as_deref() {
            // The forward pass already returns the score from the side to move's perspective (the
            // two accumulators are concatenated side-to-move first), so unlike the hand-crafted
            // score below it takes no `pov()` flip. The accumulator is rebuilt from the position
            // here rather than maintained incrementally through the search.
            let accumulator = Accumulator::from_position(network, &self.pos);
            let cp = nnue::forward(network, &accumulator, self.pos.turn());
            return Score::cp(cp as i16);
        }

        // The incremental accumulator is the working value; the from-scratch evaluation is only its
        // debug-build reference. The per-make assertion in `sync_eval_after_make` already guards the
        // accumulator at every node, and this reasserts it at the point the value is actually
        // consumed.
        debug_assert_eq!(
            self.eval_state.score(),
            self.pos.static_eval(),
            "incremental evaluation disagrees with from-scratch recomputation"
        );
        Score::cp(self.eval_state.score() * self.pov())
    }

    /// Makes a move on the search position and updates the evaluation accumulator to match.
    ///
    /// # Safety
    ///
    /// Carries the same contract as [`Position::make_move_unchecked`]: `mov` must be a legal move
    /// generated for the current position.
    #[inline(always)]
    unsafe fn make_move(&mut self, mov: &Move) {
        self.eval_stack.push(self.eval_state);
        self.pos.make_move_unchecked(mov);
        self.sync_eval_after_make();
    }

    /// Makes a move validated against the current position and updates the evaluation accumulator.
    ///
    /// The checked counterpart of [`Search::make_move`], for the few call sites that have not already
    /// established the move's legality.
    #[inline(always)]
    fn make_move_checked(&mut self, mov: &Move) {
        self.eval_stack.push(self.eval_state);
        self.pos.make_move(mov);
        self.sync_eval_after_make();
    }

    /// Folds the move just made into the accumulator and, under debug builds, checks the result.
    #[inline(always)]
    fn sync_eval_after_make(&mut self) {
        self.pos.replay_last_move_deltas(&mut self.eval_state);
        debug_assert_eq!(
            self.eval_state,
            EvalState::from_position(&self.pos),
            "incremental evaluation diverged from a from-scratch recomputation after a move"
        );
    }

    /// Unmakes the most recent move and restores the accumulator that went with the prior position.
    ///
    /// Restoration is a copy of the value `make_move` saved, not a recomputation, so it is exact and
    /// cheap however deep the search has gone.
    #[inline(always)]
    fn unmake_move(&mut self) {
        self.pos.unmake_move();
        self.eval_state = self
            .eval_stack
            .pop()
            .expect("unmake_move without a matching make_move");
    }

    /// Passes the turn to the opponent for a null-move search, carrying the evaluation accumulator
    /// across unchanged.
    ///
    /// A null move moves no piece, so the placement-derived accumulator is identical on both sides
    /// of it; the perspective flip is applied by [`Self::pov`] when the value is read, exactly as
    /// for an ordinary move. The saved copy is still pushed so [`Self::unmake_null_move`] can pop it
    /// symmetrically with [`Self::unmake_move`], keeping one stack discipline for both kinds of move.
    #[inline(always)]
    fn make_null_move(&mut self) {
        self.eval_stack.push(self.eval_state);
        self.pos.make_null_move();
        debug_assert_eq!(
            self.eval_state,
            EvalState::from_position(&self.pos),
            "a null move must leave the evaluation accumulator unchanged"
        );
    }

    /// Unmakes a null move and restores the accumulator that went with the prior position.
    #[inline(always)]
    fn unmake_null_move(&mut self) {
        self.pos.unmake_null_move();
        self.eval_state = self
            .eval_stack
            .pop()
            .expect("unmake_null_move without a matching make_null_move");
    }

    /// Whether the forward-pruning steps (futility, null move) are active. Always true in normal
    /// builds; a test can switch it off to search a position with the pruning bypassed.
    #[inline(always)]
    fn forward_pruning_enabled(&self) -> bool {
        #[cfg(test)]
        {
            !self.forward_pruning_disabled
        }
        #[cfg(not(test))]
        {
            true
        }
    }

    /// Whether reverse futility pruning is active. Always true in normal builds; a test can switch it
    /// off to search a position with the whole-node prune bypassed and compare against it.
    #[inline(always)]
    fn rfp_enabled(&self) -> bool {
        #[cfg(test)]
        {
            !self.rfp_disabled
        }
        #[cfg(not(test))]
        {
            true
        }
    }

    /// Whether late-move reduction is active. Always true in normal builds; a test can switch it off
    /// to search a position at full depth and compare against the reduced search.
    #[inline(always)]
    fn lmr_enabled(&self) -> bool {
        #[cfg(test)]
        {
            !self.lmr_disabled
        }
        #[cfg(not(test))]
        {
            true
        }
    }

    /// Whether late-move (move-count) pruning is active. Always true in normal builds; a test can
    /// switch it off to search a position with the quiet tail kept and compare against the pruned
    /// search.
    #[inline(always)]
    fn lmp_enabled(&self) -> bool {
        #[cfg(test)]
        {
            !self.lmp_disabled
        }
        #[cfg(not(test))]
        {
            true
        }
    }

    /// Whether the search extensions are active. Always true in normal builds; a test can switch them
    /// off to hold a search to its nominal depth.
    #[inline(always)]
    fn extensions_enabled(&self) -> bool {
        #[cfg(test)]
        {
            !self.extensions_disabled
        }
        #[cfg(not(test))]
        {
            true
        }
    }

    /// Whether the quiescence static-exchange cuts — the losing-capture cut and the delta cut — are
    /// active. Always true in normal builds; a test can switch them off to compare a position
    /// searched with and without the cuts.
    #[inline(always)]
    fn see_pruning_enabled(&self) -> bool {
        #[cfg(test)]
        {
            !self.see_pruning_disabled
        }
        #[cfg(not(test))]
        {
            true
        }
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
    ///
    /// Wraps [`Self::quiesce_inner`] so that every exit from quiescence passes one check: the
    /// score must lie in the band a node can actually hold. Quiescence returns `alpha` and `beta`
    /// directly as fail-soft scores, so a window bound that escaped the encoding would become a
    /// node score, and `Debug`/`Display` would render it as nonsense or trip their parity
    /// assertions. See [`Score::is_node_score`].
    ///
    /// `ply` is the distance from the root of the node quiescence was entered at, and grows with
    /// each capture followed. Nothing bounds it yet: a quiescence tree can in principle run deeper
    /// than the search stack covers, so the ply is carried but not used to index per-ply state.
    /// Capping the quiescence tree is the reason the value is threaded here at all.
    fn quiesce<T: Thread, Node: NodeType>(
        &mut self,
        alpha: Score,
        beta: Score,
        ply: usize,
    ) -> NodeResult {
        let result = self.quiesce_inner::<T, Node>(alpha, beta, ply);

        if let Some(score) = result {
            debug_assert!(
                score.is_node_score(),
                "quiescence returned {score:?} outside the node score band \
                 (window {alpha:?}..{beta:?}, ply {ply})",
            );
        }

        result
    }

    fn quiesce_inner<T: Thread, Node: NodeType>(
        &mut self,
        mut alpha: Score,
        mut beta: Score,
        ply: usize,
    ) -> NodeResult {
        self.trace.visit_q_node();

        debug_assert!(!Node::root());
        debug_assert!(Score::INF_N <= alpha);
        debug_assert!(alpha < beta);
        debug_assert!(beta <= Score::INF_P);
        debug_assert!(Node::pv() || alpha.inc_one() == beta);

        if self.stopping() {
            return None;
        }

        // Step 1. Check for an immediate draw. Quiet check evasions can repeat positions, so this
        // must happen before following another evasion.
        //
        // This must use the same boundary as the main search: the fifty-move rule counts 100 plies,
        // not 50. Comparing the clock against 50 here reported a draw at 25 moves.
        if self.is_history_draw() {
            return Some(Score::zero());
        }

        // Normalize search bounds into the range a node can return, on the same terms as `search`.
        //
        // This is not mate-distance pruning. Quiescence once had no equivalent normalization,
        // which let the bound excursion compound:
        // `child_bound` is exact, so `Score(20_101)` became the next ply's alpha, then
        // `Score(-20_102)`, and so on. Quiescence returns `alpha` and `beta` directly as fail-soft
        // scores, so those out-of-band bounds became node scores.
        alpha = alpha.clamp(Score::mate(0), Score::mate(1));
        beta = beta.clamp(Score::mate(0), Score::mate(1));
        if alpha >= beta {
            return Some(alpha);
        }

        // The window this node was given, kept for classifying whatever value it ends up storing.
        // Nothing below is allowed to move it, which is why the cutoff at Step 4 does not narrow the
        // live window: a bound recorded against a window a previous search supplied would describe
        // that search's result rather than this node's.
        let alpha_on_entry = alpha;

        // Sampled after the draw check above, on the same terms as the main search: if a
        // history-sensitive draw is claimed anywhere below this node, its value depends on how the
        // position was reached and must not be published as position-intrinsic. See
        // `Search::history_draws`.
        let history_draws_on_entry = self.history_draws;

        // Step 3. Load transposition table entry.
        let tt_entry = self.tt.probe(self.pos.zobrist().0);
        // Captured before the entry is consumed by the Step 4 cutoff filter. Reused as the stand-pat
        // value at Step 5, which is what the static evaluation is here; see [`Snapshot::eval`].
        let tt_eval = tt_entry.as_ref().and_then(Snapshot::eval);
        match tt_entry {
            Some(_) => self.trace.hash_hit(),
            None => self.trace.hash_miss(),
        }

        // Step 4. Check for early TT cutoff.
        if !Node::pv() {
            // A quiescence node searches to [`Self::QUIESCENCE_DRAFT`], so every entry in the table
            // is deep enough for it: its own earlier results, and any main-search result, which is
            // strictly better informed. The stored score remains an alpha-beta bound; it is never a
            // replacement for the position's static evaluation.
            //
            // Any verified entry may be trusted, with or without a move, for the reason set out in
            // the main search's Step 3: identity is established by the full-key check inside
            // `Table::probe`, not by whether the stored move happens to be playable here. The two
            // searches deliberately behave the same way, and quiescence not needing the move for
            // ordering is why it never looks at one.
            //
            // The clock gate applies here for the same reason it applies in the main search: a
            // stored value never accounts for the fifty-move rule, so it may only be reused where
            // the rule is still out of reach.
            if let Some(entry) = tt_entry.filter(|e| self.clock_permits_tt_reuse(e.depth())) {
                match entry.bound() {
                    Bound::Exact => {
                        return Some(entry.score());
                    }
                    Bound::Lower => {
                        if entry.score() >= beta {
                            return Some(entry.score());
                        }
                    }
                    Bound::Upper => {
                        if entry.score() <= alpha {
                            return Some(entry.score());
                        }
                    }
                }
            }
        }

        let in_check = self.pos.in_check();

        // Step 5. Static evaluation. Stand pat is not a legal option while in check, so a node in
        // check carries no static evaluation to publish. Otherwise a verified hit's cached
        // evaluation is the stand-pat value directly — it is position-intrinsic, so there is no need
        // to recompute it (see [`Snapshot::eval`]).
        let eval = if in_check {
            None
        } else {
            let stand_pat = match tt_eval {
                Some(stored) => {
                    debug_assert_eq!(
                        stored,
                        self.evaluate(),
                        "cached static evaluation disagrees with recomputation"
                    );
                    stored
                }
                None => self.evaluate(),
            };

            if stand_pat >= beta {
                // The value returned is the hard-fail `beta`, but what is *known* is the stronger
                // statement that this node is worth at least `stand_pat`. Recording the stronger
                // bound lets a later visit with a higher beta still cut off here.
                self.store_quiescence(
                    stand_pat,
                    Some(stand_pat),
                    Bound::Lower,
                    &Move::null(),
                    history_draws_on_entry,
                );
                return Some(beta);
            }

            if alpha < stand_pat {
                alpha = stand_pat;
            }

            Some(stand_pat)
        };

        if in_check {
            let moves = self.pos.generate::<BasicMoveList, AllGen, Legal>();
            return self.quiesce_evasions::<T, Node>(
                alpha,
                beta,
                ply,
                &moves,
                history_draws_on_entry,
            );
        }

        // Step 6. Loop through all the moves until no moves remain or a beta cutoff occurs.
        let mut best_move = Move::null();
        let mut moves = OrderedMoves::new();
        'move_loop: while moves.load_next_phase(QMoveLoader::from(self)) {
            for mov in &mut moves {
                if self.stopping() {
                    break 'move_loop;
                }

                // Static quiescence cuts, decided from the pre-move position and applied once the
                // move is on the board. This loop is only reached with the side to move not in check
                // — an in-check node hands off to `quiesce_evasions` above — so no reply here is a
                // forced evasion. The cuts apply to captures only: a queen promotion is not a capture
                // in isolation and is always worth resolving. A capture that gives check is exempt —
                // a checking capture at the horizon is how a sacrifice delivers mate, and its child,
                // being in check, searches every evasion or detects checkmate — but whether it checks
                // is only known once it is made, so the decision is prepared here and taken below.
                let statically_cut = self.see_pruning_enabled() && mov.is_capture() && {
                    // Delta cut: the most this capture can add to the stand-pat value is the piece it
                    // takes plus, for a promoting capture, a queen's premium over a pawn. If even
                    // that optimistic ceiling plus a cushion for what a bare material count cannot see
                    // still falls short of alpha, the capture cannot lift this node. Not taken against
                    // a mate-distance alpha, where a centipawn margin means nothing.
                    let delta_cut = alpha.is_cp() && {
                        let stand_pat = eval
                            .expect("quiescence past the in-check handoff always has a stand pat");
                        let mut optimistic =
                            i32::from(piece_value(self.pos.piece_at_sq(mov.dest()).type_of()));
                        if mov.is_promo() {
                            optimistic += i32::from(piece_value(PieceType::Queen))
                                - i32::from(piece_value(PieceType::Pawn));
                        }
                        i32::from(stand_pat.to_i16())
                            + optimistic
                            + i32::from(QUIESCENCE_DELTA_MARGIN)
                            <= i32::from(alpha.to_i16())
                    };

                    // SEE cut: a capture the exchange swing-off scores as losing material hands the
                    // opponent a favourable recapture and is not searched.
                    delta_cut
                        || self
                            .see(
                                mov.orig(),
                                mov.dest(),
                                self.pos.piece_at_sq(mov.dest()).type_of(),
                                self.pos.piece_at_sq(mov.orig()).type_of(),
                            )
                            .to_i16()
                            < QUIESCENCE_SEE_THRESHOLD
                };

                // SAFETY: quiescence moves originate from move generation for `self.pos`.
                unsafe { self.make_move(&mov) };

                // Take the prepared cut now that the move's check status is known: a capture that
                // neither wins its exchange nor could plausibly reach alpha is dropped, unless it
                // gives check and might yet force mate.
                if statically_cut && !self.pos.in_check() {
                    self.unmake_move();
                    self.trace.see_skip_node();
                    continue;
                }

                // As in the main search: start the child's cluster fetch as soon as its key exists,
                // so the miss overlaps the descent instead of stalling in front of the probe.
                self.tt.prefetch(self.pos.zobrist().0);
                let child =
                    self.quiesce::<T, Node>(beta.child_bound(), alpha.child_bound(), ply + 1);
                self.unmake_move();
                // An aborted child leaves no usable value, and returning here without storing is
                // what keeps a truncated subtree out of the table.
                let score = child?.neg().inc_mate();

                if score >= beta {
                    self.store_quiescence(score, eval, Bound::Lower, &mov, history_draws_on_entry);
                    return Some(beta);
                }

                if score > alpha {
                    alpha = score;
                    best_move = mov;
                }
            }
        }

        // A stop breaks out of the loop with some captures unexamined, so `alpha` describes a
        // subtree that was never finished. It is neither returned nor stored.
        if self.stopping() {
            return None;
        }

        self.store_quiescence(
            alpha,
            eval,
            self.quiescence_bound(alpha, alpha_on_entry),
            &best_move,
            history_draws_on_entry,
        );
        Some(alpha)
    }

    fn quiesce_evasions<T: Thread, Node: NodeType>(
        &mut self,
        mut alpha: Score,
        beta: Score,
        ply: usize,
        moves: &BasicMoveList,
        history_draws_on_entry: u64,
    ) -> NodeResult {
        // In check there is no stand pat, so the caller's alpha reaches here untouched and is still
        // the window this node was given.
        let alpha_on_entry = alpha;

        if moves.is_empty() {
            // Checkmate: terminal, certain, and with no continuation to record. This is the entry
            // shape that a move-gated cutoff can never reuse, which is why the cutoff paths in both
            // searches are gated on the score alone.
            self.store_quiescence(
                Score::mate(0),
                None,
                Bound::Exact,
                &Move::null(),
                history_draws_on_entry,
            );
            return Some(Score::mate(0));
        }

        let mut best_move = Move::null();

        for mov in moves {
            if self.stopping() {
                return None;
            }

            self.make_move_checked(mov);
            let child = self.quiesce::<T, Node>(beta.child_bound(), alpha.child_bound(), ply + 1);
            self.unmake_move();
            let score = child?.neg().inc_mate();

            if score >= beta {
                self.store_quiescence(score, None, Bound::Lower, mov, history_draws_on_entry);
                return Some(beta);
            }

            if score > alpha {
                alpha = score;
                best_move = *mov;
            }
        }

        self.store_quiescence(
            alpha,
            None,
            self.quiescence_bound(alpha, alpha_on_entry),
            &best_move,
            history_draws_on_entry,
        );
        Some(alpha)
    }

    /// Classifies a quiescence value that neither reached beta nor was cut short.
    ///
    /// Quiescence fails hard, so the value it returns is `alpha`, and what that means depends
    /// entirely on whether anything raised it. A raised alpha was produced by a child that scored
    /// strictly inside its own window, or by a stand pat that no capture beat; either way it is the
    /// position's quiescence value rather than a threshold, so it is exact. An alpha that never
    /// moved carries no information beyond "nothing here reached it", which is an upper bound.
    #[inline(always)]
    fn quiescence_bound(&self, alpha: Score, alpha_on_entry: Score) -> Bound {
        if alpha > alpha_on_entry {
            Bound::Exact
        } else {
            Bound::Upper
        }
    }

    /// Publishes a completed quiescence result at [`Self::QUIESCENCE_DRAFT`].
    ///
    /// Every caller has already established that the value came from work that ran to completion:
    /// an aborted quiescence subtree propagates `None` and never arrives here. The remaining
    /// condition is the one the main search applies at Step 24 — a value that a history-sensitive
    /// draw contributed to is not a property of the position, so it is dropped rather than stored.
    #[inline]
    fn store_quiescence(
        &self,
        score: Score,
        eval: Option<Score>,
        bound: Bound,
        mov: &Move,
        history_draws_on_entry: u64,
    ) {
        if self.history_draws == history_draws_on_entry {
            self.tt.store(
                self.pos.zobrist().0,
                score,
                eval,
                Self::QUIESCENCE_DRAFT,
                bound,
                mov,
            );
        }
    }

    fn emit_progress(&self, depth: u8, score: Score) {
        self.emit(SearchEvent::Progress(SearchProgress {
            depth,
            score,
            elapsed: self.trace.live_elapsed(),
            nodes: self.trace.nodes_visited(),
            principal_variation: self.pvt.pv().copied().collect(),
            hashfull: self.tt.hashfull(),
            nps: self.trace.live_nps() as u32,
        }));
    }

    /// The reported depth is the node's remaining depth, which at the root is the iteration depth.
    /// UCI has no representation for a non-positive depth, and the root never has one.
    fn emit_current_move(&self, depth: Depth, mov: &Move, num: u8) {
        debug_assert!(depth >= 1);
        self.emit(SearchEvent::CurrentMove(CurrentMove {
            depth: depth.clamp(1, Depth::from(u8::MAX)) as u8,
            current_move: *mov,
            number: num,
        }));
    }

    fn emit(&self, event: SearchEvent) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
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
                " misses:     {:>8} ({:.1}%)",
                self.trace.hash_misses().separated_string(),
                self.trace.hash_misses() as f64 / self.trace.hash_probes() as f64 * 100.
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
            let attempts = self.trace.killer_attempts();
            let cutoffs = self.trace.killer_cutoffs();
            for (slot, (&searched, &cut)) in attempts.iter().zip(cutoffs.iter()).enumerate() {
                let rate = if searched > 0 {
                    cut as f64 / searched as f64 * 100.
                } else {
                    0.
                };
                println!(
                    "killer slot {}: {} searched, {} cutoffs ({:.2}%)",
                    slot + 1,
                    searched,
                    cut,
                    rate
                );
            }
        }
    }
}

pub struct MoveLoader<'a, 'search> {
    search: &'a mut Search<'search>,
    hash_move: Option<Move>,
    /// Ply from the root of the node being ordered, used to find that ply's killer moves.
    ply: usize,
}

impl<'a, 'engine> MoveLoader<'a, 'engine> {
    /// Create a `MoveLoader` from the passed `Search`.
    #[inline(always)]
    pub fn from(search: &'a mut Search<'engine>, hash_move: Option<Move>, ply: usize) -> Self {
        MoveLoader {
            search,
            hash_move,
            ply,
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
        // Both slots are loaded in recency order. Which of them was actually searched, and which
        // produced a cutoff, is attributed by slot in the main move loop after staged ordering has
        // dropped any killer that duplicated an earlier phase.
        let (km1, km2) = self.search.kt.probe(self.ply, &self.search.pos);
        if let Some(km) = km1 {
            movelist.push(km);
        }
        if let Some(km) = km2 {
            movelist.push(km);
        }
    }

    fn load_counter(&mut self, movelist: &mut ScoredMoveList) {
        // When folded, the counter contributes as a score bonus in `score_quiets` and has no stage.
        if FOLD_COUNTER_INTO_QUIETS {
            return;
        }
        let contexts = self.search.continuation_contexts(self.ply);
        if let Some(counter) = self.counter_move(contexts[0]) {
            movelist.push(counter);
        }
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

    fn score_capture_history(&mut self, captures: Scorer) {
        for (mov, score) in captures {
            if mov.is_capture() {
                // SAFETY: a capture is made by a real piece onto a square whose captured piece is a
                // real, non-king type (en passant reports its captured pawn directly), so the key is
                // always in range.
                let mover = self.search.pos.piece_at_sq(mov.orig());
                let captured = self.search.captured_piece_type(mov);
                let history = unsafe {
                    self.search
                        .capture_history
                        .get_unchecked(mover, mov.dest(), captured)
                };
                // The score currently holds the static exchange value, which the phase partition has
                // already consumed; adding the bounded history term only reorders captures within the
                // phase they were placed in.
                *score = score.saturating_add(capture_history_order_term(history));
            }
        }
    }

    fn score_quiets(&mut self, quiets: Scorer) {
        let turn = self.search.pos.turn();
        let contexts = self.search.continuation_contexts(self.ply);
        // The counter move only influences scoring when it is folded rather than staged; otherwise
        // its own stage yields it and it is suppressed from the quiets entirely.
        let folded_counter = if FOLD_COUNTER_INTO_QUIETS {
            self.counter_move(contexts[0])
        } else {
            None
        };

        for (mov, score) in quiets {
            // SAFETY: these are legal quiet moves, so both squares are valid and the mover is a real
            // piece on its origin square.
            let cur_piece = self.search.pos.piece_at_sq(mov.orig());
            let mut raw = unsafe {
                self.search
                    .history
                    .get_unchecked(mov.orig(), mov.dest(), turn)
            };
            for (dist, ctx) in contexts.iter().enumerate() {
                if let Some((prev_piece, prev_to)) = *ctx {
                    // SAFETY: `dist` is a tracked continuation distance and every piece is real.
                    raw += unsafe {
                        self.search.cont_hist.get_unchecked(
                            dist,
                            prev_piece,
                            prev_to,
                            cur_piece,
                            mov.dest(),
                        )
                    };
                }
            }
            if folded_counter == Some(*mov) {
                raw += COUNTER_FOLD_BONUS;
            }
            *score = history_ordering_score(raw);
        }
    }
}

impl MoveLoader<'_, '_> {
    /// The recorded counter move for the one-ply-back context, if one exists and is a legal move in
    /// this position. Returns `None` when there is no preceding move, no reply has been stored, or
    /// the stored reply is not legal here — the legality check that keeps an externally stored move
    /// from being executed unsafely.
    #[inline]
    fn counter_move(&self, one_ply_back: Option<(Piece, Square)>) -> Option<Move> {
        let (prev_piece, prev_to) = one_ply_back?;
        let counter = self.search.counter.get(prev_piece, prev_to);
        (!counter.is_null() && self.search.pos.valid_move(&counter)).then_some(counter)
    }
}

/// Move loader for the quiescence search.
///
/// The staged picker this drives yields only queen promotions and captures, in that order. It
/// deliberately loads no quiet moves and no check evasions: `quiesce` tests for check up front and
/// routes an in-check node to `quiesce_evasions`, which searches every reply from a plain move list,
/// before the staged loop is ever entered. So this loader only runs with the side to move not in
/// check, where a bare quiet cannot improve the stand-pat value and is skipped by design. There is
/// consequently no `load_quiets`/`score_quiets` here — a quiet segment would always be empty.
pub struct QMoveLoader<'a, 'search> {
    search: &'a mut Search<'search>,
}

impl<'a, 'engine> QMoveLoader<'a, 'engine> {
    /// Create a `MoveLoader` from the passed `Search`.
    #[inline(always)]
    pub fn from(search: &'a mut Search<'engine>) -> Self {
        QMoveLoader { search }
    }
}

impl<'a, 'search> Loader for QMoveLoader<'a, 'search> {
    fn load_promotions(&mut self, movelist: &mut ScoredMoveList) {
        self.search
            .pos
            .generate_in::<_, QueenPromotions, Legal>(movelist);
    }

    fn load_captures(&mut self, movelist: &mut ScoredMoveList) {
        self.search.pos.generate_in::<_, Captures, Legal>(movelist);
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
}

#[cfg(test)]
mod tests;
