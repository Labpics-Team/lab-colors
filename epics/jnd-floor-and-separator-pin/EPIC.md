---
id: jnd-floor-and-separator-pin
title: "Ratify DECORATIVE_FLOOR_MIN to the Lc15 thin-line floor and lock a quantization-robust floor invariant on every background"
status: active
priority: 1
created: 2026-06-23
goal: "Set DECORATIVE_FLOOR_MIN to the Lc15 ratified floor, make the separator track it, and prove the floor HOLDS after 8-bit quantization on every background."
success_criteria:
  - "DECORATIVE_FLOOR_MIN == 15.0 and the separator literal tracks the floor (no silent .max() clamp masking a sub-floor literal)."
  - "A property test sweeps EVERY 8-bit background and FAILS when a continuous-Lc-15 target quantizes below 15 on any background (the #080808 -> continuous Lc 14.955 cliff is covered)."
  - "The constant carries a GROUNDED inventory marker citing docs/decisions/surface-jnd.md."
  - "File surface stays disjoint from the sentiment/S_PERC_MIN/RoleChroma region; gh pr checks all-green on pinned CI; heterogeneous external review with no unaddressed Critical/High; clean squash merge."
depends_on: []
---

# Ratify the Lc15 decorative floor + lock the quantization-robust floor invariant

## Goal
Set `DECORATIVE_FLOOR_MIN` to the Lc15 ratified floor, make the separator track it, and prove the floor holds AFTER 8-bit quantization on every background.

## Success Criteria
- [ ] `DECORATIVE_FLOOR_MIN == 15.0`; separator literal tracks the floor; no sub-floor literal hidden behind `.max()`.
- [ ] Property test sweeps every background; a continuous-Lc-15 target that quantizes below 15 on any background is a FAILURE (RED-proven).
- [ ] Constant carries a GROUNDED inventory marker citing `docs/decisions/surface-jnd.md`.
- [ ] `gh pr checks` all-green on pinned CI; external review clean; clean `--squash` merge.

## Class of problem closed
**Quantization-cliff masking of a perceptual floor.** A floor written as a *continuous* Lc threshold (15.0) is NOT the same guarantee as "the emitted, on-grid, 8-bit-quantized colour clears 15.0 on every background." `#080808 -> continuous Lc 14.955` is the witness: a continuous target rounds DOWN through the floor after the 256-code lattice snap. The class is "every continuous-target floor that can quantize below itself on some background" — closed by a property/sweep invariant, not by a single fixed input.

## Layers (Clean / DDD — dependency direction)
```
Layer 0  spaces/* (cam16, srgb, oklab)      <- pure colour science, no deps          [DO NOT TOUCH]
Layer 1  lpc.rs / solve.rs                  <- contrast kernel + grey-axis solver     [DO NOT TOUCH]
Layer 2  semantic.rs                        <- role table + DECORATIVE_FLOOR_MIN +     [THE ONLY EDIT SURFACE]
                                               Role::Separator decorative magnitude
         semantic.rs #[cfg(test)]           <- the quantization-robust property test   [NEW TEST, same file/sibling]
```
Dependency direction is inward-only: semantic.rs depends on solve/lpc; nothing in solve/lpc/spaces depends on the floor constant. Raising the floor is a Layer-2 policy change with zero reach into Layers 0-1.

