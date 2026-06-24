---
id: green-ci-and-triage
chapter: review-and-merge
epic: jnd-floor-and-separator-pin
title: "Green CI on pinned toolchain; triage RED external signals per N6"
status: ready
priority: 1
depends_on:
  - reconcile-prs
blocks:
  - heterogeneous-review
agent_profile:
  category: deep
  skills: [dive-github, verification-receive]
started: null
completed: null
refine_after: []
---

# Green CI on pinned toolchain; triage RED external signals per N6

## What
Push the converged branch, open/mark the PR ready, and drive `gh pr checks <pr>` to all-green on the pinned CI (Rust 1.96.0, actions by SHA): the four jobs — `cargo audit (rustsec)`, `clippy + rustfmt`, `test`, `wasm build + headless test + size`. For any RED signal, triage per N6 (the receiving-feedback discipline) BEFORE any done-claim: classify (real defect vs flake vs out-of-scope pre-existing), fix real defects, quarantine/flag flakes, and never green-wash. A "CodeRabbit pass = Review skipped" or rate-limit pass is NOT a real review signal (note from prior PRs) — do not treat it as the heterogeneous review.

## Must NOT Do
- Do NOT bypass CI (`--no-verify`, `--force`) or claim done on a non-isolated/faked green.
- Do NOT mask a pre-existing failure — fix or explicitly flag it.
- Do NOT skip lint: match the CI's exact golangci/clippy+rustfmt invocation locally before pushing.

## Verification
- [ ] `gh pr checks <pr>` shows all four jobs PASS on the pinned-toolchain runs.
- [ ] Every RED-then-green transition is triaged + recorded (cause, fix/flag).
- [ ] Local `cargo test -p labcolors-core` + clippy + rustfmt pass before push.

## References
- `.github/workflows/ci.yml` — the 4-job pinned CI.
- N6 / verification-receive — RED-signal triage discipline.
