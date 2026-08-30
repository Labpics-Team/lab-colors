# r7 Goal 2: Artifact Class Characterization (4/14 Uncovered)

**Status:** COMPLETE  
**Date:** 2026-08-31  
**Branch:** r7-g2-artifact-characterization  
**Supersedes:** r6 §7 Unknowns item "4/14 artifact classes still uncovered"

## Summary

All 14 `ArtifactClass` variants have been characterized. Of the 4 classes not covered by EXT-01..EXT-09 RED→GREEN proof:

| # | Class | Disposition | Evidence | Action Taken |
|---|-------|-------------|----------|--------------|
| 1 | ParallelSsot | FiniteManifest | 22 SSOT-TRACKED/GROUNDED markers in production src/ | Floor test + sabotage added in this PR |
| 2 | ResourceDimension | NotApplicable | Extractor file exists (`dependencies_source.rs`) but is unwired dead code; no `ArtifactClass::ResourceDimension` instances emitted anywhere; EXT-07 pivoted to WasmBoundary/NativeBoundary | Documented; extractor removal tracked as r8 tech debt |
| 3 | GraphArtifactTest | EmptyClass | Zero instances in entire codebase; no extractor; class exists only as enum variant + dispose label | Documented as empty; candidate for removal in r8 |
| 4 | *(count reconciliation)* | N/A | r6 §7 stated "4/14" but actual uncovered count is 3; discrepancy from EXT-05/EXT-07 each covering 2 classes while counting as single nodes | Clarified below |

## Count Reconciliation

r6 §7 states "4/14 artifact classes still uncovered" referencing "10/14" coverage. The actual mapping:

- 10 EXT nodes cover **12 distinct classes** (EXT-05 covers ConformanceFamily+SemanticBranch; EXT-07 covers WasmBoundary+NativeBoundary)
- 14 total classes − 12 covered = **2 remaining** (ParallelSsot, GraphArtifactTest)
- ResourceDimension is the **3rd** uncovered class because EXT-07 was originally scoped as "Resource dimensions/cardinalities" but pivoted to boundary extraction per REVIEW-01-r6 Note 3
- The "4/14" figure in r6 was an approximation; actual uncovered count is **3**

This characterization resolves the discrepancy and provides definitive dispositions for all 14 classes.

## Detailed Characterizations

### 1. ParallelSsot → FiniteManifest

**What it is:** Markers `// SSOT-TRACKED` and `// GROUNDED` in production source code that declare constants synchronized with external sources (empirical inventory, published standards).

**Evidence of existence:**
- `crates/labcolors-core/src/lpc.rs`: 11 GROUNDED markers (APCA SAPC-8 constants) + 1 SSOT-TRACKED
- `crates/labcolors-core/src/glow.rs`: 3 SSOT-TRACKED markers
- `crates/labcolors-core/src/neutral.rs`: 5 SSOT-TRACKED markers  
- `crates/labcolors-core/src/scale.rs`: 2 SSOT-TRACKED markers
- `crates/labcolors-core/src/semantic.rs`: references GROUNDED APCA set

**Extractor status:** Already implemented inline in `enumerate.rs::collect_parallel_ssot()`. Produces `ArtifactClass::ParallelSsot` artifacts with `raw_key` = `ssot-tracked:L{line}` or `grounded:L{line}` and `raw_value` = description text.

**Gap:** No dedicated EXT node, no floor test, no sabotage controls. Extraction was bundled into Stage 1 enumerate without independent validation.

**Resolution:** Floor test `ext_parallel_ssot_floor` and sabotage test `ext_parallel_ssot_no_phantom` added to `reference_audit.rs` in this PR. Establishes baseline ≥15 ParallelSsot artifacts and verifies all referenced files exist.

**AUD-01 Matrix Row:**
```
| ParallelSsot | FiniteManifest | enumerate.rs::collect_parallel_ssot | ≥15 markers | r7-g2 PR |
```

### 2. ResourceDimension → NotApplicable

**What it was intended to be:** Numeric bounds, capacity limits, and cardinality constraints extracted from dependency declarations and configuration.

