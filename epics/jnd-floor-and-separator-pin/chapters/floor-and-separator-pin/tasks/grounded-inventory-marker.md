---
id: grounded-inventory-marker
chapter: floor-and-separator-pin
epic: jnd-floor-and-separator-pin
title: "Attach a GROUNDED inventory marker citing surface-jnd.md to DECORATIVE_FLOOR_MIN"
status: ready
priority: 1
depends_on:
  - raise-floor-const
blocks: []
agent_profile:
  category: deep
  skills: [dive-rust, write-docs]
started: null
completed: null
refine_after: []
---

# Attach a GROUNDED inventory marker to DECORATIVE_FLOOR_MIN

## What
Exit-criterion: "the constant carries a GROUNDED inventory marker citing docs/decisions/surface-jnd.md." Add the marker on `DECORATIVE_FLOOR_MIN`. GROUND the choice first: check whether PR #98 (`empirical-inventory R4 gate`, `tests/empirical_inventory.rs` + `docs/decisions/empirical-inventory.md`) is the canonical marker format in this repo. 
- If the inventory gate is already merged/expected on main: follow ITS marker convention exactly (e.g. the `GROUNDED:` tag the gate parses) and cite `surface-jnd.md`. Declare the dependency on #98 in the PR description.
- If not (minimal-diff path): add a self-contained doc-comment marker that states the verdict (`derived-to-root (engine cliff 7.30) + authoritatively-sourced (Lc15 thin-line, APCA)`) and cites `docs/decisions/surface-jnd.md` §1b/§2. Do NOT invent a gate format the repo's tests don't enforce.

## Must NOT Do
- Do NOT fabricate a marker syntax that no test/tool reads — ground it in the repo's actual convention (read #98 or any existing `GROUNDED:`/inventory comment first).
- Do NOT absorb PR #98's whole gate into this PR — if you depend on it, flag the dependency; keep this PR's diff minimal.

## Verification
- [ ] The const's doc comment cites `docs/decisions/surface-jnd.md` and names the Lc15 thin-line basis.
- [ ] If an inventory-gate test exists on main, it PASSES for this const (the marker is in the format the gate parses).
- [ ] The marker distinguishes the sourced perceptual floor (Lc15) from the engine emission cliff (7.30) — no re-conflation.

## References
- `docs/decisions/surface-jnd.md` §1, §4, §7 (invariants to lock), §8 (verdict table).
- PR #98 / `crates/labcolors-core/tests/empirical_inventory.rs`, `docs/decisions/empirical-inventory.md` — the inventory-gate marker convention (read before choosing format).
