---
id: t2-enumerate-and-classify
chapter: c01-ground-and-author
epic: empirical-inventory-gate
title: "Enumerate every const/Default f64 literal in the 6 perceptual modules; classify POLICY vs STANDARD"
status: ready
priority: 1
depends_on:
  - t1-branch-and-rebase
blocks:
  - t3-author-inventory-and-markers
agent_profile:
  category: deep
  skills: [dive-rust-core, craft-arch]
started: null
completed: null
refine_after: []
---

# Enumerate & classify the audit surface

## What
Build the authoritative partition that everything downstream keys off. For each of `semantic.rs`, `scale.rs`, `sentiment.rs`, `neutral.rs`, `lpc.rs`, `lcs.rs`:

1. Enumerate every `const IDENT: f64 = <literal>;` and every numeric literal in a `Default` impl / struct-field default.
2. For each, read its doc-comment + usage to classify:
   - **POLICY** (perceptual aesthetic choice, no upstream paper pins it) → goes in inventory, gets a marker. Default marker `// NEEDS-SCIENCE` unless an in-code citation already grounds it, then `// GROUNDED`.
   - **STANDARD** (CIECAM16/UCS/Hellwig/APCA/WCAG/Oklab matrix/D65/Yb=20/L_A=64/derivation-identity/pure numeric EPS) → goes in the `numeric_method` allowlist, gets NO marker, NO row.
3. Produce the `numeric_method` allowlist (~30 entries) of STANDARD const names, and the POLICY list with `(file, line, name, value, one-line reason it is policy)`.

Adjudicate the genuinely ambiguous ones explicitly (e.g. `lpc::EXP_*`, `CONTRAST_SCALE`, `LO_*` are APCA *tuning* — decide policy-vs-standard by whether apca-w3 pins the exact value; if tuned for this engine → POLICY). `S_PERC_MIN` is derivation-identity (R2), exclude from R4. `HK_CHROMA_EXPONENT=0.587` is Hellwig → STANDARD.

Output the partition as a markdown table in the task's working note (feeds t3 + t4 directly).

## Must NOT Do
- Do NOT edit any source yet (markers are t3).
- Do NOT guess a value's provenance — read the doc-comment / cite the paper, or mark it POLICY/NEEDS-SCIENCE (honest default), never silently STANDARD.
- Do NOT include inline fn-body literals (`mp_ref*1.5`, `.powf(0.6)`, inline `sqrt` chroma) as rows — they are the tracked known-limitation, out of scope for the detector.

## Verification
- [ ] Every `const … : f64` in all 6 modules appears exactly once in either the POLICY list or the allowlist (no orphan, no double-count) — checkable by `grep -c`.
- [ ] Each POLICY entry has a one-line justification; each STANDARD entry names its standard/regime.
- [ ] No standard (Hellwig/APCA/UCS/D65/L_A=64/Yb=20/derivation-identity) is in the POLICY list.
- [ ] The POLICY count is recorded (this becomes the expected marker-count and row-count; reconciles the brief's "≈30" against reality).

## References
- `crates/labcolors-core/src/semantic.rs`, `sentiment.rs`, `lpc.rs` — the consts already enumerated in EPIC "Audit surface" table (starting partition, not final).
- EPIC.md "Audit surface" — the live-enumerated starting table.
