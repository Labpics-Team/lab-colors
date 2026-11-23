# Lab Colors для Android

Kotlin вызывает общий Rust Core через сгенерированный UniFFI API. Описание
характера цвета, связей с поверхностями и предпочтений передаётся через
`ProgramWireBuilderV3`. Core вычисляет связанные значения заново при изменении
контекста. Builder кодирует декларации без собственной цветовой математики.

## Подключение

Распакуйте `LabColors.android.zip` проверенной ревизии. Добавьте вложенный
`repository` как Maven-репозиторий и зависимость `pics.lab:colors-android:<version>`.
Версия и точный Git commit находятся в `build-metadata.json`; архив и каждый
файл привязаны SHA-256 в `android-release-manifest.json`. Замените `<version>`
значением `version` из этого манифеста: разные исходные ревизии не используют
одну Maven-координату. Артефакт из PR требует
успешных Android consumer jobs этой же ревизии и сам по себе не является релизом.
Публикация в Maven Central не выполняется автоматически.

```kotlin
// settings.gradle.kts
repositories {
    maven { url = uri("vendor/labcolors/repository") }
    google()
    mavenCentral()
}
// build.gradle.kts приложения, Android Gradle Plugin 8.0 или новее
android {
    compileOptions {
        isCoreLibraryDesugaringEnabled = true
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
}
dependencies {
    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs:2.1.5")
    implementation("pics.lab:colors-android:<version>")
}
```

POM и Gradle metadata передают Kotlin и JNA зависимости. Подключение отдельного
AAR через `files(...)` теряет эти метаданные и не является поддерживаемым
маршрутом. Rust, Cargo, NDK и внутренние пути Lab Colors приложению не нужны.
Consumer-правила R8 входят в AAR. Для API 23 приложению нужен показанный выше
core-library desugaring: JNA использует Java API, добавленные в Android API 24.
Этот флаг приложения не передаётся через POM; одной установки Java 8
source/target недостаточно. Desugaring совместим с R8.

Поставка содержит arm64-v8a, armeabi-v7a, x86 и x86_64; минимальный Android API 23.
CI исполняет внешний debug-потребитель и приложение после R8 на x86_64
эмуляторах API 23 и Android с 16-КБ страницами. ARM ABI проверяются сборкой и
инспекцией ELF, не запуском на физическом телефоне. Все нативные зависимости
конечного APK проверяются на выравнивание; проверяется и упаковка ZIP.

## Использование

```kotlin
import pics.lab.colors.*

val wire = ProgramWireBuilderV3().apply {
    colorTarget(21u, byteArrayOf(0, 103, 244.toByte()),
        ProgramWireColorOptionsV3(appearance = 61u))
    surfaceInputPort(31u)
    solidPaint(41u, 21u)
    inputSurface(51u, 31u)
    pointOccurrence(61u, 41u, 51u)
    wcag22VisibleUnary(true, 81u, 61u, Wcag22Criterion.SC143_TEXT_DEFAULT)
    output(91u, 41u)
    assignment(listOf(21u))
}.finish()
compileProgramWire(wire, 1u).use { runtime ->
    val backgrounds = listOf(byteArrayOf(-1, -1, -1), byteArrayOf(0, 0, 0))
    for ((index, background) in backgrounds.withIndex()) {
        runtime.updateObserved((index + 1).toULong(), listOf(1u), background, 1u).use { snapshot ->
            check(snapshot.state() == "ready")
            val channels = snapshot.outputRgb(0u).map { it.toInt() and 255 }
            println("${snapshot.outputSlot(0u)}: $channels; ${snapshot.optimality()}")
        }
    }
}
```

Один синий источник здесь связан с местом использования. Смена фона меняет
вычисленное значение, сохраняя объявленный характер и требование для текста.
Числовые ID задаёт приложение; Core не назначает им роли дизайн-системы.

`UInt` и `ULong` сохраняют полный диапазон идентификаторов и ревизий.
`ByteArray` содержит необработанные байты, а `finish()` выдаёт независимую копию.
JVM-вызовы `assignment` и `ProgramWireColorOptionsV3`, скомпилированные с v0.3,
сохраняются. Прежние constructor/copy-перегрузки остаются в байткоде;
новый Kotlin-код использует полную форму с `referenceAppearance`.
Builder принадлежит одному вызывающему контексту; отказ не меняет принятую
декларацию. Сессия допускает перенос между потоками, но одновременная запись
и повторный вход получают явный отказ, не скрытую очередь.

## Настройки цвета и связи

`colorTarget(id, anchor, options)` принимает исходный sRGB8 и
`ProgramWireColorOptionsV3`. Неуказанные поля `identity` передаются как
неуказанные; значения по умолчанию определяет общий Core.

| Поле | Значение |
| --- | --- |
| `domain` | `SRGB` или `NEUTRAL` для нейтрального видимого цвета. |
| `identity.maxHueDeviation` | Допустимое отклонение тона Oklab в градусах, `0..180`. |
| `identity.chroma`, `identity.lightness` | Пара упорядоченных границ в `0..1`. |
| `maxChange` | Максимальное расстояние Oklab от исходного цвета, `0..2`; по умолчанию `2`. |
| `appearance` | ID occurrence, на котором оценивается характер видимого цвета. |
| `referenceAppearance` | ID occurrence с исходным видом на объявленной опорной поверхности; не совмещается с `appearance`. |
| `tone` | `ProgramWireToneV3(from, curve)`: предпочтительная светлота относительно входной поверхности. Кривая содержит минимум две пары, начинается с входа `0`, заканчивается `1`; входы строго возрастают, обе координаты в `0..1`. |

