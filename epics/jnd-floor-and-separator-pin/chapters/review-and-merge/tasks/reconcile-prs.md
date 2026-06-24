---
id: reconcile-prs
chapter: review-and-merge
epic: jnd-floor-and-separator-pin
title: "Converge onto one PR; close/supersede the competing DRAFTs"
status: ready
priority: 1
depends_on: []
blocks:
  - green-ci-and-triage
agent_profile:
  category: deep
  skills: [dive-github, dive-git]
started: null
completed: null
refine_after: []
---

# Converge onto one PR; close/supersede the competing DRAFTs

## What
Three DRAFT PRs target this scope. Pick ONE delivery vehicle (a fresh minimal branch off main, OR #100 trimmed to scope) and explicitly close/supersede the rest with a recorded reason:
- **#100** `ship/separator-tracks-floor-jnd-15` — on-scope semantic.rs changes, ALL CI GREEN, but also edits solve.rs + neutral.rs (outside the disjoint-surface mandate). Action: harvest its semantic.rs work; DROP solve.rs/neutral.rs edits unless an executor proves they are load-bearing for the floor (if so, FLAG and escalate — that widens scope).
- **#99** `7.6->15.5 + shadow consts + provenance gate` — REJECT: 15.5 is unsourced (floor is 15.0); shadow-const lift violates surface-jnd §3 HARD BLOCKER. Close as superseded.
- **#98** OPEN `empirical-inventory R4 gate` — owns the inventory-marker machinery. Either depend on it (declare in PR body) or self-contain the marker (minimal diff). Do not absorb its whole gate.
Branch off `main` (never commit to main). Confirm the final branch diff is semantic.rs + epic docs only.

## Must NOT Do
- Do NOT leave a competing DRAFT open that could be merged by mistake.
- Do NOT carry #99's shadow-const lift or 15.5 value.
- Do NOT silently keep #100's solve.rs/neutral.rs edits — flag if they seem needed.

## Verification
- [ ] Exactly one branch/PR carries the scope; #99 (and #100 if not the vehicle) closed/superseded with a recorded note.
- [ ] Final branch `git diff --stat` against main shows only `crates/labcolors-core/src/semantic.rs` + `epics/...`.

## References
- PRs #98/#99/#100 (`gh pr view`), EPIC.md Notes (prior-art reconciliation).
