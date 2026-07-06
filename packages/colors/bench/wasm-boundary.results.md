# JS↔WASM boundary — recheck / resolve cost

Reproduce: `node --expose-gc bench/wasm-boundary.bench.mjs`
Fixture: frozen canonical labui passport, 28 colour-role foregrounds, theme `dark`,
median of batched runs after warmup. Machine: node v24.15.0, Windows, wasm `-Oz`.
`alloc B/call` is a forced-GC heap-retention proxy (under-reports transient garbage).

## Where the per-frame cost lives (profiling split, baseline)

The per-frame primitive is `recheckContrast`. Splitting one ×1 call (28 roles):

| part                                   | ns/call | share |
|----------------------------------------|--------:|------:|
| full recheck                           |  ~47000 |  100% |
| boundary only (marshal + parse + alloc)|   ~2547 |    5% |
| colour math (CAM16 + WCAG transcendentals) | ~44600 | 95% |

Conclusion: the marshaling hypotheses (UTF‑8 re‑encode of the 28 hex strings, the
returned `Float64Array` copy, re‑passing unchanged args) are real but only ~5% of
the cost. 95% is transcendental colour math (~28 CAM16 forwards + the WCAG
re‑encode/re‑linearise per role). The win had to come from cutting `powf`, not
from the boundary.

## Before / after (shipped, byte-identical, no public API change)

| call                          | before ns | after ns | Δ      |
|-------------------------------|----------:|---------:|-------:|
| recheckContrast ×1 (28 roles) |    ~46200 |    36593 | −21%   |
| recheckContrast ×3 (28 roles) |   ~150400 |   110101 | −27%   |

What changed (both provably byte-identical on the 8-bit grid):
1. WCAG display value taken straight from the byte (`byte/255`) instead of
   `quantised_display` — removes the per-channel `srgb_gamma` `powf` (identity
   `byte/255 == quantised_display(decode(byte))`, pinned exhaustively over all 256
   codes).
2. The background's relative luminance is linearised once per call, not
   re-linearised inside every foreground's `contrast_ratio`.

The remaining recheck cost is dominated by the per-foreground CAM16 forward
(`bg_luma`), which is irreducible for a distinct colour within a single call.

## Prototype (NEW API — owner decision, NOT wired to any runtime)

`_recheckContrastMulti(bgHexes, fgHexes, theme)` rechecks one foreground set
against several background samples in one call, sharing each foreground's CAM16
forward across all samples. Byte-identical, pair for pair, to N `recheckContrast`
calls.

| call                        | after ns | ns/role | vs recheck ×3 |
|-----------------------------|---------:|--------:|--------------:|
| recheckContrast ×3 (3 calls)|   110101 |  1310.7 |          1.0x |
| _recheckMulti 3bg (1 call)  |    43634 |   519.5 |          2.5x |

The controller's worst-case backdrop loop (`adaptTheme` with a 1–3 sample gradient
/ image) rechecks the SAME foregrounds against every sample; the CAM16 forward is
background-independent, so batching the samples collapses N−1 of the forwards. A
2.5x win at 3 samples. Adopting it changes the controller↔engine call shape, so it
is a design decision for the package owner — left as an unlisted `_`-prefixed
method, measured only.

## resolveTheme (on-breach re-solve; NOT touched by this work)

Measured separately, unchanged: cache-hit ≈ 657µs, cache-miss ≈ 1303µs. Both are
dominated by the JS-object projection (`project_resolved` builds ~106 role objects
via hundreds of `Reflect.set` FFI calls) — reruns on every call even on a cache
hit. Off the per-frame path (`resolveTheme` fires only on a sustained, debounced
breach), but the projection is the obvious next target if resolve latency ever
matters; flagged for the owner, not addressed here. (Addressed by #54 — next
section.)

## resolveTheme projection (#54, this wave)

Reproduce: `node --expose-gc packages/colors/bench/wasm-boundary.bench.mjs`
(same harness/fixture/machine as above). Re-measured baseline at this branch
point: hit 618619 ns, changing-bg 650531 ns (prior wave quoted ≈657µs/≈1303µs
on an earlier tree state).

| call                                | before ns | after ns | Δ      |
|-------------------------------------|----------:|---------:|-------:|
| resolveTheme cache-hit ×1           |    618619 |    76041 | −87.7% |
| resolveTheme changing-bg ×1         |    650531 |    92053 | −85.8% |
| recheckContrast ×1 (28 roles, ctrl) |     36593 |   ~36600 | noise  |

After-numbers are medians of 3 harness runs (hit 81741/76041/75212, min
69505; changing-bg 145748/86255/92053).

What changed (no public API change, output byte-identical):
1. The projection is built in Rust (`crates/labcolors-wasm/src/project.rs`) as
   one JSON string; the boundary is crossed twice per call (string out + one
   `JSON.parse`) instead of hundreds of `Reflect::set` FFI calls building ~106
   role objects.
2. The JSON text is memoised per live contract-cache entry (`Weak` +
   `Rc::ptr_eq`; memo capacity mirrors the engine cache). A repeat hit
   re-serialises nothing; ABA is impossible because the memo key is the entry
   allocation itself, not the input key.
3. f64s are emitted via ryu shortest round-trip form — the same algorithm
   `serde_json` uses — pinned bit-for-bit by Rust tests.

Byte-identity evidence:
- golden snapshot of the full projection, generated on the pre-optimisation
  baseline (`test/gen-resolve-projection-golden.mjs`), asserted structurally
  after (`test/resolve-projection-parity.test.mjs`); projection fingerprint
  `bf20165b…` stable across runs.
- Rust tests: every emitted f64 survives JSON round-trip bit-for-bit; the
  hand-rolled serialiser output equals `serde_json`'s canonical encoding.
- recheck byte-oracle (`7c072d28…`) untouched and green.

Caveat on the "miss" row: after warmup the changing-bg pool (4096) is resident
in the engine cache, so that row measures projection under a changing key, not
a full solve. A true cold-key solve (12000 unique backgrounds, same probe
style) is ≈4.36 ms median and is dominated by the Rust core solve
(`crates/labcolors-core`) — out of scope for #54 (core owned by a parallel
effort; not touched).
