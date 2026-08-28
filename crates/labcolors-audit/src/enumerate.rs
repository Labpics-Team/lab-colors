use std::path::Path;

use crate::types::RawArtifact;

/// Стадия 1: извлечение сырых артефактов из исходного дерева.
///
/// На данном этапе — stub, возвращающий пустой вектор. Полная реализация
/// будет обходить `source_root`, парсить Rust-файлы и извлекать публичные
/// API, экспорты, операции и другие классы артефактов по таксономии AUD-01.
pub fn enumerate_production_artifacts(_source_root: &Path) -> Vec<RawArtifact> {
    // TODO(AUD-01): реализовать обход дерева и извлечение артефактов
    Vec::new()
}
