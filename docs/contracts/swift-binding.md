# Swift API

`LabColors` предоставляет публичные операции Program через UniFFI из
`labcolors-ffi`. Swift builder кодирует канонический ProgramWire, но не содержит
решателя. [Подключение и пример](../../bindings/swift/DISTRIBUTION.md),
[сборка и проверки](../../bindings/swift/README.md) описаны отдельно.

## Программа и наблюдения

`ProgramWireBuilderV1` поддерживает источники, фиксированные и конечные цели,
семейства, поверхности, прозрачность, композицию, контексты, ограничения и выходы.
`ProgramWireBuilderV2` добавляет `selectionRelease`: приоритет полных состояний
с атомарным выбором цвета и прозрачности. Ошибка добавления декларации сохраняет
ранее принятые builder-байты. `finish()` возвращает независимый `Data`.

`ProgramWireBuilderV3` добавляет `computedTarget(_:domain:anchor:maxDelta:)`
и `assignment(_:)`. `ProgramWireDomainV3.box(lower:upper:)` задаёт включённые
границы RGB8, `gray(lower:upper:)` — множество `[g, g, g]` в указанном диапазоне.
Опора `anchor` принадлежит клиенту и может лежать вне области. `maxDelta` —
конечный неотрицательный допуск с конечным квадратом в метрике
[Program V3](program-assignment-v3.md).
`assignment` перечисляет каждую вычисляемую цель ровно один раз, в порядке
лексикографического предпочтения. Смысл идентификаторов задаёт приложение.

```swift
let builder = try ProgramWireBuilderV3()
    .computedTarget(21, domain: .gray(lower: 0, upper: 255),
                    anchor: [64, 64, 64], maxDelta: 1)
    .assignment([21])
```

Остальная декларация — поверхности, opacity, occurrences, ограничения и outputs —
использует методы V1. Core сохраняет допустимую авторскую композицию; иначе
назначает цвета в области и допуске. Обязательные ограничения не становятся
целью максимизации. V3 допускает фиксированные и вычисляемые цели; смешение
вычисляемого назначения с конечным выбором пока возвращает `program_unsupported`.

`compileProgramWire(bytes:streamId:)` создаёт `ProgramRuntime`.
`updateObserved` принимает ревизию, идентификаторы сценариев и плоский RGB8-буфер:
сначала все поверхности первого сценария, затем второго и так далее.
`surfaceCount` задаёт число поверхностей в каждом сценарии.
`updateUnknown` передаёт явную неизвестность с идентификатором причины.
Ревизия имеет тип `UInt64`, идентификаторы имеют тип `UInt32`.

`ProgramSnapshot` предоставляет `snapshotVersion()`, `revision()`, `reason()`,
`state()`, `outputCount()`, `outputSlot`, `outputRgb` и `outputOpacity`.
V1/V2 возвращают snapshot version 1 с прежними `waiting`, `ready`, `stale`,
`failed`; их outputs могут содержать предыдущий успешный результат.
V3 возвращает version 2: только `ready` содержит текущие outputs.
`unsatisfied` означает доказанное отсутствие решения, `incomplete` —
незавершённый поиск, `waiting`/`stale` — отсутствие пригодного наблюдения.
Во всех этих состояниях version 2 `outputCount()` равен нулю. `reason()` имеет
тип `String?` и равен `"limit"` только при `incomplete`. `revision()` сообщает
принятую ревизию, в том числе когда текущего результата нет.
Снимок и полученные `Data` не изменяются после следующего обновления.
Точный повтор ревизии и конфликтующее содержимое одной ревизии различаются Core.
Отклонённый ввод не занимает ревизию и не публикует частичное состояние.

## Связь с отображением

`attachProgramWire` связывает программу с `ProgramPointSinkHost`.
`tryInstall` получает типизированный `ProgramPointSinkIntent` с операцией,
ревизией, старой и новой последовательностью, binding epoch и целевым выходом.
Host подтверждает всю команду или сохраняет прежнее внешнее состояние.
`confirmExact` подтверждает прежнее состояние и stamp без повторной установки:
после отзыва `point` равен `nil`. Такое подтверждение допустимо, только если
host также хранит отозванное состояние и совпадающие sequence/binding epoch.
Callback исполняется синхронно на вызывающем потоке; поток UI выбирает приложение.

Attachment предоставляет обновления, `ProgramAttachedSnapshot`, render и
`materializationAuthority`. `materializationAuthorityFor` проверяет ожидаемые
revision, content identity и binding epoch; старое свидетельство не становится
текущим. Render и authority раскрывают контекст, терминальный композит и
`rendererProvenance`. Значение `unverified` не удостоверяет физический экран.
`ProgramAttachedSnapshot` предоставляет те же version, revision и reason.
Новая принятая ревизия с `incomplete` или `unsatisfied` отзывает прежний paint;
ошибка host оставляет предыдущий head. Сохранённый ready snapshot описывает
свою ревизию и не даёт права повторно устанавливать старый paint. Attachment
работает с одним terminal point; несколько outputs не означают общей транзакции UI.

Приложение сначала отзывает весь внешний scope, затем вызывает
`dispose(confirmed: true)`. Неподтверждённый отзыв отклоняется. После завершения
обновления запрещены. ARC освобождает память, но не отзывает чужой UI;
обратную ссылку host на attachment храните слабо.

Изменяемая сессия имеет одного исполнителя. Её можно передать другому потоку;
пересекающиеся вызовы и повторный вход не ждут замок, а получают отказ.
Независимые сессии не используют общую блокировку.

## Метаданные и численные операции

`numericalCapabilityManifest` возвращает полный реестр численных возможностей.
`issueSourceCertificateEnvelope` выдаёт envelope источника Core, а
`decodeCertificateEnvelope` возвращает только недоверенные метаданные,
не payload или producer capability. Source certificate относится к исходному
Git-дереву, не к hash исполняемого файла и не к человеческому восприятию.

Скалярные `contrast`, `recheck`, `evaluateWcag22`, `solveContrast`, `composite`
и `minAlpha` сохранены. `solveContrast` подбирает нейтральный цвет; для произвольных
разрешённых пар цвета и прозрачности используйте конечные цели Program.

## Отказы

Program и envelope возвращают `LabColorsError.Refused` с машинными `code` и
`operation`. Ошибки wire, графа, обновления, ресурсов, host и устаревшего
свидетельства остаются различимыми. `program_unsupported` отличает известное,
но не поддержанное сочетание возможностей от недопустимых bytes/графа.
`incomplete` — состояние принятой ревизии, а не ошибка ресурсов при admission.
Отказ host не стирает предыдущий head.

У скалярных операций сохранён `ColorError`: `InvalidColor`, `InvalidAlpha`,
`Failure`, `IncompatibleCoreContract`. `FailureCategory` различает отсутствие
решения, незавершённый поиск и недопустимый запрос. Неизвестный вариант Core
не преобразуется в ложный успешный результат.
