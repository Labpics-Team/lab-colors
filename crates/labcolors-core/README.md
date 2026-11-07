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

Точность относится к объявленной операции encoded-sRGB8. Она не удостоверяет
неизвестный рендерер, дисплей или пространственное поле размытия.
[Conformance-пакет](../../conformance/README.md) различает точный композит,
численный допуск измерений и квантованный результат решателя.

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

`issue_source_certificate_v1()` создаёт envelope без caller-owned payload или
identity. Источник — встроенный descriptor проверенного Git-дерева
`crates/labcolors-core`. `producer_revision` содержит полный tree object ID;
content identity адресует descriptor. Это идентичность исходного артефакта Core,
не hash исполняемого файла, remote authentication или научное доказательство.
[Точный профиль и условия сборки](contracts/certificate-source-tree-v1.md)
определяют bytes, область identity и отказ при недоступном источнике.

`SourceCertificateV1` предоставляет canonical bytes через `as_bytes()` и
fallible `try_to_bytes()`, а Rust consumer заимствует private capability через
`attestation()`. Он передаёт собственный полный ожидаемый `AdmissionKeyV1`
в `admit`; tuple из недоверенных bytes не заменяет это ожидание.
`CertificateProducerErrorV1::ProducerIdentityUnavailable` означает, что сборка
не получила проверенный descriptor. Остальные операции Core и untrusted decode
при этом доступны; выдуманная revision не подставляется.

`AdmissionStateV1` — caller-owned in-memory single-writer ledger: первый exact
key даёт `Accepted`, повтор тех же bytes — `DuplicateNoop`, другой binding —
`BindingConflict`. Missing/foreign attestation, tuple mismatch, future schema,
unknown selectors, truncation, digest mismatch и исчерпание capacity дают
закрытые typed errors без изменения предыдущего state. Хранение между
процессами, сеть и удалённая аутентификация не входят в этот контракт.
Научная оценка объявленной точки имеет отдельный `point_evaluation`-вход.

## Инструменты разработки

Core не читает исходники, Cargo-манифесты или CI-конфигурацию в runtime.
Файловый аудит выполняют `labcolors-audit` и отдельные Python-инструменты;
[руководство](../../docs/how-to/source-audit.md) описывает их область.
Build-script отдельно формирует descriptor исходного дерева для сертификата.

## Обновление Program без временных входных массивов

`program_wire::ProgramObservationV1::from_rgb8` принимает заимствованные ID
сценариев и плоскую RGB8-матрицу в порядке входов скомпилированной программы.
Проверка формы не выделяет память. `ProgramSessionV1::update_observed_view`
и `ProgramAttachmentV1::update_observed_view` используют тот же admission,
выбор результата и атомарную транзакцию, что методы с owned-сценариями.
WASM и native-адаптеры передают свои входы через этот тип.

```rust
use labcolors_core::program_wire::{
    ProgramObservationV1, ProgramRuntimeErrorV1, ProgramSessionV1, ProgramSnapshotV1,
};

fn update(
    session: &mut ProgramSessionV1,
    revision: u64,
    ids: &[u32],
    rgb8: &[u8],
    surface_count: usize,
) -> Result<ProgramSnapshotV1, ProgramRuntimeErrorV1> {
    let observation = ProgramObservationV1::from_rgb8(ids, rgb8, surface_count)
        .map_err(|_| ProgramRuntimeErrorV1::Update)?;
    session.update_observed_view(revision, observation)
}
```

Принятые значения копируются в хранилище Session до возврата. Входы можно
изменять или освобождать после вызова; ранее возвращённый snapshot остаётся
независимым даже после уничтожения runtime.

В проверяемом прогретом пути с достаточными ёмкостями рабочих буферов
один output хранится прямо в snapshot без аллокаций; несколько outputs
требуют одного точно зарезервированного буфера. Реаллокаций в обоих случаях
нет. Native-вызов с одним output выделяет только `Arc` результата.
Компиляция, создание Session, заполнение ещё не использованных рабочих
слотов и рост кардинальности учитываются отдельно. Защитные JS-копии,
сгенерированный транспорт и native `Arc` результата не входят в эту цифру.
[Измерение границ](../../docs/how-to/benchmark-program.md#выделения-памяти)
проверяет также стоимость адаптера, не скрывая её за показателем Core.
