# Исторический Linux x86_64 прогон Swift/UniFFI pack 1.0.0 (#84)

> Статус: **SUPERSEDED для текущего релиза**. Этот зафиксированный прогон
> аттестовал `packVersion=1.0.0` / `coreVersion=0.1.0`. Breaking-релиз
> переводит контракт на `packVersion=2.0.0` / `coreVersion=0.2.0`, поэтому
> результаты ниже остаются воспроизводимым историческим свидетельством, но не
> доказательством актуального пака. Новая аттестация обязана дополнительно
> воспроизвести 7 alpha-векторов, включая точный half-tie
> `#C0B2FA @ 0.122` над `#000000` → `#17161F`.

Текущая runner-политика исключает платные GitHub-hosted macOS-раннеры, поэтому
этот исторический Swift-биндинг был проверен **локально** в официальном
swift-контейнере на Linux x86_64. Тот же UniFFI-биндинг вызвал ядро под
`x86_64-unknown-linux-gnu`. `DRIFT_TOL = 1e-6` задаёт правило сравнения пака
(SSOT — `crates/labcolors-conformance/src/lib.rs`), но сам по себе не доказывает
поведение на другой ОС или архитектуре.

Прогон воспроизводим: `bindings/swift/ci/run-conformance.sh` — единый источник
для этого прогона и для self-hosted CI-джобы (`.github/workflows/native-conformance.yml`).

## Платформа и тулчейн

| | |
|---|---|
| Дата исторического прогона | 2026-07-08 |
| Аттестованный контракт | conformance pack `1.0.0`, core `0.1.0` |
| Ядро/контейнер | `swift:6.1` (Docker Desktop, Linux x86_64 / WSL2) |
| Swift | `6.1.3 (swift-6.1.3-RELEASE)`, target `x86_64-unknown-linux-gnu` |
| Rust | `rustc 1.96.1 (31fca3adb 2026-06-26)`, `cargo 1.96.1` |
| Команда | `docker run --rm -v <repo>:/src:ro swift:6.1 bash /src/bindings/swift/ci/run-conformance.sh` |
| Exit code контейнера | `0` (при `set -euo pipefail` — все шаги прошли) |

## Артефакты сборки

Ядро собрано под Linux (обе линковки из `crate-type`):

```
-rw-r--r-- 2 root root 216780120 target/debug/liblabcolors.a
-rwxr-xr-x 2 root root  68667704 target/debug/liblabcolors.so
```

uniffi-bindgen сгенерировал биндинги из `liblabcolors.so` (library-mode):
`labcolors.swift`, `labcolorsFFI.h`, `labcolorsFFI.modulemap`. Экспортированы 8
функций FFI: `composite`, `contrast`, `coreVersion`, `ladderAlpha`, `minAlpha`,
`muddiness`, `recheck`, `solveContrast`.

## Rust-раннер-референс (в том же контейнере, Linux x86_64)

Ядро на Linux воспроизвело закоммиченный пак (сгенерированный на Windows) в
пределах толерантности. Это свидетельство именно для показанного Linux x86_64
runtime; оно не является Apple/macOS-аттестацией:

```
test result: ok. 4 passed; 0 failed;   (labcolors-conformance lib)
test result: ok. 9 passed; 0 failed;   (reference_runner: семейства + метаданные + дайджест + WCAG-якоря + LF)
test result: ok. 8 passed; 0 failed;   (labcolors-ffi Rust-юниты)
```

## Swift test (пак против выхода FFI)

```
[4/15]  Compiling LabColors labcolors.swift
[9/18]  Compiling LabColorsConformanceTests ConformanceTests.swift
[25/26] Linking LabColorsPackageTests.xctest

Test Suite 'All tests' started at 2026-07-08 05:56:06.567
Test Case 'ConformanceTests.testAlpha' passed (0.001 seconds)
Test Case 'ConformanceTests.testContrasts' passed (0.101 seconds)
Test Case 'ConformanceTests.testCoreVersionMatchesManifest' passed (0.0 seconds)
Test Case 'ConformanceTests.testLadders' passed (0.0 seconds)
Test Case 'ConformanceTests.testMuddiness' passed (0.0 seconds)
Test Case 'ConformanceTests.testSolve' passed (0.001 seconds)
Test Suite 'ConformanceTests' passed
	 Executed 6 tests, with 0 failures (0 unexpected) in 0.104 (0.104) seconds
Test Suite 'All tests' passed
	 Executed 6 tests, with 0 failures (0 unexpected) in 0.104 (0.104) seconds
```

**Итог: 6 Swift-тестов, 0 провалов, 0.104 с.** Каждое семейство пака
воспроизведено биндингом через FFI: `testContrasts` (40 векторов),
`testLadders` (25), `testAlpha` (6, композит-hex точно), `testSolve` (6, вкл.
честный `unreachable`), `testMuddiness` (4), `testCoreVersionMatchesManifest`.

## Что доказал этот исторический прогон

Рантайм-ядро Rust `0.1.0`, вызванное со Swift-стороны через UniFFI на Linux
x86_64, воспроизвело pack `1.0.0` в пределах conformance-толерантности — **ядро
было динамическим, не запечённым**. Этот результат не переносится автоматически
ни на pack `2.0.0`, ни на macOS/arm64, ни на iOS. Для них нужен отдельный
исполняемый прогон; допуск `DRIFT_TOL` не заменяет такое evidence.
