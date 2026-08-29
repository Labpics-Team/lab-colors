//! Source-file manifest extractor for EXT-01 (plan r6).
//!
//! Enumerates every production `.rs` file under `crates/*/src/**/*.rs`,
//! excluding test modules, integration-test directories, benchmarks and
//! build scripts. Each entry carries the relative workspace path, owning
//! crate name, SHA-256 content address and a public-module flag derived from
//! the filename convention (`lib.rs` / `mod.rs`).
//!
//! The manifest is deterministic: entries are sorted by their relative path
//! using byte-order comparison so repeated runs on the same tree produce
//! identical output regardless of filesystem enumeration order.

use std::fs;
use std::path::Path;

use crate::sha256;

/// One row of the production source manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFileEntry {
    /// Workspace-relative path using forward slashes (e.g. `crates/labcolors-core/src/lib.rs`).
    pub path: String,
    /// Crate name extracted from the second path segment after `crates/`.
    pub crate_name: String,
    /// Lowercase hexadecimal SHA-256 of the file contents.
    pub sha256: String,
    /// True when the file is a module root (`lib.rs` or `mod.rs`).
    pub is_public_module: bool,
}

/// Walks `workspace_root/crates/*/src/**/*.rs` and returns a sorted manifest
/// of production source files.
///
/// Excluded paths:
/// - any file whose name ends with `_tests.rs` or `_test.rs`;
/// - any file under a `tests/` or `benches/` directory component;
/// - any file named `build.rs`.
///
/// # Panics
///
/// Panics only on unrecoverable I/O errors (missing workspace, unreadable
/// file). Callers in test contexts should ensure the workspace root exists.
pub fn enumerate_production_sources(workspace_root: &Path) -> Vec<SourceFileEntry> {
    let crates_dir = workspace_root.join("crates");
    let mut entries = Vec::new();

    let crate_dirs = match fs::read_dir(&crates_dir) {
        Ok(iter) => iter,
        Err(_) => return entries,
    };

    for crate_entry in crate_dirs.flatten() {
        if !crate_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let crate_name = crate_entry.file_name().to_string_lossy().into_owned();
        let src_dir = crate_entry.path().join("src");
        if !src_dir.is_dir() {
            continue;
        }
        collect_rs_files(&src_dir, workspace_root, &crate_name, &mut entries);
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries
}

fn collect_rs_files(
    dir: &Path,
    workspace_root: &Path,
    crate_name: &str,
    out: &mut Vec<SourceFileEntry>,
) {
    let iter = match fs::read_dir(dir) {
        Ok(iter) => iter,
        Err(_) => return,
    };

    for entry in iter.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();

        if path.is_dir() {
            if file_name == "tests" || file_name == "benches" {
                continue;
            }
            collect_rs_files(&path, workspace_root, crate_name, out);
            continue;
        }

        if !file_name.ends_with(".rs") {
            continue;
        }
        if file_name == "build.rs" {
            continue;
        }
        if file_name.ends_with("_tests.rs") || file_name.ends_with("_test.rs") {
            continue;
        }

        let relative = path
            .strip_prefix(workspace_root)
            .expect("source file lives under workspace root")
            .to_string_lossy()
            .replace('\\', "/");

        let contents = fs::read(&path).expect("readable production source file");
        let digest = sha256::digest(&contents);
        let sha256_hex = hex_encode(digest.as_bytes());

        let is_public_module = file_name == "lib.rs" || file_name == "mod.rs";

        out.push(SourceFileEntry {
            path: relative,
            crate_name: crate_name.to_string(),
            sha256: sha256_hex,
            is_public_module,
        });
    }
}

fn hex_encode(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in *bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}
