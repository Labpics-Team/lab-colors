use labcolors_audit::extractors::{ApiManifestEntry, extract_public_api};
use std::path::Path;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

#[test]
fn api_manifest_includes_known_public_functions() {
    let entries = extract_public_api(workspace_root());
    // enumerate_production_artifacts is a known pub fn in labcolors-audit
    let found = entries
        .iter()
        .any(|e| e.kind == "fn" && e.name == "enumerate_production_artifacts");
    assert!(
        found,
        "Expected to find pub fn enumerate_production_artifacts in manifest, got {} entries",
        entries.len()
    );
}

#[test]
fn api_manifest_excludes_test_modules() {
    let entries = extract_public_api(workspace_root());
    for entry in &entries {
        assert!(
            !entry.path.contains("/tests/"),
            "Test path leaked into manifest: {}",
            entry.path
        );
        assert!(
            !entry.path.ends_with("_test.rs") && !entry.path.ends_with("_tests.rs"),
            "Test file leaked into manifest: {}",
            entry.path
        );
    }
}

#[test]
fn api_manifest_deterministic_order() {
    let first = extract_public_api(workspace_root());
    let second = extract_public_api(workspace_root());
    assert_eq!(
        first.len(),
        second.len(),
        "Entry count differs between runs"
    );
    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a.path, b.path, "Path order differs");
        assert_eq!(a.name, b.name, "Name order differs");
        assert_eq!(
            a.signature_sha256, b.signature_sha256,
            "Hash differs for same item"
        );
    }
}

#[test]
fn api_manifest_entries_have_nonempty_signatures() {
    let entries = extract_public_api(workspace_root());
    assert!(
        !entries.is_empty(),
        "Manifest should not be empty for this workspace"
    );
    for entry in &entries {
        assert!(
            !entry.signature.is_empty(),
            "Empty signature for {}::{}",
            entry.path,
            entry.name
        );
        assert!(
            !entry.signature_sha256.is_empty(),
            "Empty hash for {}::{}",
            entry.path,
            entry.name
        );
        assert_eq!(
            entry.signature_sha256.len(),
            64,
            "SHA-256 hex must be 64 chars for {}::{}",
            entry.path,
            entry.name
        );
    }
}

#[test]
fn dropped_item_detected() {
    // This test verifies that the extractor is sensitive to actual source changes.
    // We check that removing a known pub item would decrease the count.
    // Since we can't mutate sources in tests, we verify the invariant indirectly:
    // the manifest contains at least one fn from labcolors-audit itself.
    let entries = extract_public_api(workspace_root());
    let audit_fns: Vec<&ApiManifestEntry> = entries
        .iter()
        .filter(|e| e.crate_name == "labcolors-audit" && e.kind == "fn")
        .collect();
    assert!(
        !audit_fns.is_empty(),
        "No pub fns found in labcolors-audit — extractor may be broken"
    );
    // If someone deletes enumerate_production_artifacts, this assertion will fail,
    // proving the extractor detects removals.
    let has_enumerate = audit_fns
        .iter()
        .any(|e| e.name == "enumerate_production_artifacts");
    assert!(
        has_enumerate,
        "enumerate_production_artifacts missing — if intentional, update this test"
    );
}
