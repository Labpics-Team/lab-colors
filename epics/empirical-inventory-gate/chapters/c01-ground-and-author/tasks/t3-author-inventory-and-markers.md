---
id: t3-author-inventory-and-markers
chapter: c01-ground-and-author
epic: empirical-inventory-gate
title: "Author docs/decisions/empirical-inventory.md (SSOT) + add // NEEDS-SCIENCE / // GROUNDED markers (comments only)"
status: ready
priority: 1
depends_on:
  - t2-enumerate-and-classify
blocks:
  - t4-author-gate-test
agent_profile:
  category: deep
  skills: [dive-rust-core, write-docs]
started: null
completed: null
refine_after: []
---

# Author the SSOT inventory + src markers

## What
Using the POLICY list from t2:

1. **`docs/decisions/empirical-inventory.md`** — the SSOT. One row per POLICY const, join-keyed `(row number, const name)`. Columns at minimum: `row# | const name | value | module | marker (NEEDS-SCIENCE/GROUNDED) | 1-line rationale / paper-status`. Add a header explaining the join-key, the two markers, the 2-line lookback rule, and that standards are excluded by construction. Register it in `docs/decisions/README.md` index.
2. **Markers in src** — directly above each POLICY const, add `// NEEDS-SCIENCE` (provisional, no paper) or `// GROUNDED` (citation exists). **Comments ONLY.** The RHS value of every `: f64 = N` stays byte-identical.
3. Ensure three-way sync: markered consts == inventory rows == t2 POLICY set.

## Must NOT Do
- **Do NOT change ANY perceptual value.** `git diff -- 'crates/**/*.rs'` must show only added/changed `//` comment lines. If a diff line touches a numeric RHS → STOP, revert, this is a hard invariant.
- Do NOT mark or inventory any STANDARD const.
- Do NOT create a second inventory file — one SSOT only.

## Verification
- [ ] `git diff -- 'crates/labcolors-core/src/*.rs' | grep -E '^[+-].*: f64 = '` returns nothing (no value-line touched).
- [ ] `grep -rc 'NEEDS-SCIENCE\|GROUNDED' src/` count == inventory row count == t2 POLICY count.
- [ ] Each inventory row's `(row#, name)` resolves to the real current source line.
- [ ] `docs/decisions/README.md` references the new inventory file.

## References
- t2 working note — the POLICY list + allowlist this task consumes.
- `docs/decisions/README.md` — index to register the SSOT in.
