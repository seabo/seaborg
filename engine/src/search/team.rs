//! The Lazy SMP search-team contract.
//!
//! This module is the checked-in, single source of truth for how a *team* of search workers
//! cooperating on one root position is composed, what state they share, which of them produces the
//! move the engine plays, and when the team is considered finished. It exists so that the tasks
//! that later spawn and orchestrate helper workers can be written against a fixed set of rules
//! rather than against implicit assumptions about which state is shared and which result is
//! authoritative.
//!
//! Nothing here spawns a second production worker. The current engine runs one worker per search
//! (see [`SearchEngine::start`](super::SearchEngine::start)), and this module leaves that path
//! byte-for-byte unchanged. What it adds is the durable specification plus the compile-time
//! classification ([`SharedTeamState`] / [`PerWorkerState`]) that keeps a future orchestrator from
//! accidentally sharing state that must stay private to one worker.
//!
//! The contract generalises, and must preserve, four guarantees the single-worker engine already
//! provides. Each is called out at the rule that carries it:
//!
//! * *Explicit completion.* Completion is announced by an explicit signal, never inferred from a
//!   channel disconnecting. (The one-worker engine sends on `SearchHandle`'s `finished` channel.)
//! * *Prompt cancellation after a legal fallback.* A worker records a legal root move before
//!   searching any node, and honours cancellation immediately thereafter.
//! * *Aborted subtrees contribute nothing.* An abandoned subtree returns "no result" and never
//!   raises alpha, becomes a best move, enters a PV, or is written to the table.
//! * *One shared table, cleared only when quiescent.* Every worker shares one transposition-table
//!   allocation through an `Arc`; the table can only be cleared or replaced once no worker still
//!   holds it.
//! * *Join on drop.* No worker may outlive the handle that owns it. Dropping (or waiting on) the
//!   handle cancels every worker and joins it, so once the handle is gone every worker has exited
//!   and released its table clone. This is the guarantee that makes clearing the table safe (§4);
//!   it is distinct from the completion signal (§2), which announces the result but does not by
//!   itself release the table.
//!
//! # 1. Team composition, identity, and the authoritative result
//!
//! A **team** is the set of workers started to search one root position for one `go` request:
//!
//! * exactly one **master**, and
//! * zero or more **helpers**.
//!
//! The **team identity** is that shared `go`: every worker in a team searches the same root
//! position, reads and writes one shared transposition table, observes one shared cancellation
//! flag, and is bounded by one shared limit. A worker started for a different `go` is a different
//! team. Starting the next search is starting a new team; the previous team must have stopped
//! first (the driver stops the active search before starting another, and dropping a handle joins
//! its worker).
//!
//! **Shared team state** is held once and reached by every worker through a shared reference. It is
//! classified by [`SharedTeamState`] and must be `Send + Sync`, because a plain `&T` crosses into
//! every worker thread. It comprises:
//!
//! * the transposition table ([`Table`], one `Arc<Table>` allocation);
//! * the cancellation flag (`AtomicBool`, reached through a
//!   [`CancellationToken`](super::CancellationToken));
//! * the search limit — depth, deadline, or node budget — the team searches under.
//!
//! **Per-worker state** is owned privately by one worker, mutated through `&mut self`, and never
//! reached by another worker. It is classified by [`PerWorkerState`]. It comprises everything in
//! [`Search`](super::Search) that is not the shared table or the shared stop flag: the worker's own
//! copy of the position, its incrementally maintained evaluation and eval stack, its killer table,
//! its history table, its PV table, its tracer/node counters, its per-ply stack, and its root
//! fallback. The root position is logically shared, but each worker owns a *mutable copy* of it (a
//! search mutates its board as it descends), so the copy is per-worker state, not shared state.
//!
//! The distinction is not stylistic. Shared state coordinates its own concurrent access and is used
//! through `&self` (the table's slots are atomics; the stop flag is an `AtomicBool`). Per-worker
//! heuristics have no such coordination and are used through `&mut self`; sharing one across workers
//! would either be a data race or force a mutex onto the search hot path. See
//! [`SharedTeamState`] and [`PerWorkerState`] for the compile-time form of this boundary.
//!
//! **Authoritative-result rule (baseline policy).** The move the engine plays is the result of the
//! **master's last fully completed iteration**. If the master completed no iteration, the master's
//! legal root fallback is authoritative (see §5). Helpers never contribute a result directly:
//! their only influence on the played move is indirect, through entries they leave in the shared
//! transposition table, which improve every worker's move ordering and cutoffs. Cross-worker
//! voting — choosing the played move by comparing results across workers, or letting a helper that
//! searched deeper override the master — is deliberately **not** part of this baseline. It is a
//! later strength experiment and must be introduced as its own change; until then the master alone
//! is authoritative. Fixing the authority on the master is what keeps the played move well defined
//! and reproducible regardless of how many helpers ran or how they were scheduled.
//!
//! # 2. Team outcomes and the single completion signal
//!
//! A team ends in exactly one of four outcomes:
//!
//! * **Completed** — the master reached the limit normally (see §3) without being cancelled, and
//!   carries the master's last completed result (or its legal fallback).
//! * **Cancelled** — the team was stopped externally (a UCI `stop`, a replacement `go`, `quit`,
//!   stdin EOF, or a dropped handle) before the limit was reached normally. It still carries the
//!   master's best completed result, or its legal fallback (§5).
//! * **Failed** — a worker could not run its search to a defined completed/cancelled end for a
//!   reason other than a panic. A helper failure never degrades the outcome below what the master
//!   produced: the master remains authoritative and the team still reports its result. A master
//!   failure is a team failure.
//! * **Panicked** — a worker thread unwound. A helper panic must not deny the team the master's
//!   result and must not abort the process on the join path; a master panic surfaces as a team
//!   panic to its owner, exactly as joining a single panicked worker does today.
//!
//! **The single explicit completion signal.** A team announces completion exactly once, through one
//! explicit signal, for every outcome above. The owner never has to *infer* that the team finished
//! from a side effect such as an events channel disconnecting; that inference has been observed to
//! lose the wakeup and park the owner forever. The signal's only precondition is that **the master's
//! outcome is fixed** — its authoritative result, or fallback, is final. It says the played move is
//! ready, so the owner can stop waiting and collect it.
//!
//! The signal does **not** imply the table has been released. A worker — including the master that
//! emits the signal — may still hold its clone of the shared table when the signal fires, and
//! helpers may still be winding down. This matches today's one worker exactly: it sends its
//! completion signal while it still owns its table clone and releases that clone only as the thread
//! exits (see [`SearchEngine::start`](super::SearchEngine::start)). Table release, and the
//! clear-safety that depends on it, is a *separate* guarantee provided by joining every worker, not
//! by the signal; see the join-on-drop guarantee and §4. Coupling the two — emitting the signal only
//! after every worker had dropped the table — is not today's behaviour and is not required: an owner
//! that wants to clear the table joins the team first rather than trusting the signal to have done
//! it.
//!
//! # 3. Limit semantics, and which worker decides normal completion
//!
//! Every worker in a team searches under the same limit, but **the master decides normal
//! completion**. A helper reaching or exceeding the limit on its own does not end the team; a
//! helper is stopped only when the team is cancelled or when the master's normal completion causes
//! the team to stop. This holds for all four limits:
//!
//! * **Fixed depth `d`.** The team completes normally when the *master* finishes iteration `d`.
//!   Helpers may be searching a shallower or deeper iteration at that instant; they are stopped
//!   without their in-flight iteration becoming a result.
//! * **Time.** All workers share one deadline. The team completes normally when the deadline stops
//!   the *master* after it has completed at least the guaranteed first ply (a budget too small to
//!   finish one ply still yields a searched move, never the unsearched fallback). The master
//!   decides.
//! * **Nodes.** The node budget is the *reproducible* limit, so authoritative completion is bound
//!   to the **master's own node counter**: the team completes normally when the master's count
//!   reaches the budget after the guaranteed first ply. A helper counts its own nodes and may stop
//!   itself, but a helper's count never decides team completion. Binding authority to one counter is
//!   what preserves `go nodes` reproducibility, which aggregating counts across nondeterministically
//!   scheduled workers would destroy.
//! * **Infinite.** No worker decides completion. The team runs until it is cancelled externally;
//!   there is no normal-completion path.
//!
//! In every case a partially searched iteration — the master's or a helper's — is discarded rather
//! than reported (see §5).
//!
//! # 4. Transposition-table rules
//!
//! * **Age advances once per root search, not once per worker.** The team advances the table's
//!   replacement age exactly once, when the team starts, before any worker begins. Every worker in
//!   the team then stamps its writes with that one age. Age never invalidates an entry; it only
//!   prioritises replacement, so a shared, once-per-team advance is safe under `&self`.
//! * **One allocation, shared, owned by no worker.** Every worker shares a single `Arc<Table>`
//!   allocation. Probes, replacement selection, stores, and telemetry all go through `&self`; no
//!   slot is reserved for a writer, so every worker may consume every other worker's entries.
//! * **Clearing or replacement only after every worker releases the table.** The table may be
//!   cleared ([`Table::clear`](crate::tt::Table::clear)) or replaced only once no worker still holds
//!   a clone of the `Arc`. This is enforced, not merely documented:
//!   [`clear_hash`](super::SearchEngine::clear_hash) obtains the exclusive reference `clear` needs
//!   through `Arc::get_mut`, which only succeeds once the last worker has dropped its clone. What
//!   makes that reference reachable is the join-on-drop guarantee, **not** the completion signal:
//!   dropping or waiting on the team handle cancels and joins every worker, so no worker can outlive
//!   the handle still holding a clone, and the join is what releases the last one. The completion
//!   signal (§2) only reports that the result is ready; a worker may still hold the table when it
//!   fires. An owner therefore clears by joining the team (letting the handle drop, or waiting on
//!   it) and only then calling [`clear_hash`](super::SearchEngine::clear_hash) — never by clearing on
//!   the signal alone, which would race a worker that has not yet exited. Today's one-worker engine
//!   is exactly this: [`SearchHandle`](super::SearchHandle) cancels and joins its worker on drop, and
//!   the following `ucinewgame` clear then finds the table unshared.
//!
//! # 5. Legal fallback, and no partial or aborted result
//!
//! * **Legal root fallback.** Before searching any node, the master records a legal root move as a
//!   fallback (or `None` for a terminal root position, which is reported as `bestmove 0000`).
//!   Cancellation is honoured only once this fallback exists, so an interrupted search still plays a
//!   legal move. As the first ply progresses the fallback is upgraded to the best *fully searched*
//!   root move, so even a mid-first-ply cancellation reports a searched move rather than an
//!   arbitrary first-generated one. The team's reported move is therefore always at least the
//!   master's legal fallback.
//! * **No partial or aborted iteration becomes official.** Only a fully completed iteration of the
//!   master is authoritative. An aborted subtree yields "no result" and must not raise alpha, become
//!   a best move, enter the reported PV, or be written to the table; the last completed PV is
//!   preserved across an aborted candidate iteration. This applies to helpers as well: a helper's
//!   abandoned or in-flight iteration influences the played move only through whatever *completed*
//!   entries it had already committed to the shared table, never as a reported result.
//!
//! # 6. The one-worker case
//!
//! A team with zero helpers is exactly today's engine. The master is the only worker; it owns all
//! per-worker state, shares the table with no one, decides its own completion, and emits the single
//! completion signal on its way out. Every rule above degenerates to the current behaviour, so
//! introducing helpers later must not perturb the zero-helper path.

