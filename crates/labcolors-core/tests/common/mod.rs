//! Общие вспомогательные функции для интеграционных тестов.
//!
//! Модуль подключается через `mod common;` в каждом тестовом файле,
//! которому нужен путь к SSOT-инвентарю или к директории исходников.
//!
//! `allow(dead_code)`: модуль компилируется отдельно в каждый тест-бинарь, а
//! разные бинари используют разные подмножества хелперов (например,
//! `agnostic_production_surface` берёт только `src_dir`, `empirical_inventory` — все
//! три). Неиспользованный в конкретном бинаре хелпер — не мёртвый код, а общий
//! инструмент; глушим ложный `dead_code` на уровне модуля.
#![allow(dead_code)]

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
