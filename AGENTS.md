# Seaborg

Seaborg is a Rust workspace for a chess engine and UCI executable.

For Rust changes, run `cargo fmt --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`, and
`cargo test --workspace` before finishing. These three are the
repository-required checks; strict Clippy is a gate, not advisory, so a warning
fails the change exactly as a test failure does. Fix warnings at the source.
Use a local `#[allow]` only where the warned construct is genuinely required,
with a comment stating why.

## Task lifecycle

Backlog task statuses have these meanings:

- `To Do`: eligible for an implementation agent to claim.
- `In Progress`: exclusively owned implementation or rework is underway.
- `In Review`: implementation is committed and ready for independent review.
- `Changes Requested`: blocking review findings must be resolved on the same task.
- `Ready to Merge`: an independent reviewer approved a specific immutable commit.
- `Needs Human`: automation cannot proceed safely without a decision or intervention.
- `Done`: the approved work was merged successfully. Approval alone is not done.

Use the repository skills for task processing:

- `$implement` for selection, implementation, and review rework.
- `$review` for independent review and Backlog verdict recording.

Implementation agents must not approve their own work, move tasks to `Ready to
Merge` or `Done`, or create follow-up tasks for review findings. Review agents
must not fix the implementation they are reviewing. Blocking findings stay on
the original task with stable finding IDs. Review only committed, immutable
targets; any implementation change invalidates prior approval.

Every task is implemented on one dedicated task branch in one dedicated
worktree. Create or reattach that worktree before changing task state. Code,
plans, handoffs, review findings, and approval metadata are committed on the
task branch; do not mirror lifecycle edits onto the primary branch. Reviewers
reuse the same branch and worktree (or reattach the existing branch if its local
worktree was removed). The primary branch receives the complete task record
only when the approved task branch is merged.

Keep Backlog active-branch checking enabled so the board can surface task state
from feature branches. A task invocation authorizes local, task-scoped commits
in its worktree, but not pushing or merging.

The transition rules, review format, and manual handoff procedure are defined in
`TASK_LIFECYCLE.md`.

<!-- BACKLOG.MD GUIDELINES START -->
<!-- backlog.md-instructions-version: 1.48.0 -->

<CRITICAL_INSTRUCTION>

## Backlog.md Workflow

This project uses Backlog.md for task and project management.

**For every user request in this project, run `backlog instructions overview` before answering or taking action.**

Use the overview to decide whether to search, read, create, or update Backlog tasks.

Before task lifecycle actions, read the matching detailed guide:

- `backlog instructions task-creation` before creating or splitting tasks
- `backlog instructions task-execution` before planning, changing status or assignee, adding a plan or implementation notes, or implementing task work
- `backlog instructions task-finalization` before checking acceptance criteria, writing final summaries, or moving tasks to terminal statuses

Use `backlog <command> --help` before running unfamiliar commands. Help shows options, fields, and examples.

Do not edit Backlog task, draft, document, decision, or milestone markdown files directly. Use the `backlog` CLI so metadata, relationships, and history stay consistent.

</CRITICAL_INSTRUCTION>

<!-- BACKLOG.MD GUIDELINES END -->
