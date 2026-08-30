//! Claims manifest extractor for EXT-06.
//!
//! Scans production source files under `crates/*/src/**/*.rs` and extracts
//! verifiable claims: production invariants (`assert!`, `debug_assert!`),
//! compile-time invariants (`const _: () = assert!(...)`), test contracts
//! (`#[test]` functions) and documentation contracts (`# Panics`, `# Safety`,
//! `# Invariants` sections). Each claim carries the workspace-relative path,
//! owning crate name, 1-based line number, classified kind, trimmed source
//! expression and the SHA-256 digest of the entire file so downstream
//! consumers can detect both phantom and mutated entries.
//!
//! The manifest is deterministic: entries are sorted by `(path, line)` using
//! byte-order comparison so repeated runs on the same tree produce identical
//! output regardless of filesystem enumeration order.

use std::fs;
use std::path::Path;

use crate::sha256;

/// Classification of a verifiable source claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    /// Runtime `assert!` / `debug_assert!` guarding a production invariant.
    ProductionInvariant,
    /// Compile-time assertion embedded in a `const` context.
    CompileTimeInvariant,
    /// Test contract: a function annotated with `#[test]`.
    TestContract,
    /// Documentation contract section (`# Panics`, `# Safety`, `# Invariants`).
    DocContract,
}

/// One row of the claims manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimEntry {
    /// Stable identifier derived from `(path, line)` — unique within a manifest.
    pub claim_id: String,
    /// Workspace-relative path using forward slashes.
    pub path: String,
    /// Crate name extracted from the second path segment after `crates/`.
    pub crate_name: String,
    /// 1-based line number where the claim appears.
    pub line: u32,
    /// Classified claim kind.
    pub kind: ClaimKind,
    /// Trimmed source text of the line containing the claim.
    pub expression: String,
    /// Lowercase hexadecimal SHA-256 of the entire source file.
    pub source_sha256: String,
}

/// Computes the lowercase hexadecimal SHA-256 digest of `bytes`.
///
/// Exposed for test-only verification of sabotage controls; production
/// consumers should rely on [`ClaimEntry::source_sha256`] rather than
/// recomputing digests directly.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = sha256::digest(bytes);
    hex_encode(digest.as_bytes())
}

/// Walks `workspace_root/crates/*/src/**/*.rs` and returns a sorted manifest
/// of verifiable claims.
///
/// Excluded paths mirror [`super::source_manifest::enumerate_production_sources`]:
/// test modules (`*_tests.rs`, `*_test.rs`), integration-test directories,
/// benchmarks and build scripts are skipped so the manifest reflects only
/// production-shipped surface.
///
/// # Panics
///
/// Panics only on unrecoverable I/O errors (missing workspace, unreadable
/// file). Callers in test contexts should ensure the workspace root exists.
pub fn extract_claims(workspace_root: &Path) -> Vec<ClaimEntry> {
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
        collect_claims(&src_dir, workspace_root, &crate_name, &mut entries);
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    entries
}

fn collect_claims(
    dir: &Path,
    workspace_root: &Path,
    crate_name: &str,
    out: &mut Vec<ClaimEntry>,
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
            collect_claims(&path, workspace_root, crate_name, out);
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

        let text = match std::str::from_utf8(&contents) {
            Ok(text) => text,
            Err(_) => continue,
        };

        for (index, raw_line) in text.lines().enumerate() {
            let line_number = index as u32 + 1;
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let kind = classify_line(trimmed);
            if let Some(kind) = kind {
                let claim_id = format!("{}:{}:{}", relative, line_number, kind_label(kind));
                out.push(ClaimEntry {
                    claim_id,
                    path: relative.clone(),
                    crate_name: crate_name.to_string(),
                    line: line_number,
                    kind,
                    expression: trimmed.to_string(),
                    source_sha256: sha256_hex.clone(),
                });
            }
        }
    }
}

fn classify_line(trimmed: &str) -> Option<ClaimKind> {
    // Documentation contracts take precedence: a line starting with `///` or
    // `//!` followed by one of the recognised section headers is a doc claim
    // even when the comment happens to mention `assert!` elsewhere.
    if let Some(rest) = doc_comment_body(trimmed) {
        let header = rest.trim_start();
        if header.starts_with("# Panics")
            || header.starts_with("# Safety")
            || header.starts_with("# Invariants")
        {
            return Some(ClaimKind::DocContract);
        }
    }

    // Compile-time invariants: `const _: () = assert!(...)` patterns. We look
    // for the leading `const` keyword followed later on the same line by an
    // `assert!` invocation; this keeps the detector line-scoped without a full
    // parser while still excluding runtime assertions that happen to appear
    // inside const blocks on separate lines.
    if trimmed.starts_with("const ") && contains_assert_macro(trimmed) {
        return Some(ClaimKind::CompileTimeInvariant);
    }

    // Test contracts: attribute lines carrying `#[test]` (optionally combined
    // with other attributes like `#[cfg(test)]`). We intentionally accept any
    // line whose attribute list includes `test` rather than requiring exact
    // equality so `#[test] #[should_panic]` remains visible.
    if is_test_attribute(trimmed) {
        return Some(ClaimKind::TestContract);
    }

    // Production invariants last so the more specific kinds above win when a
    // single line matches multiple patterns.
    if contains_assert_macro(trimmed) {
        return Some(ClaimKind::ProductionInvariant);
    }

    None
}

fn doc_comment_body(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix("///") {
        return Some(rest);
    }
    if let Some(rest) = line.strip_prefix("//!") {
        return Some(rest);
    }
    None
}

fn contains_assert_macro(line: &str) -> bool {
    line.contains("assert!(")
        || line.contains("assert_eq!(")
        || line.contains("assert_ne!(")
        || line.contains("debug_assert!(")
        || line.contains("debug_assert_eq!(")
        || line.contains("debug_assert_ne!(")
}

fn is_test_attribute(line: &str) -> bool {
    if !line.starts_with('#') {
        return false;
    }
    // Accept both outer (`#[test]`) and inner (`#![test]`) forms; ignore
    // whitespace between tokens so `#[ test ]` still matches.
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("#[test]") || compact.contains("#![test]")
}

fn kind_label(kind: ClaimKind) -> &'static str {
    match kind {
        ClaimKind::ProductionInvariant => "prod",
        ClaimKind::CompileTimeInvariant => "compile",
        ClaimKind::TestContract => "test",
        ClaimKind::DocContract => "doc",
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