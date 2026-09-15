# labcolors-core

Компилятор и runtime-resolver контрактов цветовых токенов клиента без
зависимостей.

Клиент владеет идентификаторами токенов, алиасами, иерархией, состояниями
компонентов и семантикой дизайна. Core считает идентификаторы непрозрачными и
владеет математикой: разрешением графа, контрастом, композитингом, адаптацией,
конечным output и численными сертификатами. Физические цвета являются
контекстными результатами, а не исходной схемой.

## Пример точной операции encoded-sRGB8

Объявленная эталонная операция source-over округляет точную половину вверх:

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

Гарантии точного композитинга не распространяются на неизвестный рендерер,
дисплей, пространственное поле размытия или зависящий от платформы профиль Glow
`legacy-platform-dependent-v1`. Версионированные границы описаны в
conformance-пакете и закреплённой коммитом документации релиза.

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

## Fixed certificate transport envelope v1

Модуль `labcolors_core::certificate` владеет только framing и binding
транспортного envelope. Канонические байты имеют positional-порядок `LCEN`,
big-endian length prefixes, полный producer tuple и domain-separated SHA-256
для payload и binding. Длина envelope и payload ограничена до allocation.

`UntrustedEnvelopeV1::decode` проверяет wire и возвращает только untrusted
значение. `CertificateEnvelopeV1` создаётся исключительно из sealed
producer-owned `NonSemanticTransportPayloadV1` и private
`TrustedProducerAttestationV1`; произвольные bytes, `ProgramSnapshot`, Paint,
CSS, DOM и materialization authority не конвертируются в certificate. Payload
остаётся non-semantic и не является authority result.

`AdmissionStateV1` — caller-owned in-memory single-writer ledger: первый exact
key даёт `Accepted`, повтор тех же bytes — `DuplicateNoop`, другой binding —
`BindingConflict`. Missing/foreign attestation, tuple mismatch, future schema,
unknown selectors, truncation, digest mismatch и исчерпание capacity дают
закрытые typed errors без изменения предыдущего state. Persistence, network
relay, remote authentication и science semantics принадлежат будущим ревизиям.
