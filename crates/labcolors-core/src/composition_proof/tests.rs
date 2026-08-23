//! Tests for composition proof types.
//! 54 tests total: 30 construction, 8 digest stability, 6 equality/clone,
//! 5 serde-equivalent manual field checks, 5 absence-law compliance.

use super::*;

// =============================================================================
// Construction unit tests (30)
// =============================================================================

// --- CompositionLawProofV1 ---

#[test]
fn law_proof_enumerated_constructs() {
    let proof = CompositionLawProofV1::new(
        "profile-1".into(),
        "domain-1".into(),
        CompositionLawVerificationMethodV1::Enumerated,
    )
    .expect("test invariant");
    assert_eq!(proof.profile_id, "profile-1");
    assert_eq!(proof.domain_descriptor, "domain-1");
    assert_ne!(proof.digest, [0u8; 32]);
}

#[test]
fn law_proof_analytic_bounds_constructs() {
    let proof = CompositionLawProofV1::new(
        "profile-1".into(),
        "domain-1".into(),
        CompositionLawVerificationMethodV1::AnalyticBounds,
    )
    .expect("test invariant");
    assert_eq!(
        proof.verification_method,
        CompositionLawVerificationMethodV1::AnalyticBounds
    );
}

#[test]
fn law_proof_raster_verified_constructs() {
    let proof = CompositionLawProofV1::new(
        "profile-1".into(),
        "domain-1".into(),
        CompositionLawVerificationMethodV1::RasterVerified {
            renderer_profile_id: "renderer-1".into(),
            width: 1920,
            height: 1080,
        },
    )
    .expect("test invariant");
    assert!(matches!(
        proof.verification_method,
        CompositionLawVerificationMethodV1::RasterVerified { .. }
    ));
}

#[test]
fn law_proof_empty_profile_id_rejected() {
    let err = CompositionLawProofV1::new(
        String::new(),
        "domain-1".into(),
        CompositionLawVerificationMethodV1::Enumerated,
    )
    .unwrap_err();
    assert_eq!(err, CompositionLawProofError::EmptyProfileId);
}

#[test]
fn law_proof_empty_domain_descriptor_rejected() {
    let err = CompositionLawProofV1::new(
        "profile-1".into(),
        String::new(),
        CompositionLawVerificationMethodV1::Enumerated,
    )
    .unwrap_err();
    assert_eq!(err, CompositionLawProofError::EmptyDomainDescriptor);
}

#[test]
fn law_proof_empty_renderer_profile_rejected() {
    let err = CompositionLawProofV1::new(
        "profile-1".into(),
        "domain-1".into(),
        CompositionLawVerificationMethodV1::RasterVerified {
            renderer_profile_id: String::new(),
            width: 100,
            height: 100,
        },
    )
    .unwrap_err();
    assert_eq!(err, CompositionLawProofError::EmptyRendererProfileId);
}

#[test]
fn law_proof_zero_raster_width_rejected() {
    let err = CompositionLawProofV1::new(
        "profile-1".into(),
        "domain-1".into(),
        CompositionLawVerificationMethodV1::RasterVerified {
            renderer_profile_id: "renderer-1".into(),
            width: 0,
            height: 100,
        },
    )
    .unwrap_err();
    assert_eq!(err, CompositionLawProofError::ZeroRasterDimension);
}

#[test]
fn law_proof_zero_raster_height_rejected() {
    let err = CompositionLawProofV1::new(
        "profile-1".into(),
        "domain-1".into(),
        CompositionLawVerificationMethodV1::RasterVerified {
            renderer_profile_id: "renderer-1".into(),
            width: 100,
            height: 0,
        },
    )
    .unwrap_err();
    assert_eq!(err, CompositionLawProofError::ZeroRasterDimension);
}

// --- OwnedCompositionReferenceV1 ---

#[test]
fn owned_reference_constructs() {
    let law_digest = [42u8; 32];
    let reference =
        OwnedCompositionReferenceV1::new(law_digest, "root-1".into(), "support-1".into())
            .expect("test invariant");
    assert_eq!(reference.law_proof_digest, law_digest);
    assert_ne!(reference.digest, [0u8; 32]);
}

#[test]
fn owned_reference_empty_root_id_rejected() {
    let err =
        OwnedCompositionReferenceV1::new([0u8; 32], String::new(), "support-1".into()).unwrap_err();
    assert_eq!(err, OwnedCompositionReferenceError::EmptyOwnedRootId);
}

