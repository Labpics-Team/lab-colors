---
id: t2-ci-wiring-and-optional-legs
chapter: c02-redproof-and-ci
epic: empirical-inventory-gate
title: "Confirm gate runs under cargo test --workspace; add optional no_value_drift / ci_full_gate legs if they earn it"
status: placeholder
priority: 1
depends_on:
  - t1-redproof
blocks:
  - t3-open-pr
agent_profile:
  category: deep
  skills: [dive-rust-core, craft-qa, dive-github]
started: null
completed: null
refine_after:
  - t1-redproof
---

# CI wiring + optional legs (placeholder)

Will be refined after t1. Scope to confirm at refine-time: the CI `test` job runs `cargo test --workspace` (verified in ci.yml this session), so the integration test is auto-included — verify, do NOT add a redundant job. Decide whether `no_value_drift_diff_check` (shells `git diff HEAD` for value drift) and `ci_full_gate` legs are worth their flakiness cost under pinned toolchain 1.96.0 / clippy -D warnings. Concrete steps depend on t1's final test module layout (P1).
