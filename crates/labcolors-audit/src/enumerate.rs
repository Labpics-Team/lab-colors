use std::fs;
use std::path::Path;

use crate::types::{ArtifactClass, RawArtifact};

/// Стадия 1: извлечение сырых артефактов из исходного дерева.
///
/// Обходит `source_root`, извлекая три класса артефактов:
/// - `ProductionSourceFile` — каждый `.rs` файл в `crates/`, исключая тесты, бенчи и build.rs
/// - `PublicExport` — строки `pub use` в `src/lib.rs` каждого крейта
/// - `CiBuildReleaseDeclaration` — файлы `.github/workflows/*.yml`
pub fn enumerate_production_artifacts(source_root: &Path) -> Vec<RawArtifact> {
    let mut artifacts = Vec::new();

    // 1. ProductionSourceFile + PublicExport из crates/
    let crates_dir = source_root.join("crates");
    if crates_dir.is_dir() {
        collect_rust_artifacts(source_root, &crates_dir, &mut artifacts);
    }

    // 2. CiBuildReleaseDeclaration из .github/workflows/
    let workflows_dir = source_root.join(".github").join("workflows");
    if workflows_dir.is_dir() {
        collect_ci_artifacts(source_root, &workflows_dir, &mut artifacts);
    }

    // Детерминированная сортировка: (class discriminant, module, line)
    artifacts.sort_by(|a, b| {
        let class_a = class_discriminant(a.class);
        let class_b = class_discriminant(b.class);
        class_a
            .cmp(&class_b)
            .then_with(|| a.module.cmp(&b.module))
            .then_with(|| a.line.cmp(&b.line))
    });

    artifacts
}

fn collect_rust_artifacts(root: &Path, dir: &Path, out: &mut Vec<RawArtifact>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Исключаем директории tests/ и benches/ на любом уровне
            if name == "tests" || name == "benches" {
                continue;
            }
            collect_rust_artifacts(root, &path, out);
        } else if path.extension().map_or(false, |ext| ext == "rs") {
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Исключаем build.rs
            if file_name == "build.rs" {
                continue;
            }
            // Исключаем *_tests.rs и *_test.rs
            if file_name.ends_with("_tests.rs") || file_name.ends_with("_test.rs") {
                continue;
            }

            let rel = relative_path(root, &path);

            // ProductionSourceFile
            out.push(RawArtifact {
                class: ArtifactClass::ProductionSourceFile,
                module: rel.clone(),
                line: 1,
                raw_key: rel.clone(),
                raw_value: None,
            });

            // PublicExport: сканируем только src/lib.rs
            if file_name == "lib.rs" && is_lib_rs(&path) {
                collect_public_exports(&path, &rel, out);
            }
        }
    }
}

fn is_lib_rs(path: &Path) -> bool {
    path.parent()
        .and_then(|p| p.file_name())
        .map_or(false, |name| name == "src")
}

fn collect_public_exports(lib_path: &Path, module: &str, out: &mut Vec<RawArtifact>) {
    let content = match fs::read_to_string(lib_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub use") {
            // Извлекаем путь после "pub use"
            let reexported = trimmed
                .strip_prefix("pub use")
                .unwrap_or("")
                .trim()
                .trim_end_matches(';')
                .trim();

            if !reexported.is_empty() {
                out.push(RawArtifact {
                    class: ArtifactClass::PublicExport,
                    module: module.to_string(),
                    line: line_idx + 1,
                    raw_key: reexported.to_string(),
                    raw_value: None,
                });
            }
        }
    }
}

fn collect_ci_artifacts(root: &Path, workflows_dir: &Path, out: &mut Vec<RawArtifact>) {
    let entries = match fs::read_dir(workflows_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .map_or(false, |ext| ext == "yml" || ext == "yaml")
        {
            let rel = relative_path(root, &path);
            let file_name = entry.file_name().to_string_lossy().to_string();

            out.push(RawArtifact {
                class: ArtifactClass::CiBuildReleaseDeclaration,
                module: rel,
                line: 1,
                raw_key: file_name,
                raw_value: None,
            });
        }
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Числовой дискриминант для детерминированной сортировки по классу.
fn class_discriminant(class: ArtifactClass) -> u8 {
    match class {
        ArtifactClass::ProductionSourceFile => 0,
        ArtifactClass::PublicRustApi => 1,
        ArtifactClass::PublicExport => 2,
        ArtifactClass::Operation => 3,
        ArtifactClass::ConformanceFamily => 4,
        ArtifactClass::SemanticBranch => 5,
        ArtifactClass::PublicClaim => 6,
        ArtifactClass::ResourceDimension => 7,
        ArtifactClass::DecisionSite => 8,
        ArtifactClass::WasmBoundary => 9,
        ArtifactClass::NativeBoundary => 10,
        ArtifactClass::CiBuildReleaseDeclaration => 11,
        ArtifactClass::ParallelSsot => 12,
        ArtifactClass::GraphArtifactTest => 13,
    }
}
