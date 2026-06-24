---
id: define-sweep-and-predicate
chapter: quantization-robust-invariant
epic: jnd-floor-and-separator-pin
title: "Define the background sweep + the post-quantization floor predicate"
status: ready
priority: 1
depends_on: []
blocks:
  - cover-quantization-cliff
agent_profile:
  category: deep
  skills: [craft-qa, dive-rust]
started: null
completed: null
refine_after: []
---

# Define the background sweep + the post-quantization floor predicate

## What
In `semantic.rs` `#[cfg(test)]`, add a property test that sweeps backgrounds and asserts the decorative floor holds AFTER quantization. GROUND the predicate in the real API first (read `resolve` / `Resolved` / `Solved::lc` and `BgInput::solid`):
- **Sweep**: a representative-or-exhaustive set of 8-bit backgrounds. Prefer exhaustive over the grey axis (`#000000..#FFFFFF` step-1 on the diagonal = 256 bgs) PLUS a chroma sample, OR the full per-channel sweep if step-budget allows. The corpus MUST include near-black backgrounds where the cliff lives (`#080808` and neighbours). Sweep BOTH viewing conditions (`srgb` and `dim`) since the dark-anchor path differs.
- **Predicate** per background `bg`: resolve `Role::Separator` (the Lc-decorative role). The result is either `Resolved::Color { solved, .. }` -> assert `solved.lc().abs() >= 15.0` (the emitted hex is already on-grid/quantized — `solved.lc()` measures the EMITTED colour, confirm this by reading `Solved`), OR `Resolved::Unreachable(_)` -> ALLOWED (honest no-headroom, not a floor violation). A `Color` with `|Lc| < 15.0` is a FAILURE. A panic / `None` is a FAILURE.
- Assert the threshold is the SOURCED `15.0`, not `DECORATIVE_FLOOR_MIN - 1.0` (the old `:2317` tolerance is the continuous-only weak check this REPLACES/COMPLEMENTS — do not copy its slack).

## Must NOT Do
- Do NOT assert on `DECORATIVE_FLOOR_MIN - 1.0`; the new invariant is exact `>= 15.0` post-quantization.
- Do NOT measure a CONTINUOUS target; measure the EMITTED, quantized colour's |Lc| (confirm `solved.lc()` reflects the on-grid hex, not the continuous solve target — read `Solved` to verify; if it returns the continuous target, re-measure from the emitted hex via the public contrast API).
- Do NOT touch sentiment/`S_PERC_MIN`/`RoleChroma`, solve.rs, neutral.rs.

## Verification
- [ ] The test compiles and runs against the current tree.
- [ ] `solved.lc()` is confirmed (by reading `Solved`) to reflect the QUANTIZED emitted colour, not the continuous solve target. If not, the test re-measures from the emitted hex.
- [ ] The sweep includes `#080808` + near-black neighbours and both viewing conditions.

## References
- `crates/labcolors-core/src/semantic.rs` — `resolve`, `Resolved`, `Role::Separator`, existing sweep tests (`legal_floor_is_held_across_a_full_background_sweep` ~:1745, decorative-floor test ~:2300-2322) as structural templates.
- `crates/labcolors-core/src/solve.rs` — `Solved`, `Unreachable`, `BgInput` (READ-ONLY, to ground `.lc()` semantics).
- `docs/decisions/surface-jnd.md` §1c (#080808-class cliff reasoning), §2 (engine checkpoints).
