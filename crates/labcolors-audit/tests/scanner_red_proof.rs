use labcolors_audit::{ArtifactClass, assign_dispositions, enumerate_production_artifacts};

/// Workspace root: CARGO_MANIFEST_DIR = crates/labcolors-audit, so we need
/// two .parent() calls to reach the actual workspace root (not crates/).
fn workspace_root() -> &'static std::path::Path {
    // Cached via thread-local to avoid repeated path construction.
    // In tests this is called once per test; the static is fine because
    // CARGO_MANIFEST_DIR is a compile-time constant.
    static ROOT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent() // crates/
            .and_then(|p| p.parent()) // workspace root
            .expect("audit crate is two levels below workspace root")
            .to_path_buf()
    })
}

/// RED-proof floor: ParallelSsot extractor finds at least the known SSOT markers.
///
/// Baseline: 51 occurrences of SSOT-TRACKED/GROUNDED across 6 production files
/// in labcolors-core/src/ (measured 2026-08-29). Floor set conservatively at 40
/// to absorb minor refactors without breaking the invariant.
#[test]
fn parallel_ssot_floor_count() {
    let source_root = workspace_root();

    let artifacts = enumerate_production_artifacts(source_root);
    let ssot_count = artifacts
        .iter()
        .filter(|a| a.class == ArtifactClass::ParallelSsot)
        .count();

    assert!(
        ssot_count >= 40,
        "ParallelSsot floor violated: expected >= 40, found {ssot_count}. \
         SSOT-TRACKED/GROUNDED markers may have been removed or scanner regressed."
    );
}

/// RED-proof floor: PublicClaim extractor finds module-level doc sections.
///
/// Baseline: 5 `//! #` headings across labcolors-conformance and labcolors-ffi
/// lib.rs files (measured 2026-08-29). Floor set at 3 to absorb minor edits.
#[test]
fn public_claim_floor_count() {
    let source_root = workspace_root();

    let artifacts = enumerate_production_artifacts(source_root);
    let claim_count = artifacts
        .iter()
        .filter(|a| a.class == ArtifactClass::PublicClaim)
        .count();

    assert!(
        claim_count >= 3,
        "PublicClaim floor violated: expected >= 3, found {claim_count}. \
         Module doc sections may have been removed or scanner regressed."
    );
}

/// Determinism: two consecutive scans produce identical artifact lists.
///
/// Both extractors must be pure functions of the filesystem state.
/// Non-deterministic output breaks digest stability and CI reproducibility.
#[test]
fn parallel_ssot_and_public_claim_are_deterministic() {
    let source_root = workspace_root();

    let first = enumerate_production_artifacts(source_root);
    let second = enumerate_production_artifacts(source_root);

    let first_ssot: Vec<_> = first
        .iter()
        .filter(|a| a.class == ArtifactClass::ParallelSsot)
        .collect();
    let second_ssot: Vec<_> = second
        .iter()
        .filter(|a| a.class == ArtifactClass::ParallelSsot)
        .collect();
    assert_eq!(
        first_ssot, second_ssot,
        "ParallelSsot extraction is non-deterministic between runs"
    );

    let first_claims: Vec<_> = first
        .iter()
        .filter(|a| a.class == ArtifactClass::PublicClaim)
        .collect();
    let second_claims: Vec<_> = second
        .iter()
        .filter(|a| a.class == ArtifactClass::PublicClaim)
        .collect();
    assert_eq!(
        first_claims, second_claims,
        "PublicClaim extraction is non-deterministic between runs"
    );
}

/// Dispose integration: ParallelSsot and PublicClaim produce NotAssessed dispositions.
///
/// Until EXT-09 registries are wired, both classes must disposition to NotAssessed
/// with specific replan triggers — not Orphaned or Defective.
#[test]
fn new_classes_dispose_to_not_assessed() {
    let source_root = workspace_root();

    let raw = enumerate_production_artifacts(source_root);
    let dispositioned = assign_dispositions(&raw);

    for artifact in &dispositioned {
        match artifact.raw.class {
            ArtifactClass::ParallelSsot => {
                assert!(
                    matches!(
                        artifact.disposition,
                        labcolors_audit::Disposition::NotAssessed { .. }
                    ),
                    "ParallelSsot '{}' should be NotAssessed, got {:?}",
                    artifact.raw.raw_key,
                    artifact.disposition
                );
            }
            ArtifactClass::PublicClaim => {
                assert!(
                    matches!(
                        artifact.disposition,
                        labcolors_audit::Disposition::NotAssessed { .. }
                    ),
                    "PublicClaim '{}' should be NotAssessed, got {:?}",
                    artifact.raw.raw_key,
                    artifact.disposition
                );
            }
            _ => {}
        }
    }
}
