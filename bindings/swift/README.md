# Нативный биндинг labcolors (Swift / UniFFI)

Swift-поверхность динамического Rust-ядра через UniFFI: сгенерированный Swift
вызывает `labcolors-core` в runtime, а не сериализует токены на сборке.
Экспортируется рантайм-контраст-ядро (см. `crates/labcolors-ffi`): контраст,
резолв, лестницы, подложка→α и низкоуровневое решение точки Glow.


Текущее исполняемое доказательство этой поверхности — pinned Swift 6.1.3 в
одноразовой эфемерной GitHub-hosted VM (Linux x86_64): тулчейн приходит из
закреплённого content-addressed digest официального OCI-образа
`swift:6.1.3-noble`. Оно не является аттестацией Apple ABI,
macOS/arm64 или iOS.

## Что здесь

- `Package.swift` — SwiftPM: системный модуль (`labcolorsFFI`) + Swift-обёртка
  (`LabColors`) + conformance-тесты.
- Swift/C sources **генерируются в CI** (uniffi-bindgen) и не коммитятся.
- `Tests/LabColorsConformanceTests` — прогон закоммиченного пака
  `conformance/vectors/*.json` против выхода FFI.

## Сборка и тест

Swift валидируется в **digest-закреплённом OCI-образе swift:6.1.3-noble на
эфемерной GitHub-hosted VM (Linux x86_64)** (тот же UniFFI-биндинг, тот же пак,
ядро под `x86_64-unknown-linux-gnu`); публичный репозиторий получает
GitHub-hosted минуты бесплатно, macOS-референс остаётся ручным и не входит в
PR-гейт. `DRIFT_TOL` задаёт правило сравнения, но
не заменяет прогоны на других платформах. Единый скрипт —
`ci/run-conformance.sh` — используют и локальный прогон, и CI-джоба
(`.github/workflows/native-conformance-worker.yml`); workflow
`.github/workflows/native-conformance.yml` остаётся только event-caller.

Локально на Linux x86_64 нужны точные Swift 6.1.3 и Rust 1.96.0:

```sh
# из корня репозитория
RUST_TOOLCHAIN=1.96.0 \
SWIFT_TOOLCHAIN=6.1.3 \
GITHUB_WORKSPACE="$PWD" \
RUNNER_TEMP="${TMPDIR:-/tmp}" \
bash bindings/swift/ci/run-conformance.sh
```

Нативный macOS/arm64 path сейчас представлен только ручной reference-джобой
`swift-conformance-macos-reference` (`workflow_dispatch`): это **не gate** PR,
`main` или release. Исполняемое доказательство остаётся явно ограничено
описанным выше pinned Linux x86_64 контекстом.

## Толерантность conformance

Числовые поля зафиксированных семейств conformant в пределах
`DRIFT_TOL = 1e-6` (SSOT пака:
`crates/labcolors-conformance/src/lib.rs`): для зависимых от libm путей
(`powf`/`atan2`/`ln`) битовая идентичность f64 между runtime не гарантируется;
реализации могут расходиться на несколько ULP (~1e-13).
Композит-hex точен относительно объявленного encoded-sRGB8 operation profile,
который фиксирует порядок IEEE binary64 операций и квантование; исполняемое
Swift/UniFFI evidence сейчас ограничено описанным выше pinned Linux x86_64
runtime. Solve-hex — квантование трансцендентного резолва, ±1 LSB на канал.
Неуспешный solve возвращает `ColorError.Failure(category, code)`: category —
закрытый enum `FailureCategory`, а не произвольная строка. Он
отделяет доказанную `unreachable` от `unresolved` и `rejected`,
а code задаёт конкретную машинную причину. Оба поля приходят из одного
core-owned descriptor и проверяются conformance-паком.

Glow проверяется другим контрактом: `stable-v1` обязан вернуть типизированный
`Indeterminate` (`site_id` + неразделимое typed `evidence`), если доказанной
границы нет; прежний CAM16/libm-dependent путь доступен только как явный
`legacy-platform-dependent-v1`. Нативный гейт не заявляет CAM16 bit parity:
`bit-exact` относится только к encoded-sRGB8 screen-композиту и его certificate.
Native output кодирует provenance самим вариантом `GlowPointDecision`:
`stableExactNoop`, `legacyReached`, `legacyUnreachable` либо `indeterminate`.
Отдельных `decisionProfile`, `decisionGuarantee`, `targetStatus` и
`selectionDiagnosticProfile` в output больше нет — поэтому клиент физически не
может собрать «stable + legacy guarantee» или другую невозможную комбинацию.
Requested `GlowDecisionProfile` по-прежнему обязателен как input функции;
`GlowPointValue` хранит только общий exact screen-composite payload и его
composite profile/guarantee.

`stableExactNoop` сам означает byte-exact недостижимость без appearance
selection; `legacyReached` / `legacyUnreachable` сами несут CAM16/libm legacy
provenance и target outcome. `indeterminate` является stable отказом выбрать
состояние без sound bound. Typed `NumericalIndeterminacy.intervalOverlap`
остаётся законным outward evidence неопределённости.

Граница самостоятельно валидирует tint, background и конечный `targetDj > 0`:
только эти ошибки становятся `ColorError.InvalidGlowRequest`. Ошибка core после
успешной валидации, неизвестный численный variant или новый `SolveFailure` без
public descriptor считаются `ColorError.IncompatibleCoreContract`; adapter не
выдаёт им выдуманный fallback-code.
