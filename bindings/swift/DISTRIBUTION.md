# Lab Colors для Swift

Самостоятельный SwiftPM-пакет для macOS и iOS. Вычисления выполняет общее
Rust-ядро, уже включённое в XCFramework. Rust, Cargo и Node.js потребителю не нужны.

## Подключить пакет

Распакуйте `LabColors.swiftpm.zip`. Добавьте каталог `LabColors` как локальный
пакет в Xcode или зависимость своего SwiftPM-проекта:

```swift
.package(path: "Vendor/LabColors")
```

Цель приложения подключает `.product(name: "LabColors", package: "LabColors")`.
Сохраните архив и `SHA256SUMS` вместе с версией приложения. Это распространяемый
пакет, а не каталог разработки `bindings/swift` из репозитория.

Архив содержит macOS arm64/x86_64, iOS arm64 и iOS Simulator arm64/x86_64.
Минимальные версии платформ и идентичность сборки записаны в
`build-metadata.json`. История исходников определяется `sourceCommit`,
а `coreTree` относится только к исходникам математического ядра.

## Подбирать цвет при смене фона

```swift
import Foundation
import LabColors

let program = try ProgramWireBuilderV2()
    .finiteTarget(21, candidates: [
        ProgramCandidate(id: 201, rgb: [0, 0, 0], opacity: 1),
        ProgramCandidate(id: 202, rgb: [255, 255, 255], opacity: 1),
    ])
    .selectionRelease(1, rankGroups: [
        [ProgramSelectionState(id: 1, choices: [ProgramSelectionChoice(target: 21, candidate: 201)])],
        [ProgramSelectionState(id: 2, choices: [ProgramSelectionChoice(target: 21, candidate: 202)])],
    ])
    .surfaceInputPort(31)
    .solidPaint(41, target: 21)
    .inputSurface(51, input: 31)
    .sourceOverOccurrence(61, subject: 41, against: 51,
        adaptingLuminance: 64, backgroundRatio: 0.2, surround: .average)
    .presentationRoot(71, terminal: 61)
    .presentationTarget(71, occurrence: 61)
    .wcag22VisibleUnary(true, id: 81, occurrence: 61, criterion: .sc143TextDefault)
    .output(91, paint: 41)
    .finish()
let runtime = try compileProgramWire(bytes: program, streamId: 1)
let snapshot = try runtime.updateObserved(revision: 1, scenarioIds: [1],
    surfaces: Data([255, 255, 255]), surfaceCount: 1)
if snapshot.state() == "ready" {
    let rgb = try snapshot.outputRgb(index: 0)
    print(Array(rgb)) // [0, 0, 0]
}
```

На чёрном фоне следующая ревизия выбирает белый цвет. Если обязательные
ограничения не выполнены, результат не является готовым. Приложение обрабатывает
`waiting`, `stale` и `failed` явно, не подменяя их успешной проверкой.
Снимок владеет своими данными и не изменяется после следующего обновления.

## Вычислять цвет из авторской опоры

V3 позволяет объявить характер цвета и получить новое значение без списка
кандидатов. Опора и условия относятся к конечному видимому применению:

```swift
let computed = try ProgramWireBuilderV3()
    .colorTarget(21, anchor: [0, 103, 244],
        options: ProgramWireColorOptionsV3(appearance: 61))
    .surfaceInputPort(31)
    .solidPaint(41, target: 21)
    .inputSurface(51, input: 31)
    .pointOccurrence(61, paint: 41, surface: 51)
    .wcag22VisibleUnary(true, id: 81, occurrence: 61, criterion: .sc143TextDefault)
    .assignment([21])
    .output(91, paint: 41)
    .finish()
let runtime = try compileProgramWire(bytes: computed, streamId: 1)
let frame = try runtime.updateObserved(revision: 1, scenarioIds: [1],
    surfaces: Data([255, 255, 255]), surfaceCount: 1)
if frame.state() == "ready" {
    print(try frame.copyOutputTable())
    print(frame.optimality() as Any)
}
```

`ProgramWireColorIdentityV3` задаёт необязательные hue/chroma/lightness-границы,
`ProgramWireToneV3` — кривую предпочтительной светлоты от входной поверхности.
Для кривой от видимой подложки используйте
`colorTarget(_:anchor:options:backdropTone:)` с `compose` или `composeReference`.
В `options` укажите `appearance` или `referenceAppearance` и оставьте `tone` пустым.
Пропущенные настройки сохраняют пакетные дефолты Core. `computedTarget`
отдельно принимает явный RGB-box или серую ось, а `visibleSeparationHard`
ограничивает расстояние Oklab от итогового цвета до его подложки.
`assignment` перечисляет вычисляемые targets в порядке предпочтений; пустой
список проверяет единственный authored tuple графа из фиксированных цветов.

`ready` означает соблюдение обязательных условий; `optimality()` отдельно
возвращает `exact` или `unfinished`. `unsatisfied` доказывает отсутствие
допустимого решения в объявленном домене, `incomplete` означает исчерпание
бюджета до найденного допустимого результата. `assessmentReport()` и
`diagnostics()` возвращают типизированные данные этого снимка. Нативный
builder кодирует декларацию; он не выполняет цветовые расчёты.

## Связать результат с отображением

Для V3 `runtime.updateObservedWith` подготавливает снимок и передаёт его
в `ProgramUpdateHost.accept`. Обработчик устанавливает поверхность и связанные
цвета целиком; `true` принимает ревизию, `false` или исключение до установки
сохраняют предыдущую. Метод возвращает `Bool`, а нужный снимок обработчик
сохраняет сам. [Контракт обновления](../../docs/contracts/swift-binding.md#связь-с-отображением).

`attachProgramWire` принимает программы V1/V2 и `ProgramPointSinkHost`. Его `tryInstall` получает
полную команду `setAll`, `revokeAll` или `confirmExact` с ревизией и ожидаемым
состоянием. `true` означает, что вся команда установлена атомарно; при `false`
или ошибке внешний приёмник обязан сохранить прежнее состояние.
Callback вызывается синхронно на потоке вызывающего кода. UI-поток выбирает
приложение. Повторный вход в тот же attachment получает типизированный отказ,
а не блокирует поток. Параллельные независимые runtime не имеют общего замка.

Приложение владеет внешним отображением: сначала отзывает принадлежащий ему
scope, затем вызывает `dispose(confirmed: true)`. `false` ничего не завершает.
ARC освобождает память, но не подтверждает отзыв чужого UI. Callback, который
ссылается на attachment, должен удерживать его слабо, чтобы не создавать цикл.
Исторический снимок или materialization authority не заменяет текущую проверку
ревизии, identity и binding epoch.

## Границы результата

Program использует объявленную модель encoded-sRGB8. Ядро не читает экран и
не управляет SwiftUI/UIKit. Преобразование результата в цвет UI и получение
наблюдений принадлежат приложению. `rendererProvenance == "unverified"`
не подтверждает физические пиксели или человеческое восприятие.

Код распространяется под MIT. Встроенные данные имеют отдельные условия
CC-BY-4.0 и CC-BY-SA-4.0. Лицензии, источники и атрибуция всех перечисленных
зависимостей находятся в `THIRD-PARTY-NOTICES.md` и `dependency-licenses.json`.
