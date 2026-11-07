# Контракт платформенных адаптеров

Математика, граф Program, ревизии наблюдений, выбор результата и ошибки
принадлежат Rust Core. `program_wire` является общей границей исполнения.
WASM и UniFFI вызывают её, не создавая отдельного решателя или урезанного
нативного движка. Изменение формулы не требует ручного переноса в другой язык.

## Три независимые ответственности

Языковая граница переводит типы, ошибки и владение ресурсами. Swift и Kotlin
генерируются из одного UniFFI-контракта. Новый поддерживаемый UniFFI язык
не требует ещё одной реализации цветовых операций.
Языковой builder только кодирует канонический wire и проверяется независимыми
байтовыми примерами. V4 builder получает wire через общий генератор Core;
полный растр потребляется через сгенерированный callback с собственными байтами и отчётом.
[Контракт полного растра](final-raster-v1.md) задаёт off/lint/mandatory и область.
Предварительно вычисленные токены не заменяют runtime.

Платформенная поставка владеет ABI, архитектурами, минимальными версиями ОС,
упаковкой и установкой. Эти сведения не входят в Core и не становятся
параметрами цветового графа. Возможность скомпилировать Rust для платформы
не равна поддержке готового SDK.

Интеграция с отображением получает наблюдения и атомарно применяет результат
через host-порт. DOM, SwiftUI, UIKit и другие UI-системы остаются внешними
потребителями. Адаптер UI не пересчитывает цвет и не объявляет неизвестный
renderer подтверждённым. Приложение сохраняет владение внешним scope и его отзывом.

## Point и binding-scope

UniFFI сохраняет два представления callback. `attachProgramWire` передаёт
`ProgramPointSinkHost` прежний `ProgramPointSinkIntent`: stamps, ревизию,
`sinkOutput` и optional paint. `attachProgramWireWithBindings` передаёт
`ProgramScopeSinkHost` полный `ProgramScopeSinkIntent`: все sink outputs,
points и presentation composites с собственными RGB8-байтами. Полный scope
сохраняется и при отзыве. Оба входа используют один Program owner, solver,
обновления и полные Core snapshots. Представление скалярного callback
не ограничивает результаты Core и не требует копировать полные списки scope.

`ProgramAttachedSnapshot.renderCount` и `renderAt` перечисляют renders.
`render` и вызовы authority без выбранного binding требуют однозначности:
`render` возвращает отсутствие при нескольких renders, authority возвращает
`program_materialization_ambiguous_bindings`. Выбранный binding проверяется
через `materializationAuthorityForBinding(root, occurrence)`.

## Условие поддержки

Для новой платформы необходимы готовый устанавливаемый пакет, полный публичный
сценарий Program и обязательная проверка реального внешнего потребителя.
Одного сгенерированного файла или успешного теста Rust недостаточно.

Проверка поверхности извлекает операции из реальных WASM/UniFFI экспортов.
Swift сравнивает их с публичными символами собранного пакета, Kotlin проверяет
методы установленного AAR через отражение. Методы авторинга обоих языков
сравниваются с декларациями действующего JS-пакета.
Проверки имеют отрицательные примеры: отсутствующий метод, скрытый экспорт,
неверное число аргументов и пустой результат обнаружения не проходят.

[Общие истории](../../conformance/program-runtime-v1.json) проверяют изменение
фона, отказ и восстановление через каждый подключённый потребитель.
Отдельные проверки покрывают типизированные ошибки, повтор и конфликт ревизий,
сохранение старого head, callback, владение снимками и завершение.
Реестр численных возможностей сравнивается целиком, а не только по checksum.

ABI и установка проверяются на поддерживаемых средах. В Apple-поставке
исполняются macOS и iOS Simulator; сборка для физического iOS-устройства
проверяет компиляцию и линковку, но не заменяет запуск на устройстве.
В Android-поставке исполняется внешний потребитель на минимальном API и среде
с 16-КБ страницами; отдельно запускается приложение после R8. ABI, зависимости
и упаковка проверяются в конечном APK, а не только в собственной библиотеке.
Область Android-запусков и отличие от проверки ARM-сборки задаёт
[контракт поставки](../../bindings/kotlin/README.md).
Тесты выполняются над теми же байтами архива, которые сохраняются для выпуска.

Неподдерживаемая возможность должна оставаться явным отказом или отсутствием
заявления о поддержке. Скрывать её за заглушкой, фиксированными токенами или
успехом соседней платформы нельзя.

## Kotlin: снимок и полный растр

`snapshotVersion()` равен `1u` для V1/V2 и `2u` для V3. В version 1 сохранена
прежняя семантика outputs, включая предыдущий успешный результат. В version 2
outputs доступны только в `ready`; `unsatisfied`, `incomplete`, `waiting` и
`stale` имеют нулевой `outputCount()`. `unsatisfied` означает доказанное
отсутствие решения, `incomplete` — незавершённый поиск. `reason(): String?`
равен `"limit"` только у `incomplete`; `revision(): ULong` всегда сообщает
принятую ревизию. Сохранённый snapshot остаётся неизменным, но не подтверждает
актуальность старого цвета.

`ProgramWireBuilderV4(sourcePort, destinationPort, operatorId, outputSlot, cleanliness)`
принимает `ULong` raster ports и operator, `UInt` output slot и
`ProgramFinalRasterCleanlinessRequirement`: `OFF`,
`LINT_EXACT_NOMINAL_SRGB8_V2` или `MANDATORY_EXACT_NOMINAL_SRGB8_V2`.
`finish()` получает bytes через сгенерированную функцию `finalRasterWire`.
Core исполняет source-over и сохраняет переданные locked premultiplied RGBA8.

`runtime.updateFinalRaster(revision, width, height, dpr, context, sourcePort,
source, destinationPort, destination)` принимает полные `ByteArray` обоих
источников. Core проверяет extent, DPR, предел 65536 пикселей, bytes и ревизию.
`finalRasterContentIdentity()` возвращает 32-byte identity программы.

`consumeFinalRaster(revision, content, output, context, consumer)` проверяет
текущий допуск до синхронного `ProgramFinalRasterConsumer.consume(raster)`.
Callback получает owned `ProgramFinalRasterBytes`: scope, width, height,
devicePixelRatio, revision, outputSlot, RGB8 pixels и cleanliness.
`true` подтверждает приём; `false` даёт `program_field_consumer_rejected`;
`ProgramFinalRasterCallbackException.InvocationFailed` —
`program_field_consumer_failed`. Память полученных records и массивов
принадлежит Kotlin и сохраняется после `close()` runtime.

Cleanliness передаёт requirement, verdict, checkedPixelCount, violationCount,
firstViolation, profile и profileRelease. Lint сохраняет Ready и точные pixels
при нарушении. Mandatory сохраняет Ready только при соответствии конвенции;
`finalRasterRefusal()` сообщает код и первый witness отказа. Failed-растр даёт
`program_field_stale` при потреблении до вызова callback.
Копия frame сохраняет сведения своей ревизии; текущий допуск проверяет новый вызов Core.
`assessmentReport()` field-снимка возвращает `null`.
[Полный контракт растра](final-raster-v1.md) задаёт область
opaque RGB8, физическую модель и ограничения.
