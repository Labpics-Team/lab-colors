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

## `resolveTheme`: ограниченный и согласованный контрактный кэш

Повторный аудит 2026-07-10 исправил сам сценарий измерения: прежний генератор
использовал `& 0xfff`, поэтому после прогрева повторял 4096 фонов и называл
попадания промахами. Текущий benchmark не повторяет ни одного sRGB8-ключа за
весь запуск, отдельно измеряет чередование двух тем и печатает high-water
линейной памяти WASM.

Одинаковый сценарий на Node v24.15.0, `wasm -Oz`, median после прогрева:

| сценарий | прежний кэш | тематические слоты | изменение |
|---|---:|---:|---:|
| один и тот же ключ | 70,6 µs | 76,3 µs | минимум обоих прогонов ≈67 µs |
| чередование двух тем | 279,1 µs | 81,3 µs | 3,43× быстрее |
| честный уникальный промах | 2,582 ms | 3,052 ms | не относится к ускорению hit-пути |
| память после полного прогона | 68,563 MiB | 1,438 MiB | в 47,7× ниже high-water |

Почему память теперь ограничена доказуемо: вместо `HashMap` на 4096 полных тем
структура содержит по одному именованному слоту на каждый вариант публичного
`Theme`. DTO и его JSON лежат в одном `ResolvedSnapshot`, поэтому попадание
решателя не может одновременно быть промахом сериализации. Поток произвольной
длины удерживает не более четырёх payload; это отдельно проверяется через
`Weak`, а не только через счётчик записей.

Число честного промаха приведено для полноты, но не трактуется как A/B самого
кэша: между сборками менялась математика core, и fingerprint `resolve vars`
изменился (`06ea61dc` → `6f222912`). Recheck-физика осталась байт-идентичной
(`2806bf67` в обоих прогонах). Кэш не должен маскировать стоимость настоящего
solve; его контракт — быстрый точный hit и ограниченная память.
