---
id: heterogeneous-review
chapter: review-and-merge
epic: jnd-floor-and-separator-pin
title: "Heterogeneous external review of FINAL state; close pending audit #10"
status: ready
priority: 1
depends_on:
  - green-ci-and-triage
blocks:
  - squash-merge
agent_profile:
  category: deep
  skills: [verification-santa, verification-review, verification-receive]
started: null
completed: null
refine_after: []
---

# Heterogeneous external review of FINAL state; close pending audit #10

## What
Run a heterogeneous external reviewer (CodeRabbit CLI, or a SAST substitute if CodeRabbit credits are exhausted/rate-limited — record which) on the FINAL state of the converged PR. A "Review skipped"/rate-limit pass does NOT count — confirm a real review ran (`gh api .../reviews` + CLI output). Triage findings via verification-receive: no unaddressed Critical/High before merge. Any code edit made in response RE-OPENS the gate (re-review the new final state). Fold in the pre-existing pending task **#10 "CoVe audit: PR #99 separator floor verification"**: re-aim its audit at THIS epic's exit-criteria (floor==15.0, separator tracks, quantization-robust invariant RED-proven, disjoint surface) — not at #99's out-of-scope approach — and close it when satisfied.

## Must NOT Do
- Do NOT accept a rate-limit / "Review skipped" as the heterogeneous review.
- Do NOT self-review as the heterogeneous reviewer (it must be a different tool/agent on the final state).
- Do NOT merge with an open Critical/High or an unincorporated reviewer fix.

## Verification
- [ ] A real heterogeneous review ran on the FINAL state (tool recorded; review body present, not skipped).
- [ ] Zero unaddressed Critical/High; any fix re-triggered a re-review.
- [ ] Pending task #10 closed against this epic's exit-criteria, with evidence.

## References
- prior-PR note: zero-credit CodeRabbit can pass-through as "skipped" — verify via `gh api` reviews.
- Task #10 (CoVe audit, currently pending).
