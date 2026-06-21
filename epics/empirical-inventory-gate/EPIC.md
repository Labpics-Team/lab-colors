---
id: empirical-inventory-gate
title: "Empirical-inventory gate + SSOT as a tracked CLASS-guard (RED-proof, zero value drift)"
status: active
priority: 1
created: 2026-06-21
goal: "Land a tracked, RED-proof hygiene gate that fails CI whenever a numeric perceptual-policy constant lacks a paper-trail marker + an inventory row — without changing a single perceptual value."
success_criteria:
  - "`cargo test --workspace -p labcolors-core` is green with the gate present in the tree (R4 hygiene regime passes)."
  - "`red_proof_audit_probe` deterministically turns GATE-1 RED and names the spliced `_AUDIT_PROBE` literal — proving the guard bites (not green-from-birth)."
  - "Every scanned perceptual-policy const carries a `// NEEDS-SCIENCE` or `// GROUNDED` marker AND a row in `docs/decisions/empirical-inventory.md`; counts are equal (markers ↔ rows)."
  - "No standard (CIECAM16/UCS/Hellwig/APCA/WCAG/Oklab matrices/D65/Yb=20/L_A=64) is marked NEEDS-SCIENCE or appears as an inventory row — exclusion is by construction (detector scoped to const/Default sites + numeric_method allowlist)."
  - "Zero perceptual constant values change in the whole epic — `git diff` of src/ touches only comment markers, never the RHS of any `: f64 = N`."
  - "Merged via its own PR on `feat/v2-governance`, CI green pre-push, after isolated verification by 2+ verifiers; all findings (incl. pre-existing) resolved."
depends_on: []
---

# Empirical-inventory gate + SSOT as a tracked CLASS-guard

## Goal
Land a tracked, RED-proof hygiene gate that fails CI whenever a numeric perceptual-policy constant lacks a paper-trail marker + an inventory row — without changing a single perceptual value.

## ⚠️ Reality reconciliation (live-verified 2026-06-21, REPLACES the brief's stale snapshot)

The brief assumed the gate was "written but not committed" and resumable from the working tree. **It is not.** Ground truth from this session:

- **HEAD = `f21aac7` on `main`** (brief said `f6a017a` / `feat/v2-governance`). Working tree is **clean** — `git status --porcelain` is empty.
- `docs/decisions/empirical-inventory.md` and `crates/labcolors-core/tests/empirical_inventory.rs` **do not exist** in the working tree, on `main`, on `feat/v2-governance` (HEAD `f6a017a`, contains only `README.md` + `surface-jnd.md`), on the `worktree-agent-*` branches, or anywhere in git history (`git log --all -- <paths>` empty). Graphiti (`daniel-agent-local`) has **no record** of the gate.
- The **only** surviving trace is compiled binaries `target/debug/deps/empirical_inventory-*.exe` whose `.d` files point at `crates/labcolors-core/tests/empirical_inventory.rs`. The prior run authored, compiled, ran, and **lost** the source (uncommitted → wiped). This is the Constitution's "память не записана = стёрта" data-loss class, already known for this repo.
- **The brief's load-bearing literals are STALE.** Current `main` (and `f6a017a`) have `DECORATIVE_FLOOR_MIN = 7.6` at semantic.rs:**152** (brief: `15.0` @ :166). `SHADOW_STEP = DECORATIVE_FLOOR_MIN/10.0` **does not exist**; what exists is `SHADOW_{MINOR,AMBIENT,PENUMBRA,MAJOR}_JND = {8.0, 9.5, 11.5, 14.0}` @ :200–203. The `decorative(8.0)` separator literal survives at semantic.rs:**1008** (brief: :1078).

**Consequence for the plan:** this is **re-authoring against current `main`, not resurrection.** The gate logic, two markers, RED-proof design, and join-key concept from the brief are sound and are kept verbatim as the contract. But the SSOT inventory rows and the join-key `(row#, const name)` **must be regenerated against current line numbers**, and the 30-row count is an *estimate to be reconciled by enumeration in C01*, not a given. Branch the work off `feat/v2-governance` per the scope, but rebase it onto current `main` first so the gate scans the live tree.

## Audit surface (live-enumerated — the consts the detector must classify)

Scoped to `const … : f64 = <literal>` and `Default`-field literal sites in the 6 perceptual modules. Standards excluded **by construction** via a ~30-entry `numeric_method` allowlist.

