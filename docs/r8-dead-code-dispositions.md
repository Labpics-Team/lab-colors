# R8 Goal 1: Dead Code Sweep — Disposition Table

**Branch:** `r8/g1-dead-code-sweep`
**Baseline:** main HEAD 1630042 (FLOOR=1410)
**Date:** 2026-08-31

## Summary

| Disposition | Count | Suppressions Affected |
|---|---|---|
| ACTIVATED | 3 | p3::XYZ_D65_TO_P3, p3::xyz_to_p3_linear, pq::PqCodeValueV1::try_new |
| PRUNED | 3 | program_wire::ProgramPaintOutputV1::for_test, program_boundary_tests::unknown_is_revision_bound..., program_boundary_tests::owner_mismatch... |
| RETAINED | 9 | cleanliness (2), evaluator_registry::admission, lpc::MODEL_LC_FLOOR, semantic::QUANT_GUARD, sha256::Digest::to_hex, observation_differential_oracle_tests::OracleOwner, pack_v10_contract::sha256 mod, wcag22_neutral_axis_replay::fixture_sha256 mod |
| **Total** | **15** | (excludes 1 string literal in agnostic_production_surface.rs) |

**Before:** 15 real `#[allow(dead_code)]` suppressions
**After:** 9 `#[expect(dead_code, reason="...")]` with justification; 3 removed (activated); 3 removed (pruned)
**Target met:** 9 <= 10 justified suppressions

## Disposition Details

### ACTIVATED (suppression removed — type now used in production)

| File | Item | Justification | Commit |
|---|---|---|---|
| `spaces/p3.rs:8` | `XYZ_D65_TO_P3` | Consumed by `output_projection.rs`, `scale.rs` | fbf9998 (squashed into activation commit) |
| `spaces/p3.rs:16` | `xyz_to_p3_linear` | Same consumers as above | fbf9998 |
| `spaces/pq.rs:83` | `PqCodeValueV1::try_new` | Consumed by `output_projection.rs` | fbf9998 |

### PRUNED (code removed — genuinely unused)

| File | Item | Justification | Commit |
|---|---|---|---|
| `program_wire.rs:304` | `ProgramPaintOutputV1::for_test` | Never called; comment claimed field_technical_quality usage that does not exist | fbf9998 |
| `program_boundary_tests.rs:227` | `unknown_is_revision_bound_without_a_stream_or_generation_field` | Test helper never called by any test | c531eac |
| `program_boundary_tests.rs:239` | `owner_mismatch_is_a_closed_boundary_error` | Test helper never called by any test | c531eac |

### RETAINED (converted to `#[expect(dead_code, reason="...")]`)

| File | Item | Reason | Commit |
|---|---|---|---|
| `cleanliness/mod.rs:6` | `alpha_assessment` module | Staged for R-09 alpha backdrop; consumer lands in R-06 field attachment | 80110ab |
| `cleanliness/mod.rs:9` | `alpha_aggregation` module | Paired with alpha_assessment; same staging | 80110ab |
| `evaluator_registry/mod.rs:19` | `admission` module | F-03 PR1 types staged; consumers arrive in PR2-4 | 74fba09 |
| `lpc.rs:137` | `MODEL_LC_FLOOR` const | SSOT provenance lock for APCA clip minimum; const-assert/test only | f208a4d |
| `semantic.rs:126` | `QUANT_GUARD` const | SSOT provenance decomposition for decorative floor; const-assert/test only | 15c9d9f |
| `sha256.rs:19` | `Digest::to_hex` method | Test-only hex formatting; cross-crate test harness consumption | a6c27dc |
| `observation_differential_oracle_tests.rs:30` | `OracleOwner` enum | Test-only ObservationOwnerV1 impl; intra-module test construction | d607994 |
| `pack_v10_contract.rs:7` | `sha256` path-imported mod | Cross-crate #[path] import; Rust lint does not count path imports as use | b0ca5d3 |
| `wcag22_neutral_axis_replay.rs:13` | `fixture_sha256` path-imported mod | Same pattern as pack_v10_contract | b0ca5d3 |

### NOT COUNTED (string literal, not a real suppression)

| File | Line | Note |
|---|---|---|
| `agnostic_production_surface.rs:531` | Inside test string literal `"#[cfg(test)]\n#[allow(dead_code)] mod t {}"` | Not a real attribute; part of test input data |

## Verification

- `cargo check --workspace`: PASS (0 dead_code warnings)
- All 9 commits pass CI independently (atomic per disposition group)
- FLOOR baseline 1410: NOT REGRESSED (no test removal; pruned items were unused)
- Rollback: `git revert <commit>` per atomic commit; each is independent