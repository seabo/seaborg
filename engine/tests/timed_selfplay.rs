//! Fast timed self-play must never forfeit or hang.
//!
//! This is a regression fixture for the two failure modes that once made seaborg
//! unusable at fast timed controls under a UCI match runner (backlog task-32 and
//! task-34): returning an illegal `bestmove 0000` — a null move — when the clock
//! ran low, or hanging past the deadline instead of answering.
//!
//! Unlike the reproducible node-budget self-play in `engine::selfplay`, this
//! drives the *timed* path end to end: the per-move budget comes from
//! [`TimeControl::to_move_time`], the search runs under [`SearchLimit::Time`],
//! and the clock is stepped down by the wall-clock the move actually consumed
//! and back up by the increment, exactly as a GUI or FastChess would. The game
//! is therefore played against the real allocation policy and the real deadline,
//! including the moves where the clock has drained to nothing.
//!
//! The assertions are deliberately about safety, not strength: every move played
//! from a non-terminal position must exist and be legal, and every game must
//! reach a terminal position or the ply cap rather than running forever. The
//! fact that each `wait()` returns at all is itself the no-hang guarantee — a
//! genuine hang would never let the loop finish.

use std::time::{Duration, Instant};

use chess::mono_traits::{All, Legal};
use chess::movelist::BasicMoveList;
use chess::position::Position;

use engine::search::{SearchEngine, SearchLimit};
use engine::time::TimeControl;

/// A single fast timed control, in milliseconds: base clock and per-move
/// increment. Kept small so a whole game plays out in well under a second even
/// in a slow debug build, while still driving the allocation policy and the
/// deadline through the depleted-clock regime the failures lived in.
struct Control {
    base_ms: u64,
    inc_ms: u64,
}

/// No search from a non-terminal position may take longer than this to answer.
///
/// A real hang would block `wait()` forever and time the test out, so this is
/// not the primary no-hang guard; it exists to convert a merely pathological
/// slow move into a clear assertion failure with a ply number rather than an
/// opaque harness timeout. It is generous enough that ordinary debug-build
/// search — where even a first ply can be slow — never trips it.
const MOVE_ANSWER_CEILING: Duration = Duration::from_secs(10);

/// Play one whole game from `start` under `control`, single-engine self-play,
/// asserting at every move that the timed search returns a legal move and that
/// the game terminates.
fn play_timed_game(engine: &SearchEngine, start: Position, control: &Control, max_plies: usize) {
    let mut position = start;
    // Independent clocks per colour, stepped as a match runner would.
    let mut white_clock = control.base_ms;
    let mut black_clock = control.base_ms;

    for ply in 0..max_plies {
        // A position with no legal move is a natural terminal — checkmate or
        // stalemate — and the game is over. This is the only clean exit below
        // the ply cap, and reaching either proves the loop terminates.
        let legal = position.generate::<BasicMoveList, All, Legal>();
        if legal.is_empty() {
            return;
        }

        let turn = position.turn();
        let control_now = TimeControl::new(
            white_clock,
            black_clock,
            control.inc_ms,
            control.inc_ms,
            None,
        );
        // `to_move_time` takes a full-move number; two plies to a full move.
        let move_number = (ply as u32) / 2 + 1;
        let budget_ms = control_now.to_move_time(move_number, turn);

        let started = Instant::now();
        let outcome = engine
            .start(
                position.clone(),
                SearchLimit::Time(Duration::from_millis(budget_ms)),
            )
            .wait();
        let elapsed = started.elapsed();

        // The clock may have been empty: `to_move_time` can legitimately return
        // a zero budget. The guaranteed first ply still owes a legal move, so an
        // absent result here is the `bestmove 0000` forfeit the fixture exists to
        // catch.
        let result = outcome.result().unwrap_or_else(|| {
            panic!(
                "{}+{}: no result at ply {ply} (a `bestmove 0000` forfeit) \
                 with clocks {white_clock}/{black_clock}ms, budget {budget_ms}ms",
                control.base_ms, control.inc_ms
            )
        });
        let mov = result.best_move.unwrap_or_else(|| {
            panic!(
                "{}+{}: null best move at ply {ply} with budget {budget_ms}ms",
                control.base_ms, control.inc_ms
            )
        });

        // The move must be one the position actually admits. Membership in the
        // freshly generated legal list is the authoritative oracle — the same
        // generator the search itself draws from — and, unlike `Position::valid_move`,
        // it recognises castles. A null or stale move is simply absent from it.
        assert!(
            (&legal).into_iter().any(|legal_move| *legal_move == mov),
            "{}+{}: search returned {mov:?} at ply {ply}, not among the position's \
             legal moves (budget {budget_ms}ms)",
            control.base_ms,
            control.inc_ms
        );

        assert!(
            elapsed < MOVE_ANSWER_CEILING,
            "{}+{}: search took {elapsed:?} at ply {ply} (budget {budget_ms}ms) — a hang",
            control.base_ms,
            control.inc_ms
        );

        // Step the mover's clock as a runner would: charge the wall-clock the
        // move actually cost, credit the increment, and floor at zero. Charging
        // real elapsed rather than the allotment is deliberate — it walks the
        // clock down into the depleted regime where the failures used to appear.
        let clock = if turn.is_white() {
            &mut white_clock
        } else {
            &mut black_clock
        };
        *clock = clock
            .saturating_sub(elapsed.as_millis() as u64)
            .saturating_add(control.inc_ms);

        position.make_move(&mov);
    }

    // Reaching the cap without a natural terminal is fine: the game between two
    // equal engines can legitimately shuffle to a draw the cap stands in for.
    // The point the cap proves is only that the loop ends — that nothing hung.
}

/// Starting positions spanning opening, a sharp middlegame, and a spare endgame
/// close to terminal, so the timed path is exercised well away from the start
/// position too.
fn starts() -> Vec<Position> {
    let fens = [
        // Kiwipete: a busy middlegame with many legal moves and tactics.
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        // A king-and-pawn endgame: few pieces, short searches, near-terminal.
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    ];
    let mut positions = vec![Position::start_pos()];
    positions.extend(
        fens.into_iter()
            .map(|fen| Position::from_fen(fen).expect("valid FEN")),
    );
    positions
}

#[test]
fn fast_timed_self_play_never_forfeits_or_hangs() {
    chess::init::init_globals();

    // A sudden-death control and two increment controls, all fast. The
    // sudden-death clock actually reaches zero within the cap, which is the
    // case that used to forfeit; the increment controls keep the clock alive so
    // the game runs the full ply cap under sustained time pressure.
    let controls = [
        Control {
            base_ms: 300,
            inc_ms: 0,
        },
        Control {
            base_ms: 300,
            inc_ms: 10,
        },
        Control {
            base_ms: 120,
            inc_ms: 5,
        },
    ];

    let mut engine = SearchEngine::new(8);
    for control in &controls {
        for start in starts() {
            // A fresh table per game keeps the games independent.
            engine.new_game();
            play_timed_game(&engine, start, control, 24);
        }
    }
}
