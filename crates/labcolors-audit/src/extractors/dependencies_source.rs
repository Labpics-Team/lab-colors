use crate::input::{self, AuditInputError};
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
pub fn extract_dependencies(
    workspace_root: &Path,
) -> Result<Vec<DependencyEntry>, AuditInputError> {
    let mut entries = Vec::new();
    for path in input::crate_directories(workspace_root)? {
        entries.extend(parse_cargo_toml(&path.join("Cargo.toml"))?);
    }

    entries.sort_by(|a, b| {
        a.crate_name
            .cmp(&b.crate_name)
            .then_with(|| a.section.cmp(&b.section))
            .then_with(|| a.dep_name.cmp(&b.dep_name))
    });

    Ok(entries)
}

fn parse_cargo_toml(path: &Path) -> Result<Vec<DependencyEntry>, AuditInputError> {
    let content = input::text(path)?;
    let manifest: CargoToml = toml::from_str(&content)
        .map_err(|error| AuditInputError::invalid(path, error.to_string()))?;
    let crate_name = manifest
        .package
        .ok_or_else(|| AuditInputError::invalid(path, "отсутствует package.name"))?
        .name;

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

    Ok(entries)
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
