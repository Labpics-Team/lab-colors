---
id: c03-verify-and-merge
epic: empirical-inventory-gate
title: "Isolated 2-verifier audit, record known-limitation ADR, merge, persist to Graphiti"
user_story: "technical enabler"
enabler_for: "The epic DoD's verification + merge + memory clauses (constitution: 2 isolated verifiers, Graphiti write after merge)."
status: draft
priority: 3
depends_on:
  - c02-redproof-and-ci
refine_after:
  - t3-open-pr
parallel_group: ""
agent_profile:
  category: deep
  skills: [verification-receive, craft-arch]
---

# Verify, document the gap, merge, remember

## User Story
Technical enabler for: the DoD's isolated-verification + merge + memory clauses. No end user.

## Purpose
Close the loop per constitution. Two isolated verifiers on adjacent themes — one audits the gate logic + RED-proof (does it actually bite? any false-negative const it misses?), one audits inventory↔marker three-way sync + the standards-exclusion invariant (any standard leaked into a row? any policy const with no row?). `verification-runner` (RED-proof, mutates tree) runs SOLO/worktree, never parallel with read-only reviewers. Accept findings via `verification-receive`; fix ALL (incl. pre-existing) before merge. Record the detector's known-limitation (blind to inline fn-body perceptual literals: `mp_ref*1.5`, `.powf(0.6)`, inline `sqrt` chroma) as an explicit section in an ADR (surface-jnd or a dedicated decisions note). Merge the PR, then write the result to Graphiti `daniel-agent-local`. Tasks are placeholders until the PR exists and the actual diff is reviewable.

## Tasks
| Task | Status | Agent | Depends On |
|------|--------|-------|------------|
| `tasks/t1-isolated-verification.md` | placeholder | deep | (c02) |
| `tasks/t2-known-limitation-adr.md` | placeholder | deep | t1-isolated-verification |
| `tasks/t3-merge-and-graphiti.md` | placeholder | deep | t2-known-limitation-adr |

## Exit Criteria
- [ ] **Given** 2+ isolated verifiers on adjacent themes, **When** the PR is reviewed and accepted via `verification-receive`, **Then** every finding (incl. pre-existing) is resolved, real `cargo test` output is attached, and verification is recorded as actually-happened (not self-review).
- [ ] **Given** the detector's known blind spot, **When** the work merges, **Then** an ADR section explicitly names the inline-literal limitation (`mp_ref*1.5`, `hue_purity.powf(0.6)`, inline `chroma=sqrt`) as tracked-not-fixed, so it is not silently forgotten.
- [ ] **Given** the merged governance PR, **When** the epic closes, **Then** `git diff main..merge` shows zero perceptual-value changes, and a Graphiti episode (`daniel-agent-local`) records the gate, its blast radius (test-only, top of dep graph), the RED-proof evidence, and the tracked known-limitation.
