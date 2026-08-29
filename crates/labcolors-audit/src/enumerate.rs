use std::fs;
use std::path::Path;

use crate::types::{ArtifactClass, RawArtifact};

/// РЎС‚Р°РґРёСЏ 1: РёР·РІР»РµС‡РµРЅРёРµ СЃС‹СЂС‹С… Р°СЂС‚РµС„Р°РєС‚РѕРІ РёР· РёСЃС…РѕРґРЅРѕРіРѕ РґРµСЂРµРІР°.
///
/// РћР±С…РѕРґРёС‚ `source_root`, РёР·РІР»РµРєР°СЏ С‚СЂРё РєР»Р°СЃСЃР° Р°СЂС‚РµС„Р°РєС‚РѕРІ:
/// - `ProductionSourceFile` вЂ” РєР°Р¶РґС‹Р№ `.rs` С„Р°Р№Р» РІ `crates/`, РёСЃРєР»СЋС‡Р°СЏ С‚РµСЃС‚С‹, Р±РµРЅС‡Рё Рё build.rs
/// - `PublicExport` вЂ” СЃС‚СЂРѕРєРё `pub use` РІ `src/lib.rs` РєР°Р¶РґРѕРіРѕ РєСЂРµР№С‚Р°
/// - `CiBuildReleaseDeclaration` вЂ” С„Р°Р№Р»С‹ `.github/workflows/*.yml`
pub fn enumerate_production_artifacts(source_root: &Path) -> Vec<RawArtifact> {
    let mut artifacts = Vec::new();

    // 1. ProductionSourceFile + PublicExport РёР· crates/
    let crates_dir = source_root.join("crates");
    if crates_dir.is_dir() {
        collect_rust_artifacts(source_root, &crates_dir, &mut artifacts);
    }

    // 2. CiBuildReleaseDeclaration РёР· .github/workflows/
    let workflows_dir = source_root.join(".github").join("workflows");
    if workflows_dir.is_dir() {
        collect_ci_artifacts(source_root, &workflows_dir, &mut artifacts);
    }

    // 3. WasmBoundary из crates/labcolors-wasm/src/
    let wasm_src = source_root.join("crates").join("labcolors-wasm").join("src");
    if wasm_src.is_dir() {
        collect_wasm_boundaries(source_root, &wasm_src, &mut artifacts);
    }

    // 4. NativeBoundary из crates/labcolors-ffi/src/
    let ffi_src = source_root.join("crates").join("labcolors-ffi").join("src");
    if ffi_src.is_dir() {
        collect_native_boundaries(source_root, &ffi_src, &mut artifacts);
    }

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
            // РСЃРєР»СЋС‡Р°РµРј РґРёСЂРµРєС‚РѕСЂРёРё tests/ Рё benches/ РЅР° Р»СЋР±РѕРј СѓСЂРѕРІРЅРµ
            if name == "tests" || name == "benches" {
                continue;
            }
            collect_rust_artifacts(root, &path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let file_name = entry.file_name().to_string_lossy().to_string();

            // РСЃРєР»СЋС‡Р°РµРј build.rs
            if file_name == "build.rs" {
                continue;
            }
            // РСЃРєР»СЋС‡Р°РµРј *_tests.rs Рё *_test.rs
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

            // PublicExport: СЃРєР°РЅРёСЂСѓРµРј С‚РѕР»СЊРєРѕ src/lib.rs
            if file_name == "lib.rs" && is_lib_rs(&path) {
                collect_public_exports(&path, &rel, out);
            }
        }
    }
}

fn is_lib_rs(path: &Path) -> bool {
    path.parent()
        .and_then(|p| p.file_name())
        .is_some_and(|name| name == "src")
}

fn collect_public_exports(lib_path: &Path, module: &str, out: &mut Vec<RawArtifact>) {
    let content = match fs::read_to_string(lib_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub use") {
            // РР·РІР»РµРєР°РµРј РїСѓС‚СЊ РїРѕСЃР»Рµ "pub use"
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
                .is_some_and(|ext| ext == "yml" || ext == "yaml")
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

/// Р§РёСЃР»РѕРІРѕР№ РґРёСЃРєСЂРёРјРёРЅР°РЅС‚ РґР»СЏ РґРµС‚РµСЂРјРёРЅРёСЂРѕРІР°РЅРЅРѕР№ СЃРѕСЂС‚РёСЂРѕРІРєРё РїРѕ РєР»Р°СЃСЃСѓ.
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


/// Извлечение WasmBoundary артефактов из crates/labcolors-wasm/src/.
///
/// Сканирует все .rs файлы в wasm crate на наличие #[wasm_bindgen] атрибутов.
/// Каждый атрибут маркирует публичную JS-доступную функцию, struct или impl блок.
fn collect_wasm_boundaries(root: &Path, wasm_src: &Path, out: &mut Vec<RawArtifact>) {
    for path in collect_rs_files(wasm_src) {
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel = relative_path(root, &path);
        let mut lines = content.lines().enumerate().peekable();

        while let Some((line_idx, line)) = lines.next() {
            let trimmed = line.trim();
            if trimmed == "#[wasm_bindgen]" {
                let decl = lines.peek()
                    .map(|(_, l)| l.trim())
                    .unwrap_or("");

                let signature = if decl.starts_with("pub fn ") || decl.starts_with("pub struct ") || decl.starts_with("impl ") || decl.starts_with("extern ") {
                    decl.to_string()
                } else {
                    format!("#[wasm_bindgen] at L{}", line_idx + 1)
                };

                out.push(RawArtifact {
                    class: ArtifactClass::WasmBoundary,
                    module: rel.clone(),
                    line: line_idx + 1,
                    raw_key: signature,
                    raw_value: None,
                });
            }
        }
    }
}


/// Итеративный обход всех .rs файлов в директории (без рекурсии).
fn collect_rs_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut result = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() && path.extension().is_some_and(|e| e == "rs") {
                result.push(path);
            }
        }
    }
    result
}

/// Извлечение NativeBoundary артефактов из crates/labcolors-ffi/src/.
///
/// Сканирует все .rs файлы в ffi crate на наличие #[uniffi::export] атрибутов.
/// Каждый атрибут маркирует публичную FFI-доступную функцию.
fn collect_native_boundaries(root: &Path, ffi_src: &Path, out: &mut Vec<RawArtifact>) {
    for path in collect_rs_files(ffi_src) {
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel = relative_path(root, &path);
        let mut lines = content.lines().enumerate().peekable();

        while let Some((line_idx, line)) = lines.next() {
            let trimmed = line.trim();
            if trimmed == "#[uniffi::export]" {
                let decl = lines.peek()
                    .map(|(_, l)| l.trim())
                    .unwrap_or("");

                let signature = if decl.starts_with("pub fn ") {
                    decl.to_string()
                } else {
                    format!("#[uniffi::export] at L{}", line_idx + 1)
                };

                out.push(RawArtifact {
                    class: ArtifactClass::NativeBoundary,
                    module: rel.clone(),
                    line: line_idx + 1,
                    raw_key: signature,
                    raw_value: None,
                });
            }
        }
    }
}