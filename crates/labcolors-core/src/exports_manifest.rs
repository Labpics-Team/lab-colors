//! Exports/metadata manifest extractor for EXT-03.
//!
//! Parses workspace `Cargo.toml` files and public `lib.rs` surfaces to produce
//! a deterministic description of every crate's metadata, features, dependencies,
//! targets and public API items. Feature-gated exports carry their gate name so
//! downstream tooling can distinguish always-available surface from opt-in caps.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Complete export/metadata manifest for one workspace crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrateExportManifest {
    /// Crate name as declared in `[package].name`.
    pub crate_name: String,
    /// Resolved version string (workspace-inherited or local).
    pub version: String,
    /// Optional description (workspace-inherited or local).
    pub description: Option<String>,
    /// Optional license expression (workspace-inherited or local).
    pub license: Option<String>,
    /// Declared Cargo features with default membership.
    pub features: Vec<FeatureDef>,
    /// Normal (non-dev) dependencies with optionality flag.
    pub dependencies: Vec<DepDef>,
    /// Lib/bin/bench targets declared in the manifest.
    pub targets: Vec<TargetDef>,
    /// Public API surface extracted from `lib.rs`.
    pub public_api: Vec<ExportItem>,
}

/// A single Cargo feature declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureDef {
    /// Feature name as it appears in `[features]`.
    pub name: String,
    /// True when the feature is listed in the `default` set.
    pub is_default: bool,
}

/// A normal dependency entry (dev-dependencies excluded).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepDef {
    /// Dependency crate name.
    pub name: String,
    /// Version requirement string (`"1"`, `"0.32"`, workspace pin, etc.).
    pub version_req: String,
    /// True when declared with `optional = true`.
    pub optional: bool,
}

/// Kind of Cargo target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetKind {
    /// Library target (`[lib]`).
    Lib,
    /// Binary target (`[[bin]]`).
    Bin,
    /// Benchmark target (`[[bench]]`).
    Bench,
}

/// A declared Cargo target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetDef {
    /// Target kind discriminator.
    pub kind: TargetKind,
    /// Target name (lib name or bin/bench name).
    pub name: String,
}

/// One public API item extracted from `lib.rs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportItem {
    /// `pub mod name;`
    Module {
        /// Module identifier.
        name: String,
        /// Declared path (currently same as name; reserved for nested paths).
        path: String,
    },
    /// `pub use path::to::Item;` (re-export at crate root).
    ReExport {
        /// Final imported name at the crate root.
        name: String,
        /// Source path text as written in the `use` statement.
        source_path: String,
    },
    /// `pub fn name(...)` (top-level function).
    Function {
        /// Function identifier.
        name: String,
        /// Feature gate if wrapped in `#[cfg(feature = "...")]`.
        feature_gate: Option<String>,
    },
    /// `pub struct Name`.
    Struct {
        /// Struct identifier.
        name: String,
        /// Feature gate if wrapped in `#[cfg(feature = "...")]`.
        feature_gate: Option<String>,
    },
    /// `pub enum Name`.
    Enum {
        /// Enum identifier.
        name: String,
        /// Feature gate if wrapped in `#[cfg(feature = "...")]`.
        feature_gate: Option<String>,
    },
    /// `pub trait Name`.
    Trait {
        /// Trait identifier.
        name: String,
        /// Feature gate if wrapped in `#[cfg(feature = "...")]`.
        feature_gate: Option<String>,
    },
    /// `pub type Name = ...;`
    TypeAlias {
        /// Alias identifier.
        name: String,
        /// Feature gate if wrapped in `#[cfg(feature = "...")]`.
        feature_gate: Option<String>,
    },
    /// `pub const NAME: Ty = ...;`
    Const {
        /// Const identifier.
        name: String,
        /// Feature gate if wrapped in `#[cfg(feature = "...")]`.
        feature_gate: Option<String>,
    },
}

