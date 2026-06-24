---
id: raise-floor-const
chapter: floor-and-separator-pin
epic: jnd-floor-and-separator-pin
title: "Set DECORATIVE_FLOOR_MIN 7.6 -> 15.0 at semantic.rs:152"
status: ready
priority: 1
depends_on: []
blocks:
  - separator-tracks-floor
  - grounded-inventory-marker
agent_profile:
  category: deep
  skills: [dive-rust, dive-rust-core]
started: null
completed: null
refine_after: []
---

# Set DECORATIVE_FLOOR_MIN 7.6 -> 15.0

## What
Edit the single constant `const DECORATIVE_FLOOR_MIN: f64 = 7.6;` at `crates/labcolors-core/src/semantic.rs:152` to `15.0`. This is the continuous Lc15 thin-line discernibility target sourced in `docs/decisions/surface-jnd.md` §1b (two APCA-author primary sources). The doc comment above it (`:146-152`) currently conflates "reliable emission floor" with "JND" — fix the comment so it states the perceptual floor is Lc 15 (surface-jnd §1b) and that the 7.30 analytic engine cliff is a SEPARATE quantity (surface-jnd §1a/§4). The grounded-inventory-marker task adds the citation; this task fixes the value + de-conflates the prose.

## Must NOT Do
- Do NOT touch `SHADOW_*_JND` (`:200-203`) — HARD BLOCKER (surface-jnd §3).
- Do NOT touch `solve.rs`, `neutral.rs`, `lpc.rs`, `spaces/*`.
- Do NOT use 15.5 or any value other than 15.0 (15.5 is PR #99's unsourced number; the sourced floor is 15.0).
- Do NOT add a compile-time assertion that sweeps the shadow rungs against the new floor (it would make 8.0..14.0 fail to compile; shadows are exempt/blocked).

## Verification
- [ ] `DECORATIVE_FLOOR_MIN == 15.0` (grep confirms single occurrence at :152).
- [ ] `cargo build -p labcolors-core` succeeds (no const-assertion breaks).
- [ ] Doc comment no longer equates the floor with the 7.30 engine cliff.

## References
- `docs/decisions/surface-jnd.md` §1a (7.30 cliff is engine math, not JND), §1b (Lc15 sourced floor), §4 (de-conflate the comment).
- `crates/labcolors-core/src/semantic.rs:146-152` — the const + its doc comment.
- `crates/labcolors-core/src/semantic.rs:1166` — `decorative_contract` clamps `.max(DECORATIVE_FLOOR_MIN)` (consumer).
