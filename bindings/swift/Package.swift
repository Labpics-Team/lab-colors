// swift-tools-version:5.9
import PackageDescription

// Пакет разработки. Скрипт CI генерирует ABI из текущей Rust-библиотеки.
// Самостоятельная Apple-поставка использует тот же Swift API и XCFramework.
let package = Package(
    name: "LabColors",
    products: [.library(name: "LabColors", targets: ["LabColors"])],
    targets: [
        .systemLibrary(name: "labcolorsFFI", path: "Sources/labcolorsFFI"),
        .target(name: "LabColors", dependencies: ["labcolorsFFI"], path: "Sources/LabColors"),
        .testTarget(
            name: "LabColorsConformanceTests",
            dependencies: ["LabColors"],
            path: "Tests/LabColorsConformanceTests",
            resources: [.copy("Fixtures")]
        ),
    ]
)
