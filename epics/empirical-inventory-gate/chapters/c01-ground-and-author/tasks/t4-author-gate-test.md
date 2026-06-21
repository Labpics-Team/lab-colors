---
id: t4-author-gate-test
chapter: c01-ground-and-author
epic: empirical-inventory-gate
title: "Author crates/labcolors-core/tests/empirical_inventory.rs (pure std) — GATE 1/2/3 + join-key sanity, GREEN on synced tree"
status: ready
priority: 1
depends_on:
  - t3-author-inventory-and-markers
blocks: []
agent_profile:
  category: deep
  skills: [dive-rust-core, craft-qa]
started: null
completed: null
refine_after: []
---

# Author the gate test

## What
Write `crates/labcolors-core/tests/empirical_inventory.rs` — **pure std, zero new deps** (labcolors-core stays zero-dep, issue #29). The test reads `src/*.rs` of the 6 perceptual modules as text and reads `docs/decisions/empirical-inventory.md`, then asserts:

- **GATE 1 — untracked-const (differential, class=differential):** every detected POLICY const/Default literal (const/Default sites only, allowlist subtracted) has a marker within a 2-line lookback. A const without a marker → RED, naming the const. Closes the CLASS "new magic number lands with no paper-trail."
- **GATE 2 — allowlist-subset (property, class=property):** every inventory row's const still exists at a resolvable `(row#, name)` and every markered const has a row. Catches STALE rows after reformat/line-drift; survives reformatting because it keys on `(row#, name)` not raw byte offset.
- **GATE 3 — unmarked-provisional (contract, class=contract):** every const marked `// NEEDS-SCIENCE` has a row whose status is provisional (and vice versa) — the marker/inventory contract holds.
- **join-key sanity:** `(row#, name)` pairs in the inventory are unique and resolve; no duplicate keys.
- **Standards-exclusion invariant (enforced by construction):** the detector is scoped to const/Default sites and subtracts the `numeric_method` allowlist, so no standard can be flagged or required-as-a-row. Add an explicit assertion that none of the known standard names (Hellwig/APCA/UCS/L_A=64/Yb=20/D65) appear as inventory rows.

The hermetic RED-proof (`red_proof_audit_probe`) and optional legs (`no_value_drift_diff_check` shelling `git diff`, `ci_full_gate`) are authored in C02. This task lands GATE 1/2/3 + join-key GREEN against the synced tree.

## Must NOT Do
- Do NOT add any dependency to `labcolors-core` — pure std only.
- Do NOT make the detector scan inline fn-body literals (known-limitation, out of scope).
- Do NOT assert any perceptual value's magnitude (that is R3, not R4) — assert presence of marker + row, never the number itself.
- Do NOT leave the test green-from-birth with no real assertion (assertion-free = test theater; RED-proof in C02 will prove it bites).

## Verification
- [ ] `cargo test -p labcolors-core --test empirical_inventory` is GREEN on the synced tree.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass on the new file.
- [ ] Grepping the test shows GATE 1/2/3 + join-key + standards-exclusion assertions present and non-trivial (each has a real `assert!`/`panic!` path that names the offending const).
- [ ] No new entry in any `Cargo.toml [dependencies]`.

## References
- t3 inventory + markers — the data the gate validates.
- `crates/labcolors-core/tests/{hue_sweep,symmetry,continuity}.rs` — existing pure-std test style to mirror.
- `crates/labcolors-core/src/golden_tests.rs` — R1 reference style (for contrast: gate is R4, NOT a golden).
