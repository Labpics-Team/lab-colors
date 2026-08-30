# r7 Tech Debt Audit

**Date:** 2026-08-31
**Baseline:** main HEAD 39add39 (r6 complete, FLOOR=1360)
**Purpose:** Inform REPLAN-01-r7 scope. READ-ONLY analysis.

## Summary

| Category | Count |
|---|---|
| TODO/FIXME/HACK/XXX comments | 1 |
| Ignored tests () | 5 |
|  suppressions | 30 (across 16 files) |
| Failing CI workflows (last 5 runs) | 3 |
| Unmerged remote branches | 47 |

## CI Failures (main)

Last 5 workflow runs on main:

| Run ID | Workflow | Status |
|---|---|---|
| 33340308435 | Native conformance (Swift) | ✅ success |
| 33340308432 | CI | 🔄 in_progress |
| 33340307701 | full-domain-corpus.yml | ❌ failure |
| 33340306868 | verification-lanes.yml | ❌ failure |
| 33340307295 | mutation-worker.yml | ❌ failure |

Job logs unavailable via  (returns "log not found"). Jobs array empty for failed runs — likely expired or permission-restricted artifacts.

**Known pre-existing failures from r6 validation:**
- Python proof test:  — ValueError: substring not found
- Python proof test:  — invalid  argument 'launch'
- EXT-06 standalone claims extractor exists only on  branch (b656933), not merged to main
- EXT-01 delivered within AUD-01 scanner rather than as standalone extractor

## TODO/FIXME/HACK/XXX Comments

Only 1 occurrence in Rust source under :

| File | Line | Comment |
|---|---|---|
|  | 67 |  |

**Assessment:** Minimal marker debt. Single TODO is scoped to R-07 PR-C and tracks a deliberate staging decision.

## Ignored Tests ()

5 ignored test sites across 4 files:

| File | Line | Context |
|---|---|---|
|  | 317 | Reference vector test |
|  | 1237-1240 | Manual-only neutral axis test ("не гейт CI") |
|  | 2613, 2940 | Golden emit / resolve set tests |
|  | 461 | sRGB space test |

**Assessment:** All appear intentionally excluded from CI gate (golden generation, manual verification, expensive reference vectors). None indicate broken tests hidden by ignore.

## Dead Code Suppressions ()

30 occurrences across 16 files. Heaviest concentration:

| File | Count | Notes |
|---|---|---|
|  | 10 | Staged types for R-05/R-06/R-07 (explicit comments) |
|  | 3 | Projection types |
|  | 3 | Test scaffolding |
|  | 2 | P3 color space types |
| Other (12 files) | 1 each | Various staged/test types |

**Assessment:** Majority are explicitly commented as staged for future revisions (R-05 through R-07). This is intentional forward-staging, not accumulated dead code. However, 30 suppressions is a signal — r7 should evaluate which staged types can be activated or pruned.

## Unmerged Branches (47)

### Active feature tracks:
- ,  — R-07 restorative work
-  — R-08 composition proofs
- , ,  — R-09 alpha backdrop (3 branches)
-  — R-10 field technical quality
-  — WASM error mapping
- , , ,  — O-13 arena work (4 branches)
-  — V-12 dual interval
-  — V-17b incremental runtime
-  — F-01 LCS freeze
-  — F-03 evaluator registry
-  — R-01 evaluator wiring
-  — R-02 sentiment
- ,  — R-04 clean potential (2 branches)
-  — R-06 field attachment
-  — Sym series
-  — Graph01

### Extractor branches (EXT series):
- 
- 
- 
- 
- 
- 
- 
- 
- 
- 

### Maintenance/fix branches:
- 
- 
- 
- 
- 
- 
- 
- 
- 

### Open PRs:
- , , , , 

## Recommendations for r7 Scope

1. **CI failures are pre-existing and known.** The 3 failing workflows (full-domain-corpus, verification-lanes, mutation-worker) are documented r6 residuals. r7 should either fix these as explicit nodes or formally defer with exit evidence.

2. **Dead code suppressions warrant a sweep.** 30  markers with explicit R-XX staging comments should be evaluated: activate types whose revision has arrived, prune types whose revision was superseded.

3. **Branch hygiene.** 47 unmerged branches include multiple parallel tracks (R-07 through R-10, O-13, EXT series). r7 planning should identify which branches are live vs. abandoned. The EXT series (9 branches) is particularly relevant given EXT-01/EXT-06 delivery gaps noted in r6.

4. **Ignored tests are intentional.** No action needed unless r7 specifically targets golden/reference vector infrastructure.

5. **Single TODO is tracked.** The R-07 PR-C  marker belongs to the restorative track already in scope.
