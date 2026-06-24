---
id: floor-and-separator-pin
epic: jnd-floor-and-separator-pin
title: "Set DECORATIVE_FLOOR_MIN = 15.0, separator tracks it, grounded inventory marker"
user_story: "As a designer using @labpics/colors, I want hairline separators and decorative roles to clear the Lc15 thin-line discernibility floor, so that borders are never sub-perceptual (invisible-for-many-users) on any background."
enabler_for: ""
status: ready
priority: 1
depends_on: []
refine_after: []
parallel_group: ""
agent_profile:
  category: deep
  skills: [dive-rust, dive-rust-core, craft-arch]
---

# Set DECORATIVE_FLOOR_MIN = 15.0, separator tracks it, grounded inventory marker

## User Story
As a designer using `@labpics/colors`, I want hairline separators and decorative roles to clear the **Lc 15** thin-line discernibility floor (the APCA-author "point of invisibility for thin lines/borders"), so that a separator is never emitted sub-perceptual on any background. (P7 — INVEST; benefit is real and user-facing: visible-just-enough hairlines, never invisible.)

## Purpose
Ratify the already-sourced Lc15 decorative floor. `DECORATIVE_FLOOR_MIN` (semantic.rs:152) goes 7.6 -> 15.0. The separator literal `decorative(8.0)` (semantic.rs:1008) is raised to track the floor (>=15.0), so the source states the contract honestly rather than relying on the silent `.max()` clamp. The const carries a GROUNDED inventory marker citing `docs/decisions/surface-jnd.md` (§1b/§2). Shadow rungs are NOT touched (surface-jnd §3 HARD BLOCKER). Edit surface is semantic.rs only.

## Tasks
| Task | Status | Agent | Depends On |
|------|--------|-------|------------|
| `tasks/raise-floor-const.md` | ready | deep | — |
| `tasks/separator-tracks-floor.md` | ready | deep | raise-floor-const |
| `tasks/grounded-inventory-marker.md` | ready | deep | raise-floor-const |
| `tasks/regenerate-goldens.md` | ready | deep | separator-tracks-floor |

## Exit Criteria
These ARE the acceptance criteria — Given/When/Then.
- [ ] Given the source, When inspected, Then `DECORATIVE_FLOOR_MIN == 15.0` at semantic.rs:152 (the continuous Lc15 thin-line target).
- [ ] Given the separator role, When resolved on every golden background, Then its literal magnitude reads `>= 15.0` in source AND the emitted separator |Lc| is `>= 15.0` continuous — no sub-floor literal masked by `.max()`.
- [ ] Given the const, When read, Then it carries a doc-comment inventory marker that cites `docs/decisions/surface-jnd.md` (§1b Lc15 thin-line floor) AND separately states the engine emission cliff is the distinct `(LO_CLIP - offset)*LC_SCALE = 7.30` quantity (un-conflated, per surface-jnd §4).
- [ ] Given shadow rungs, When the diff is reviewed, Then `SHADOW_*_JND` literals are UNCHANGED (HARD BLOCKER respected) and any sub-floor-clamp on them is explicitly flagged in a code comment, not silently ratified.
- [ ] Given the golden snapshot table, When tests run, Then regenerated `separator` rows reflect the raised floor and `cargo test -p labcolors-core` is GREEN.
- [ ] Given the whole diff, When file paths are listed, Then only `crates/labcolors-core/src/semantic.rs` (+ the epic docs) changed — sentiment/`S_PERC_MIN`/`RoleChroma`, solve.rs, neutral.rs, lpc.rs untouched.
