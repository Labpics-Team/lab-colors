---
id: cover-quantization-cliff
chapter: quantization-robust-invariant
epic: jnd-floor-and-separator-pin
title: "Prove the #080808 -> 14.955 quantization cliff is covered and distinguished"
status: ready
priority: 1
depends_on:
  - define-sweep-and-predicate
blocks:
  - red-proof-the-invariant
agent_profile:
  category: deep
  skills: [craft-qa, dive-rust]
started: null
completed: null
refine_after: []
---

# Prove the #080808 quantization cliff is covered

## What
The scope names a specific witness: `#080808 -> continuous Lc 14.955 quantizes below a continuous Lc-15 target`. Confirm the sweep actually exercises this class and that the predicate would CATCH a 14.955-style sub-floor emission. Concretely:
- Verify (by running) what the separator resolves to on `#080808` under the new floor: it must be either an emitted hex measuring `>= 15.0`, or `Unreachable`. If the engine emits a 14.955-class colour, that is the bug the floor raise + this invariant must surface — root-cause it (does the solver round the continuous-15 target DOWN at this background? does it need the `NEIGHBOR_STEPS` bridge?). FLAG, do not silently widen the assertion.
- Ensure the property holds for the WHOLE near-black class, not just the single `#080808` input (sweep `#000000..#101010` per channel at minimum). This is what makes it Class C (property over the class), not Class B (single-input regression).

## Must NOT Do
- Do NOT special-case `#080808` with a bespoke `if bg == "#080808"` assertion — that is `test(X)`, not the class. The sweep must cover it as one member.
- Do NOT relax `>= 15.0` to absorb a 14.955 emission. If the engine genuinely cannot clear 15 at some background, that background must resolve `Unreachable`; if it emits a sub-floor Color instead, that is a real defect to root-cause (within scope: it may mean the separator literal or floor needs the on-grid value that clears 15, per surface-jnd §2 checkpoints).

## Verification
- [ ] Running the sweep, `#080808` and its near-black neighbours each resolve to either |Lc| >= 15 Color or Unreachable — recorded with actual emitted hex + measured |Lc|.
- [ ] The cliff is covered as a swept member, not a special-cased input.
- [ ] If any background emits a sub-floor Color, it is root-caused and FLAGGED (not assertion-relaxed).

## References
- `docs/decisions/surface-jnd.md` §1c, §2 — cliff reconciliation + on-grid checkpoints.
- `crates/labcolors-core/src/solve.rs` — `NEIGHBOR_STEPS` (#44 QuantizationGap bridge), READ-ONLY.