#[test]
fn owned_reference_empty_support_domain_rejected() {
    let err =
        OwnedCompositionReferenceV1::new([0u8; 32], "root-1".into(), String::new()).unwrap_err();
    assert_eq!(err, OwnedCompositionReferenceError::EmptySupportDomainId);
}

// --- BaseAcceptedOnSupportCertificateV1 ---

#[test]
fn base_accepted_constructs() {
    let cert = BaseAcceptedOnSupportCertificateV1::new(
        "support-1".into(),
        "eval-ctx-1".into(),
        "pred-set-1".into(),
        1000,
    )
    .expect("test invariant");
    assert_eq!(cert.verified_count, 1000);
    assert_ne!(cert.digest, [0u8; 32]);
}

#[test]
fn base_accepted_empty_support_domain_rejected() {
    let err = BaseAcceptedOnSupportCertificateV1::new(
        String::new(),
        "eval-ctx-1".into(),
        "pred-set-1".into(),
        1000,
    )
    .unwrap_err();
    assert_eq!(err, BaseAcceptedOnSupportError::EmptySupportDomainId);
}

#[test]
fn base_accepted_empty_eval_context_rejected() {
    let err = BaseAcceptedOnSupportCertificateV1::new(
        "support-1".into(),
        String::new(),
        "pred-set-1".into(),
        1000,
    )
    .unwrap_err();
    assert_eq!(err, BaseAcceptedOnSupportError::EmptyEvaluationContextId);
}

#[test]
fn base_accepted_empty_predicate_set_rejected() {
    let err = BaseAcceptedOnSupportCertificateV1::new(
        "support-1".into(),
        "eval-ctx-1".into(),
        String::new(),
        1000,
    )
    .unwrap_err();
    assert_eq!(err, BaseAcceptedOnSupportError::EmptyPredicateSetId);
}

#[test]
fn base_accepted_zero_verified_count_rejected() {
    let err = BaseAcceptedOnSupportCertificateV1::new(
        "support-1".into(),
        "eval-ctx-1".into(),
        "pred-set-1".into(),
        0,
    )
    .unwrap_err();
    assert_eq!(err, BaseAcceptedOnSupportError::ZeroVerifiedCount);
}

// --- NoIntroducedRejectCertificateV1 ---

#[test]
fn no_reject_constructs() {
    let cert =
        NoIntroducedRejectCertificateV1::new("root-1".into(), [7u8; 32], "post-pass-1".into())
            .expect("test invariant");
    assert_eq!(cert.pre_pass_snapshot_digest, [7u8; 32]);
    assert_ne!(cert.digest, [0u8; 32]);
}

#[test]
fn no_reject_empty_root_id_rejected() {
    let err = NoIntroducedRejectCertificateV1::new(String::new(), [0u8; 32], "post-pass-1".into())
        .unwrap_err();
    assert_eq!(err, NoIntroducedRejectError::EmptyOwnedRootId);
}

#[test]
fn no_reject_empty_verification_id_rejected() {
    let err = NoIntroducedRejectCertificateV1::new("root-1".into(), [0u8; 32], String::new())
        .unwrap_err();
    assert_eq!(err, NoIntroducedRejectError::EmptyPostPassVerificationId);
}

// --- WholeFieldCoverageProofV1 ---

#[test]
fn whole_field_enumeration_constructs() {
    let proof = WholeFieldCoverageProofV1::new(
        "field-1".into(),
        WholeFieldCoverageMethodV1::Enumeration {
            occurrence_count: 500,
        },
        "desc-1".into(),
    )
    .expect("test invariant");
    assert!(matches!(
        proof.method,
        WholeFieldCoverageMethodV1::Enumeration {
            occurrence_count: 500
        }
    ));
}

#[test]
fn whole_field_analytic_extrema_constructs() {
    let proof = WholeFieldCoverageProofV1::new(
        "field-1".into(),
        WholeFieldCoverageMethodV1::AnalyticExtrema {
            critical_point_count: 12,
        },
        "desc-1".into(),
    )
    .expect("test invariant");
    assert!(matches!(
        proof.method,
        WholeFieldCoverageMethodV1::AnalyticExtrema {
            critical_point_count: 12
        }
    ));
}

