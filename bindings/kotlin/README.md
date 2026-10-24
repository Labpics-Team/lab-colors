# Lab Colors для Android

Kotlin вызывает общий Rust Core через сгенерированный UniFFI API. Builder
кодирует ProgramWire v1/v2 без JavaScript. Цветовая математика и выбор
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
// build.gradle.kts приложения
dependencies { implementation("pics.lab:colors-android:<version>") }
```

POM и Gradle metadata передают Kotlin и JNA зависимости. Подключение отдельного
AAR через `files(...)` теряет эти метаданные и не является поддерживаемым
маршрутом. Rust, Cargo, NDK и внутренние пути Lab Colors приложению не нужны.
Consumer-правила R8 входят в AAR.

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

## Привязка к интерфейсу и ресурсы

`attachProgramWire` принимает `ProgramPointSinkHost`. `tryInstall` выполняется
синхронно на вызывающем потоке и подтверждает целиком установленную команду.
`false` или исключение допустимы только до внешнего эффекта. Android main thread
выбирает приложение, движок не делает скрытый dispatch. Compose/UI-kit остаются
внешними потребителями; подтверждение рендера не выводится из расчёта цвета.

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
