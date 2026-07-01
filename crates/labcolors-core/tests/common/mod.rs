//! Общие вспомогательные функции для интеграционных тестов.
//!
//! Модуль подключается через `mod common;` в каждом тестовом файле,
//! которому нужен путь к SSOT-инвентарю или к директории исходников.

use std::path::PathBuf;

/// Корневая директория крейта (`crates/labcolors-core/`).
/// Определяется через `CARGO_MANIFEST_DIR` — не зависит от рабочего каталога.
pub fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Директория исходников крейта (`crates/labcolors-core/src/`).
pub fn src_dir() -> PathBuf {
    crate_root().join("src")
}

/// Путь к SSOT-инвентарю параметров (`docs/empirical-inventory.md`)
/// относительно корня воркспейса (два уровня выше корня крейта).
pub fn inventory_path() -> PathBuf {
    crate_root()
        .join("..")
        .join("..")
        .join("docs")
        .join("empirical-inventory.md")
}