#[test]
fn whole_field_interval_enclosure_constructs() {
    let proof = WholeFieldCoverageProofV1::new(
        "field-1".into(),
        WholeFieldCoverageMethodV1::IntervalEnclosure { interval_count: 64 },
        "desc-1".into(),
    )
    .expect("test invariant");
    assert!(matches!(
        proof.method,
        WholeFieldCoverageMethodV1::IntervalEnclosure { interval_count: 64 }
    ));
}

#[test]
fn whole_field_raster_constructs() {
    let proof = WholeFieldCoverageProofV1::new(
        "field-1".into(),
        WholeFieldCoverageMethodV1::RendererBoundRaster {
            renderer_profile_id: "renderer-1".into(),
            width: 256,
            height: 256,
        },
        "desc-1".into(),
    )
    .expect("test invariant");
    assert!(matches!(
        proof.method,
        WholeFieldCoverageMethodV1::RendererBoundRaster { .. }
    ));
}

#[test]
fn whole_field_empty_identity_rejected() {
    let err = WholeFieldCoverageProofV1::new(
        String::new(),
        WholeFieldCoverageMethodV1::Enumeration {
            occurrence_count: 1,
        },
        "desc-1".into(),
    )
    .unwrap_err();
    assert_eq!(err, WholeFieldCoverageError::EmptyFieldIdentity);
}

#[test]
fn whole_field_empty_descriptor_rejected() {
    let err = WholeFieldCoverageProofV1::new(
        "field-1".into(),
        WholeFieldCoverageMethodV1::Enumeration {
            occurrence_count: 1,
        },
        String::new(),
    )
    .unwrap_err();
    assert_eq!(err, WholeFieldCoverageError::EmptyCoverageDescriptor);
}

#[test]
fn whole_field_zero_occurrences_rejected() {
    let err = WholeFieldCoverageProofV1::new(
        "field-1".into(),
        WholeFieldCoverageMethodV1::Enumeration {
            occurrence_count: 0,
        },
        "desc-1".into(),
    )
    .unwrap_err();
    assert_eq!(err, WholeFieldCoverageError::ZeroOccurrenceCount);
}

#[test]
fn whole_field_zero_critical_points_rejected() {
    let err = WholeFieldCoverageProofV1::new(
        "field-1".into(),
        WholeFieldCoverageMethodV1::AnalyticExtrema {
            critical_point_count: 0,
        },
        "desc-1".into(),
    )
    .unwrap_err();
    assert_eq!(err, WholeFieldCoverageError::ZeroCriticalPointCount);
}

#[test]
fn whole_field_zero_intervals_rejected() {
    let err = WholeFieldCoverageProofV1::new(
        "field-1".into(),
        WholeFieldCoverageMethodV1::IntervalEnclosure { interval_count: 0 },
        "desc-1".into(),
    )
    .unwrap_err();
    assert_eq!(err, WholeFieldCoverageError::ZeroIntervalCount);
}

#[test]
fn whole_field_empty_raster_profile_rejected() {
    let err = WholeFieldCoverageProofV1::new(
        "field-1".into(),
        WholeFieldCoverageMethodV1::RendererBoundRaster {
            renderer_profile_id: String::new(),
            width: 100,
            height: 100,
        },
        "desc-1".into(),
    )
    .unwrap_err();
    assert_eq!(err, WholeFieldCoverageError::EmptyRendererProfileId);
}

#[test]
fn whole_field_zero_raster_dimension_rejected() {
    let err = WholeFieldCoverageProofV1::new(
        "field-1".into(),
        WholeFieldCoverageMethodV1::RendererBoundRaster {
            renderer_profile_id: "renderer-1".into(),
            width: 0,
            height: 100,
        },
        "desc-1".into(),
    )
    .unwrap_err();
    assert_eq!(err, WholeFieldCoverageError::ZeroRasterDimension);
}

// =============================================================================
// Digest stability tests (8)
// =============================================================================

#[test]
fn law_proof_digest_deterministic() {
    let a = CompositionLawProofV1::new(
        "p".into(),
        "d".into(),
        CompositionLawVerificationMethodV1::Enumerated,
    )
    .expect("test invariant");
    let b = CompositionLawProofV1::new(
        "p".into(),
        "d".into(),
        CompositionLawVerificationMethodV1::Enumerated,
    )
    .expect("test invariant");
    assert_eq!(a.digest, b.digest);
}

