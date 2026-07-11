# Нативный биндинг labcolors (Swift / UniFFI)

Swift-поверхность динамического Rust-ядра через UniFFI: сгенерированный Swift
вызывает `labcolors-core` в runtime, а не сериализует токены на сборке.
Экспортируется рантайм-контраст-ядро (см. `crates/labcolors-ffi`): контраст,
резолв, лестницы, подложка→α, мутность и low-level Glow point decision с
обязательным numerical profile.

Текущее исполняемое доказательство этой поверхности — pinned Swift-контейнер на
Linux x86_64. Оно не является аттестацией Apple ABI, macOS/arm64 или iOS.

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
пак, ядро под `x86_64-unknown-linux-gnu`). `DRIFT_TOL` задаёт правило сравнения,
но не заменяет прогоны на других платформах. Единый скрипт —
`ci/run-conformance.sh` — используют и локальный прогон, и self-hosted CI-джоба
(`.github/workflows/native-conformance.yml`).

Локально (нужен Docker):

```sh
# из корня репозитория
docker run --rm -v "$PWD":/src:ro \
    swift:6.1.3@sha256:e1cdaf7ddc9de37d8561da7a260535236694fca8c1b67d3129d47d8b180a9394 \
    bash /src/bindings/swift/ci/run-conformance.sh
```

Историческая аттестация pack `1.0.0` (числа, версии, платформа):
`docs/conformance/local-swift-attestation.md`; она явно superseded для текущего
pack `2.0.0` и не подменяет новый CI-прогон. На нативном macOS (если появится
БЕСПЛАТНЫЙ раннер) — джоба `swift-conformance-macos-reference` (ручной
`workflow_dispatch`) в том же workflow.

## Толерантность conformance

Числовые поля зафиксированных семейств conformant в пределах
`DRIFT_TOL = 1e-6` (SSOT пака:
`crates/labcolors-conformance/src/lib.rs`): байт-точность f64
кросс-платформенно невозможна (libm-шум `powf`/`atan2`/`ln` ~1e-13).
Композит-hex — чистая IEEE-алгебра, точен; solve-hex — квантование
трансцендентного резолва, ±1 LSB на канал.

Glow проверяется другим контрактом: `stable-v1` обязан вернуть типизированный
`Indeterminate` (`site_id` + неразделимое typed `evidence`), если доказанной
границы нет; прежний CAM16/libm-dependent путь доступен только как явный
`legacy-platform-dependent-v1`. Нативный гейт не заявляет CAM16 bit parity:
`bit-exact` относится только к encoded-sRGB8 screen-композиту и его certificate.
