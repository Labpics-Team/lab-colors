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

## Ограниченная проверка выполнимости WCAG 2.2

`wcag22_feasibility` полностью проверяет конечный домен, но не выбирает и не
ранжирует цвета. Клиент объявляет непрозрачные для Core идентификаторы связи и
вхождения, применимый критерий WCAG 2.2 и точные соседние цвета sRGB8. Core
канонизирует декларации, вычисляет каждую пару «кандидат × сосед» и возвращает
один запечатанный терминал: `Feasible`, `Infeasible` либо декларационный
`NotEvaluated`. `Infeasible` означает отсутствие решения только в проверенном
конечном домене; это не утверждение об отсутствии цвета вне него.

Прямой Core по умолчанию включает `wcag22-feasibility` и зависящую от неё
возможность `wcag22-explicit-feasibility`. Базовая зависимость Protocol включает
только neutral-axis feasibility; non-default feature
`wcag22-explicit-selection` добавляет атомарную операцию над явным клиентским
набором. Compiler WASM/npm и UniFFI/Swift включают эту feature и публикуют обе
offline-операции. Все поверхности используют один математический компилятор и
не входят в runtime WASM.

В V1 доступны две формы одного компилятора. Совместимый вход `evaluate`
перечисляет зарегистрированную нейтральную ось: ровно 256 кодов `[v, v, v]`,
где `v` принимает каждое целое значение от 0 до 255. Вход
`explicit::evaluate` принимает непустой клиентский набор пар «непрозрачный ID +
неизменяемый финальный `Srgb8`». Core сортирует точные UTF-8-байты ID, отклоняет
их повторы и сам выводит мощность, digest, матрицу и partition. Разные ID с
одинаковыми физическими байтами остаются разными кандидатами. Ни один из входов
не выводит размер текста, компонентную семантику, применимость или предпочтение
из ID. Feasibility сам ничего не ранжирует; явный клиентский порядок применяется
только отдельным запечатанным selection-шагом после полного доказательства.

Для `C` кандидатов и `E` канонических применимых рёбер выполняется ровно
`W=C×E` атомарных проверок. При `E>0` единственный упакованный буфер имеет
`B=ceil(C×E/8)+ceil(C/8)` байт; при `E=0` он пуст. Эти величины выводятся и
проверяются до выделения памяти. Остальная стоимость не маскируется формулой
`C×E`: отдельно выполняются линейный просмотр деклараций, сортировка кандидатов
и связей сравнением точных байтов ID, сортировка соседей внутри каждой связи и
линейное хеширование канонического результата. Превышение ресурсов,
противоречивая декларация, ошибка выделения памяти и нарушение инварианта
вычислителя или компилятора возвращаются типизированными ошибками; частичного
или запасного результата нет.

Полные проверки формул `C×E`, случая `E=0`, повторов ID, ресурсных отказов и
запрета частичного результата перечислены в разделе «Конечная компиляция
выполнимости WCAG 2.2» [карты верификации](../../docs/verification-map.md).

```rust
# #[cfg(feature = "wcag22-feasibility")]
# fn feasibility_example() -> Result<(), Box<dyn std::error::Error>> {
use labcolors_core::{
    Srgb8,
    wcag22::Wcag22CriterionV1,
    wcag22_feasibility::{
        DomainIdV1, OccurrenceId, RelationId, RelationV1, RequestV1,
        ResourceProfileIdV1, evaluate,
    },
};

let relation = RelationV1::applicable(
    RelationId::try_new("label-on-surface")?,
    OccurrenceId::try_new("button/label")?,
    Wcag22CriterionV1::Sc143TextDefault,
    vec![Srgb8::new([0; 3]), Srgb8::new([255; 3])],
)?;
let request = RequestV1::try_new(
    DomainIdV1::Srgb8NeutralAxis,
    vec![relation],
    ResourceProfileIdV1::Compile,
)?;

let result = evaluate(request)?;
assert!(result.is_feasible());
if let Some(evaluated) = result.evaluated() {
    let candidates: Vec<_> = evaluated
        .feasible_candidates()
        .map(|candidate| candidate.bytes())
        .collect();
    assert_eq!(candidates, vec![[0x75; 3], [0x76; 3]]);
}
# Ok(())
# }
# fn main() {}
```