#[test]
fn law_proof_digest_changes_on_input_change() {
    let a = CompositionLawProofV1::new(
        "p".into(),
        "d".into(),
        CompositionLawVerificationMethodV1::Enumerated,
    )
    .expect("test invariant");
    let b = CompositionLawProofV1::new(
        "p".into(),
        "d2".into(),
        CompositionLawVerificationMethodV1::Enumerated,
    )
    .expect("test invariant");
    assert_ne!(a.digest, b.digest);
}

#[test]
fn owned_reference_digest_deterministic() {
    let a = OwnedCompositionReferenceV1::new([1u8; 32], "r".into(), "s".into())
        .expect("test invariant");
    let b = OwnedCompositionReferenceV1::new([1u8; 32], "r".into(), "s".into())
        .expect("test invariant");
    assert_eq!(a.digest, b.digest);
}

#[test]
fn owned_reference_digest_changes_on_law_proof_digest_change() {
    let a = OwnedCompositionReferenceV1::new([1u8; 32], "r".into(), "s".into())
        .expect("test invariant");
    let b = OwnedCompositionReferenceV1::new([2u8; 32], "r".into(), "s".into())
        .expect("test invariant");
    assert_ne!(a.digest, b.digest);
}

#[test]
fn base_accepted_digest_deterministic() {
    let a = BaseAcceptedOnSupportCertificateV1::new("s".into(), "e".into(), "p".into(), 100)
        .expect("test invariant");
    let b = BaseAcceptedOnSupportCertificateV1::new("s".into(), "e".into(), "p".into(), 100)
        .expect("test invariant");
    assert_eq!(a.digest, b.digest);
}

#[test]
fn no_reject_digest_deterministic() {
    let a = NoIntroducedRejectCertificateV1::new("r".into(), [0u8; 32], "v".into())
        .expect("test invariant");
    let b = NoIntroducedRejectCertificateV1::new("r".into(), [0u8; 32], "v".into())
        .expect("test invariant");
    assert_eq!(a.digest, b.digest);
}

#[test]
fn whole_field_digest_deterministic_per_method() {
    let method = WholeFieldCoverageMethodV1::Enumeration {
        occurrence_count: 10,
    };
    let a = WholeFieldCoverageProofV1::new("f".into(), method.clone(), "d".into())
        .expect("test invariant");
    let b = WholeFieldCoverageProofV1::new("f".into(), method, "d".into()).expect("test invariant");
    assert_eq!(a.digest, b.digest);
}

#[test]
fn cross_type_digest_collision_impossible() {
    // Same raw string fed to two different type prefixes must produce different digests.
    // We compare the digest of a law proof with a manually computed digest using
    // a different prefix to confirm the prefix tag prevents collisions.
    use crate::sha256;
    let profile_id = "same-input";
    let domain_descriptor = "same-input";
    let law = CompositionLawProofV1::new(
        profile_id.into(),
        domain_descriptor.into(),
        CompositionLawVerificationMethodV1::Enumerated,
    )
    .expect("test invariant");

    // Compute what the digest would be with a wrong prefix
    let mut hasher = sha256::Hasher::new();
    hasher.update(b"WrongPrefix:");
    hasher.update(profile_id.as_bytes());
    hasher.update(b"|");
    hasher.update(domain_descriptor.as_bytes());
    hasher.update(b"|");
    hasher.update(b"enumerated");
    let wrong_digest = *hasher.finalize().as_bytes();

    assert_ne!(law.digest, wrong_digest);
}

// =============================================================================
// Equality and clone tests (6)
// =============================================================================