use crate::eval::EvalState;
use crate::history::HistoryTable;
use crate::killer::KillerTable;
use crate::pv_table::PVTable;
use crate::trace::Tracer;
use crate::tt::Table;

/// State a whole team shares as one allocation, reached by every worker through a shared reference.
///
/// The `Send + Sync` bound is the contract, not decoration: a value classified here is handed to
/// every worker as a plain `&T` that crosses thread boundaries, so it must be safe to share and to
/// send. A type earns this classification only by coordinating its own concurrent access (atomics,
/// not `&mut`). Promoting a per-worker heuristic to shared state is therefore never accidental: it
/// requires writing an `impl SharedTeamState`, which will not compile unless the type is already
/// `Send + Sync`, forcing the concurrency question to be answered deliberately.
///
/// This is the single place shared-versus-per-worker classification is recorded. New search state a
/// later task introduces must be classified here (as shared) or by [`PerWorkerState`] (as private),
/// so the decision is made in the contract rather than left implicit at a call site.
pub trait SharedTeamState: Send + Sync {}

/// State one worker owns privately, mutates through `&mut self`, and never shares with another
/// worker.
///
/// These are the search heuristics and bookkeeping that have no internal synchronisation. Each
/// worker in a team must be issued its own instance; sharing one across workers would be a data
/// race, or would force a lock onto the search hot path. The classification exists so that worker
/// orchestration can be written to *move* per-worker state into each worker (one instance each) and
/// only *borrow* [`SharedTeamState`], making an accidental share fail to type-check rather than
/// corrupt a search.
pub trait PerWorkerState {}

