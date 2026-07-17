# labcolors-core

Dependency-free compiler and runtime resolver for client-owned colour-token
contracts.

The client owns token identifiers, aliases, hierarchy, component states and
design semantics. The core treats those identifiers as opaque and owns the
mathematics: graph resolution, contrast, compositing, adaptation, finite output
and numerical certificates. Physical colours are contextual results, not the
source schema.

## Exact encoded-sRGB8 example

The declared source-over reference rounds the exact half-tie upward:

```rust
use labcolors_core::alpha::composite_over_srgb8;

# fn main() -> Result<(), String> {
let composite = composite_over_srgb8(
    [0xC0, 0xB2, 0xFA],
    0.122,
    [0x00, 0x00, 0x00],
)?;
assert_eq!(composite, [0x17, 0x16, 0x1F]);
# Ok(())
# }
```

Exact composite guarantees do not extend to an unknown renderer, display,
spatial blur field or the explicit platform-dependent legacy Glow decision.
See the conformance pack and commit-pinned release documentation for versioned
boundary details.

## Точная оценка WCAG 2.2 и независимый оракул

`wcag22::evaluate_wcag22_srgb8` / `evaluate_wcag22_hex` — точная fail-closed
оценка одной финальной пары sRGB8 по явно заявленному критерию; Core не выводит
применимость из ID, роли или типографики. Q55-границы светимости доказаны
закоммиченным артефактом (`contracts/wcag22-srgb8-q55-v1.bin` + proof).

Роли доказательств разделены. Независимый Python-оракул
`scripts/verify_wcag22_neutral_axis.py` пересчитывает решения нейтральной оси
рациональной арифметикой без производственного Q55 и Rust-вычислителя; его
артефакт запинен по SHA-256 и replay-ится через публичный вычислитель в
`tests/wcag22_neutral_axis_replay.rs`. Изменение соседей, критерия либо
домена может изменить множество решений и требует повторного полного
вычисления.
