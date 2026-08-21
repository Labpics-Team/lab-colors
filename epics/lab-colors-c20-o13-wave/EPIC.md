# Epic: lab-colors C-20 (C7e Glow hard-cut)

## Mission
Lower the Glow physical branch into the compiled field operator invocation using the existing V7 `EncodedSrgb8ScreenOpaqueBackdrop` kernel. Remove the legacy `solve_screen_alpha_for_dj` runtime path (CAM16/libm target-seeking) from the semantic resolver. Preserve byte-identity of the screen composite via the `encoded_srgb8_screen_channel` SSOT in `field_effect.rs`. Maintain `GlowDecisionProfileV1`/Stable/Legacy selection as compile-time wire config only — at runtime, only the stable exact screen law is executed.

## North Invariants
1. **Single screen-law SSOT**: `encoded_srgb8_screen_channel` in `field_effect.rs` is the ONLY implementation of the encoded-sRGB8 screen formula. `glow.rs` delegates to it; no duplicate formula exists.
2. **No legacy CAM16 target-seeking**: `solve_screen_alpha_for_dj_legacy` and `QuantisedComposites` stream are removed. The solver no longer searches for a target ΔJ' using CAM16 J'.
3. **Byte-identity preservation**: The stable exact noop path produces bit-identical composite bytes to the JS reference.
4. **Wire config only**: `GlowDecisionProfileV1::LegacyPlatformDependentV1` remains in the wire/config schema for backward compatibility but maps to `StableOnly` or `Indeterminate` at runtime.
5. **CI green gate**: All checks pass before merge.
6. **Dual isolated review PASS**: Both verification-runner and arch-reviewer must PASS.

## Non-Goals
- O-13 (reusable arenas) — deferred to next wave.
- Public API expansion — no new exports.
- RecheckErrorV1, serde terminal_projection, BindingError::InvalidIndex — deferred post-C7c.

## Scopes

### Scope 1: Remove legacy Glow solver
**Repo**: lab-colors
**Exit criteria**:
- `solve_screen_alpha_for_dj_legacy`, `QuantisedComposites`, `quantised_composites` removed from `glow.rs`.
- `GlowTargetStatus::LegacyReached` and `LegacyUnreachable` removed.
- `solve_screen_alpha_for_dj` only executes stable exact noop logic; `ExplicitCompatibility` maps to `Indeterminate` if not noop.
- `semantic.rs` `CompiledGlowInvocationV1::resolve` no longer matches `NumericalDecisionV1::Compatibility`.
- Golden tests and boundary tests updated to reflect removal of legacy target-seeking.
- CI green, dual isolated review PASS.

## Evidence Cutoff
2026-08-21T08:00:00Z (main at f2c49a3, PR #589 merged).