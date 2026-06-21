---
id: t1-redproof
chapter: c02-redproof-and-ci
epic: empirical-inventory-gate
title: "Add hermetic red_proof_audit_probe — splice _AUDIT_PROBE into in-memory scan, assert GATE-1 RED"
status: placeholder
priority: 1
depends_on: []
blocks:
  - t2-ci-wiring-and-optional-legs
agent_profile:
  category: deep
  skills: [dive-rust-core, craft-qa]
started: null
completed: null
refine_after:
  - t4-author-gate-test
---

# RED-proof (placeholder)

Will be refined after `c01/t4-author-gate-test` completes and the real gate scanner API (how it ingests source text, how GATE-1 reports a failure) is known. The probe must reuse that exact scanner on an in-memory copy so the RED-proof is a true mirror of production behavior — its shape depends on t4's implementation. Do NOT pre-specify the API here (P1: no guessing future state).

Refinement must produce: in-memory splice of `_AUDIT_PROBE`, deterministic RED assertion that names the literal, and proof the real tree/values stay untouched.
