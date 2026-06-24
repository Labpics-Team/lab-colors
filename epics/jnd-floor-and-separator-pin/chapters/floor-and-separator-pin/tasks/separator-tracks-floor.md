---
id: separator-tracks-floor
chapter: floor-and-separator-pin
epic: jnd-floor-and-separator-pin
title: "Raise the separator literal at semantic.rs:1008 to track the Lc15 floor"
status: ready
priority: 1
depends_on:
  - raise-floor-const
blocks:
  - regenerate-goldens
agent_profile:
  category: deep
  skills: [dive-rust, dive-rust-core]
started: null
completed: null
refine_after: []
---

# Raise the separator literal to track the floor

## What
The separator is declared `(Role::Separator, decorative(8.0))` at `crates/labcolors-core/src/semantic.rs:1008` (NOT :1078 — the brief's line number is stale; 1078 is an unrelated match arm in `Resolved::solved()`). Raise the literal so it tracks the floor: set it to the Lc15 floor (sourced band [Lc15, Lc18], surface-jnd §2; pick the floor `15.0` unless the owner's eye-calibration set point inside the band is already recorded — if so, cite it). The point: the SOURCE must read `>= 15.0`, so the separator's contract is stated honestly and is not a sub-floor `8.0` silently clamped up by `decorative_contract`'s `.max()`.

## Must NOT Do
- Do NOT leave `decorative(8.0)` relying on the `.max()` clamp — that hides a sub-floor literal (the exact conflation surface-jnd §4 removes).
- Do NOT exceed Lc 30 (shape minimum; >Lc30 on a 1px hairline = over-separation = dirt, surface-jnd §2).
- Do NOT touch shadow rows, sentiment, solve.rs, neutral.rs.

## Verification
- [ ] Source at :1008 reads a magnitude `>= 15.0` (and `<= 18.0` per the sourced band).
- [ ] `provisional_magnitudes_drive_the_decorative_result` test still passes (separator magnitude still drives the result).
- [ ] Resolved separator |Lc| on `#FFFFFF` is `>= 15.0` continuous (checkpoint: surface-jnd §2 gives Lc15 -> `#DFDFDF`).

## References
- `crates/labcolors-core/src/semantic.rs:1007-1008` — separator declaration.
- `docs/decisions/surface-jnd.md` §2 — sourced band [Lc15, Lc18]; engine checkpoints on white.
