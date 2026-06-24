---
id: red-proof-the-invariant
chapter: quantization-robust-invariant
epic: jnd-floor-and-separator-pin
title: "RED-prove the invariant bites, then restore GREEN"
status: ready
priority: 1
depends_on:
  - cover-quantization-cliff
blocks: []
agent_profile:
  category: deep
  skills: [craft-qa, verification-runner]
started: null
completed: null
refine_after: []
---

# RED-prove the invariant bites, then restore GREEN

## What
A green-from-birth test is a bug (CLAUDE.md: red must fall first). Prove the property test BITES against the real predicate, with isolated evidence:
1. Temporarily set the floor / separator to a value that quantizes below 15 on some swept background (e.g. revert `DECORATIVE_FLOOR_MIN` to 7.6, OR set a continuous target that the engine rounds to a 14.955-class emission on `#080808`). Run the test -> it MUST FAIL, and the failure message must name the offending background + its sub-floor |Lc|.
2. Capture the RED output as evidence.
3. Restore the ratified floor (15.0 + separator tracking). Run -> GREEN.
4. Hand the RED-proof to `verification-runner` (isolated) so the bite is verified independently, not self-attested.

## Must NOT Do
- Do NOT claim the invariant works without a captured RED run (no test theater).
- Do NOT leave the temporary sub-floor value committed.
- Do NOT self-verify the RED-proof; use an isolated verifier (CLAUDE.md verification rule).

## Verification
- [ ] Captured RED output exists showing the test failing on a named background with measured sub-floor |Lc|.
- [ ] After restoring the floor, `cargo test -p labcolors-core` is GREEN.
- [ ] `verification-runner` independently confirms the RED-proof (recorded).

## References
- `docs/decisions/surface-jnd.md` §7 — invariants to lock.
- CLAUDE.md — TDD RED-proof requirement; isolated verification.
