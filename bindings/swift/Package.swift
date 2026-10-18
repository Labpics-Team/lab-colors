// swift-tools-version:5.9
// SwiftPM-пакет нативного биндинга labcolors связывает Rust-библиотеку
// со сгенерированными UniFFI Swift-биндингами и проверяет
// conformance-векторы против ядра. Активный CI-гейт собирает пакет в pinned
// Swift-контейнере на Linux x86_64; ручной macOS/arm64 path не является
// достигнутой аттестацией. Файлы в `Sources/LabColors` и
// `Sources/labcolorsFFI` генерирует uniffi-bindgen ПЕРЕД `swift test` (см.
// .github/workflows/native-conformance.yml); сгенерированное не коммитится.
import PackageDescription

let package = Package(
    name: "LabColors",
    products: [
        .library(name: "LabColors", targets: ["LabColors"])
    ],
    targets: [
        // Системный модуль: C-заголовок + module.modulemap (генерируется как
        // labcolorsFFI.h + labcolorsFFI.modulemap → переименовывается в
        // module.modulemap в CI). Экспортирует extern-C FFI-символы ядра.
        .systemLibrary(name: "labcolorsFFI", path: "Sources/labcolorsFFI"),
        // Модуль labcolors.swift, сгенерированный из текущего FFI-контракта.
        .target(
            name: "LabColors",
            dependencies: ["labcolorsFFI"],
            path: "Sources/LabColors"
        ),
        // Conformance-тесты: грузят закоммиченные векторы из ../../conformance,
        // сверяют их с выходом FFI. Линкуют Rust-библиотеку из
        // target/debug (путь относительно корня пакета bindings/swift).
        .testTarget(
            name: "LabColorsConformanceTests",
            dependencies: ["LabColors"],
            path: "Tests/LabColorsConformanceTests",
            linkerSettings: [
                .unsafeFlags([
                    "-L../../target/debug",
                    "-llabcolors",
                ])
            ]
        ),
    ]
)