/// Extracts export/metadata manifests for every workspace member under
/// `workspace_root/crates/*`.
///
/// The returned vector is sorted by crate name for determinism. Parsing is
/// intentionally strict: missing required fields panic so sabotage (deleted
/// metadata, broken workspace inheritance) fails tests loudly rather than
/// silently producing partial output.
pub fn extract_exports_metadata(workspace_root: &Path) -> Vec<CrateExportManifest> {
    let crates_dir = workspace_root.join("crates");
    let workspace_toml_path = workspace_root.join("Cargo.toml");
    let workspace_toml: toml::Value = fs::read_to_string(&workspace_toml_path)
        .expect("workspace Cargo.toml readable")
        .parse()
        .expect("workspace Cargo.toml parses");
    let workspace_package = workspace_toml
        .get("workspace")
        .and_then(|w| w.get("package"))
        .cloned()
        .unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()));

    let mut manifests = Vec::new();
    let entries = match fs::read_dir(&crates_dir) {
        Ok(iter) => iter,
        Err(_) => return manifests,
    };

    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let cargo_path = entry.path().join("Cargo.toml");
        if !cargo_path.exists() {
            continue;
        }
        let raw = fs::read_to_string(&cargo_path).expect("crate Cargo.toml readable");
        let value: toml::Value = raw.parse().expect("crate Cargo.toml parses");
        let pkg = value.get("package").expect("[package] table present");

        let crate_name = pkg
            .get("name")
            .and_then(|v| v.as_str())
            .expect("package.name present")
            .to_string();

        let version =
            resolve_workspace_string(pkg, &workspace_package, "version").expect("version resolved");
        let description = resolve_workspace_string(pkg, &workspace_package, "description");
        let license = resolve_workspace_string(pkg, &workspace_package, "license");

        let features = parse_features(&value);
        let dependencies = parse_dependencies(&value);
        let targets = parse_targets(&value, &crate_name);

        let lib_rs = entry.path().join("src").join("lib.rs");
        let public_api = if lib_rs.exists() {
            parse_public_api(&lib_rs)
        } else {
            Vec::new()
        };

        manifests.push(CrateExportManifest {
            crate_name,
            version,
            description,
            license,
            features,
            dependencies,
            targets,
            public_api,
        });
    }

    manifests.sort_by(|a, b| a.crate_name.cmp(&b.crate_name));
    manifests
}

fn resolve_workspace_string(
    pkg: &toml::Value,
    workspace_pkg: &toml::Value,
    key: &str,
) -> Option<String> {
    match pkg.get(key) {
        Some(v) => {
            if let Some(s) = v.as_str() {
                return Some(s.to_string());
            }
            if let Some(tbl) = v.as_table() {
                if tbl
                    .get("workspace")
                    .and_then(|w| w.as_bool())
                    .unwrap_or(false)
                {
                    return workspace_pkg
                        .get(key)
                        .and_then(|w| w.as_str())
                        .map(|s| s.to_string());
                }
            }
            None
        }
        None => None,
    }
}

