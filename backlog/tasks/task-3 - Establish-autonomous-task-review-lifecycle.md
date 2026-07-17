---
id: TASK-3
title: Establish task implementation and review lifecycle
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 16:43'
updated_date: '2026-07-17 17:43'
labels: []
dependencies: []
references:
  - 'https://github.com/MrLesk/Backlog.md/issues/783'
documentation:
  - TASK_LIFECYCLE.md
modified_files:
  - AGENTS.md
  - backlog/config.yml
  - >-
    backlog/tasks/task-1.1 -
    Refactor-search-behind-a-reusable-typed-engine-API.md
  - .agents/skills/implement/SKILL.md
  - .agents/skills/implement/agents/openai.yaml
  - .agents/skills/review/SKILL.md
  - .agents/skills/review/agents/openai.yaml
  - TASK_LIFECYCLE.md
type: chore
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Define and install a repository-side workflow for manually invoked task implementation, independent review, rework, approval, and human-controlled merging. Record the existing TASK-1.1 review using that contract so it becomes the first concrete review handoff.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Backlog statuses distinguish implementation, review, requested changes, merge readiness, completion, and human intervention
- [ ] #2 TASK-1.1 contains structured review findings with stable IDs and is returned to a non-terminal rework state
- [ ] #3 Implementation guidance creates or reuses a dedicated task branch and worktree before task mutation, and review guidance reuses or reattaches that same branch
- [ ] #4 Code and Backlog lifecycle metadata are committed together on the task branch; the primary branch receives task history only through merge
- [ ] #5 Checked-in implement and review skills enforce branch, worktree, commit, review-target, and merge-authority boundaries
- [ ] #6 Cross-branch scanning remains enabled; the Backlog.md 1.48.0 ambiguity regression and temporary diagnostic workaround are documented and linked upstream
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Restore Backlog.md active-branch scanning and document the confirmed upstream ambiguity defect instead of adopting a single-master workaround. 2. Define one persistent branch and worktree per task, with Backlog metadata committed on that branch so cross-branch views can observe lifecycle state. 3. Update the implement and review skills to create, locate, reuse, and hand off task worktrees deterministically while preserving independent review. 4. Validate skill metadata, lifecycle text, Backlog configuration, and the local ambiguity reproduction against Backlog.md 1.48.0.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Configured To Do, In Progress, In Review, Changes Requested, Ready to Merge, Needs Human, and Done. Recorded TASK-1.1 review attempt 1 with REV-1-01 through REV-1-03, reopened its acceptance criteria, cleared its stale final summary, and moved it to Changes Requested.

Added root lifecycle rules, the repo-scoped $implement and $review skills, and the manual-session lifecycle guide. The finalized model uses one persistent task branch and sibling worktree across implementation, review, and rework. Task-scoped code and Backlog metadata are committed on that branch; reviewers reuse or reattach it; push and merge remain human-controlled; Done is recorded on the primary branch after merge.

Restored check_active_branches=true. Backlog.md 1.48.0 is the latest release. The local browser still returns HTTP 409 for TASK-1.1 while backlog doctor and /api/tasks/duplicates report no duplicate, matching upstream issue #783. Disabling branch checks is documented only as a temporary diagnostic workaround.

Validation: both skills pass the official quick_validate.py validator; generated openai.yaml metadata references $implement and $review; backlog config reports active-branch checks enabled; backlog doctor reports no duplicate IDs; /api/task/TASK-1.1 reproduces HTTP 409 and /api/tasks/duplicates returns no findings; git diff --check passes.

Handoff constraint: this shared checkout contains unrelated Rust and Backlog changes from earlier sessions. TASK-3 remains In Progress until its lifecycle artifacts can be isolated and committed for independent review without disturbing those changes.
<!-- SECTION:NOTES:END -->
