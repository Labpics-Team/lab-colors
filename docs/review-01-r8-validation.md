# REVIEW-01-R8: Plan Validation

**Reviewer:** Independent (isolated axis)
**Date:** 2026-08-31
**Target:** docs/draft-r8.md
**Reference artifacts:** r7-tech-debt-audit.md, r7-artifact-class-characterization.md, r8-branch-audit.md, agents-config/plans/SPEC.md

## Axis 1: Plan-Contract Conformance (SPEC §4)

| § | Section | Verdict | Finding |
|---|---------|---------|---------|
| 4.1 | Frontmatter | FAIL | `status: DRAFT` present in frontmatter — SPEC §4.1 explicitly forbids `status` in new revisions ("Legacy-поле `status` ... запрещено в новой"). Must be removed; lifecycle is managed solely via ACTIVE.md |
| 4.1 | Frontmatter `owns` | PASS | Two scopes declared with canonical `<owner>/<repo>:<scope>` format; no overlap within track |
| 4.2 | Delta | FAIL | Section absent. SPEC requires Delta for r2+ with table "факт → свежее доказательство → решение". R7 Completion Summary is not a Delta — it reports prior status, not what changed between r7 and r8 or why this revision exists |
| 4.3 | Objective and acceptance | FAIL | Five goals listed instead of one. SPEC §4.3 mandates "Одна цель" with numbered acceptance criteria. Multiple goals must be expressed as DAG nodes under a single objective, not as separate top-level objectives |
| 4.4 | Facts / Evidence | FAIL | Section absent. Evidence cutoff stated inline but no Facts table (Факт | Evidence | Вывод). Key claims (FLOOR=1410, 30 suppressions, 47 branches) are asserted without structured evidence links |
| 4.5 | Assumptions | FAIL | Section absent. No ASM-NN numbered assumptions. Unknowns section conflates unknowns with assumptions — SPEC distinguishes them (§4.5 vs §4.11) |
| 4.6 | Invariants | FAIL | Section absent. No INV-NN numbered invariants. FLOOR non-regression is mentioned per-goal but never formalized as a track-level invariant |
| 4.7 | DAG | FAIL | No Mermaid flowchart. Dependencies described in prose and a flat table, but no graph with node IDs, edges, or status table (Node | Status | Exit evidence). Node IDs (e.g., DC-01, BH-01) are absent |
| 4.8 | Gates | FAIL | Section absent. Acceptance criteria exist per goal but no consolidated gate list referencing node IDs and invariant IDs |
| 4.9 | Rollback | PASS | Concrete rollback paths per scenario (plan revert, git revert, branch reflog). Meets SPEC requirement for specific commands |
| 4.10 | Smells / CAPA | FAIL | Section absent. No CAPA table carried forward from prior revisions. Risk table exists but is not CAPA-format (Severity | Finding | CAPA) |
| 4.11 | Unknowns | PASS | Four explicit unknowns listed. Properly separated from facts |

**Axis 1 Verdict: FAIL (9/12 sections non-conformant)**

The document is a well-structured draft but does not conform to SPEC §4. It reads as a task breakdown rather than a compilable plan. The structural gaps (Delta, Facts, Assumptions, Invariants, DAG, Gates, CAPA) mean an executing agent cannot derive execution order, prove readiness, or detect assumption invalidation without ad-hoc reasoning.

## Axis 2: Future-Axis Review

### Second-Order Consequences

| Goal | Consequence | Severity | Assessment |
|------|-------------|----------|------------|
| G1 Dead Code Sweep | Activating staged types may expose incomplete implementations that pass compilation but fail at runtime in downstream consumers | Medium | Mitigated by atomic PRs + CI, but no mention of integration test harness validating activated types against real audit runs. Unit tests alone insufficient for types previously suppressed |
| G2 Branch Hygiene | Already completed per r8-branch-audit.md (only main + r8-g4-wasm01-harness remain). Goal as written is stale — 47 branches no longer exist | High | Plan describes work already done. Acceptance criteria reference a state that has been superseded by actual cleanup. This goal should be marked DONE with exit evidence pointing to r8-branch-audit.md, or removed from scope |
| G3 Enum Cleanup | Removing GraphArtifactTest reduces enum cardinality from 14→13. Any external serialization format (including WASM boundary) using discriminant values will shift. #[non_exhaustive] catches Rust consumers but NOT cross-language boundaries | Medium | G4 WASM harness depends on stable discriminants. If G3 executes before G4 finalizes the wire format, discriminant 13 gap may cause silent misalignment in WASM deserialization. Order dependency is acknowledged (G3 before G4) but risk of discriminant instability is not named |
| G4 WASM-01 Harness | Round-trip property test locks in current serialization format as contract. Future changes to ArtifactClass enum (additions, removals, reordering) will require coordinated updates across native + WASM boundary + test fixtures | High | This is the correct behavior (contract stability), but the plan does not address versioning strategy for the WASM boundary. What happens when r9 adds a new ArtifactClass? Is there a wire format version field? Without this, every enum change becomes a breaking change |
| G5 Independent Review | Self-referential: this review IS G5. Circular dependency if G5 is a prerequisite for G1-G4 execution | Low | G5 correctly placed as last goal with dependency on G1-G4 scoping. No issue if treated as validation gate rather than execution blocker |