## Blast radius (Hyrum)
- **`DECORATIVE_FLOOR_MIN` (semantic.rs:152, NOT 153)** — read by `decorative_contract` (`semantic.rs:1166`, `.max(DECORATIVE_FLOOR_MIN)`) and by the existing floor test (`~:2317`). Raising 7.6 -> 15.0 raises the clamp for the separator + shadow stack. Shadow rung *literals* (8.0/9.5/11.5/14.0, `:200-203`) are BELOW 15.0 today; under the new floor they would be silently clamped to 15.0, collapsing their strict order. **This is the principal blast hazard** — see Notes.
- **`Role::Separator` decorative magnitude (semantic.rs:1008, NOT 1078)** — `(Role::Separator, decorative(8.0))`. Brief's "1078" is a stale line number; 1078 is an unrelated match arm in `Resolved::solved()`. The real separator literal is line 1008. Raising it to track the floor changes the emitted separator hex in the golden table (`:3208+`).
- **Golden snapshot rows** (`:3184-3428`) for `separator` on every background WILL change — expected, must be regenerated as part of the diff, not fought.
- **Out of blast radius (MUST NOT TOUCH):** sentiment / `S_PERC_MIN` / `RoleChroma` region, `solve.rs`, `neutral.rs`, `lpc.rs`, `spaces/*`. PR #100 (the prior attempt) touched solve.rs + neutral.rs — this epic deliberately does NOT, to keep the surface disjoint from `sentiment-and-tint-laws-ground`.

## Chapters
| Chapter | Status | Priority |
|---------|--------|----------|
| `chapters/floor-and-separator-pin/` | ready | 1 |
| `chapters/quantization-robust-invariant/` | ready | 1 |
| `chapters/review-and-merge/` | ready | 2 |

## Backlog
Deferred owner requests live in the sibling `BACKLOG.md`. Drain `[OPEN]` items each grounding/NextWork cycle and on resume, anchored to their sacred Verbatim quote, before declaring the epic complete.

## Notes — prior art, hazards, decisions (READ before executing)
- **Three competing DRAFT PRs already exist** for this scope. Reconcile, do NOT add a fourth blindly:
  - **#100** `ship/separator-tracks-floor-jnd-15` — closest to brief, ALL CI GREEN, but touches `solve.rs` + `neutral.rs` (wider than the scope's disjoint-surface mandate). Decision: harvest its semantic.rs changes; reject its solve.rs/neutral.rs reach unless an executor proves they are load-bearing for the floor (flag, do not silently absorb).
  - **#99** `7.6->15.5` + lifts shadow consts + provenance gate — does MORE than scope (shadow consts are a HARD BLOCKER per surface-jnd §3; 15.5 != the sourced 15.0). REJECT the shadow lift; it violates surface-jnd.
  - **#98** OPEN `empirical-inventory R4 gate` — owns the GROUNDED inventory-marker machinery (`tests/empirical_inventory.rs`, `docs/decisions/empirical-inventory.md`). The exit-criterion "constant carries a GROUNDED inventory marker citing surface-jnd.md" can be satisfied by a self-contained grounded doc-comment marker on the const (minimal diff). If the executor rebases onto #98's gate instead, that is a dependency to declare, not a silent merge.
  - Pre-existing pending task **#10** "CoVe audit: PR #99 separator floor verification" — fold its verification into chapter `review-and-merge` (#99's approach is partly out-of-scope; verify against THIS epic's exit-criteria, not #99's).
- **surface-jnd.md is the authoritative source.** §1b/§2: thin-line/hairline floor = **Lc 15** (two APCA-author primary sources). §3: shadow ramp is a **HARD BLOCKER** — do NOT lift/ratify shadow rung values in this epic.
- **The separator literal must be RAISED, not left at 8.0 to be clamped.** `decorative_contract` clamps `magnitude.max(DECORATIVE_FLOOR_MIN)`, so leaving `decorative(8.0)` would still emit >=15 — but that hides a sub-floor literal behind the clamp, which is exactly the conflation surface-jnd §4 tells us to remove. The literal must read >=15 so the source states the contract honestly and `provisional_magnitudes_drive_the_decorative_result` stays meaningful.
- **The const-assertion hazard:** if a compile-time assertion exists/added requiring every named decorative magnitude `>= DECORATIVE_FLOOR_MIN` (surface-jnd §7.1), raising the floor to 15.0 makes the shadow literals (8.0..14.0) FAIL TO COMPILE. Resolution within scope: this epic does not add that const-assertion over the shadow rungs (shadows are blocked); it scopes the assertion to the separator only, OR documents the shadow exemption. Flag if a pre-existing assertion already covers shadows.
