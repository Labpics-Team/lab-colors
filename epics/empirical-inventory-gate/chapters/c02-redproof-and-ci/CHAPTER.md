---
id: c02-redproof-and-ci
epic: empirical-inventory-gate
title: "Prove the guard bites (hermetic RED-proof) and wire it into the workspace test gate"
user_story: "technical enabler"
enabler_for: "The epic's DoD clause 'RED-proof доказан реальным прогоном' and 'gate присутствует в дереве / CI зелёный'."
status: draft
priority: 2
depends_on:
  - c01-ground-and-author
refine_after:
  - t4-author-gate-test
parallel_group: ""
agent_profile:
  category: deep
  skills: [dive-rust-core, craft-qa]
---

# RED-proof & CI wiring

## User Story
Technical enabler for: the DoD's "guard must bite" and "gate runs in CI" clauses. No end user.

## Purpose
A gate that is green-from-birth is theater. This chapter adds the hermetic, deterministic RED-proof — `red_proof_audit_probe` splices an untracked `_AUDIT_PROBE` literal into an **in-memory copy** of the scan input and asserts GATE-1 goes RED and names `_AUDIT_PROBE` — proving the guard bites without ever mutating the real tree or values. It also confirms the gate participates in `cargo test --workspace` (the CI `test` job already runs `cargo test --workspace`, so a `tests/` integration test is picked up automatically — verify, don't duplicate). Optional legs (`no_value_drift_diff_check`, `ci_full_gate`) are added only if they earn their keep under the pinned toolchain. Tasks are placeholders until C01 lands and the real gate API is known.

## Tasks
| Task | Status | Agent | Depends On |
|------|--------|-------|------------|
| `tasks/t1-redproof.md` | placeholder | deep | (c01) |
| `tasks/t2-ci-wiring-and-optional-legs.md` | placeholder | deep | t1-redproof |
| `tasks/t3-open-pr.md` | placeholder | deep | t2-ci-wiring-and-optional-legs |

## Exit Criteria
- [ ] **Given** `red_proof_audit_probe`, **When** it splices `_AUDIT_PROBE` into an in-memory copy of the scan, **Then** GATE-1 deterministically RED and the failure message names `_AUDIT_PROBE` — and the real tree/values are untouched (re-running the normal gate is still GREEN).
- [ ] **Given** `cargo test --workspace`, **When** CI runs, **Then** the empirical-inventory gate executes as part of it (no extra job needed) and the whole workspace test job is green on the branch.
- [ ] **Given** the gate branch, **When** the PR is opened, **Then** it is its OWN PR (governance-only), CI is green pre-merge, and `git diff` of the PR shows zero perceptual-value changes (only the gate test, the inventory doc, comment markers, and the known-limitation ADR note).
