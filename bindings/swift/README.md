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

## Сборка и тест (только macOS)

На Windows/Linux Swift-стороны нет — сборка и `swift test` идут в CI-джобе на
macos-раннере (`.github/workflows/native-conformance.yml`). Локально на macOS:

```sh
# из корня репозитория
cargo build -p labcolors-ffi --features cli
cargo run -p labcolors-ffi --features cli --bin uniffi-bindgen -- \
    generate --library target/debug/liblabcolors.dylib --language swift \
    --out-dir /tmp/gen
# разложить сгенерированное в пакет
cp /tmp/gen/labcolors.swift          bindings/swift/Sources/LabColors/
cp /tmp/gen/labcolorsFFI.h           bindings/swift/Sources/labcolorsFFI/
cp /tmp/gen/labcolorsFFI.modulemap   bindings/swift/Sources/labcolorsFFI/module.modulemap
# тест
cd bindings/swift && swift test
```

## Толерантность conformance

Числовые поля conformant в пределах `DRIFT_TOL = 1e-6` (канон ядра, `lut.rs`):
байт-точность f64 кросс-платформенно невозможна (libm-шум `powf`/`atan2`/`ln`
~1e-13). Композит-hex — чистая IEEE-алгебра, точен; solve-hex — квантование
трансцендентного резолва, ±1 LSB на канал.