#[test]
fn law_proof_clone_equality() {
    let a = CompositionLawProofV1::new(
        "p".into(),
        "d".into(),
        CompositionLawVerificationMethodV1::AnalyticBounds,
    )
    .expect("test invariant");
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn owned_reference_clone_equality() {
    let a = OwnedCompositionReferenceV1::new([9u8; 32], "r".into(), "s".into())
        .expect("test invariant");
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn base_accepted_clone_equality() {
    let a = BaseAcceptedOnSupportCertificateV1::new("s".into(), "e".into(), "p".into(), 42)
        .expect("test invariant");
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn no_reject_clone_equality() {
    let a = NoIntroducedRejectCertificateV1::new("r".into(), [3u8; 32], "v".into())
        .expect("test invariant");
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn whole_field_clone_equality() {
    let a = WholeFieldCoverageProofV1::new(
        "f".into(),
        WholeFieldCoverageMethodV1::IntervalEnclosure { interval_count: 8 },
        "d".into(),
    )
    .expect("test invariant");
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn different_inputs_produce_unequal_instances() {
    let a = CompositionLawProofV1::new(
        "p1".into(),
        "d".into(),
        CompositionLawVerificationMethodV1::Enumerated,
    )
    .expect("test invariant");
    let b = CompositionLawProofV1::new(
        "p2".into(),
        "d".into(),
        CompositionLawVerificationMethodV1::Enumerated,
    )
    .expect("test invariant");
    assert_ne!(a, b);
}

// =============================================================================
// Field-level round-trip tests (5) — replaces serde round-trip since core is
// dependency-free. Verifies that Clone preserves all fields including digest,
// which is the invariant that wire serialization would also need to satisfy.
// Serde wrappers will be added in PR-C when untrusted deserialization arises.
// =============================================================================

#[test]
fn law_proof_field_round_trip_all_methods() {
    let methods = vec![
        CompositionLawVerificationMethodV1::Enumerated,
        CompositionLawVerificationMethodV1::AnalyticBounds,
        CompositionLawVerificationMethodV1::RasterVerified {
            renderer_profile_id: "r".into(),
            width: 64,
            height: 64,
        },
    ];
    for method in methods {
        let original =
            CompositionLawProofV1::new("p".into(), "d".into(), method).expect("test invariant");
        let restored = original.clone();
        assert_eq!(original.profile_id, restored.profile_id);
        assert_eq!(original.domain_descriptor, restored.domain_descriptor);
        assert_eq!(original.verification_method, restored.verification_method);
        assert_eq!(original.digest, restored.digest);
        assert_eq!(original, restored);
    }
}

#[test]
fn owned_reference_field_round_trip() {
    let original = OwnedCompositionReferenceV1::new([5u8; 32], "r".into(), "s".into())
        .expect("test invariant");
    let restored = original.clone();
    assert_eq!(original.law_proof_digest, restored.law_proof_digest);
    assert_eq!(original.owned_root_id, restored.owned_root_id);
    assert_eq!(original.support_domain_id, restored.support_domain_id);
    assert_eq!(original.digest, restored.digest);
    assert_eq!(original, restored);
}

#[test]
fn base_accepted_field_round_trip() {
    let original = BaseAcceptedOnSupportCertificateV1::new("s".into(), "e".into(), "p".into(), 999)
        .expect("test invariant");
    let restored = original.clone();
    assert_eq!(original.support_domain_id, restored.support_domain_id);
    assert_eq!(
        original.evaluation_context_id,
        restored.evaluation_context_id
    );
    assert_eq!(original.predicate_set_id, restored.predicate_set_id);
    assert_eq!(original.verified_count, restored.verified_count);
    assert_eq!(original.digest, restored.digest);
    assert_eq!(original, restored);
}

#[test]
fn no_reject_field_round_trip() {
    let original = NoIntroducedRejectCertificateV1::new("r".into(), [11u8; 32], "v".into())
        .expect("test invariant");
    let restored = original.clone();
    assert_eq!(original.owned_root_id, restored.owned_root_id);
    assert_eq!(
        original.pre_pass_snapshot_digest,
        restored.pre_pass_snapshot_digest
    );
    assert_eq!(
        original.post_pass_verification_id,
        restored.post_pass_verification_id
    );
    assert_eq!(original.digest, restored.digest);
    assert_eq!(original, restored);
}

#[test]
fn whole_field_field_round_trip_all_methods() {
    let methods = vec![
        WholeFieldCoverageMethodV1::Enumeration {
            occurrence_count: 100,
        },
        WholeFieldCoverageMethodV1::AnalyticExtrema {
            critical_point_count: 5,
        },
        WholeFieldCoverageMethodV1::IntervalEnclosure { interval_count: 32 },
        WholeFieldCoverageMethodV1::RendererBoundRaster {
            renderer_profile_id: "rp".into(),
            width: 128,
            height: 128,
        },
    ];
    for method in methods {
        let original =
            WholeFieldCoverageProofV1::new("f".into(), method, "d".into()).expect("test invariant");
        let restored = original.clone();
        assert_eq!(original.field_identity, restored.field_identity);
        assert_eq!(original.method, restored.method);
        assert_eq!(original.coverage_descriptor, restored.coverage_descriptor);
        assert_eq!(original.digest, restored.digest);
        assert_eq!(original, restored);
    }
}

// =============================================================================
// Absence-law compliance tests (5)
// =============================================================================

/// Reads the source files of this module and checks for forbidden patterns.
/// These are static-analysis-style tests that enforce V7 staging requirements.
mod absence_law {
    use std::fs;
    use std::path::Path;

    fn read_module_file(name: &str) -> String {
        // cargo test runs with CWD set to the package root (crates/labcolors-core),
        // not the workspace root. Navigate relative to that.
        let path = Path::new("src/composition_proof").join(name);
        fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e))
    }

    fn production_source_files() -> Vec<(&'static str, String)> {
        vec![
            ("mod.rs", read_module_file("mod.rs")),
            ("law_proof.rs", read_module_file("law_proof.rs")),
            ("owned_reference.rs", read_module_file("owned_reference.rs")),
            ("base_accepted.rs", read_module_file("base_accepted.rs")),
            ("no_reject.rs", read_module_file("no_reject.rs")),
            ("whole_field.rs", read_module_file("whole_field.rs")),
        ]
    }

    #[test]
    fn no_unwrap_in_composition_proof_module() {
        for (name, content) in production_source_files() {
            for (line_no, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("//") || trimmed.starts_with("///") {
                    continue;
                }
                assert!(
                    !trimmed.contains(".unwrap()"),
                    "{name}:{} contains .unwrap(): {trimmed}",
                    line_no + 1
                );
                assert!(
                    !trimmed.contains(".expect("),
                    "{name}:{} contains .expect(): {trimmed}",
                    line_no + 1
                );
            }
        }
    }

    #[test]
    fn no_unsafe_in_composition_proof_module() {
        for (name, content) in production_source_files() {
            for (line_no, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("//") || trimmed.starts_with("///") {
                    continue;
                }
                if trimmed.contains("unsafe ") || trimmed == "unsafe {" {
                    panic!("{name}:{} contains unsafe: {trimmed}", line_no + 1);
                }
            }
        }
    }

    #[test]
    fn no_arc_mutex_in_types() {
        for (name, content) in production_source_files() {
            for (line_no, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("//") || trimmed.starts_with("///") {
                    continue;
                }
                assert!(
                    !trimmed.contains("Arc<") && !trimmed.contains("Mutex<"),
                    "{name}:{} contains Arc or Mutex: {trimmed}",
                    line_no + 1
                );
            }
        }
    }

    #[test]
    fn dead_code_expect_present_on_all_types() {
        let type_def_files = vec![
            ("law_proof.rs", read_module_file("law_proof.rs")),
            ("owned_reference.rs", read_module_file("owned_reference.rs")),
            ("base_accepted.rs", read_module_file("base_accepted.rs")),
            ("no_reject.rs", read_module_file("no_reject.rs")),
            ("whole_field.rs", read_module_file("whole_field.rs")),
        ];
        for (name, content) in type_def_files {
            let lines: Vec<&str> = content.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("pub(crate) struct ")
                    || trimmed.starts_with("pub(crate) enum ")
                {
                    let mut found = false;
                    for j in (0..i).rev() {
                        let prev = lines[j].trim();
                        if prev.starts_with("#[allow(dead_code") || prev.starts_with("#[expect(dead_code") {
                            found = true;
                            break;
                        }
                        if prev.starts_with("pub(crate) ")
                            || prev.starts_with("impl ")
                            || prev == "}"
                        {
                            break;
                        }
                    }
                    assert!(
                        found,
                        "{name}:{} missing #[expect(dead_code)] on: {trimmed}",
                        i + 1
                    );
                }
            }
        }
    }

    #[test]
    fn no_legacy_composition_types() {
        for (name, content) in production_source_files() {
            assert!(
                !content.contains("UnversionedComposition"),
                "{name} contains legacy UnversionedComposition"
            );
            assert!(
                !content.contains("CompositionProofV0"),
                "{name} contains legacy CompositionProofV0"
            );
            assert!(
                !content.contains("LegacyComposition"),
                "{name} contains legacy LegacyComposition"
            );
        }
    }
}
