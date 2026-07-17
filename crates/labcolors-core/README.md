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

Прямой Core по умолчанию включает возможность `wcag22-feasibility`. Protocol
и адаптеры публикуют одну offline-операцию — complete feasibility; все
поверхности используют один математический компилятор и не входят в runtime
WASM.

Совместимый вход `evaluate` перечисляет зарегистрированную нейтральную ось:
ровно 256 кодов `[v, v, v]`, где `v` принимает каждое целое значение от 0
до 255. Вход не выводит размер текста, компонентную семантику, применимость
или предпочтение из ID. Feasibility сам ничего не ранжирует и ничего не
выбирает: он доказывает допустимость каждого кандидата в объявленном конечном
домене, и только.

Для `C` кандидатов и `E` канонических применимых рёбер выполняется ровно
`W=C×E` атомарных проверок. При `E>0` единственный упакованный буфер имеет
`B=ceil(C×E/8)+ceil(C/8)` байт; при `E=0` он пуст. Эти величины выводятся и
проверяются до выделения памяти. Остальная стоимость не маскируется формулой
`C×E`: отдельно выполняются линейный просмотр деклараций, сортировка связей
сравнением точных байтов ID, сортировка соседей внутри каждой связи и линейное
хеширование канонического результата. Превышение ресурсов, противоречивая
декларация, ошибка выделения памяти и нарушение инварианта вычислителя или
компилятора возвращаются типизированными ошибками; частичного или запасного
результата нет.

Формулы `C×E`, случай `E=0`, ресурсные отказы и запрет частичного результата
исполняются непосредственно в `src/wcag22_feasibility_tests.rs` и
`tests/wcag22_feasibility.rs`.

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
