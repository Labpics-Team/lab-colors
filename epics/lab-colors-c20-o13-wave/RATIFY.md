# RATIFY — lab-colors C-20 (C7e Glow hard-cut)

## Human Freeze-Gate
This document is the **human approval checkpoint** for the ship workflow.

### What was planned
- **Mission**: Lower Glow physical branch into compiled field operator invocation using existing V7 `EncodedSrgb8ScreenOpaqueBackdrop` kernel; remove legacy `solve_screen_alpha_for_dj` runtime path from semantic resolver; preserve byte-identity via `encoded_srgb8_screen_channel` SSOT.
- **Scopes**: 1 scope decomposed with explicit exit criteria.
- **North Invariants**: 6 invariants declared (single screen-law SSOT, no legacy CAM16 target-seeking, byte-identity preservation, wire config only, CI green gate, dual isolated review PASS).
- **Non-Goals**: O-13 deferred, no public API expansion, no RecheckErrorV1/serde/BindingError changes.

### What will be built
1. **Scope 1 (Remove legacy Glow solver)**: Remove `solve_screen_alpha_for_dj_legacy`, `QuantisedComposites`, `quantised_composites` from `glow.rs`; remove `GlowTargetStatus::LegacyReached` and `LegacyUnreachable`; make `solve_screen_alpha_for_dj` execute only stable exact noop logic (map `ExplicitCompatibility` to `Indeterminate` if not noop); update `semantic.rs` `CompiledGlowInvocationV1::resolve` to no longer match `NumericalDecisionV1::Compatibility`; update golden/boundary tests.

### Gates before any code lands
- Design → RedProof (TDD red MUST fall) → Green → Refactor → Review (arch+appsec+correctness+CodeRabbit+frontier+base) → Verify (refute) → Fix → isolated PASS → Deliver (PR; armed-merge ONLY on PASS+>=2 reviews+CI green+0 Critical/High).
- North invariants confirmed by isolated judge before completion claim.

### Approval required
**DO NOT PROCEED TO BUILD LOOP WITHOUT HUMAN SIGN-OFF.**
Reply with one of:
- `APPROVE` — proceed to build loop as planned.
- `REVISE <feedback>` — adjust EPIC.md/STATE.json per feedback, then re-present RATIFY.md.
- `ABORT` — stop ship workflow; preserve STATE.json for potential future resume.

---
*Generated: 2026-08-21T08:35:00Z*
*Epic dir: lab-colors-c20-o13-wave*
*Evidence cutoff: 2026-08-21T08:00:00Z*