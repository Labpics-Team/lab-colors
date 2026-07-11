// swift-tools-version:5.9
// SwiftPM-пакет нативного биндинга labcolors. Линкует СТАТИЧЕСКУЮ Rust-библиотеку
// (`liblabcolors.a`) + сгенерированные UniFFI Swift-биндинги, и прогоняет
// conformance-пак против рантайм-ядра. Активный CI-гейт собирает пакет в pinned
// Swift-контейнере на Linux x86_64; ручной macOS/arm64 path не является
// достигнутой аттестацией. Файлы в `Sources/LabColors` и
// `Sources/labcolorsFFI` генерирует uniffi-bindgen ПЕРЕД `swift test` (см.
// .github/workflows/native-conformance.yml). В репозитории эти каталоги несут
// лишь .gitkeep — сгенерированное не коммитится.
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
        // Swift-обёртка (сгенерированный labcolors.swift) поверх системного модуля.
        .target(
            name: "LabColors",
            dependencies: ["labcolorsFFI"],
            path: "Sources/LabColors"
        ),
        // Conformance-тесты: грузят закоммиченные векторы из ../../conformance,
        // сверяют их с выходом FFI и отдельно проверяют типизированный Glow
        // decision contract. Линкуют статическую Rust-библиотеку из
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
