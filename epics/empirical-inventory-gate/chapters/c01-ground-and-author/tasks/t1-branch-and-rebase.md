---
id: t1-branch-and-rebase
chapter: c01-ground-and-author
epic: empirical-inventory-gate
title: "Create the governance branch off feat/v2-governance, rebased onto current main"
status: ready
priority: 1
depends_on: []
blocks:
  - t2-enumerate-and-classify
agent_profile:
  category: deep
  skills: [dive-git]
started: null
completed: null
refine_after: []
---

# Create the governance branch

## What
The prior run's artifacts are gone (see EPIC reality-reconciliation). Prepare a clean branch to re-author on. Per scope: branch off `feat/v2-governance`. But `feat/v2-governance` (HEAD `f6a017a`) is behind `main` (`f21aac7`) and the gate must scan the LIVE tree, so rebase the new branch onto current `main` (or branch directly off `main` and cherry-pick the two v2-governance ADR docs if rebase conflicts on the doc index — decide by inspecting the 2-commit delta: `f6a017a` README index + `f571f4f` surface-jnd skeleton).

- Create branch e.g. `feat/empirical-inventory-gate` from `feat/v2-governance`, then `git rebase main` (or `git merge main`); resolve so the gate will see current `semantic.rs` etc.
- Confirm `docs/decisions/surface-jnd.md` + `docs/decisions/README.md` are present (they carry the known-limitation home, t4 of C02).
- Do NOT commit yet — this task only establishes the branch and a clean baseline.

## Must NOT Do
- Do NOT branch off or commit to `main` directly without preserving v2-governance's two ADR docs.
- Do NOT touch any `src/*.rs` in this task.
- Do NOT use `--force` on any shared branch; do NOT `--no-verify`.

## Verification
- [ ] `git branch --show-current` is the new gate branch, not `main`.
- [ ] `git log --oneline -3` shows current `main` HEAD (`f21aac7` or newer) as an ancestor AND the v2-governance ADR commits present.
- [ ] `crates/labcolors-core/src/semantic.rs` line 152 reads `const DECORATIVE_FLOOR_MIN: f64 = 7.6;` (live tree confirmed).
- [ ] `git status --porcelain` is clean (no stray edits).

## References
- `crates/labcolors-core/src/semantic.rs:152` — the canary literal proving the live tree is scanned.
- `docs/decisions/README.md`, `docs/decisions/surface-jnd.md` — only on v2-governance; must survive the rebase.
