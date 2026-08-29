use std::fs;
use std::path::Path;

use serde::Deserialize;

/// Одна зависимость, извлечённая из Cargo.toml крейта.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyEntry {
    pub crate_name: String,
    pub dep_name: String,
    pub version: String,
    pub features: Vec<String>,
    pub optional: bool,
    pub path_dep: bool,
    pub section: String,
}

#[derive(Deserialize)]
struct CargoToml {
    package: Option<PackageSection>,
    dependencies: Option<toml::Table>,
    #[serde(rename = "dev-dependencies")]
    dev_dependencies: Option<toml::Table>,
    #[serde(rename = "build-dependencies")]
    build_dependencies: Option<toml::Table>,
}

#[derive(Deserialize)]
struct PackageSection {
    name: String,
}

/// Извлекает зависимости всех крейтов workspace из `crates/*/Cargo.toml`.
///
/// Результат детерминированно отсортирован по (crate_name, section, dep_name).
pub fn extract_dependencies(workspace_root: &Path) -> Vec<DependencyEntry> {
    let mut entries = Vec::new();
    let crates_dir = workspace_root.join("crates");

    let dir_entries = match fs::read_dir(&crates_dir) {
        Ok(e) => e,
        Err(_) => return entries,
    };

    for entry in dir_entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let cargo_toml_path = path.join("Cargo.toml");
        if !cargo_toml_path.is_file() {
            continue;
        }
        if let Some(crate_entries) = parse_cargo_toml(&cargo_toml_path) {
            entries.extend(crate_entries);
        }
    }

    entries.sort_by(|a, b| {
        a.crate_name
            .cmp(&b.crate_name)
            .then_with(|| a.section.cmp(&b.section))
            .then_with(|| a.dep_name.cmp(&b.dep_name))
    });

    entries
}

fn parse_cargo_toml(path: &Path) -> Option<Vec<DependencyEntry>> {
    let content = fs::read_to_string(path).ok()?;
    let manifest: CargoToml = toml::from_str(&content).ok()?;
    let crate_name = manifest.package.as_ref()?.name.clone();

    let mut entries = Vec::new();

    collect_section(
        &manifest.dependencies,
        &crate_name,
        "dependencies",
        &mut entries,
    );
    collect_section(
        &manifest.dev_dependencies,
        &crate_name,
        "dev-dependencies",
        &mut entries,
    );
    collect_section(
        &manifest.build_dependencies,
        &crate_name,
        "build-dependencies",
        &mut entries,
    );

    Some(entries)
}

fn collect_section(
    table: &Option<toml::Table>,
    crate_name: &str,
    section: &str,
    out: &mut Vec<DependencyEntry>,
) {
    let Some(table) = table else {
        return;
    };

    for (dep_name, value) in table {
        let (version, features, optional, path_dep) = parse_dep_value(value);
        out.push(DependencyEntry {
            crate_name: crate_name.to_string(),
            dep_name: dep_name.clone(),
            version,
            features,
            optional,
            path_dep,
            section: section.to_string(),
        });
    }
}

fn parse_dep_value(value: &toml::Value) -> (String, Vec<String>, bool, bool) {
    match value {
        toml::Value::String(v) => (v.clone(), Vec::new(), false, false),
        toml::Value::Table(t) => {
            let version = t
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let features = t
                .get("features")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let optional = t.get("optional").and_then(|v| v.as_bool()).unwrap_or(false);

            let path_dep = t.get("path").is_some();

            (version, features, optional, path_dep)
        }
        _ => (String::new(), Vec::new(), false, false),
    }
}
