---
id: review-and-merge
epic: jnd-floor-and-separator-pin
title: "Reconcile the competing PRs, green CI on pinned toolchain, heterogeneous review, clean squash merge"
user_story: "technical enabler"
enabler_for: "Ships the ratified Lc15 floor + quantization-robust invariant to main as one clean, reviewed, CI-green PR — the delivery gate for both prior chapters."
status: ready
priority: 2
depends_on:
  - floor-and-separator-pin
  - quantization-robust-invariant
refine_after: []
parallel_group: ""
agent_profile:
  category: deep
  skills: [dive-github, verification-santa, verification-receive]
---

# Reconcile PRs, green CI, heterogeneous review, clean squash merge

## User Story
Technical enabler. Delivers both prior chapters to main as ONE minimal, reviewed, CI-green PR, reconciling the three competing DRAFT PRs (#99/#100/#98) instead of adding a fourth conflicting attempt, and folding in pending audit task #10.

## Purpose
Converge the work onto a single PR whose diff is `crates/labcolors-core/src/semantic.rs` (+ epic docs) only. Decide the PR strategy against the prior art: harvest #100's on-scope semantic.rs changes (reject its solve.rs/neutral.rs reach unless proven load-bearing), reject #99's out-of-scope shadow-const lift and its unsourced 15.5, and either depend-on or self-contain #98's inventory-marker. Run the pinned-toolchain CI (Rust 1.96.0, actions-by-SHA), triage any RED external signal per N6 BEFORE any done-claim, get a heterogeneous external review (CodeRabbit / SAST substitute) on the FINAL state, and squash-merge with no manual-host edit.

## Tasks
| Task | Status | Agent | Depends On |
|------|--------|-------|------------|
| `tasks/reconcile-prs.md` | ready | deep | — |
| `tasks/green-ci-and-triage.md` | ready | deep | reconcile-prs |
| `tasks/heterogeneous-review.md` | ready | deep | green-ci-and-triage |
| `tasks/squash-merge.md` | ready | deep | heterogeneous-review |

## Exit Criteria
Given/When/Then.
- [ ] Given the three prior DRAFT PRs, When this work converges, Then exactly ONE PR carries the scope; the others are closed or explicitly superseded with a recorded reason (no orphaned competing PR left to merge by accident).
- [ ] Given the PR diff, When file paths are listed, Then only `crates/labcolors-core/src/semantic.rs` (+ `epics/...` docs) changed — disjoint from sentiment/`S_PERC_MIN`/`RoleChroma`, solve.rs, neutral.rs.
- [ ] Given `gh pr checks <pr>`, When run on the pinned-toolchain CI, Then ALL checks are green (audit/clippy+rustfmt/test/wasm); any RED external signal is triaged per N6 and resolved BEFORE any done-claim.
- [ ] Given a heterogeneous external reviewer (CodeRabbit / SAST substitute, recorded), When it reviews the FINAL state, Then there are no unaddressed Critical/High findings; pending task #10's CoVe audit concern is closed against THIS epic's exit-criteria.
- [ ] Given the merge, When performed, Then it is a clean `gh pr merge --squash` with no manual-host edit; the merge branch is deleted.
