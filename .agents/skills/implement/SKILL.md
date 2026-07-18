---
name: implement
description: Implement or rework Seaborg Backlog.md tasks in a dedicated task branch and worktree, commit task-scoped code and lifecycle metadata, and prepare an immutable target for independent review. Use when Codex is asked to select, claim, implement, continue, or address review findings for a task in To Do or Changes Requested. Do not use for independent review, approval, pushing, merging, or moving a task to Done; use the review skill for review.
---

# Implement

Follow `../../../TASK_LIFECYCLE.md`. Keep code and Backlog lifecycle metadata on
one persistent task branch; never mirror task status changes onto the primary
branch.

## Select and enter the task worktree

1. Run `backlog instructions overview`, then read
   `backlog instructions task-execution` before lifecycle changes.
2. If the user names a task, read it. Otherwise select deterministically:
   - Prefer `Changes Requested` over `To Do`.
   - Then prefer higher priority, lower ordinal, and lower task ID.
   - Skip unfinished dependencies and work owned by another active session.
3. Accept only `To Do` or `Changes Requested`.
4. Inspect `git worktree list --porcelain` and existing local branches.
   - For `To Do`, create a task branch from the primary branch and attach it to
     a dedicated sibling worktree before changing the task.
   - For `Changes Requested`, reuse the original task branch and its worktree.
     If the worktree was removed, reattach that branch; do not create a new
     branch for a review attempt.
5. Change directory into the task worktree. Confirm its branch, base, and
   working-tree state before proceeding. Stop if unrelated changes there cannot
   be separated safely.

Use a stable branch such as `task-1.1-typed-engine-api` and a sibling worktree
such as `../seaborg-worktrees/task-1.1-typed-engine-api`. Never attach the task
branch to the primary worktree.

## Claim and plan

Run all Backlog mutations from the task worktree through the CLI. Move the task
to `In Progress`, assign the worker, research the repository, and record the
current plan. Commit the task-file change on the task branch so active-branch
views can observe the claim.

Invocation of this skill authorizes local, task-scoped commits in the task
branch and worktree. It does not authorize pushing or merging.

## Implement or rework

For new work, implement every acceptance criterion within scope. For rework:

- Read all review comments and enumerate unresolved `REV-N-NN` findings.
- Resolve every blocking finding on the same task branch.
- Add regression tests for correctness fixes.
- Record `Resolved REV-N-NN` with the behavior changed and verification run.
- Do not create a follow-up merely to defer a blocking finding.

Preserve unrelated changes. Do not edit the task's copy on the primary branch.

## Stop when the correct fix is larger than the task

Sometimes a defect is a symptom of a design that makes the correct behavior hard
to express, and the honest fix is a structural change the task never
anticipated. You are authorized to say so. Do not narrow the fix to what the
task literally allows merely because a narrow patch is within reach and
finishing is rewarded; a local patch that entrenches the wrong structure costs
the project more than the delay.

Distinguish two cases:

- The correct fix is within this task's surface but large or tedious. Do it.
- The correct fix requires changing structure shared with other code — an API,
  an abstraction boundary, a data representation, an invariant other modules
  rely on. Stop rather than work around it.

To stop, leave the worktree clean, move the task to `Needs Human`, and record
the proximate issue, why the in-scope fix is wrong or merely cosmetic, the
structural change you believe is correct, and its rough cost. Do not begin the
refactor and do not create the follow-up task yourself; the scope decision is
the human's.

## Create the review handoff

1. Read `backlog instructions task-finalization` for evidence requirements, but
   leave acceptance checks and the final summary to the independent reviewer.
2. Run the repository-required checks and focused tests. The required checks are
   `cargo fmt --check`,
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and
   `cargo test --workspace`. Strict Clippy is a gate, not advisory: a warning
   fails the handoff exactly as a test failure does. Never hand a branch to
   review with Clippy warnings outstanding, and record each check's result in
   the handoff `Verification` block.

   Fix warnings at the source rather than suppressing them. Use a local
   `#[allow]` only where the warned construct is genuinely required, with a
   comment stating why. If a warning is pre-existing and genuinely outside the
   task's scope, say so explicitly under `Known failures` with evidence that it
   reproduces at the base commit; do not silently leave it for the reviewer.
3. Commit all task-scoped implementation changes. The resulting clean commit is
   the immutable implementation target.
4. Append concise implementation notes and a handoff comment:

```text
Implementation handoff
Branch: <task branch>
Worktree: <absolute local path>
Base: <base sha>
Implementation target: <sha>
Resolved findings: <IDs or none>
Verification:
- <command>: <result>
Known failures: <none, or exact baseline evidence>
```

5. Move the task to `In Review` and commit this task-only handoff change. Leave
   the task branch and worktree clean for the reviewer.

Never approve, push, merge, move the task to `Ready to Merge` or `Done`, or
check acceptance criteria merely because implementation exists.