| Module | Perceptual-POLICY consts (must inventory) | STANDARD consts (must be excluded) |
|---|---|---|
| `semantic.rs` | `DECORATIVE_FLOOR_MIN=7.6`, `SHADOW_{MINOR,AMBIENT,PENUMBRA,MAJOR}_JND`, `NEUTRAL_HUE_DEG=286`, `NEUTRAL_TINT_RATIO=0.10`, `TINT_TARGET_MP=6.1`, `TINT_HUE_STIFFNESS=9.0`, `TINT_PERCEPTIBLE_MP_FLOOR=1.5`, `CUSP_HALF_WINDOW_DEG=40`, `STRICT_STEP=0.5`, `LIGHTNESS_SETTLE`, `decorative(8.0)` @ :1008 | numeric EPS (`RATIO_BISECT_EPS=1e-9`, …) — excluded as non-perceptual |
| `sentiment.rs` | `DEFAULT_HARDNESS=2.0`, `CHROMA_FRACTION=0.88` | `S_PERC_MIN` (derivation-identity, R2 — recomputed, not policy) |
| `lpc.rs` | `SOFT_CLAMP_*`, `EXP_*`, `CONTRAST_SCALE=1.14`, `LO_*` (APCA-tuning policy — to be adjudicated) | `HK_CHROMA_EXPONENT=0.587` (Hellwig 2022), `LC_SCALE=100`, `DELTA_Y_MIN` (APCA standard) |
| `scale.rs`, `neutral.rs`, `lcs.rs` | (0 module-level f64 consts today — confirm no inline `Default` policy literals) | — |

Exact policy/standard classification per const is decided in C01-t2 by reading each const's doc-comment + provenance; the table is the starting partition, not the final inventory.

## Success Criteria
- [ ] `cargo test --workspace -p labcolors-core` green with gate present (R4 passes).
- [ ] `red_proof_audit_probe` deterministically RED + names `_AUDIT_PROBE`.
- [ ] Every scanned policy const has a marker AND an inventory row; marker count == row count.
- [ ] No standard is marked NEEDS-SCIENCE or appears as a row (invariant enforced by detector scoping).
- [ ] Zero perceptual values changed — `git diff` src/ touches only comment markers.
- [ ] Merged via own PR on `feat/v2-governance`, CI green pre-push, 2+ isolated verifiers, all findings resolved.

## Chapters
| Chapter | Status | Priority |
|---------|--------|----------|
| `chapters/c01-ground-and-author/` | ready | 1 |
| `chapters/c02-redproof-and-ci/` | draft | 2 |
| `chapters/c03-verify-and-merge/` | draft | 3 |

## Backlog
Deferred owner requests live in the sibling `BACKLOG.md` (drain each cycle; non-empty backlog = epic NOT complete).

## Notes / Constraints / Risks
- **CLASS of problem closed:** "magic perceptual number without a paper-trail" — a hygiene/governance regime (R4). The gate does not assert math (R1), derivation-identity (R2), or behavioral non-drift (R3); it asserts *every policy literal is marked + inventoried*. Do not conflate regimes (a flat failure count across R1–R4 is a reporting error).
- **Layers / dependency direction (Clean):** the gate is a **test-only consumer** at the very top of the dependency graph. It reads `src/*.rs` as text (or via a small pure-std scanner in the test) + reads `docs/decisions/empirical-inventory.md`. It depends on src; **nothing in src depends on it**. Zero new runtime deps — `labcolors-core` stays zero-dep (issue #29); the gate is `pure std` in `tests/`.
- **Toolchain:** CI pins `RUST_TOOLCHAIN=1.96.0`, workspace `edition=2024`, `rust-version=1.85`. The gate must compile and pass under 1.96.0 and `cargo clippy --workspace --all-targets -- -D warnings` (lint job) and `cargo fmt --all --check`.
- **KNOWN-LIMITATION (track, do NOT fix here):** detector is scoped to `const`/`Default`-field sites + the allowlist, so it is **blind to inline fn-body literals that ARE perceptual policy** — e.g. `mp_ref*1.5`, `hue_purity.powf(0.6)`, and the inline `chroma = sqrt(a²+b²)` recompute (no `oklab::chroma()` primitive exists; §1.3 of spec). These must be recorded as a known-limitation section in the surface-jnd / a dedicated ADR, NOT closed in this epic.
- **Out of scope (hard):** changing any perceptual value; adding `oklab::chroma()`; extending the detector to inline literals; touching R1/R2/R3 tests; any value-drift "fix." This epic only pins the contract.
- **Verification:** isolated, 2+ verifiers on adjacent themes (one auditor reads the gate logic + RED-proof; one reads the inventory↔marker sync + the standards-exclusion invariant), accepted via `verification-receive`. `verification-runner` (RED-proof, mutates tree) runs SOLO or in a worktree — never parallel with read-only reviewers (Graphiti: verification-runner-mutates-tree).
- **Constitution gates:** PR-only, CI green before push, no commit to `main`, no `--no-verify`/`--force`. Record result in Graphiti (`daniel-agent-local`) after merge.
