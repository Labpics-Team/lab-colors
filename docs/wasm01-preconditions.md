# WASM-01 Preconditions: Indexed Projection Input Specification

**Status:** DRAFT
**Date:** 2026-08-31
**Branch:** r7-g3-wasm01-preconditions
**Depends on:** r7 Goal 2 (Artifact Class Characterization), AUD-01 Extension (r6)
**Supersedes:** draft-r7.md §Goal 3 informal description

## 1. Required Artifact Classes

WASM-01 ("indexed projection") consumes audit artifacts to produce a deterministic, verifiable projection of the codebase's public capabilities through the WASM boundary. The following 12 of 14 `ArtifactClass` variants are required inputs. Two classes are excluded per r7 Goal 2 characterization.

### 1.1 Boundary Definition (Primary Inputs)

| Class | Disposition | Extractor | Role in WASM-01 |
|-------|-------------|-----------|-----------------|
| WasmBoundary | FiniteManifest | `enumerate.rs::collect_wasm_boundaries` | Defines the JS-accessible surface (`#[wasm_bindgen]` sites). WASM-01 indexes these as projection entry points. |
| NativeBoundary | FiniteManifest | `enumerate.rs::collect_native_boundaries` | Defines the FFI-accessible surface (`#[uniffi::export]` sites). WASM-01 uses these to verify parity between WASM and native projections. |

### 1.2 API Surface (Projection Content)

| Class | Disposition | Extractor | Role in WASM-01 |
|-------|-------------|-----------|-----------------|
| PublicRustApi | FiniteManifest | `extractors/api_manifest.rs::extract_public_api` | Full pub API manifest with signatures and SHA-256 fingerprints. WASM-01 maps each `WasmBoundary` to its underlying Rust API entry. |
| PublicExport | FiniteManifest | `enumerate.rs::collect_public_exports` | Re-export paths from `lib.rs`. WASM-01 resolves indirection between boundary symbols and canonical module locations. |
| Operation | FiniteManifest | `extractors/operations_source.rs::extract_operations` | Pub functions and methods with normalized signatures. WASM-01 cross-references boundary entries against operation manifest for completeness. |

### 1.3 Structural Context (Validation & Integrity)

| Class | Disposition | Extractor | Role in WASM-01 |
|-------|-------------|-----------|-----------------|
| ProductionSourceFile | FiniteManifest | `enumerate.rs::enumerate_production_artifacts` | File inventory for coverage verification. WASM-01 asserts no production source is unreachable from the boundary. |
| ConformanceFamily | FiniteManifest | `extractors/conformance_families.rs::extract_conformance_branches` | Match arms and if-chains defining conformance logic. WASM-01 verifies all decision sites are reachable through projected APIs. |
| SemanticBranch | FiniteManifest | Same extractor as ConformanceFamily (EXT-05 covers both) | Feature gates and cfg branches. WASM-01 ensures feature-gated boundaries are correctly conditioned in the projection. |
| DecisionSite | FiniteManifest | `enumerate.rs` (inline extraction via conformance_families) | Aggregate of match/if/cfg sites. WASM-01 uses these as integrity anchors: every decision site must map to at least one boundary or be explicitly excluded. |

### 1.4 Provenance & Metadata

| Class | Disposition | Extractor | Role in WASM-01 |
|-------|-------------|-----------|-----------------|
| ParallelSsot | FiniteManifest | `enumerate.rs::collect_parallel_ssot` | SSOT-TRACKED and GROUNDED markers. WASM-01 includes provenance attestations in the projection manifest for constants derived from external standards. |
| PublicClaim | FiniteManifest | `enumerate.rs::collect_public_claims` | Module-level doc-comment assertions (`//! #`). WASM-01 references claims as human-readable capability descriptions in the projection metadata. |
| CiBuildReleaseDeclaration | FiniteManifest | `enumerate.rs::collect_ci_artifacts` | CI workflow files. WASM-01 records which CI gates validate the WASM artifact itself, closing the build-to-runtime evidence chain. |

### 1.5 Excluded Classes

| Class | Disposition | Rationale |
|-------|-------------|-----------|
| ResourceDimension | NotApplicable | Dead code (`dependencies_source.rs` unwired). Type-level constraints are compile-time; runtime resource interface is captured by WasmBoundary/NativeBoundary. Removal tracked as r8 tech debt. |
| GraphArtifactTest | EmptyClass | Zero instances in codebase. Speculative placeholder from initial AUD-01 schema. Candidate for enum removal in r8. |

## 2. Contract Dependencies

WASM-01 depends on the following types from `labcolors-core` and `labcolors-audit`:

### 2.1 Core Types (labcolors-core)

