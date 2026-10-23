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

`compileProgramWire(bytes:streamId:)` создаёт `ProgramRuntime`.
`updateObserved` принимает ревизию, идентификаторы сценариев и плоский RGB8-буфер:
сначала все поверхности первого сценария, затем второго и так далее.
`surfaceCount` задаёт число поверхностей в каждом сценарии.
`updateUnknown` передаёт явную неизвестность с идентификатором причины.
Ревизия имеет тип `UInt64`, идентификаторы имеют тип `UInt32`.

`ProgramSnapshot` предоставляет `state()`, `outputCount()`, `outputSlot`,
`outputRgb` и `outputOpacity`. `ready` отличается от `waiting`, `stale` и `failed`.
Снимок и полученные `Data` не изменяются после следующего обновления.
Точный повтор ревизии и конфликтующее содержимое одной ревизии различаются Core.
Отклонённый ввод не занимает ревизию и не публикует частичное состояние.

## Связь с отображением

`attachProgramWire` связывает программу с `ProgramPointSinkHost`.
`tryInstall` получает типизированный `ProgramPointSinkIntent` с операцией,
ревизией, старой и новой последовательностью, binding epoch и целевым выходом.
Host подтверждает всю команду или сохраняет прежнее внешнее состояние.
Callback исполняется синхронно на вызывающем потоке; поток UI выбирает приложение.

Attachment предоставляет обновления, `ProgramAttachedSnapshot`, render и
`materializationAuthority`. `materializationAuthorityFor` проверяет ожидаемые
revision, content identity и binding epoch; старое свидетельство не становится
текущим. Render и authority раскрывают контекст, терминальный композит и
`rendererProvenance`. Значение `unverified` не удостоверяет физический экран.

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
свидетельства остаются различимыми. Отказ host не стирает предыдущий head.

У скалярных операций сохранён `ColorError`: `InvalidColor`, `InvalidAlpha`,
`Failure`, `IncompatibleCoreContract`. `FailureCategory` различает отсутствие
решения, незавершённый поиск и недопустимый запрос. Неизвестный вариант Core
не преобразуется в ложный успешный результат.
