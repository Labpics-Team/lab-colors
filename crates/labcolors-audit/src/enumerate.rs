use std::path::Path;

use crate::input::{self, AuditInputError, InputResult};
use crate::types::{ArtifactClass, RawArtifact};

/// Извлекает синтаксический инвентарь Rust и CI из полного читаемого дерева.
/// Отсутствующий каталог, ссылка или повреждённый исходник прерывают весь обход.
pub fn enumerate_production_artifacts(
    source_root: &Path,
) -> Result<Vec<RawArtifact>, AuditInputError> {
    let mut artifacts = Vec::new();
    for path in input::rust_files(source_root)? {
        let content = input::text(&path)?;
        input::rust(&path, &content)?;
        let module = input::relative(source_root, &path)?;
        artifacts.push(RawArtifact {
            class: ArtifactClass::ProductionSourceFile,
            module: module.clone(),
            line: 1,
            raw_key: module.clone(),
            raw_value: None,
        });
        if path.file_name().is_some_and(|name| name == "lib.rs") && is_lib_rs(&path) {
            collect_public_exports(&content, &module, &mut artifacts);
            collect_public_claims(&content, &module, &mut artifacts);
        }
        collect_parallel_ssot(&content, &module, &mut artifacts);
        collect_boundaries(&content, &module, &mut artifacts);
    }
    collect_ci_artifacts(
        source_root,
        &source_root.join(".github/workflows"),
        &mut artifacts,
    )?;
    artifacts.sort_by(|left, right| {
        class_discriminant(left.class)
            .cmp(&class_discriminant(right.class))
            .then_with(|| left.module.cmp(&right.module))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.raw_key.cmp(&right.raw_key))
    });
    Ok(artifacts)
}

fn is_lib_rs(path: &Path) -> bool {
    path.parent()
        .and_then(|p| p.file_name())
        .is_some_and(|name| name == "src")
}

fn collect_public_exports(content: &str, module: &str, out: &mut Vec<RawArtifact>) {
    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub use") {
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

fn collect_ci_artifacts(
    root: &Path,
    directory: &Path,
    out: &mut Vec<RawArtifact>,
) -> InputResult<()> {
    for entry in input::directory(directory)? {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "yml" || extension == "yaml")
        {
            input::text(&path)?;
            let module = input::relative(root, &path)?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| AuditInputError::invalid(&path, "путь не является UTF-8"))?;
            out.push(RawArtifact {
                class: ArtifactClass::CiBuildReleaseDeclaration,
                module,
                line: 1,
                raw_key: name.to_owned(),
                raw_value: None,
            });
        }
    }
    Ok(())
}

/// Извлечение маркеров `// SSOT-TRACKED` и `// GROUNDED` из production-кода.
///
/// Каждый маркер — декларация параллельного SSOT: константа в коде
/// синхронизирована с внешним источником (docs/empirical-inventory.md или
/// опубликованный стандарт). raw_value содержит описание после тире.
fn collect_parallel_ssot(content: &str, module: &str, out: &mut Vec<RawArtifact>) {
    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("// SSOT-TRACKED") || trimmed.starts_with("// GROUNDED") {
            let marker_type = if trimmed.contains("SSOT-TRACKED") {
                "ssot-tracked"
            } else {
                "grounded"
            };

            // Описание после em-dash или " - "
            let description = trimmed
                .split_once('\u{2014}')
                .or_else(|| trimmed.split_once(" - "))
                .map(|(_, desc)| desc.trim())
                .unwrap_or(trimmed);

            out.push(RawArtifact {
                class: ArtifactClass::ParallelSsot,
                module: module.to_string(),
                line: line_idx + 1,
                raw_key: format!("{}:L{}", marker_type, line_idx + 1),
                raw_value: Some(description.to_string()),
            });
        }
    }
}

/// Извлечение публичных утверждений из модульных doc-комментариев `//! #`.
///
/// Секции `//! # Заголовок` в `lib.rs` крейтов декларируют архитектурные
/// законы, контракты и инварианты — публичные утверждения о поведении,
/// подлежащие верификации. raw_value содержит текст заголовка секции.
fn collect_public_claims(content: &str, module: &str, out: &mut Vec<RawArtifact>) {
    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if let Some(heading) = trimmed.strip_prefix("//! #") {
            let heading = heading.trim();
            if !heading.is_empty() {
                out.push(RawArtifact {
                    class: ArtifactClass::PublicClaim,
                    module: module.to_string(),
                    line: line_idx + 1,
                    raw_key: format!("module-doc:{}", heading),
                    raw_value: Some(heading.to_string()),
                });
            }
        }
    }
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
    }
}

/// Атрибуты транспортной границы ищутся в источниках независимо от имени пакета.
fn collect_boundaries(content: &str, module: &str, out: &mut Vec<RawArtifact>) {
    let mut lines = content.lines().enumerate().peekable();
    while let Some((index, line)) = lines.next() {
        let value = line.trim();
        let class = if value == "#[wasm_bindgen]" || value.starts_with("#[wasm_bindgen(") {
            ArtifactClass::WasmBoundary
        } else if value == "#[uniffi::export]" || value.starts_with("#[uniffi::export(") {
            ArtifactClass::NativeBoundary
        } else {
            continue;
        };
        let declaration = lines.peek().map(|(_, line)| line.trim()).unwrap_or("");
        let signature = if declaration.starts_with("pub fn ")
            || declaration.starts_with("pub struct ")
            || declaration.starts_with("impl ")
            || declaration.starts_with("extern ")
        {
            declaration.to_owned()
        } else {
            format!("{value} at L{}", index + 1)
        };
        out.push(RawArtifact {
            class,
            module: module.to_owned(),
            line: index + 1,
            raw_key: signature,
            raw_value: None,
        });
    }
}
