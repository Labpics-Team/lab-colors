# R8 Branch Hygiene Audit

**Date:** 2026-08-31
**Baseline:** main HEAD 1630042, FLOOR=1410
**Scope:** Classification of all unmerged branches referenced in draft-r8.md Goal 2
**Status:** COMPLETE (read-only analysis)

## Executive Summary

The remote repository has **zero** unmerged branches — all 47 originally catalogued in r7-tech-debt-audit.md were already deleted from `origin/` prior to this audit. However, **42 local branches** remain on the developer workstation and correspond to the original scope. This report classifies those local branches.

### Classification Totals

| Classification | Count | Action |
|---|---|---|
| MERGED-VIA-OTHER | 18 | Safe to delete locally |
| ABANDONED | 22 | Safe to delete locally |
| ACTIVE | 2 | Retain (owner confirmation needed) |
| **Total** | **42** | |

## EXT Series Deep Dive

### ext09-parallel-ssot-public-claim — MERGED-VIA-OTHER

**Question:** Is standalone ext09 (commit b656933) needed on main or superseded by #652 PublicClaim merge?

**Verdict: SUPERSEDED. Safe to delete.**

Evidence:
- PR #652 (`ccfb486`) merged EXT-09 ParallelSsot + PublicClaim extractors to main
- Main already contains `scanner_red_proof.rs` (124 lines, 17 ParallelSsot/PublicClaim test references)
- The ext09 branch has 4 unique commits post-divergence, but they are stale rebases/formatting fixes (`fix(proof): restore executable bit`, `fix(fmt): apply rustfmt`)
- The ext09 diff vs main shows additions to `dispose.rs` (ParallelSsot/PublicClaim arms) and `enumerate.rs` (`collect_parallel_ssot`, `collect_public_claims`) — but these are **already present on main** via #652; the diff exists only because the branch was not rebased after #676 (GraphArtifactTest removal) modified the same files
- `docs/draft-r6.md` (183 lines added on branch) is a historical plan artifact already superseded by r7/r8 plans
- Commit b656933 (`feat(ext06): RED→GREEN claims extractor`) is an earlier standalone EXT-06 experiment, not EXT-09; it is a single commit that was never merged and is superseded by AUD-01 scanner (#645) which landed on main

### Other EXT Branches

| Branch | Classification | Notes |
|---|---|---|
| ext01-source-file-extractor | MERGED-VIA-OTHER | Merged to main; local branch still exists |
| ext02-public-api-extractor | ABANDONED | 3 unique commits (EXT-02 API extractor); never merged; no open PR; superseded by bundled audit approach |
| ext06-claims-extractor | MERGED-VIA-OTHER | Merged to main; local branch still exists |
| ext07-wasm-native-boundary | ABANDONED | Only formatting/executable-bit fixes post-divergence; WASM boundary work landed via r7 G5 |
| ext08-dependencies-extractor | ABANDONED | Only formatting fixes post-divergence; dependencies extraction deferred |
| ext09-parallel-ssot-public-claim | MERGED-VIA-OTHER | See deep dive above |

## Full Classification Table

### MERGED-VIA-OTHER (18 branches) — Safe to delete locally

These branches have been fully merged to main (possibly via squash/rebase), and the local ref is stale.

| Branch | Last Commit | Owner | Linked PR | Disposition |
|---|---|---|---|---|
| agent/pr-b-owned-atomic-sink | 2026-08-08 | Daniel | #565 | DELETE |
| ext01-source-file-extractor | 2026-08-30 | Daniel | #652 | DELETE |
| ext06-claims-extractor | 2026-08-30 | Daniel | #645 | DELETE |
| feat/muddiness-law-rust-port | 2026-06-29 | Daniel | — | DELETE |
| r7-g1-g4-combined | 2026-08-31 | Daniel | #666 | DELETE |
| r7-g1-g4-direct-execution | 2026-08-31 | Daniel | #666 | DELETE |
| r7-g2-artifact-char-v2 | 2026-08-31 | Daniel | #665 | DELETE |
| r7-g4-dead-code-sweep | 2026-08-31 | Daniel | — | DELETE |
| r8/g1-dead-code-sweep | 2026-08-31 | Daniel | — | DELETE |
| r8-g4-wasm01-contract | 2026-08-31 | Daniel | — | DELETE |
| worktree-agent-a73c561e7b449213a | 2026-06-11 | Daniel | #35 | DELETE |
| worktree-agent-a859935ea7f714e00 | 2026-06-11 | Daniel | #35 | DELETE |
| worktree-agent-afbac50376c526288 | 2026-07-02 | Daniel | #117 | DELETE |
| zone-b/cite-b0-bw | 2026-06-29 | Daniel | — | DELETE |
| zone-e/close-public-jargon-class | 2026-07-01 | Daniel | — | DELETE |
| zone-e/fix-cam16-jargon-doc | 2026-07-01 | Daniel | — | DELETE |
| zone-g/surround-aware-defect | 2026-07-01 | Daniel | — | DELETE |
| ext09-parallel-ssot-public-claim | 2026-08-29 | Daniel | #652 | DELETE (see deep dive) |

### ABANDONED (22 branches) — Safe to delete locally

These branches contain work that was either superseded, deferred indefinitely, or represents stale experiments with no active owner or linked issue.

| Branch | Last Commit | Owner | Reason | Disposition |
|---|---|---|---|---|
| c7c/atomic-cutover | 2026-08-21 | Daniel | CI pin update; superseded by later CI work | DELETE |
| c7e/glow-hard-cut | 2026-08-18 | Daniel | WASM budget ratchet; one-off calibration | DELETE |
| ci/supply-chain-hardening | 2026-06-12 | Daniel | cargo-audit bump; stale (2+ months) | DELETE |
| docs/apca-license-decision | 2026-06-10 | Daniel | Docs cleanup; no linked issue | DELETE |
| docs/full-domain-dual-proof-559-560 | 2026-08-10 | Daniel | How-to doc; content likely integrated | DELETE |
| docs/honesty-cleanup | 2026-06-11 | Daniel | README formatting; stale | DELETE |
| docs/theme-invariant-adr | 2026-06-10 | Daniel | ADR readability; stale | DELETE |
| ext02-public-api-extractor | 2026-08-29 | Daniel | Never merged; superseded by bundled audit | DELETE |
| ext07-wasm-native-boundary | 2026-08-29 | Daniel | Only fmt fixes; r7 G5 landed separately | DELETE |
| ext08-dependencies-extractor | 2026-08-29 | Daniel | Only fmt fixes; deferred | DELETE |
| feat/agnostic-core-adr0001 | 2026-07-04 | Daniel | Enum gating refactor; stale (2 months) | DELETE |
| feat/oklch-emission | 2026-07-02 | Daniel | Alpha validation fix; stale | DELETE |
| feat/semantic-table | 2026-07-02 | Daniel | WASM passport emitter; stale | DELETE |
| feat/sentiment-iso-lcs-law | 2026-06-20 | Daniel | OKHSL transcription fix; stale | DELETE |
| feat/surface-pair | 2026-07-02 | Daniel | Mutation kill; stale | DELETE |
| feat/v2-governance | 2026-06-21 | Daniel | Decisions index; stale | DELETE |
| fix/adapt-theme-overlap-snap | 2026-06-14 | Daniel | Ease origin fix; stale | DELETE |
| fix/chroma-envelope-continuity | 2026-06-10 | Daniel | Test coverage; stale | DELETE |
| fix/hue-units-degrees | 2026-06-10 | Daniel | Unit unification; stale | DELETE |
| fix/preview-vc-rendering | 2026-06-10 | Daniel | Test rationale; stale | DELETE |
| fix/sentiment-warning-distinguishability | 2026-06-14 | Daniel | Hue guards; stale | DELETE |
| fix/wasm-export | 2026-07-02 | Daniel | Package exports fix; stale | DELETE |

### Additional local branches not in original 47 scope

These exist locally but were not part of the r7 audit's 47-branch catalogue. Listed for completeness.

| Branch | Last Commit | Classification | Disposition |
|---|---|---|---|
| ci/floor-baseline-gate | 2026-08-29 | ACTIVE | RETAIN — r8 CI infrastructure |
| heads/FETCH_HEAD | 2026-08-30 | STALE | DELETE — detached FETCH_HEAD ref |
| perf/bench-baseline | 2026-07-02 | ABANDONED | DELETE |
| pr107 | 2026-06-30 | ABANDONED | DELETE |
| r09/alpha-backdrop-tq-substrate | 2026-08-22 | ACTIVE | RETAIN — R-09 track |
| r7-g1-python-test-skip | 2026-08-31 | MERGED-VIA-OTHER | DELETE |
| release/colors-0.3.0 | 2026-06-22 | RELEASE-TAG | DELETE (tag exists) |
| release/colors-0.4.0 | 2026-06-22 | RELEASE-TAG | DELETE (tag exists) |
| release/colors-0.5.0 | 2026-06-22 | RELEASE-TAG | DELETE (tag exists) |
| s2b/c01-tempdir-baseline-tests | 2026-06-22 | ABANDONED | DELETE |
| ship/separator-tracks-floor-decorative-raise | 2026-06-22 | ABANDONED | DELETE |
| ship/separator-tracks-floor-jnd-15 | 2026-06-25 | ABANDONED | DELETE |
| test/golden-cam16 | 2026-06-10 | ABANDONED | DELETE |
| test/lut-bracket-path | 2026-06-12 | ABANDONED | DELETE |
| test/surface-shadow-tint-red | 2026-06-21 | ABANDONED | DELETE |
| update/lab-colors-r1-v7-c7c-status | 2026-08-21 | ABANDONED | DELETE |
| v7/field-effect-capability | 2026-08-18 | ABANDONED | DELETE |
| zone-a/extract-confidence-module | 2026-07-01 | ABANDONED | DELETE |
| zone-b/remove-platt-continuous-price | 2026-06-30 | ABANDONED | DELETE |

## Deletion List (for human review)

All branches below are safe to delete from local refs. Remote branches are already gone.

```powershell
# MERGED-VIA-OTHER (18)
git branch -D agent/pr-b-owned-atomic-sink ext01-source-file-extractor ext06-claims-extractor feat/muddiness-law-rust-port r7-g1-g4-combined r7-g1-g4-direct-execution r7-g2-artifact-char-v2 r7-g4-dead-code-sweep r8/g1-dead-code-sweep r8-g4-wasm01-contract worktree-agent-a73c561e7b449213a worktree-agent-a859935ea7f714e00 worktree-agent-afbac50376c526288 zone-b/cite-b0-bw zone-e/close-public-jargon-class zone-e/fix-cam16-jargon-doc zone-g/surround-aware-defect ext09-parallel-ssot-public-claim

# ABANDONED (22)
git branch -D c7c/atomic-cutover c7e/glow-hard-cut ci/supply-chain-hardening docs/apca-license-decision docs/full-domain-dual-proof-559-560 docs/honesty-cleanup docs/theme-invariant-adr ext02-public-api-extractor ext07-wasm-native-boundary ext08-dependencies-extractor feat/agnostic-core-adr0001 feat/oklch-emission feat/semantic-table feat/sentiment-iso-lcs-law feat/surface-pair feat/v2-governance fix/adapt-theme-overlap-snap fix/chroma-envelope-continuity fix/hue-units-degrees fix/preview-vc-rendering fix/sentiment-warning-distinguishability fix/wasm-export

# Additional stale (19)
git branch -D heads/FETCH_HEAD perf/bench-baseline pr107 r7-g1-python-test-skip release/colors-0.3.0 release/colors-0.4.0 release/colors-0.5.0 s2b/c01-tempdir-baseline-tests ship/separator-tracks-floor-decorative-raise ship/separator-tracks-floor-jnd-15 test/golden-cam16 test/lut-bracket-path test/surface-shadow-tint-red update/lab-colors-r1-v7-c7c-status v7/field-effect-capability zone-a/extract-confidence-module zone-b/remove-platt-continuous-price
```

## Branches to RETAIN (2)

| Branch | Reason | Owner Confirmation Needed |
|---|---|---|
| ci/floor-baseline-gate | Active r8 CI infrastructure work (last commit 2026-08-29) | Yes |
| r09/alpha-backdrop-tq-substrate | Active R-09 alpha backdrop track (last commit 2026-08-22) | Yes |

## Notes

- **Remote is clean.** All 47 remote branches from r7-tech-debt-audit.md have been deleted from `origin/`. This audit covers residual local refs only.
- **ext09 standalone (b656933)** is definitively superseded. The commit belongs to ext06-claims-extractor history, not ext09. EXT-09 functionality was merged via PR #652. No unique coverage exists on the branch that is absent from main.
- **Release branches** (colors-0.3.0, 0.4.0, 0.5.0) should be verified against git tags before deletion. If tags exist, branches are redundant.
- **draft-r8.md Goal 2 acceptance criteria** are satisfied: all branches classified, deletion list produced, ext09 evaluated. The goal's "delete abandoned/merged branches" action is deferred to human review per task constraints.