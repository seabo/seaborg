# Backlog task implementation and review lifecycle

This workflow is for manually started implementation, review, and merge
sessions. Backlog.md is the durable record. Invoke `$implement`, `$review`, and
`$merge` in Codex; harnesses may map them to `/implement`, `/review`, and
`/merge`.

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
  merge. A human triggers each merge by invoking `$merge`, and controls
  pushing.

Invoking `$implement` or `$review` authorizes local commits scoped to that task
and worktree. It does not authorize pushing, merging, rewriting unrelated
history, or disturbing changes in another worktree. Invoking `$merge`
additionally authorizes advancing the primary branch and recording `Done` for
one approved task; it does not authorize pushing.

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

Review validates the change in isolation against the immutable `base`-to-target
diff. It deliberately does not test a prospective merge with the current
primary tip: that would break target immutability and be stale the moment
primary moves. The guarantee that primary stays green after integration lives
in `$merge`, which re-verifies the merged result at merge time.

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
commit is the branch tip presented for merge. The reviewer verifies no
implementation file changed between the two. Any later implementation change
invalidates approval and requires a fresh review.

## Merge

A human lands an approved task by invoking `$merge` (see
`.agents/skills/merge/SKILL.md`). Human invocation serializes merges; that is a
throughput assumption, not a correctness one. The skill:

1. Requires the task to be `Ready to Merge`, every dependency to be `Done`, and
   the approval intact (no implementation file changed after approval).
2. Merges—never rebases—the immutable approved target into the live primary
   tip, so the approved SHA stays intact as a parent and the merge commit is the
   integrated artifact that gets verified.
3. Runs the repository-required checks, and hot-path (perft/movegen)
   benchmarks when relevant, on that integrated result.
4. Advances primary only via a compare-and-swap: it re-reads the primary tip and
   fast-forwards to the verified merge only if the tip is unchanged since it was
   read, otherwise it discards the trial and retries against the new tip. This
   keeps primary correct even if two invocations overlap.

A clean, green integration advances primary and records `Done` on the primary
branch, because `Done` describes the result of the merge rather than
branch-local work. A textual conflict or a failing integrated check ejects the
task to `Changes Requested` with evidence and never to `Done`.

Landed code is the reviewed change forward-integrated and re-tested, not the
exact reviewed bytes; test-suite depth is the primary automated net against a
merge that is textually clean but semantically wrong. Automating this gate
(a queue integrator, speculative or batched execution, and automatic overlap
re-review) is a future enhancement tracked separately, warranted only once
manual invocation is a measured throughput bottleneck.

## Human intervention

Use `Needs Human` when safe progress requires a product or scope decision,
credentials or authority are unavailable, the review target cannot be
identified, or repeated rework has reached an impasse. Record the exact
decision needed. A clear in-scope defect belongs in `Changes Requested`, not
`Needs Human`.