`pointOccurrence(id, paint, surface)` связывает краску с поверхностью.
`occurrenceSurface` позволяет использовать результат слоя как следующую
поверхность. `visibleSeparationHard(id, occurrence, minimum, maximum)` задаёт
интервал расстояния Oklab от видимого цвета до его подложки, `0..2`.

Для собственной конечной области поиска есть
`computedTarget(id, ProgramWireDomainV3.Box(lower, upper), anchor, maxDelta)`
и `ProgramWireDomainV3.Gray(lower, upper)`. `maxDelta` неотрицателен, его квадрат
должен быть конечным. `assignment(targetsInPreferenceOrder)` объявляется один
раз и задаёт порядок предпочтений вычисляемых targets; для графа только с
фиксированными цветами список пуст. Семантическую совместимость деклараций
проверяет `compileProgramWire`.

`assignment(targetsInPreferenceOrder, ProgramWireAssignmentOptionsV3(reference = ...))`
передаёт исходные поверхности как список `ProgramWireReferenceInputV3(port, rgb)`.
`null` оставляет reference неуказанным, пустой список явно задаёт reference без
входных поверхностей. Каждый port встречается один раз. Входы и RGB копируются
при добавлении декларации; дальнейшее изменение исходных коллекций её не меняет.

`preserveTonalHierarchy(id, occurrences)` связывает минимум два различных
occurrence. Core выводит исходные отношения тонального контраста из reference и
сохраняет их при адаптации. Порядок, величины отношений и роли дизайн-системы не
зашиты в Kotlin. [Семантика reference и её границы](../../docs/explanation/reference-appearance.md)
одинаковы для всех потребителей.

У готового снимка `referenceSelections()` возвращает те же записи, что
`assessmentReport().referenceSelections`: исходный и фактический вид, контраст,
полярность, потерю силы, расстояние до виртуальной опоры и оценку достижимой силы.
Пары иерархии находятся в `assessmentReport().tonalHierarchy`. У состояния,
отличного от `ready`, `referenceSelections()` возвращает `null`. Полученные
записи сохраняются после следующих обновлений и закрытия runtime.

## Чтение всех выходов

`copyOutputTable()` у `ProgramSnapshot` и `ProgramAttachedSnapshot` возвращает
независимый `List<Double>` длиной `outputCount() * 5`. Каждая строка содержит
`[slot, red, green, blue, opacity]` в порядке поштучных геттеров снимка.
`slot` сохраняет полный диапазон u32; каналы sRGB8 представлены целыми
`0..255`, opacity — исходным f64 в `0..1`. Все эти значения переносятся без
округления. `outputSlot`, `outputRgb` и `outputOpacity` доступны для отдельного
выхода; в `ByteArray` от `outputRgb` канал 255 по-прежнему представлен байтом -1.

Применяйте цвета при `state() == "ready"`. Для V3 `optimality()` показывает,
завершён ли поиск предпочтительного результата (`exact` или `unfinished`);
обязательные условия выполнены в обоих случаях. `assessmentReport()` содержит
проверки по связям, `diagnostics()` — диагностику поиска и нарушения конкретной
проверенной попытки. Таблица состояния, отличного от `ready`, может хранить прошлые значения.
Снимок без outputs возвращает пустой список.
Сохранённый список не зависит от следующих обновлений и остаётся доступен
после `dispose` и `close()` native-объектов. Если Rust не может выделить таблицу,
метод бросает `LabColorsException.Refused` с `code: "program_resource_exhausted"`
и `operation: "copyOutputTable"`; снимок остаётся прежним.

## Привязка к интерфейсу и ресурсы

`runtime.updateObservedWith(revision, scenarioIds, surfaces, surfaceCount, host)`
передаёт готовый независимый `ProgramSnapshot` в `ProgramUpdateHost.accept`.
В обработчике скопируйте нужные значения, подготовьте обновление поверхности
и всех цветов, затем установите их одной операцией. Возврат `true` принимает
новую ревизию Core; `false` или исключение до установки сохраняют предыдущую
ревизию. Снимок принадлежит обработчику: его можно сохранить или закрыть через
`use` прямо в `accept`. Повторный вход в тот же runtime получает `program_busy`.

`updateObserved` принимает уже наблюдённые внешние факты. Для управляемого
приложением изменения поверхности вместе с цветами используйте
`updateObservedWith`. Обработчик выполняется синхронно на вызывающем потоке;
Android main thread выбирает приложение.

`attachProgramWire` принимает `ProgramPointSinkHost`. `tryInstall` выполняется
синхронно на вызывающем потоке и подтверждает целиком установленную команду.
`false` или исключение допустимы только до внешнего эффекта. Android main thread
выбирает приложение, движок не делает скрытый dispatch. Compose/UI-kit остаются
внешними потребителями; подтверждение рендера не выводится из расчёта цвета.
Эта точечная привязка принимает ProgramWire v1/v2; связанные выходы V3
устанавливаются через `ProgramUpdateHost`.

Объекты UniFFI освобождаются через `use`/`close`. Для attachment сначала отзовите
внешнюю привязку и вызовите `dispose(true)`, затем `close()`. Освобождение памяти
не означает отзыв внешнего UI. `dispose(false)` сохраняет живую привязку для
повтора. После закрытия native handle использовать объект нельзя.

## Проверки

Полнота скомпилированного Kotlin API сверяется с реальными UniFFI-экспортами,
авторинг с публичными TypeScript-декларациями. Общие байтовые примеры и истории
исполняются на Kotlin, Swift и JS; новые функции не должны оставаться только
в одном адаптере. Численные семьи и реестр возможностей берутся из общего
conformance-пака. Матрица запускается как часть обязательного Native conformance.