| Type | Location | Usage in WASM-01 |
|------|----------|------------------|
| `ProgramSessionV1` | `program_wire` | Runtime session instantiated by `compile_program_wire`. WASM-01 projects this as `ProgramRuntime`. |
| `ProgramSnapshotV1` | `program_wire` | Atomic update result. WASM-01 projects state/output accessors. |
| `ProgramScenarioV1` | `program_wire` | Observed input matrix. WASM-01 validates surface dimensions against scenario count. |
| `Srgb8` | root | Canonical color type. WASM-01 accepts raw `[u8; 3]` and constructs `Srgb8` internally. |
| `Wcag22CriterionV1` | `wcag22` | Enumerated criterion set. WASM-01 parses string input via `parse()`. |
| `Wcag22AssessmentV1` | `wcag22` | Evaluation result with evidence payload. WASM-01 serializes to JSON via `terminal_projection`. |
| `NumericalCapabilityManifestV2` | root | Capability manifest returned by `numerical_capability_manifest()`. Already projected. |
| `NumericalDecisionEvidenceV1` | root | Evidence class discriminator. WASM-01 asserts `CanonicalFiniteBounded` variant. |

### 2.2 Audit Types (labcolors-audit)

| Type | Location | Usage in WASM-01 |
|------|----------|------------------|
| `ArtifactClass` | `types.rs` | Enum discriminants used to filter and route artifacts during projection construction. |
| `RawArtifact` | `types.rs` | Input record format. WASM-01 reads `class`, `module`, `line`, `raw_key`, `raw_value`. |
| `DispositionedArtifact` | `types.rs` | Post-dispose record with `normalized_join_key`. WASM-01 joins artifacts against proof evidence via this key. |
| `ApiManifestEntry` | `extractors/api_manifest.rs` | Structured API entry with `signature_sha256`. WASM-01 uses fingerprint for change detection. |
| `ConformanceBranchEntry` | `extractors/conformance_families.rs` | Branch entry with `fingerprint`. WASM-01 verifies decision site coverage. |
| `CrateExportManifest` | `extractors/exports_manifest.rs` | Crate metadata + feature gates. WASM-01 conditions boundary availability on feature flags. |
| `OperationEntry` | `extractors/operations_source.rs` | Operation signature with hash. WASM-01 cross-references against boundary entries. |

### 2.3 Existing WASM Projection Contracts

The current `labcolors-wasm` already defines TypeScript interfaces via `typescript_custom_section`:

- `NumericalCapabilityManifestV2` / `NumericalCapabilitySiteV2` — stable, schema version 2
- `Wcag22AssessmentV1` / `Wcag22Q55BoundsV1` — stable, v1
- `ProgramRuntime` / `ProgramSnapshot` — terminal Program wire API

WASM-01 extends (does not replace) these contracts. The indexed projection adds an audit-derived manifest that maps boundary symbols to their supporting artifact evidence.

## 3. Gaps and Blockers

### 3.1 Gap: No Unified Projection Input Type

**Current state:** Each extractor returns its own entry type (`ApiManifestEntry`, `ConformanceBranchEntry`, etc.) with no common trait or unified container. WASM-01 would need to accept heterogeneous collections.

**Proposed resolution:** Define a `ProjectionInput` struct in `labcolors-audit` that aggregates all 12 required artifact classes into typed fields. This is a new type, not a refactor of existing extractors. Estimated effort: 2-3h.

**Blocking severity:** Medium. WASM-01 can proceed without it by accepting individual vectors, but the unified type prevents parameter ordering bugs and makes the contract self-documenting.

### 3.2 Gap: WasmBoundary/NativeBoundary Extractors Lack Fingerprints

**Current state:** `collect_wasm_boundaries` and `collect_native_boundaries` in `enumerate.rs` emit `RawArtifact` with `raw_key = signature` but no SHA-256 fingerprint. Other extractors (`api_manifest`, `conformance_families`, `operations_source`) all include fingerprints for integrity verification.

**Proposed resolution:** Add `signature_sha256` computation to both boundary collectors. This aligns them with the established pattern and enables WASM-01 to detect phantom boundary entries. Estimated effort: 1h.

**Blocking severity:** Low. WASM-01 can compute hashes at consumption time, but having them at extraction time is consistent and enables sabotage controls.

### 3.3 Gap: ParallelSsot Extraction Has No Dedicated EXT Node

**Current state:** `collect_parallel_ssot` is inline in `enumerate.rs` with floor test added in r7-g2, but lacks an independent EXT node with sabotage controls comparable to EXT-01..EXT-09.