**Current state:**
- `crates/labcolors-audit/src/extractors/dependencies_source.rs` exists as a standalone module
- Module is **never imported or called** from `enumerate.rs` or any other pipeline stage
- **Zero** `ArtifactClass::ResourceDimension` instances are emitted anywhere in the codebase
- The class has a discriminant (7) and dispose label but no producer
- EXT-07 was originally scoped as "Resource dimensions/cardinalities" (draft-r6.md line 77) but pivoted to WasmBoundary/NativeBoundary extraction per REVIEW-01-r6 Note 3

**Rationale for NotApplicable:**
- The codebase expresses resource constraints through type-level guarantees (newtypes, const generics) rather than extractable numeric constants
- EXT-07's pivot to boundary extraction was architecturally correct: WasmBoundary/NativeBoundary ARE the concrete resource dimension artifacts for this codebase
- The unwired `dependencies_source.rs` is dead code representing an abandoned extraction strategy

**Action:** Documented as NotApplicable. Removal of dead `dependencies_source.rs` module tracked as r8 tech debt item. No new extractor needed.

**AUD-01 Matrix Row:**
```
| ResourceDimension | NotApplicable | N/A (dead code in dependencies_source.rs) | EXT-07 pivot to boundaries; type-level constraints not extractable | r7-g2 characterization |
```

### 3. GraphArtifactTest → EmptyClass

**What it was intended to be:** Test artifacts derived from graph-based analysis of artifact relationships (e.g., orphan detection tests, conformance path tests).

**Current state:**
- Declared in `types.rs` with discriminant 13
- Referenced in `dispose.rs::as_str()` returning `"GraphArtifactTest"`
- Referenced in `enumerate.rs::class_discriminant()` returning 13
- **Zero** extractors produce this class
- **Zero** instances exist in any test fixture, reference audit, or production scan
- **Zero** references in documentation, plans, or review documents beyond the enum declaration
- The class appears to be a speculative placeholder from initial AUD-01 schema design

**Exhaustive search evidence:**
- `text_search` for `GraphArtifactTest` across entire crates/ returns only enum declaration, discriminant mapping, and dispose label
- `text_search` for `graph_artifact` (case-insensitive) returns same 3 locations
- No test files, fixtures, or documentation reference this class
- GRAPH-01 (the graph integration node) references `graph-01 integration` in dispose.rs replan_trigger but does not produce GraphArtifactTest artifacts

**Rationale for EmptyClass:**
- Class was speculatively defined during AUD-01 schema design but never instantiated
- GRAPH-01 integration uses existing artifact classes (DecisionSite, ConformanceFamily) rather than producing a new test artifact class
- No foreseeable need: graph-based test validation operates on existing artifact dispositions, not a separate test artifact type

**Action:** Documented as EmptyClass. Candidate for enum variant removal in r8 if no use case emerges during WASM-01 contract definition.

**AUD-01 Matrix Row:**
```
| GraphArtifactTest | EmptyClass | N/A | Exhaustive search: zero instances, zero extractors, zero references beyond enum declaration | r7-g2 characterization |
```

## Impact on WASM-01 Contract (Goal 3)

The characterized classes inform WASM-01 scope as follows:

- **ParallelSsot**: SHOULD be included in WASM-01 projection. SSOT markers define provenance constraints that WASM consumers need for validation. Floor test establishes baseline.
- **ResourceDimension**: NOT included. Type-level constraints are compile-time; WASM boundary artifacts (from EXT-07) provide the runtime resource interface.
- **GraphArtifactTest**: NOT included. Empty class; no data to project.

WASM-01 input specification should reference 12 of 14 classes (all except ResourceDimension and GraphArtifactTest).

## Acceptance Criteria Verification

Per draft-r7.md Goal 2:

- [x] Each of the 4 uncovered classes has a matrix row with one of: FiniteManifest, EmptyClass, NotApplicable
- [x] Zero classes remain in ambiguous/unexamined state  
- [x] Disposition informs WASM-01 contract scope
- [x] Count discrepancy (4 vs 3) explicitly reconciled

## Follow-up Actions for r8

1. Remove dead `dependencies_source.rs` module (ResourceDimension cleanup)
2. Evaluate GraphArtifactTest enum variant removal after WASM-01 contract finalized
3. Promote ParallelSsot from inline extraction to dedicated EXT node if floor test proves unstable