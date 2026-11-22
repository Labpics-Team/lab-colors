# Lab Colors для desktop JVM

`pics.lab:colors-desktop` поставляет Kotlin Program API и общий Rust runtime. JNA автоматически загружает библиотеку из JAR; Rust, `jna.library.path` и ручное копирование DLL не нужны.

Поддержаны Windows 10+ x64/arm64, Ubuntu 20.04+ x64/arm64, macOS 13+ arm64; Java 17+. Архив `LabColors.desktop.zip` содержит Maven-репозиторий. Версия `<coreVersion>-<sourceCommit>` записана в `desktop-release-manifest.json`; разные ревизии не заменяют уже выданные координаты.

```kotlin
repositories { maven { url = uri("path/to/extracted/repository") }; mavenCentral() }
dependencies { implementation("pics.lab:colors-desktop:<version>") }
```

API совпадает с Android: `ProgramWireBuilderV3` описывает характер цвета, поверхности и предпочтения, `compileProgramWire` передаёт вычисления общему Core. [Пример и настройки](../README.md#использование) одинаковы для Android и JVM. `updateObservedWith` принимает новую поверхность и связанные выходы через один `ProgramUpdateHost`; обычный `updateObserved` принимает уже наблюдённый контекст. Program и snapshot освобождаются через `use`/`close`; возвращённые массивы принадлежат caller. Одна библиотека содержит все платформы, но JNA загружает только библиотеку текущего процесса.

```kotlin
compileProgramWire(wire, 1024u).use { program ->
    program.updateObserved(1uL, listOf(31u), byteArrayOf(-1, -1, -1), 1u).use { snapshot ->
        check(snapshot.state() == "ready")
        val rgb = snapshot.outputRgb(0u)
        val opacity = snapshot.outputOpacity(0u)
    }
}
```

`desktop/ci/build_native.py` компилирует библиотеки и проверяет предел glibc 2.31. `package.py` объединяет только одинаковую ревизию и одинаковый сгенерированный API. `test_consumer.py` запускает отдельное приложение на точном архиве: история Program, общий байтовый V3-эталон, изменение вычисленных цветов на разных фонах, отчёты, атомарный отказ/исключение обработчика, повторный вход и освобождение ресурсов.