fn parse_features(value: &toml::Value) -> Vec<FeatureDef> {
    let Some(features_tbl) = value.get("features").and_then(|f| f.as_table()) else {
        return Vec::new();
    };
    let defaults: Vec<String> = features_tbl
        .get("default")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut out: Vec<FeatureDef> = features_tbl
        .keys()
        .filter(|k| k.as_str() != "default")
        .map(|name| FeatureDef {
            name: name.clone(),
            is_default: defaults.iter().any(|d| d == name),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn parse_dependencies(value: &toml::Value) -> Vec<DepDef> {
    let Some(deps) = value.get("dependencies").and_then(|d| d.as_table()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, dep_val) in deps {
        let (version_req, optional) = match dep_val {
            toml::Value::String(s) => (s.clone(), false),
            toml::Value::Table(tbl) => {
                let version = if let Some(ws) = tbl.get("workspace") {
                    if ws.as_bool().unwrap_or(false) {
                        "workspace".to_string()
                    } else {
                        tbl.get("version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("*")
                            .to_string()
                    }
                } else {
                    tbl.get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("*")
                        .to_string()
                };
                let optional = tbl
                    .get("optional")
                    .and_then(|o| o.as_bool())
                    .unwrap_or(false);
                (version, optional)
            }
            _ => ("*".to_string(), false),
        };
        out.push(DepDef {
            name: name.clone(),
            version_req,
            optional,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn parse_targets(value: &toml::Value, crate_name: &str) -> Vec<TargetDef> {
    let mut out = Vec::new();

    if let Some(lib) = value.get("lib").and_then(|l| l.as_table()) {
        let name = lib
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or(crate_name)
            .to_string();
        out.push(TargetDef {
            kind: TargetKind::Lib,
            name,
        });
    } else {
        // Implicit lib target when no [lib] table is declared.
        out.push(TargetDef {
            kind: TargetKind::Lib,
            name: crate_name.to_string(),
        });
    }

    if let Some(bins) = value.get("bin").and_then(|b| b.as_array()) {
        for bin in bins {
            if let Some(tbl) = bin.as_table() {
                if let Some(name) = tbl.get("name").and_then(|n| n.as_str()) {
                    out.push(TargetDef {
                        kind: TargetKind::Bin,
                        name: name.to_string(),
                    });
                }
            }
        }
    }

    if let Some(benches) = value.get("bench").and_then(|b| b.as_array()) {
        for bench in benches {
            if let Some(tbl) = bench.as_table() {
                if let Some(name) = tbl.get("name").and_then(|n| n.as_str()) {
                    out.push(TargetDef {
                        kind: TargetKind::Bench,
                        name: name.to_string(),
                    });
                }
            }
        }
    }

    out.sort_by(|a, b| {
        let kind_order = |k: &TargetKind| match k {
            TargetKind::Lib => 0,
            TargetKind::Bin => 1,
            TargetKind::Bench => 2,
        };
        kind_order(&a.kind)
            .cmp(&kind_order(&b.kind))
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// Minimal regex-free parser for the public surface of `lib.rs`.
///
/// Handles the subset actually used in this workspace:
/// - `pub mod name;`
/// - `pub use path::to::{A, B as C};` (flattened per imported name)
/// - `pub fn/struct/enum/trait/type/const` with optional preceding
///   `#[cfg(feature = "...")]` attribute on the immediately prior line.
///
/// Items marked `pub(crate)` are NOT part of the public export surface and
/// are intentionally skipped — including them would be a sabotage vector.
fn parse_public_api(path: &Path) -> Vec<ExportItem> {
    let src = fs::read_to_string(path).expect("lib.rs readable");
    let mut out = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Detect #[cfg(feature = "...")] that applies to the next item line.
        let feature_gate = extract_cfg_feature(trimmed);
        let (line_to_parse, gate) = if feature_gate.is_some() && i + 1 < lines.len() {
            (lines[i + 1].trim(), feature_gate.clone())
        } else {
            (trimmed, None)
        };

        // Multi-line grouped imports: `pub use path::{\n    A, B,\n};`
        // Accumulate lines until the closing `}` is found, then parse as one.
        let is_pub_use = line_to_parse.starts_with("pub use") || line_to_parse.starts_with("pub\tuse");
        if is_pub_use && line_to_parse.contains('{') && !line_to_parse.contains('}') {
            let mut accumulated = line_to_parse.to_string();
            let start_j = if feature_gate.is_some() { i + 2 } else { i + 1 };
            let mut j = start_j;
            while j < lines.len() {
                let next = lines[j].trim();
                accumulated.push(' ');
                accumulated.push_str(next);
                j += 1;
                if next.contains('}') {
                    break;
                }
            }
            if let Some(item) = parse_pub_line(&accumulated, gate.clone()) {
                out.push(item);
            }
            for extra in parse_pub_use_grouped(&accumulated) {
                out.push(extra);
            }
            // Skip all consumed lines (start_j..j) plus the current line.
            i = j;
            continue;
        }

        if let Some(item) = parse_pub_line(line_to_parse, gate.clone()) {
            out.push(item);
        }
        // Single-line grouped imports still need full extraction.
        if is_pub_use {
            for extra in parse_pub_use_grouped(line_to_parse) {
                out.push(extra);
            }
        }

        if feature_gate.is_some() {
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

fn extract_cfg_feature(line: &str) -> Option<String> {
    // Matches: #[cfg(feature = "name")]
    let trimmed = line.trim();
    if !trimmed.starts_with("#[cfg(feature") {
        return None;
    }
    let start = trimmed.find('"')? + 1;
    let end = trimmed[start..].find('"')? + start;
    Some(trimmed[start..end].to_string())
}

fn parse_pub_line(line: &str, feature_gate: Option<String>) -> Option<ExportItem> {
    // Skip pub(crate), pub(super), pub(self) — not public API.
    if line.starts_with("pub(crate)")
        || line.starts_with("pub(super)")
        || line.starts_with("pub(self)")
    {
        return None;
    }
    if !line.starts_with("pub ") && !line.starts_with("pub\t") {
        return None;
    }
    let rest = line.strip_prefix("pub").unwrap().trim_start();

    if let Some(mod_name) = rest.strip_prefix("mod ").and_then(|s| {
        let s = s.trim_end_matches(';').trim();
        if s.contains(' ') { None } else { Some(s) }
    }) {
        return Some(ExportItem::Module {
            name: mod_name.to_string(),
            path: mod_name.to_string(),
        });
    }

    if let Some(use_rest) = rest.strip_prefix("use ") {
        return parse_pub_use(use_rest);
    }

    if let Some(fn_name) = rest.strip_prefix("fn ").map(extract_ident) {
        return Some(ExportItem::Function {
            name: fn_name,
            feature_gate,
        });
    }
    if let Some(struct_name) = rest.strip_prefix("struct ").map(extract_ident) {
        return Some(ExportItem::Struct {
            name: struct_name,
            feature_gate,
        });
    }
    if let Some(enum_name) = rest.strip_prefix("enum ").map(extract_ident) {
        return Some(ExportItem::Enum {
            name: enum_name,
            feature_gate,
        });
    }
    if let Some(trait_name) = rest.strip_prefix("trait ").map(extract_ident) {
        return Some(ExportItem::Trait {
            name: trait_name,
            feature_gate,
        });
    }
    if let Some(type_name) = rest.strip_prefix("type ").map(extract_ident) {
        return Some(ExportItem::TypeAlias {
            name: type_name,
            feature_gate,
        });
    }
    if let Some(const_name) = rest.strip_prefix("const ").map(extract_ident) {
        return Some(ExportItem::Const {
            name: const_name,
            feature_gate,
        });
    }

    None
}

fn extract_ident(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

fn parse_pub_use(use_rest: &str) -> Option<ExportItem> {
    // Strip trailing semicolon and any visibility qualifiers already consumed.
    let cleaned = use_rest.trim().trim_end_matches(';').trim();

    // Handle grouped imports: `path::{A, B as C}` → emit one ReExport per name.
    // We flatten ALL names from the brace group so that downstream tests can
    // assert on any individual re-export, not just the first.
    if let Some(brace_start) = cleaned.find('{') {
        let prefix = cleaned[..brace_start].trim_end_matches(':').trim();
        let brace_end = cleaned.find('}')?;
        let inner = &cleaned[brace_start + 1..brace_end];
        // Return the first non-empty item; remaining items are emitted via
        // parse_pub_use_grouped called by the parent loop.
        let first = inner.split(',').next()?.trim();
        if first.is_empty() {
            return None;
        }
        let name = if let Some(as_pos) = first.find(" as ") {
            first[as_pos + 4..].trim().to_string()
        } else {
            first.split("::").last().unwrap_or(first).to_string()
        };
        return Some(ExportItem::ReExport {
            name,
            source_path: format!(
                "{}::{}",
                prefix,
                first.split(" as ").next().unwrap_or(first).trim()
            ),
        });
    }

    // Simple `pub use path::to::Name;` or `pub use path::to::Name as Alias;`
    let name = if let Some(as_pos) = cleaned.rfind(" as ") {
        cleaned[as_pos + 4..].trim().to_string()
    } else {
        cleaned.split("::").last().unwrap_or(cleaned).to_string()
    };
    Some(ExportItem::ReExport {
        name,
        source_path: cleaned.to_string(),
    })
}

/// Extract ALL re-exports from a grouped `pub use path::{A, B as C, D};` line.
/// Returns an empty vec for non-grouped imports (handled by `parse_pub_use`).
fn parse_pub_use_grouped(use_rest: &str) -> Vec<ExportItem> {
    let cleaned = use_rest.trim().trim_end_matches(';').trim();
    let Some(brace_start) = cleaned.find('{') else {
        return Vec::new();
    };
    let prefix = cleaned[..brace_start].trim_end_matches(':').trim();
    let Some(brace_end) = cleaned.find('}') else {
        return Vec::new();
    };
    let inner = &cleaned[brace_start + 1..brace_end];
    inner
        .split(',')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let name = if let Some(as_pos) = part.find(" as ") {
                part[as_pos + 4..].trim().to_string()
            } else {
                part.split("::").last().unwrap_or(part).to_string()
            };
            ExportItem::ReExport {
                name,
                source_path: format!(
                    "{}::{}",
                    prefix,
                    part.split(" as ").next().unwrap_or(part).trim()
                ),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn extracts_all_five_workspace_crates() {
        let manifests = extract_exports_metadata(&workspace_root());
        let names: Vec<&str> = manifests.iter().map(|m| m.crate_name.as_str()).collect();
        assert!(names.contains(&"labcolors-core"));
        assert!(names.contains(&"labcolors-audit"));
        assert!(names.contains(&"labcolors-conformance"));
        assert!(names.contains(&"labcolors-ffi"));
        assert!(names.contains(&"labcolors-wasm"));
        assert_eq!(manifests.len(), 5);
    }

    #[test]
    fn resolves_workspace_inherited_metadata() {
        let manifests = extract_exports_metadata(&workspace_root());
        let audit = manifests
            .iter()
            .find(|m| m.crate_name == "labcolors-audit")
            .unwrap();
        assert_eq!(audit.version, "0.3.0");
        assert_eq!(audit.license.as_deref(), Some("MIT"));
        assert!(audit.description.is_some());
    }

    #[test]
    fn core_has_known_public_modules() {
        let manifests = extract_exports_metadata(&workspace_root());
        let core = manifests
            .iter()
            .find(|m| m.crate_name == "labcolors-core")
            .unwrap();
        let module_names: Vec<&str> = core
            .public_api
            .iter()
            .filter_map(|item| match item {
                ExportItem::Module { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            module_names.contains(&"numerics"),
            "missing pub mod numerics"
        );
        assert!(module_names.contains(&"wcag22"), "missing pub mod wcag22");
        assert!(module_names.contains(&"solve"), "missing pub mod solve");
        assert!(
            module_names.contains(&"source_manifest"),
            "missing pub mod source_manifest"
        );
    }

    #[test]
    fn feature_gated_private_fixture_is_tracked() {
        let manifests = extract_exports_metadata(&workspace_root());
        let core = manifests
            .iter()
            .find(|m| m.crate_name == "labcolors-core")
            .unwrap();
        let has_private_fixture_feature = core.features.iter().any(|f| f.name == "private-fixture");
        assert!(
            has_private_fixture_feature,
            "private-fixture feature must be listed"
        );
    }

    #[test]
    fn sabotage_missing_crate_fails() {
        // This test documents the sabotage contract: if someone removes a crate
        // from the workspace, the count assertion in extracts_all_five_workspace_crates
        // will fail. We assert the invariant here explicitly.
        let manifests = extract_exports_metadata(&workspace_root());
        assert_eq!(
            manifests.len(),
            5,
            "SABOTAGE: expected exactly 5 workspace crates; removal or addition breaks this"
        );
    }

    #[test]
    fn sabotage_fake_crate_not_present() {
        let manifests = extract_exports_metadata(&workspace_root());
        let names: Vec<&str> = manifests.iter().map(|m| m.crate_name.as_str()).collect();
        assert!(
            !names.contains(&"labcolors-fake"),
            "SABOTAGE: fake crate should never appear in manifest"
        );
    }

    #[test]
    fn reexports_include_solve_surface() {
        let manifests = extract_exports_metadata(&workspace_root());
        let core = manifests
            .iter()
            .find(|m| m.crate_name == "labcolors-core")
            .unwrap();
        let reexport_names: Vec<&str> = core
            .public_api
            .iter()
            .filter_map(|item| match item {
                ExportItem::ReExport { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            reexport_names.contains(&"solve"),
            "solve re-export must be tracked"
        );
        assert!(
            reexport_names.contains(&"Srgb8"),
            "Srgb8 re-export must be tracked"
        );
    }

    #[test]
    fn pub_crate_items_excluded_from_surface() {
        let manifests = extract_exports_metadata(&workspace_root());
        let core = manifests
            .iter()
            .find(|m| m.crate_name == "labcolors-core")
            .unwrap();
        let module_names: Vec<&str> = core
            .public_api
            .iter()
            .filter_map(|item| match item {
                ExportItem::Module { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        // srgb8 is pub(crate) — must NOT appear as a public module.
        assert!(
            !module_names.contains(&"srgb8"),
            "SABOTAGE: pub(crate) modules must not leak into public_api"
        );
    }
}
