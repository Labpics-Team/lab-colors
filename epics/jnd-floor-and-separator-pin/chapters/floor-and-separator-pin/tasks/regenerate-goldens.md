---
id: regenerate-goldens
chapter: floor-and-separator-pin
epic: jnd-floor-and-separator-pin
title: "Regenerate the separator golden snapshot rows for the raised floor"
status: ready
priority: 2
depends_on:
  - separator-tracks-floor
blocks: []
agent_profile:
  category: deep
  skills: [dive-rust, craft-qa]
started: null
completed: null
refine_after: []
---

# Regenerate the separator golden snapshot rows

## What
Raising the separator changes its emitted hex on every background in the golden table (`semantic.rs:3184-3428`, rows like `("srgb","#FFFFFF","separator","#E5E5EE")`). Update the EXPECTED `separator` rows to the new emitted hex — by re-running the engine and reading the actual output, NOT by hand-editing to make the test pass. Confirm each new separator hex's measured |Lc| against its background is `>= 15.0` (that the golden encodes a floor-clearing colour, not just a changed one).

## Must NOT Do
- Do NOT edit any NON-separator golden row (label-*, icon, shadow rows are out of scope and unchanged).
- Do NOT hand-fabricate hex values — regenerate from the engine, then verify |Lc| >= 15.
- Do NOT loosen any assertion to absorb a drift in another role (that would mask a regression).

## Verification
- [ ] `cargo test -p labcolors-core` is GREEN (golden table matches engine output).
- [ ] Every regenerated `separator` golden row measures |Lc| `>= 15.0` against its background.
- [ ] `git diff` of the golden block touches ONLY `separator` rows.

## References
- `crates/labcolors-core/src/semantic.rs:3184-3428` — golden snapshot table.
