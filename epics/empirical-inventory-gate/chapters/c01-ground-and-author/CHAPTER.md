---
id: c01-ground-and-author
epic: empirical-inventory-gate
title: "Branch off main, enumerate the audit surface, author the SSOT inventory + markers + gate"
user_story: "technical enabler"
enabler_for: "The whole epic: a tracked R4 hygiene guard that fails CI when a perceptual-policy literal lacks a paper-trail. (Engineer-facing governance; no end user.)"
status: ready
priority: 1
depends_on: []
refine_after: []
parallel_group: ""
agent_profile:
  category: deep
  skills: [dive-rust-core, craft-qa, craft-arch]
---

# Branch, enumerate, author

## User Story
Technical enabler for: the epic's R4 hygiene guard. No end user — this is engineer-facing governance, so it is honestly labelled an enabler, not a fabricated story.

## Purpose
Establish the worktree on the correct branch (off `feat/v2-governance`, rebased onto current `main` so the gate scans the live tree), then **enumerate the real audit surface** and classify each `const … : f64` / `Default`-field literal in the 6 perceptual modules as POLICY (inventory it) or STANDARD (exclude it). Author the three deliverables so they are mutually consistent by construction: (1) the `// NEEDS-SCIENCE` / `// GROUNDED` markers in src (comments ONLY — zero value changes), (2) the SSOT `docs/decisions/empirical-inventory.md` with one row per policy const keyed `(row#, const name)`, (3) the gate test `crates/labcolors-core/tests/empirical_inventory.rs` (pure std, zero-dep) implementing GATE-1/2/3 + join-key sanity. RED-proof and CI wiring are C02; this chapter ends GREEN with the gate scanning a synced tree.

## Tasks
| Task | Status | Agent | Depends On |
|------|--------|-------|------------|
| `tasks/t1-branch-and-rebase.md` | ready | deep | — |
| `tasks/t2-enumerate-and-classify.md` | ready | deep | t1-branch-and-rebase |
| `tasks/t3-author-inventory-and-markers.md` | ready | deep | t2-enumerate-and-classify |
| `tasks/t4-author-gate-test.md` | ready | deep | t3-author-inventory-and-markers |

## Exit Criteria
- [ ] **Given** the scope's branch requirement, **When** the worktree is prepared, **Then** work sits on a fresh branch off `feat/v2-governance` rebased onto current `main`, `git status` shows ONLY the new gate files + comment-only marker edits, and `git diff` of any `src/*.rs` shows zero changes to the RHS of any `: f64 = N`.
- [ ] **Given** the 6 perceptual modules, **When** the surface is enumerated, **Then** every `const … : f64`/`Default`-field literal is classified POLICY or STANDARD with a one-line justification, and the policy set == the set of inventory rows == the set of markered consts (three-way equal; count reconciled, not assumed to be 30).
- [ ] **Given** the join-key `(row number, const name)`, **When** `docs/decisions/empirical-inventory.md` is authored, **Then** each row's `(row#, name)` resolves to the real current source line and the file is the single SSOT (no duplicate inventories).
- [ ] **Given** the authored gate, **When** `cargo test -p labcolors-core --test empirical_inventory` runs, **Then** it compiles under toolchain 1.96.0, passes `cargo fmt --check` + `cargo clippy -- -D warnings`, and is GREEN against the synced tree (RED-proof is C02, not asserted here).
