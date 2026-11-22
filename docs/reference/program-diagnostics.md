# Диагностика отказа Program V3

`snapshot.diagnostics()` возвращает данные текущего `unsatisfied` или
`incomplete` результата. Для `ready`, `waiting` и `stale` возвращается `null`.
Свежие output-значения в отказе отсутствуют; диагностический tuple не является
результатом для отображения.

```ts
const diagnostics = snapshot.diagnostics();
// { searchVisited: bigint, searchBudget: bigint, witness: ... | null }
```

`searchVisited` считает посещения объявленного алгоритма поиска, включая
регионы. Это не количество вызовов evaluator или проверенных полных tuple.
`searchBudget` — предел того же алгоритма. `incomplete` отдельно сообщает
`reason === "limit"`: допустимый полный результат ещё не найден. При нулевом
бюджете witness равен `null`, потому что ни один attempt не выполнялся.

## Свидетельство конкретного attempt

Непустой `witness` имеет `kind: "authored-tuple"`. Core повторно проверяет
исходные authored RGB всех изменяемых target на актуальном полном наборе
физических cases. Эта диагностическая проверка ранее отвергнутого tuple не
продолжает поиск и не меняет его счётчики.

- `targets` содержит `{ targetId, source }` диагностического tuple.
- `caseScenarioIds[caseIndex]` содержит исходные scenario ID, физически
  схлопнутые в этот case.
- `violations` содержит типизированные нарушения именно этого tuple.

| `kind` | Поля | Значение |
| --- | --- | --- |
| `hard-constraint` | `constraintId`, `caseIndex`, `subject` | Нарушено конкретное объявленное hard-условие. Клиент сопоставляет ID своей декларации. |
| `target-domain` | `targetId`, `source` | Авторский source лежит вне объявленного множества RGB. |
| `color-identity` | `targetId`, `caseIndex`, `occurrenceId`, `actual` | Фактический цвет вне объявленной области характера цвета. |
| `max-change` | те же поля, `distanceSquared`, `maximumDistance` | Превышен hard-допуск изменения относительно authored anchor. Сравнение Core использует squared distance и квадрат допуска. |

Для `color-identity` и `max-change` непустой `occurrenceId` означает измерение
финального visible в указанном применении. При `null` оценено intrinsic значение.
`subject` различает target, occurrence, reference intrinsic/visible relation и
point presentation; relation reference не выдаётся за полный состав отношения.

Witness не является минимальным несовместимым набором условий и не доказывает,
что именно эти нарушения имеют все остальные кандидаты. Полнота отказа
определяется отдельно состоянием `unsatisfied`; при `incomplete` область ещё
не исчерпана. Core не подменяет это различие непрозрачной строкой `failed`.

## Владение и транзакция

Снимок владеет независимой копией диагностических данных. Последующие updates
не меняют ранее полученный witness. Все Rust-буферы копируются с проверкой
выделения памяти до принятия подготовленного перехода. Отказ host callback
сохраняет предыдущую ревизию и её цвета; вычисленный кандидат с diagnostics
можно изучить без принятия новой наблюдаемой композиции.

JS-проекция рекурсивно замораживает свои объекты и массивы. Она не вычисляет
новых оценок, не меняет решение и не выдаёт право публикации output.
