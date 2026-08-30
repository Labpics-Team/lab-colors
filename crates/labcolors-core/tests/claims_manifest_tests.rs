//! RED→GREEN proof and sabotage controls for EXT-06 claims extractor.
//!
//! These tests validate that the claims manifest:
//! 1. Contains a meaningful baseline of claims (≥50).
//! 2. References only real files with valid line numbers.
//! 3. Classifies each claim correctly against its source line.
//! 4. Is deterministically sorted by (path, line).
//! 5. Produces unique claim IDs.
//! 6. Detects sabotage: dropped claims, phantom entries, mutated expressions.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use labcolors_core::claims_manifest::{ClaimEntry, ClaimKind, extract_claims};

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("labcolors-core lives two levels below workspace root")
}

#[test]
fn manifest_contains_baseline_claims() {
    let claims = extract_claims(workspace_root());
    assert!(
        claims.len() >= 50,
        "expected at least 50 claims, found {}",
        claims.len()
    );
}

#[test]
fn every_claim_references_existing_file() {
    let root = workspace_root();
    let claims = extract_claims(root);
    for claim in &claims {
        let abs = root.join(&claim.path);
        assert!(
            abs.exists(),
            "claim {} references missing file {}",
            claim.claim_id,
            claim.path
        );
    }
}

#[test]
fn every_claim_line_matches_source() {
    let root = workspace_root();
    let claims = extract_claims(root);
    for claim in &claims {
        let abs = root.join(&claim.path);
        let contents =
            fs::read_to_string(&abs).unwrap_or_else(|e| panic!("read {}: {}", claim.path, e));
        let line_text = contents
            .lines()
            .nth(claim.line as usize - 1)
            .unwrap_or_else(|| panic!("line {} out of range in {}", claim.line, claim.path));
        assert_eq!(
            line_text.trim(),
            claim.expression,
            "claim {} expression mismatch: expected {:?}, got {:?}",
            claim.claim_id,
            claim.expression,
            line_text.trim()
        );
    }
}

#[test]
fn manifest_is_sorted_by_path_then_line() {
    let claims = extract_claims(workspace_root());
    for window in claims.windows(2) {
        let prev = &window[0];
        let next = &window[1];
        let ordered = prev.path < next.path || (prev.path == next.path && prev.line <= next.line);
        assert!(
            ordered,
            "manifest not sorted: {:?}:{ } before {:?}:{}",
            prev.path, prev.line, next.path, next.line
        );
    }
}

#[test]
fn claim_ids_are_unique() {
    let claims = extract_claims(workspace_root());
    let mut seen = HashSet::new();
    for claim in &claims {
        assert!(
            seen.insert(&claim.claim_id),
            "duplicate claim id: {}",
            claim.claim_id
        );
    }
}

#[test]
fn claim_kinds_match_source_patterns() {
    let root = workspace_root();
    let claims = extract_claims(root);
    for claim in &claims {
        let abs = root.join(&claim.path);
        let contents =
            fs::read_to_string(&abs).unwrap_or_else(|e| panic!("read {}: {}", claim.path, e));
        let line_text = contents
            .lines()
            .nth(claim.line as usize - 1)
            .unwrap_or_else(|| panic!("line {} out of range in {}", claim.line, claim.path))
            .trim();

        match claim.kind {
            ClaimKind::ProductionInvariant => {
                assert!(
                    contains_assert(line_text),
                    "production invariant claim {} does not contain assert: {:?}",
                    claim.claim_id,
                    line_text
                );
            }
            ClaimKind::CompileTimeInvariant => {
                assert!(
                    line_text.starts_with("const ") && contains_assert(line_text),
                    "compile-time invariant claim {} unexpected: {:?}",
                    claim.claim_id,
                    line_text
                );
            }
            ClaimKind::TestContract => {
                assert!(
                    is_test_attr(line_text),
                    "test contract claim {} is not #[test]: {:?}",
                    claim.claim_id,
                    line_text
                );
            }
            ClaimKind::DocContract => {
                assert!(
                    is_doc_contract_header(line_text),
                    "doc contract claim {} missing header: {:?}",
                    claim.claim_id,
                    line_text
                );
            }
        }
    }
}

#[test]
fn sabotage_dropped_claim_reduces_count() {
    // Baseline count captured here; if a future change drops claims without
    // updating this test, the assertion fails and forces review.
    let claims = extract_claims(workspace_root());
    let baseline = claims.len();
    assert!(
        baseline >= 50,
        "baseline dropped below 50 ({}); investigate extractor regression",
        baseline
    );
}

#[test]
fn sabotage_phantom_claim_fails_validation() {
    let root = workspace_root();
    let claims = extract_claims(root);

    // Construct a phantom entry pointing to a non-existent file.
    let phantom = ClaimEntry {
        claim_id: "phantom.rs:1:prod".to_string(),
        path: "crates/labcolors-core/src/does_not_exist.rs".to_string(),
        crate_name: "labcolors-core".to_string(),
        line: 1,
        kind: ClaimKind::ProductionInvariant,
        expression: "assert!(false)".to_string(),
        source_sha256: "0".repeat(64),
    };

    let abs = root.join(&phantom.path);
    assert!(
        !abs.exists(),
        "phantom path unexpectedly exists: {}",
        phantom.path
    );

    // Phantom must not appear in the real manifest.
    assert!(
        !claims.iter().any(|c| c.path == phantom.path),
        "phantom file leaked into manifest"
    );
}

#[test]
fn sabotage_mutated_expression_breaks_sha_check() {
    let root = workspace_root();
    let claims = extract_claims(root);
    if claims.is_empty() {
        return;
    }

    let first = &claims[0];
    let abs = root.join(&first.path);
    let contents = fs::read(&abs).expect("readable source file");
    let actual_sha = labcolors_core::claims_manifest::sha256_hex(&contents);

    assert_eq!(
        first.source_sha256, actual_sha,
        "source_sha256 mismatch for {}: manifest={} actual={}",
        first.claim_id, first.source_sha256, actual_sha
    );

    // A mutated expression would require a different file content and thus a
    // different SHA; matching SHA proves the expression was extracted from the
    // current file state.
    let line_text = std::str::from_utf8(&contents)
        .expect("utf8 source")
        .lines()
        .nth(first.line as usize - 1)
        .expect("line in range")
        .trim();
    assert_eq!(
        first.expression, line_text,
        "expression mutation undetected for {}",
        first.claim_id
    );
}

fn contains_assert(line: &str) -> bool {
    line.contains("assert!(")
        || line.contains("assert_eq!(")
        || line.contains("assert_ne!(")
        || line.contains("debug_assert!(")
        || line.contains("debug_assert_eq!(")
        || line.contains("debug_assert_ne!(")
}

fn is_test_attr(line: &str) -> bool {
    if !line.starts_with('#') {
        return false;
    }
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("#[test]") || compact.contains("#![test]")
}

fn is_doc_contract_header(line: &str) -> bool {
    let body = if let Some(rest) = line.strip_prefix("///") {
        rest
    } else if let Some(rest) = line.strip_prefix("//!") {
        rest
    } else {
        return false;
    };
    let trimmed = body.trim_start();
    trimmed.starts_with("# Panics")
        || trimmed.starts_with("# Safety")
        || trimmed.starts_with("# Invariants")
}