### Evolution Constraints

1. **WASM boundary versioning gap.** G4 creates a round-trip test but does not establish a wire format versioning mechanism. This constrains all future ArtifactClass evolution: any addition/removal/reorder requires simultaneous updates to program_wire.rs, labcolors-wasm boundary, AND all round-trip test fixtures. A version-tagged envelope (even minimal: magic bytes + version u8) would decouple wire format evolution from enum evolution. Not addressed in plan.

2. **Dead code activation lacks integration proof.** G1 acceptance criteria verify CI green + FLOOR non-regression. But activated types were previously suppressed precisely because they lacked consumers. Activation without demonstrating end-to-end utility (type appears in audit output, consumed by downstream stage) risks promoting speculative code from "staged" to "live but still unused" — just without the suppression marker. The sweep cleans the marker, not the underlying question of whether the type belongs.

3. **Branch hygiene goal is stale.** r8-branch-audit.md proves the 47-branch problem no longer exists. Keeping this goal in the plan with its original acceptance criteria creates a phantom objective that will trivially pass (all criteria already met) while consuming review attention. This is noise that obscures real work.

### Missed Alternatives

1. **G1: Batch disposition document before code changes.** Instead of activating/pruning types one-by-one with atomic PRs, produce a single disposition document (like r7-artifact-class-characterization.md) covering all 30 suppressions with ACTIVATED/PRUNED/RETAINED decisions and rationale. Then execute in bulk. This separates judgment from mechanics, enables batch review of decisions, and reduces PR count from potentially 30 to 1-3. Current approach optimizes for safe revert at the cost of decision coherence.

2. **G3+G4: Combined enum-boundary PR.** Since G3 (remove GraphArtifactTest) directly affects G4 (WASM serialization matrix), merging them into a single atomic change eliminates the intermediate state where discriminant 13 is removed from Rust but WASM boundary still expects 14 classes. The current sequential approach with acknowledged dependency is correct in ordering but creates a window where the two representations are inconsistent. A combined PR with unified round-trip test would close this gap.

3. **G4: Property-based testing framework vs. hand-written round-trip.** The plan specifies "round-trip property test" but does not name the framework or generation strategy. For 12 artifact classes with varying internal structure, hand-writing serialize/deserialize equality checks is error-prone. Using a property-based framework (proptest/quickcheck) with arbitrary instance generation would provide stronger coverage and serve as living documentation of each class's serializable shape. This alternative is cheaper long-term and catches edge cases that example-based tests miss.

## Summary

| Axis | Verdict | Critical Findings |
|------|---------|-------------------|
| Plan-Contract | FAIL | 9 of 12 SPEC §4 sections missing or non-conformant. Document is a task breakdown, not a compilable plan per SPEC. Requires restructuring into single objective + DAG + structured sections before ACTIVE switch |
| Future-Axis | CONDITIONAL PASS | G2 is stale (work already done). G3→G4 discriminant instability unaddressed. WASM wire versioning gap creates long-term evolution constraint. Three missed alternatives identified that could reduce risk and effort |

## Recommendations

1. **Restructure to SPEC compliance** before ACTIVE switch. Single objective, DAG with node IDs, all mandatory sections.
2. **Mark G2 as DONE** with exit evidence = r8-branch-audit.md, or remove from scope entirely.
3. **Add ASM-NN for key assumptions**: ASM-01 "No external consumers of ArtifactClass enum outside Labpics-Team/lab-colors"; ASM-02 "WASM boundary discriminant mapping matches Rust enum ordinals after G3 removal"; ASM-03 "All 30 dead_code suppressions have explicit R-XX staging comments enabling mechanical disposition".
4. **Add INV-NN for track invariants**: INV-01 "FLOOR >= 1410 at all times"; INV-02 "ArtifactClass discriminant mapping consistent between native and WASM boundary"; INV-03 "No #[allow(dead_code)] without RETAINED disposition comment".
5. **Address WASM wire versioning** in G4 scope or explicitly defer as known debt with CAPA entry.
6. **Consider combining G3+G4** into single atomic change to eliminate discriminant inconsistency window.