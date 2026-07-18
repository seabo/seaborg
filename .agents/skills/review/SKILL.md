---
name: review
description: Independently review a committed Seaborg Backlog.md task implementation in its existing task branch and worktree, then commit an approval or structured changes-requested verdict on that branch. Use when a task is In Review and Codex must inspect its full diff, verify acceptance criteria, run checks, and update Backlog without fixing implementation code. Do not use to implement findings, create a replacement review branch, push, merge, or move a task to Done; use the implement skill for rework.
---

# Review

Follow `../../../TASK_LIFECYCLE.md`. Review the implementation independently and
do not modify implementation files.

Review validates the change in isolation against the immutable base-to-target
diff. Do not test a prospective merge with the current primary tip: that breaks
target immutability and is stale the moment primary moves. The guarantee that
primary stays green after integration lives in `$merge`, which re-verifies the
merged result at merge time.

## Enter the implementation worktree

1. Run `backlog instructions overview`, read the task, and read
   `backlog instructions task-finalization` before checking acceptance criteria
   or writing a final summary.
2. Require the task to be `In Review`. Read the branch, base, implementation
   target, and worktree from its handoff comment.
3. Inspect `git worktree list --porcelain`:
   - If the task branch is attached, change directory into that worktree.
   - If it is not attached, create a dedicated worktree for the existing task
     branch at the normal sibling location.
4. Never create a review branch and never check the task branch out in the
   primary worktree. Review and Backlog verdicts stay on the implementation
   branch so they merge with the task.
5. Require a clean worktree, confirm the implementation target is an ancestor
   of the branch tip, and verify later commits contain only handoff metadata.
   If the immutable target cannot be established, record the exact blocker.

## Perform a full review

Inspect the complete diff from the recorded base through the implementation
target, not only the latest fix. Check:

- Every acceptance criterion and linked specification.
- Correctness, regressions, failure behavior, concurrency, cancellation,
  resource ownership, protocol compatibility, and public API contracts.
- Negative and boundary-case tests, not merely code presence.
- Repository-required formatting, linting, tests, and focused verification.
- Scope discipline and accidental unrelated changes.

Run the repository-required checks yourself on the implementation target rather
than trusting the handoff: `cargo fmt --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`, and
`cargo test --workspace`. Strict Clippy is a gate, not advisory: outstanding
warnings are a blocking finding, exactly as a failing test is. Cargo caches lint
results, so a fast clean run can reflect a prior build; when Clippy conformance
is load-bearing for the verdict, confirm it with a clean `CARGO_TARGET_DIR`.

Check any `#[allow]` the diff adds: each must be local and carry a comment
explaining why the warned construct is required. A broad or undocumented
allowance that merely silences the gate is a blocking finding. Distinguish
allowances the diff introduces from those already present at the base commit.

When the diff may affect move generation or search hot paths, run the speed
benchmarks and compare against `BENCHMARKS.md`. Because that baseline is locked
to specific hardware and toolchain, compare relatively rather than against its
absolute thresholds: confirm the machine is reasonably idle (no competing test
or build processes, sustained idle period), then run
`cargo bench --bench perft --bench movegen` on both the recorded base commit and
the implementation target on the same machine, and judge the delta between them.
Treat a repeatable regression beyond the `BENCHMARKS.md` thresholds as a blocking
finding; treat differences within Criterion's confidence interval as noise.

Distinguish patch-introduced defects from pre-existing failures while still
identifying baseline behavior that makes an acceptance criterion unprovable.

## Request changes

For blocking findings:

1. Assign IDs `REV-<attempt>-<two-digit sequence>` and severities.
2. Include location, impact, reproduction or reasoning, expected behavior, and
   verification evidence.
3. Append one structured review comment using the lifecycle template.
4. Uncheck affected acceptance criteria, clear any stale final summary, and move
   the task to `Changes Requested`.
5. Confirm only the task file changed, then commit the verdict on the same task
   branch. Leave the branch and worktree clean for `$implement` rework.

Do not create follow-up tasks or fix the findings.

## Approve

Approve only when objective evidence proves every acceptance criterion, all
blocking findings are resolved, and the implementation target is immutable:

1. Check each proven acceptance criterion through the CLI.
2. Write a concise final summary and approval comment naming the implementation
   SHA and verification commands.
3. Move the task to `Ready to Merge`.
4. Confirm only the task file changed, then commit the approval metadata on the
   task branch.

The approved implementation SHA remains the code target. The task-only approval
commit becomes the branch tip that a human may merge. Verify no implementation
file changed between those commits. Any later implementation change invalidates
approval and requires a fresh review.

Invocation of this skill authorizes local verdict commits on the task branch.
It does not authorize pushing, merging, or moving the task to `Done`. Use
`Needs Human` only when a decision or unavailable authority prevents safe
progress, not because review work is difficult.
