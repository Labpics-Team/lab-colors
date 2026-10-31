# Lab Colors для Android

Kotlin вызывает общий Rust Core через сгенерированный UniFFI API. Builder
кодирует ProgramWire v1/v2/v3 без JavaScript. Цветовая математика и выбор
результата не дублируются.

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

val wire = ProgramWireBuilderV1().apply {
    source(11u, byteArrayOf(20, 20, 20))
    fixedTarget(21u, 11u)
    surfaceInputPort(31u)
    solidPaint(41u, 21u)
    inputSurface(51u, 31u)
    sourceOverOccurrence(61u, 41u, 51u, 64.0, 0.2, ProgramSurround.AVERAGE)
    presentationRoot(71u, 61u)
    presentationTarget(71u, 61u)
    wcag22VisibleUnary(true, 81u, 61u, Wcag22Criterion.SC143_TEXT_DEFAULT)
    output(91u, 41u)
}.finish()
compileProgramWire(wire, 1u).use { runtime ->
    runtime.updateObserved(1uL, listOf(1u), byteArrayOf(-1, -1, -1), 1u).use { snapshot ->
        check(snapshot.state() == "ready")
        val color = snapshot.outputRgb(0u)
        // Byte хранит биты канала: 255 представлен как -1.
    }
}
```

`UInt` и `ULong` сохраняют полный диапазон идентификаторов и ревизий.
`ByteArray` содержит необработанные байты, а `finish()` выдаёт независимую копию.
Builder принадлежит одному вызывающему контексту; отказ не меняет принятую
декларацию. Сессия допускает перенос между потоками, но одновременная запись
и повторный вход получают явный отказ, не скрытую очередь.

Для вычисляемых целей используйте `ProgramWireBuilderV3`. Приложение объявляет
область, опору, допуск и порядок предпочтения; затем добавляет обычный граф
поверхностей, opacity, occurrences, ограничений и outputs:

```kotlin
val builder = ProgramWireBuilderV3()
    .computedTarget(21u, ProgramWireDomainV3.Gray(0u, 255u), byteArrayOf(64, 64, 64), 1.0)
    .assignment(listOf(21u))
```

`ProgramWireDomainV3.Gray` принимает `UByte`-границы множества `[g, g, g]`.
`Box(lower, upper)` принимает два `ByteArray` по три encoded sRGB8-канала.
Границы включены; смысл цели и опору задаёт клиент. `anchor` может лежать вне
области. `maxDelta` конечен, неотрицателен и имеет конечный квадрат в
[метрике V3](../../docs/contracts/program-assignment-v3.md).
Каждая вычисляемая цель входит в `assignment` ровно один раз; порядок задаёт
лексикографическое предпочтение. Core сохраняет допустимую авторскую композицию,
а при нарушении ограничений вычисляет разрешённую замену. Фиксированные цели
можно использовать вместе с вычисляемыми; смесь с конечным выбором пока
возвращает `program_unsupported`.

`snapshotVersion()` равен `1u` для V1/V2 и `2u` для V3. В version 1 сохранена
прежняя семантика outputs, включая предыдущий успешный результат. В version 2
outputs доступны только в `ready`; `unsatisfied`, `incomplete`, `waiting` и
`stale` имеют нулевой `outputCount()`. `unsatisfied` означает доказанное
отсутствие решения, `incomplete` — незавершённый поиск. `reason(): String?`
равен `"limit"` только у `incomplete`; `revision(): ULong` всегда сообщает
принятую ревизию. Сохранённый snapshot остаётся неизменным, но не подтверждает
актуальность старого цвета.

## Привязка к интерфейсу и ресурсы

`attachProgramWire` принимает `ProgramPointSinkHost`. `tryInstall` выполняется
синхронно на вызывающем потоке и подтверждает целиком установленную команду.
`false` или исключение допустимы только до внешнего эффекта. Android main thread
выбирает приложение, движок не делает скрытый dispatch. Compose/UI-kit остаются
внешними потребителями; подтверждение рендера не выводится из расчёта цвета.

Attached snapshot предоставляет те же `snapshotVersion`, `revision` и `reason`.
Новая принятая ревизия с `incomplete` или `unsatisfied` сначала отзывает прежний
paint. Если host отклоняет команду, сохраняется прежний head. Attachment
устанавливает один terminal point, а не транзакцию нескольких независимых UI-выходов.

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
