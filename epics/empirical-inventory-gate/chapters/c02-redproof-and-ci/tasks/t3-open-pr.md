---
id: t3-open-pr
chapter: c02-redproof-and-ci
epic: empirical-inventory-gate
title: "Commit the governance-only change set and open its own PR; CI green pre-merge"
status: placeholder
priority: 1
depends_on:
  - t2-ci-wiring-and-optional-legs
blocks: []
agent_profile:
  category: deep
  skills: [dive-git, dive-github]
started: null
completed: null
refine_after:
  - t2-ci-wiring-and-optional-legs
---

# Open the PR (placeholder)

Will be refined after t2. At refine-time: commit only the gate test, the SSOT inventory, the comment-only markers, and the known-limitation ADR note; open a single governance PR; push only after CI is green locally (constitution: CI green before push, PR-only, no `--no-verify`/`--force`). Final commit boundary depends on what t1/t2 produced (P1).
