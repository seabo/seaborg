# Backlog task implementation and review lifecycle

This workflow is for manually started implementation and review sessions.
Backlog.md is the durable record. Invoke `$implement` and `$review` in Codex;
harnesses may map them to `/implement` and `/review`.

## States

| Status | Meaning |
| --- | --- |
| `To Do` | Available for implementation. |
| `In Progress` | Implementation or review rework is underway. |
| `In Review` | A committed implementation target awaits independent review. |
| `Changes Requested` | Blocking findings must be resolved on the same task. |
| `Ready to Merge` | A reviewer approved a specific immutable implementation. |
| `Needs Human` | A decision or intervention is required. |
| `Done` | The approved task branch was merged successfully. |

`Done` means merged, not merely implemented or approved. Implementation agents
do not approve their own work, and review agents do not fix implementation code.

## Branch and worktree ownership

Use one persistent branch and one dedicated worktree per task across initial
implementation, every rework attempt, and review. Prefer a branch such as
`task-1.1-typed-engine-api` and a sibling worktree such as
`../seaborg-worktrees/task-1.1-typed-engine-api`.

- Start new `To Do` work from the repository's primary branch, but create and
  enter the task worktree before changing Backlog state or implementation files.
- For `Changes Requested`, find and reuse the original task branch. Reattach it
  to a worktree if necessary; do not create a branch per review attempt.
- Reviewers use the implementation worktree. If it no longer exists locally,
  they attach the existing task branch to the standard sibling location. They
  do not create a review branch or use the primary worktree.
- Keep code and all task-specific Backlog mutations—claim, plan, notes,
  handoff, findings, resolutions, and approval—on the task branch. Run Backlog
  writes through its CLI from that worktree.
- Commit lifecycle boundaries. Only committed branch state is portable and
  reliably visible to a Backlog browser rooted in another worktree. Do not make
  matching status edits on the primary branch.
- The primary branch receives the task file and its history through the final
  merge. The human controls push and merge operations.

Invoking `$implement` or `$review` authorizes local commits scoped to that task
and worktree. It does not authorize pushing, merging, rewriting unrelated
history, or disturbing changes in another worktree.

Before creating or reattaching a worktree, inspect `git worktree list
--porcelain`, the candidate branch, and the base commit. If required base or
task changes exist only as uncommitted data, stop rather than silently omitting
them. Unrelated dirt in another worktree must be left untouched.

## Backlog across branches

Keep `check_active_branches: true`. Backlog.md is intended to reconcile task
state across active feature branches and provide the project-wide board; a
single writable Backlog branch is not part of this lifecycle.

Backlog.md 1.48.0 has a known ambiguity regression where inherited copies of a
task can return `Task ID ... is ambiguous` even though `backlog doctor` finds no
duplicate. It is tracked upstream as
[MrLesk/Backlog.md#783](https://github.com/MrLesk/Backlog.md/issues/783).
Disabling active-branch checks can confirm the diagnosis, but is a temporary
workaround, not the repository configuration or a reason to move lifecycle
metadata onto the primary branch.

## Implement

Start `$implement` with a task ID or ask it to select work. The agent:

1. Reads the Backlog instructions and task without mutating it.
2. Selects only `Changes Requested` or `To Do`, preferring rework first.
3. Creates or reattaches the task worktree and performs all remaining work there.
4. Moves the task to `In Progress`, records a plan, and commits that claim so it
   is visible across branches.
5. Implements every acceptance criterion or resolves every open `REV-N-NN`
   finding on the same branch, recording resolution evidence.
6. Runs required checks and commits the implementation.
7. Records the immutable implementation SHA in a task-only handoff commit and
   moves the task to `In Review`.

```text
Implementation handoff
Branch: task-1.1-typed-engine-api
Worktree: /absolute/local/path/to/worktree
Base: <base sha>
Implementation target: <sha>
Resolved findings: <IDs or none>
Verification:
- <command>: <result>
Known failures: <none, or exact baseline evidence>
```

The local worktree path helps the next session on the same machine; the branch
name and SHAs are the durable handoff. The branch must be clean when handed to
review.

## Review

Start a separate `$review` session with the task ID. The reviewer locates the
recorded task branch, enters its worktree, and confirms:

- The worktree is clean.
- The implementation target exists and descends from the recorded base.
- Commits after the implementation target contain only handoff metadata.
- The full base-to-target diff matches the task and contains no accidental work.

The reviewer checks every acceptance criterion, linked documentation,
repository requirement, boundary case, and relevant failure mode without
modifying implementation files.

### Changes requested

Blocking findings remain on the original task and receive stable IDs:

```text
Review attempt: 2
Reviewed branch: task-1.1-typed-engine-api
Reviewed implementation: <sha>
Verdict: changes_requested

REV-2-01 [P1] Short finding title
Location: path/to/file.rs:123
Impact: Why acceptance or safe merging is blocked.
Reproduction: Exact command or minimal scenario.
Expected: Required behavior.

Verification:
- <command>: <result>
```

The reviewer unchecks affected acceptance criteria, clears a stale final
summary, moves the task to `Changes Requested`, and commits only that task-file
verdict on the task branch. The next `$implement` session reuses the worktree,
records `Resolved REV-2-01`, and produces a new immutable target.

Create a follow-up only for genuinely non-blocking, out-of-scope work and only
with human approval. Never defer a blocking finding merely to merge.

### Approval

When objective evidence proves every acceptance criterion, the reviewer:

1. Checks the proven acceptance criteria.
2. Writes the final summary and an approval comment naming the implementation
   SHA and verification commands.
3. Moves the task to `Ready to Merge`.
4. Commits only the approval metadata on the task branch.

The implementation SHA is the reviewed code target. Its task-only approval
commit is the branch tip presented for human merge. The reviewer verifies no
implementation file changed between the two. Any later implementation change
invalidates approval and requires a fresh review.

After the human merges the approved branch, `Done` is recorded on the primary
branch because it describes the result of the merge rather than branch-local
work. This is the only normal lifecycle mutation made directly after merge.

## Human intervention

Use `Needs Human` when safe progress requires a product or scope decision,
credentials or authority are unavailable, the review target cannot be
identified, or repeated rework has reached an impasse. Record the exact
decision needed. A clear in-scope defect belongs in `Changes Requested`, not
`Needs Human`.