Клиентский конечный набор использует те же `RelationV1` и атомарный WCAG-путь:

```rust
# #[cfg(feature = "wcag22-explicit-feasibility")]
# fn explicit_feasibility_example() -> Result<(), Box<dyn std::error::Error>> {
use labcolors_core::{
    Srgb8,
    wcag22::Wcag22CriterionV1,
    wcag22_feasibility::{
        OccurrenceId, RelationId, RelationV1, ResourceProfileIdV1,
        explicit::{
            CandidateId, CandidateV1, DomainRequestV1, RequestV1, evaluate,
            selection::{
                FirstFeasibleInDeclaredOrderV1, PolicyId, SelectionOutcomeV1, select,
            },
        },
    },
};

let domain = DomainRequestV1::try_new(vec![
    CandidateV1::new(CandidateId::try_new("brand/ink")?, Srgb8::new([18, 52, 86])),
    CandidateV1::new(CandidateId::try_new("brand/paper")?, Srgb8::new([245, 247, 250])),
])?;
let relation = RelationV1::applicable(
    RelationId::try_new("content-on-canvas")?,
    OccurrenceId::try_new("article/body")?,
    Wcag22CriterionV1::Sc143TextDefault,
    vec![Srgb8::new([255; 3])],
)?;
let result = evaluate(RequestV1::try_new(
    domain,
    vec![relation],
    ResourceProfileIdV1::Compile,
)?)?;

assert_eq!(result.evaluated().map(|value| value.candidates().len()), Some(2));
let source = result
    .selection_source()
    .expect("this fixture has at least one feasible candidate");
let policy = FirstFeasibleInDeclaredOrderV1::try_new(
    PolicyId::try_new("article/foreground-order")?,
    vec![
        CandidateId::try_new("brand/paper")?,
        CandidateId::try_new("brand/ink")?,
    ],
)?;
let outcome = select(source, policy)?;
let selected = match &outcome {
    SelectionOutcomeV1::Selected { selected, .. } => {
        selected.candidate().candidate_id().as_str()
    }
    SelectionOutcomeV1::NoSelection { .. } => panic!("fixture must select"),
};
assert_eq!(selected, "brand/ink");
# Ok(())
# }
# fn main() {}
```

`selection_source()` возвращает `Some` только для `Feasible`: `Infeasible` и
`NotEvaluated` не могут начать выбор. Политика перечисляет допустимое подмножество
ID в точном клиентском порядке. Core сначала проверяет весь список, затем берёт
первый уже доказанный feasible-ID и повторно проверяет его ровно по всем `E`
каноническим применимым рёбрам тем же вычислителем WCAG. Валидный список без
feasible-ID возвращает исчерпывающий `NoSelection`; неизвестный/повторный ID и
расхождение финальной проверки являются разными типизированными отказами, без
fallback.

В примере `Sc143TextDefault` означает явно объявленный клиентом критерий
SC 1.4.3 для обычного текста с отношением 4.5:1; Core не угадывает его по ID или
типографике. `0x75` и `0x76` — вычисленные граничные результаты именно для двух
точных соседей `#000000` и `#FFFFFF`, а не константы конфигурации или
универсальные значения воспринимаемой читаемости. Точное совместное множество
решений равно `#757575…#767676`; соседние коды `#747474` и `#777777` уже не
проходят оба ограничения одновременно.

Роли доказательств разделены. Независимый Python-оракул
`scripts/verify_wcag22_neutral_axis.py` пересчитывает множество рациональной
арифметикой без производственного Q55 и Rust-вычислителя. Тест
`exact_4_5_fixtures_are_7_2_and_proven_zero` фиксирует те же два кандидата через
публичный производственный вычислитель, а
`production_vectors_are_bound_to_the_exact_independent_oracle_fixture`
фиксирует точные байты эталона по SHA-256. Изменение соседей, критерия либо
зарегистрированного домена может изменить множество решений и требует
повторного полного вычисления.
