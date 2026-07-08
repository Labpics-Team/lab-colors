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

## Batch recheck (public API — wired into the controller)

`recheckContrastMulti(bgHexes, fgHexes, theme)` rechecks one foreground set
against several background samples in one call, sharing each foreground's CAM16
forward across all samples. Byte-identical, pair for pair, to N `recheckContrast`
calls.

| call                          | after ns | ns/role | vs recheck ×3 |
|-------------------------------|---------:|--------:|--------------:|
| recheckContrast ×3 (3 calls)  |   110101 |  1310.7 |          1.0x |
| recheckContrastMulti 3bg      |    43634 |   519.5 |          2.5x |

The controller's worst-case backdrop loop (`adaptTheme` with a 1–3 sample gradient
/ image / bg-blur / glass) rechecks the SAME foregrounds against every sample; the
CAM16 forward is background-independent, so batching the samples collapses N−1 of
the forwards — a ~2.5x win at 3 samples. `adaptTheme` now feature-detects the
method and calls it once per multi-sample frame, falling back to the per-sample
`recheckContrast` loop for single samples or engines that do not expose it.

## resolveTheme (on-breach re-solve; NOT touched by this work)

Measured separately, unchanged: cache-hit ≈ 657µs, cache-miss ≈ 1303µs. Both are
dominated by the JS-object projection (`project_resolved` builds ~106 role objects
via hundreds of `Reflect.set` FFI calls) — reruns on every call even on a cache
hit. Off the per-frame path (`resolveTheme` fires only on a sustained, debounced
breach), but the projection is the obvious next target if resolve latency ever
matters; flagged for the owner, not addressed here.
