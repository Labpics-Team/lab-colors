---
id: t2-known-limitation-adr
chapter: c03-verify-and-merge
epic: empirical-inventory-gate
title: "Record the detector's inline-literal blind spot as a tracked known-limitation ADR section"
status: placeholder
priority: 1
depends_on:
  - t1-isolated-verification
blocks:
  - t3-merge-and-graphiti
agent_profile:
  category: deep
  skills: [write-docs]
started: null
completed: null
refine_after:
  - t1-isolated-verification
---

# Known-limitation ADR (placeholder)

Will be refined after t1. Document the blind spot (inline fn-body perceptual literals `mp_ref*1.5`, `hue_purity.powf(0.6)`, inline `chroma=sqrt`; no `oklab::chroma()` primitive) as tracked-not-fixed. Exact ADR file + wording depend on what t1's verifiers surfaced (P1).
