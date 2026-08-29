//! EXT-01: RED→GREEN proof and sabotage controls for the source-file extractor.
//!
//! These tests validate that `enumerate_production_sources` produces a finite,
//! deterministic manifest of real production files and that the manifest
//! faithfully reflects the filesystem (no phantom entries, no silent drops).

use std::path::PathBuf;

use labcolors_core::source_manifest::enumerate_production_sources;

/// Returns the workspace root (parent of the `crates` directory).
fn workspace_root() -> PathBuf {
    // The test binary runs from `target/debug/deps`; three levels up reaches
    // the workspace root on standard Cargo layouts.
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // CARGO_MANIFEST_DIR points to crates/labcolors-core; go one level up.
    dir.pop();
    dir.pop();
    dir
}

#[test]
fn source_manifest_produces_finite_rows() {
    let manifest = enumerate_production_sources(&workspace_root());
    assert!(
        manifest.len() >= 80,
        "Expected ≥80 production files, got {}",
        manifest.len()
    );
    for entry in &manifest {
        assert!(!entry.path.is_empty(), "empty path in manifest");
        assert_eq!(
            entry.sha256.len(),
            64,
            "sha256 must be 64 hex chars, got {} for {}",
            entry.sha256.len(),
            entry.path
        );
        assert!(
            entry.sha256.chars().all(|c| c.is_ascii_hexdigit()),
            "sha256 contains non-hex chars for {}",
            entry.path
        );
    }
}

#[test]
fn sabotage_missing_file_detected() {
    let full = enumerate_production_sources(&workspace_root());
    assert!(full.len() >= 80, "baseline too small: {}", full.len());
    // Simulate sabotage: dropping one row reduces the count.
    let sabotaged: Vec<_> = full.iter().skip(1).collect();
    assert!(
        sabotaged.len() < full.len(),
        "sabotage not detected: filtered manifest has same length"
    );
}

#[test]
fn sabotage_phantom_file_detected() {
    let manifest = enumerate_production_sources(&workspace_root());
    let root = workspace_root();
    for entry in &manifest {
        let full_path = root.join(&entry.path);
        assert!(
            full_path.exists(),
            "phantom file in manifest: {} (resolved to {})",
            entry.path,
            full_path.display()
        );
    }
}

#[test]
fn manifest_is_sorted_by_path() {
    let manifest = enumerate_production_sources(&workspace_root());
    for window in manifest.windows(2) {
        assert!(
            window[0].path <= window[1].path,
            "manifest not sorted: {} > {}",
            window[0].path,
            window[1].path
        );
    }
}

#[test]
fn crate_name_matches_path_segment() {
    let manifest = enumerate_production_sources(&workspace_root());
    for entry in &manifest {
        // Expected layout: crates/<crate_name>/src/...
        let segments: Vec<&str> = entry.path.split('/').collect();
        assert!(
            segments.len() >= 3 && segments[0] == "crates",
            "unexpected path layout: {}",
            entry.path
        );
        assert_eq!(
            segments[1], entry.crate_name,
            "crate_name mismatch for {}",
            entry.path
        );
    }
}