**Proposed resolution:** Promote to dedicated EXT-10 node in r8 if r7-g2 floor test proves stable. Not blocking for WASM-01 first slice — the inline extraction produces correct artifacts.

**Blocking severity:** None for WASM-01. Tracked as r8 follow-up.

### 3.4 Gap: MEM-01 Resource Profile Data Unavailable

**Current state:** draft-r7.md §7 notes "Whether WASM-01 contract can be fully specified without MEM-01 resource profile data" as unknown. MEM-01 is blocked pending WASM-01 contract definition (circular dependency).

**Proposed resolution:** WASM-01 first slice does NOT require MEM-01. Resource profiles are optimization metadata, not correctness prerequisites. The indexed projection is a structural/artifact-level concern; memory/CPU budgets are a separate axis addressed after the projection contract is stable.

**Blocking severity:** None. Explicitly deferred.

### 3.5 Non-Gap: ResourceDimension and GraphArtifactTest Exclusion

Confirmed non-blocking per r7 Goal 2 characterization. These classes carry no data relevant to WASM-01. Their exclusion reduces projection complexity without sacrificing coverage.

## 4. Recommended Scope for WASM-01 First Slice

### 4.1 In Scope

1. **Boundary index construction:** Map every `WasmBoundary` and `NativeBoundary` artifact to its corresponding `ApiManifestEntry` and `OperationEntry` via signature matching. Produce a `BoundaryIndex` struct with forward/reverse lookups.

2. **Coverage assertion:** Verify every `ProductionSourceFile` is reachable from at least one boundary entry (directly or via re-export chain). Report unreachable files as warnings, not errors, in the first slice.

3. **Decision site mapping:** Associate each `ConformanceFamily`/`SemanticBranch`/`DecisionSite` artifact with the boundary entry whose implementation contains it. Flag orphaned decision sites.

4. **Provenance attachment:** Include `ParallelSsot` markers and `PublicClaim` text as metadata annotations on the boundary entries they relate to (matched by file + line proximity).

5. **CI gate reference:** Record `CiBuildReleaseDeclaration` entries that validate the WASM crate build, embedding workflow names in the projection manifest header.

6. **JSON serialization:** Produce a deterministic JSON manifest consumable by JS tooling. Schema version 1, with explicit field names matching the TypeScript conventions established in `lib.rs`.

### 4.2 Out of Scope (Deferred to WASM-02+)

- Memory/CPU resource profiling (MEM-01 dependency)
- Runtime attestation generation (requires live WASM execution harness)
- Automated diff against previous projection versions
- NativeBoundary parity enforcement (first slice indexes both but does not enforce symmetry)
- Graph-based reachability analysis beyond file-level coverage
- Feature-gate conditional projection (first slice includes all boundaries regardless of feature flags; feature-aware filtering deferred)

### 4.3 Acceptance Targets for First Slice

| Target | Baseline | Gate |
|--------|----------|------|
| All 12 required artifact classes consumed | 0 (new) | Compilation succeeds with all 12 input vectors populated |
| Boundary index covers 100% of WasmBoundary artifacts | 0 (new) | Test: `boundary_index.len() == wasm_boundary_artifacts.len()` |
| Zero phantom boundary entries (sabotage control) | N/A | Sabotage test: fabricated entry with wrong hash fails validation |
| Projection JSON parses in Node.js | 0 (new) | Integration test: `JSON.parse(output)` succeeds, schema version = 1 |
| FLOOR baseline maintained | minimum_floor=1410 | `cargo test` count >= 1410 |

## 5. Cross-Reference Validation

This specification was validated against:

- `docs/r7-artifact-class-characterization.md` — all 14 class dispositions confirmed; 12 included, 2 excluded with documented rationale
- `crates/labcolors-audit/src/types.rs` — `ArtifactClass` enum matches the 14 variants referenced herein
- `crates/labcolors-audit/src/enumerate.rs` — boundary collectors and inline extractors confirmed as source of truth for WasmBoundary, NativeBoundary, ParallelSsot, PublicClaim, PublicExport, ProductionSourceFile, CiBuildReleaseDeclaration
- `crates/labcolors-audit/src/extractors/` — api_manifest, conformance_families, operations_source, exports_manifest confirmed as source of truth for PublicRustApi, ConformanceFamily+SemanticBranch+DecisionSite, Operation, and crate metadata
- `crates/labcolors-wasm/src/lib.rs` — existing WASM boundary contracts (TypeScript interfaces, ProgramRuntime/Snapshot) confirmed as extension targets, not replacement targets
- `docs/draft-r7.md` — Goal 3 acceptance criteria satisfied: input spec lists all consumed classes with extractor references; boundary types mapped; spec reviewed against AUD-01 matrix