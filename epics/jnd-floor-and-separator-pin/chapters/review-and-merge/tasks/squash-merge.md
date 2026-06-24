---
id: squash-merge
chapter: review-and-merge
epic: jnd-floor-and-separator-pin
title: "Clean squash merge, delete branch, record outcome"
status: ready
priority: 1
depends_on:
  - heterogeneous-review
blocks: []
agent_profile:
  category: deep
  skills: [dive-github, dive-git]
started: null
completed: null
refine_after: []
---

# Clean squash merge, delete branch, record outcome

## What
With CI all-green on the pinned toolchain and the heterogeneous review clean on the final state, perform a clean `gh pr merge <pr> --squash --delete-branch`. No manual-host edit, no force, no CI bypass. After merge, record the outcome in Graphiti (`daniel-agent-local`): the ratified floor (15.0), separator tracking, the quantization-robust invariant + its blast radius (golden separator rows regenerated), and the closed/superseded competing PRs. Mark the epic `completed` only when BACKLOG.md has no `[OPEN]` items.

## Must NOT Do
- Do NOT merge to main outside the PR (PR-only).
- Do NOT leave the source branch undeleted.
- Do NOT declare the epic done with an open backlog item or an unmerged competing PR still targeting the scope.

## Verification
- [ ] PR merged via `gh pr merge --squash`; branch deleted; main contains the ratified floor.
- [ ] Graphiti episode recorded (outcome + blast radius + date MSK).
- [ ] BACKLOG.md drained; EPIC.md status -> completed.

## References
- CLAUDE.md — PR-only, no main commit, no CI bypass; Graphiti as Definition of Done.
