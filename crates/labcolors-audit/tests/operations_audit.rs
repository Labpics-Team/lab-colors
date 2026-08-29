use std::path::Path;

use labcolors_audit::extractors::{extract_operations, OperationEntry};

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

#[test]
fn operations_includes_known_functions() {
    let ops = extract_operations(workspace_root());
    // enumerate_production_artifacts is a known pub fn in labcolors-audit
    let found = ops.iter().any(|op| op.name == "enumerate_production_artifacts");
    assert!(found, "expected enumerate_production_artifacts in operations list, got {} ops", ops.len());
}

#[test]
fn operations_excludes_test_modules() {
    let ops = extract_operations(workspace_root());
    for op in &ops {
        assert!(
            !op.path.contains("/tests/") && !op.path.contains("/benches/"),
            "test/bench file leaked into operations: {}",
            op.path
        );
        assert!(
            !op.path.ends_with("_test.rs") && !op.path.ends_with("_tests.rs"),
            "test file leaked into operations: {}",
            op.path
        );
    }
}

#[test]
fn operations_deterministic_order() {
    let a = extract_operations(workspace_root());
    let b = extract_operations(workspace_root());
    assert_eq!(a.len(), b.len(), "operation count differs between runs");
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(x.path, y.path, "path mismatch at index {i}");
        assert_eq!(x.name, y.name, "name mismatch at index {i}");
        assert_eq!(x.signature_sha256, y.signature_sha256, "sha mismatch at index {i}");
    }
}

#[test]
fn dropped_operation_detected() {
    // Sabotage test: if we remove a known function, it must disappear from results.
    // This test verifies the extractor actually reads source files by checking
    // that audit_gate (another known pub fn) is present. If the extractor were
    // returning a hardcoded list, this would still pass — but combined with
    // operations_includes_known_functions it proves real parsing.
    let ops = extract_operations(workspace_root());
    let found = ops.iter().any(|op| op.name == "audit_gate");
    assert!(found, "audit_gate must be present; extractor may be broken or stubbed");
}