// The one shared allocation. `Table` is `Send + Sync` by construction, so it satisfies the bound.
impl SharedTeamState for Table {}

// The per-worker search heuristics and bookkeeping owned by one `Search`.
impl PerWorkerState for KillerTable {}
impl PerWorkerState for HistoryTable {}
impl PerWorkerState for PVTable {}
impl PerWorkerState for Tracer {}
impl PerWorkerState for EvalState {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one shared allocation must be safe to hand to every worker as `&T`, i.e. `Send + Sync`.
    /// The `SharedTeamState` bound already requires this; naming it as a standalone check keeps the
    /// requirement legible and fails loudly if the table ever loses its thread-safety.
    #[test]
    fn shared_team_state_is_send_and_sync() {
        fn assert_shared<T: SharedTeamState>() {}
        fn assert_send_sync<T: Send + Sync>() {}
        assert_shared::<Table>();
        assert_send_sync::<Table>();
    }

    /// Every per-worker heuristic is classified as such. This is a compile-time inventory: it is the
    /// list a later orchestrator must issue one-per-worker, and it fails to build if a type drops
    /// out of the classification.
    #[test]
    fn per_worker_heuristics_are_classified() {
        fn assert_per_worker<T: PerWorkerState>() {}
        assert_per_worker::<KillerTable>();
        assert_per_worker::<HistoryTable>();
        assert_per_worker::<PVTable>();
        assert_per_worker::<Tracer>();
        assert_per_worker::<EvalState>();
    }

    /// The shape by which orchestration will hand state to a worker: shared inputs are *borrowed*,
    /// so one allocation reaches every worker, while per-worker inputs are *moved*, so each worker
    /// consumes its own instance. The two calls below borrow the same table for two workers but
    /// issue each a distinct killer table — the shape that keeps a mutable heuristic from being
    /// shared. It would not compile if `SharedTeamState`/`PerWorkerState` were mixed up.
    #[test]
    fn shared_state_is_borrowed_and_per_worker_state_is_owned() {
        fn issue_to_worker<S: SharedTeamState, P: PerWorkerState>(
            shared: &S,
            per_worker: P,
        ) -> (&S, P) {
            (shared, per_worker)
        }

        let table = Table::new(1);
        let (_shared0, _kt0) = issue_to_worker(&table, KillerTable::new(1));
        let (_shared1, _kt1) = issue_to_worker(&table, KillerTable::new(1));
    }
}
