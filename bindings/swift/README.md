# Нативный биндинг labcolors (Swift / UniFFI)

Доказательство **динамического рантайм-ядра** на Apple-платформах: Swift зовёт
ядро Rust (`labcolors-core`) В РАНТАЙМЕ через UniFFI, а не сериализует токены на
сборке. Экспортируется рантайм-контраст-ядро (см. `crates/labcolors-ffi`):
контраст, резолв, лестницы, подложка→α, мутность.

## Что здесь

- `Package.swift` — SwiftPM: системный модуль (`labcolorsFFI`) + Swift-обёртка
  (`LabColors`) + conformance-тесты.
- `Sources/LabColors`, `Sources/labcolorsFFI` — **генерируются в CI**
  (uniffi-bindgen), в репозитории лишь `.gitkeep`. Коммитить сгенерированное не
  нужно — оно производно от `crates/labcolors-ffi`.
- `Tests/LabColorsConformanceTests` — прогон закоммиченного пака
  `conformance/vectors/*.json` против выхода FFI.

## Сборка и тест

Платные GitHub-hosted macOS-раннеры исключены владельцем — Swift валидируется в
официальном **swift-контейнере на Linux x86_64** (тот же UniFFI-биндинг, тот же
пак, ядро под `x86_64-unknown-linux-gnu`; кросс-платформа держится толерантностью
`DRIFT_TOL`). Единый скрипт — `ci/run-conformance.sh` — используют и локальный
прогон, и self-hosted CI-джоба (`.github/workflows/native-conformance.yml`).

Локально (нужен Docker):

```sh
# из корня репозитория
docker run --rm -v "$PWD":/src:ro swift:6.1 \
    bash /src/bindings/swift/ci/run-conformance.sh
```

Аттестация последнего зелёного прогона (числа, версии, платформа):
`docs/conformance/local-swift-attestation.md`. На нативном macOS (если появится
БЕСПЛАТНЫЙ раннер) — джоба `swift-conformance-macos-reference` (ручной
`workflow_dispatch`) в том же workflow.

## Толерантность conformance

Числовые поля conformant в пределах `DRIFT_TOL = 1e-6` (канон ядра, `lut.rs`):
байт-точность f64 кросс-платформенно невозможна (libm-шум `powf`/`atan2`/`ln`
~1e-13). Композит-hex — чистая IEEE-алгебра, точен; solve-hex — квантование
трансцендентного резолва, ±1 LSB на канал.
