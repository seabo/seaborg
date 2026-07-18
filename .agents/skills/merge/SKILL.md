---
name: merge
description: Land an approved Seaborg Backlog.md task onto the primary branch through a merge-time integration gate, then record Done. Use when a task is Ready to Merge and a human invokes Codex to integrate it. The skill merges the approved immutable target onto the live primary tip, re-runs required checks and hot-path benchmarks on the integrated result, and fast-forwards primary only if that result is clean and green. Do not use to implement, review, or approve work, and do not rebase or rewrite the approved target; use the implement and review skills for those.
---

# Merge

Follow `../../../TASK_LIFECYCLE.md`. This skill owns the `Ready to Merge` ->
`Done` transition: it is the one skill that authorizes advancing the primary
branch and recording `Done`. `$implement` and `$review` never merge; this skill
never edits implementation code or re-does review.

Human invocation serializes merges. That serialization is a throughput
assumption, not a correctness one: the compare-and-swap below keeps the primary
branch correct even if two invocations overlap.

## Preconditions

1. Run `backlog instructions overview` and read the task and its approval
   comment. Read the recorded task branch, base, and approved implementation
   target (the reviewed SHA).
2. Require the task to be `Ready to Merge`. Refuse anything else.
3. Require every dependency to be `Done`. Never land a task ahead of an
   unlanded dependency.
4. Confirm the approval is intact: the approved implementation SHA is the code
   target, later commits contain only approval metadata, and no implementation
   file changed after approval. If approval cannot be established, stop and
   record the blocker; do not merge.

Do not modify implementation files. Do not rebase or cherry-pick the approved
target: rebasing rewrites its SHAs and voids the approval pinned to them.

## Integrate with compare-and-swap

Perform the land from the primary worktree, against the live primary tip:

1. Read the current primary tip `T`.
2. Create a non-fast-forward merge commit `M` of the approved target into `T`.
   The approved SHA stays intact as a parent; `M` is the integrated artifact
   that gets verified. On a textual conflict, abort the merge and eject (below).
3. Run the repository-required checks on `M`. When the diff may affect move
   generation or search hot paths, also run `cargo bench --bench perft --bench
   movegen` on `M` and compare relatively against the recorded base per
   `BENCHMARKS.md`. A repeatable regression beyond its thresholds is a failing
   result; differences within Criterion's confidence interval are noise.
4. Re-read the primary tip. **If it still equals `T`**, advance primary to `M`.
   **If it moved**, discard `M` and restart from step 1 against the new tip; the
   verification against a stale tip is void.

Only a clean merge whose integrated checks and benchmarks pass may advance
primary. Textual cleanliness plus passing tests is the bar; a semantically
wrong merge that still passes tests can land, so treat test-suite depth as the
primary automated net and surface any overlap for human judgment (below).

## Land

When the integrated result is clean and green and the compare-and-swap
succeeded:

1. Move the task to `Done` and commit that lifecycle change (recorded on the
   primary branch, because it describes the merge result).
2. Remove the task worktree and delete the merged task branch.
3. The human controls pushing. This skill authorizes the local merge, `Done`,
   and cleanup, not pushing to a remote.

Before landing, note whether the merge touched files or modules that a
recently-landed task changed. Surface that overlap for the human to eyeball;
automated overlap re-review is a future enhancement, not part of this skill.

## Eject

For a textual conflict, or failing integrated checks or benchmarks:

1. Do not advance primary. Discard the trial merge.
2. Append a concise merge-failure comment on the task branch with the primary
   tip tested, the failing command, and its evidence.
3. Move the task to `Changes Requested` and leave the branch and worktree clean
   for `$implement` rework. Never land a task with a failing integrated result.

## Human intervention

Use `Needs Human` only when a decision or unavailable authority blocks safe
progress — a scope call on a conflict, missing credentials, or an unidentifiable
target. A clear integration failure belongs in `Changes Requested`, not
`Needs Human`.
