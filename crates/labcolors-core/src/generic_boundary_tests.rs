const APPEARANCE_SOURCE: &str = include_str!("appearance.rs");
const LCS_OCCURRENCE_SOURCE: &str = include_str!("lcs_occurrence.rs");
const PROGRAM_SESSION_SOURCE: &str = include_str!("program_session.rs");

const GENERIC_SOURCES: [(&str, &str); 3] = [
    ("appearance.rs", APPEARANCE_SOURCE),
    ("lcs_occurrence.rs", LCS_OCCURRENCE_SOURCE),
    ("program_session.rs", PROGRAM_SESSION_SOURCE),
];

const CLIENT_OR_LEGACY_VOCABULARY: [&str; 14] = [
    "ThemeConfig",
    "RoleRecipe",
    "RoleSpec",
    "NamedRoleTable",
    "PairFill",
    "PairLabel",
    "Glow",
    "Material",
    "Ladder",
    "AlphaAnalog",
    "themeHandle",
    "resolveTheme",
    "Primary",
    "Danger",
];

#[test]
fn generic_physical_and_transport_modules_contain_no_client_or_legacy_vocabulary() {
    for (path, source) in GENERIC_SOURCES {
        for forbidden in CLIENT_OR_LEGACY_VOCABULARY {
            assert!(
                !source.contains(forbidden),
                "{path} must remain client-semantic agnostic; found `{forbidden}`"
            );
        }
    }
}

#[test]
fn encoded_point_transport_does_not_claim_lcs_observation_types() {
    for forbidden in ["TristimulusSample", "LcsOccurrence", "AppearanceState"] {
        assert!(
            !PROGRAM_SESSION_SOURCE.contains(forbidden),
            "program_session.rs is encoded point transport, not `{forbidden}` evidence"
        );
    }
}

#[test]
fn program_session_module_docs_disclaim_transport_only_scope() {
    let module_docs = PROGRAM_SESSION_SOURCE
        .lines()
        .take_while(|line| line.starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();

    for required in ["transport-only", "encoded", "not", "lcs", "evidence"] {
        assert!(
            module_docs.contains(required),
            "program_session.rs module docs must explicitly disclaim transport-only scope; missing `{required}`"
        );
    }
}

#[test]
fn f0_signal_transform_has_no_renderer_alias_or_raw_xyz_production_constructor() {
    for forbidden in [
        "RenderProfileId",
        "AppearanceContextReleaseId",
        "enum ObserveError",
        "fn observe(",
        "pub(crate) fn new(\n        xyz",
    ] {
        assert!(
            !LCS_OCCURRENCE_SOURCE.contains(forbidden),
            "lcs_occurrence.rs must keep the F0 identity boundary sealed; found `{forbidden}`",
        );
    }

    for required in [
        "ColorimetricTransformReleaseId",
        "AppearanceContextSchemaReleaseId",
        "fn admitted_binding(",
        "match output_profile",
        "fn derive_sample_with_binding(",
        "binding.transform_release()",
        "xyz_d65_from_srgb8_v1",
        "ModeledTristimulusProvenanceV1",
        "ModeledTristimulusDerivationV1",
    ] {
        assert!(
            LCS_OCCURRENCE_SOURCE.contains(required),
            "lcs_occurrence.rs must retain the sealed F0 route; missing `{required}`",
        );
    }

    let raw_xyz_declaration = LCS_OCCURRENCE_SOURCE
        .lines()
        .find(|line| line.contains("fn try_from_registered_xyz("))
        .expect("registered raw-XYZ admission must remain visible to this source gate");
    assert_eq!(
        raw_xyz_declaration.trim(),
        "fn try_from_registered_xyz(",
        "registered raw-XYZ admission must remain module-private",
    );

    let raw_frame_declaration = LCS_OCCURRENCE_SOURCE
        .lines()
        .find(|line| line.contains("const fn registered("))
        .expect("registered frame constructor must remain visible to this source gate");
    assert_eq!(
        raw_frame_declaration.trim(),
        "const fn registered(",
        "registered frame construction must remain module-private",
    );

    let dispatch_start = LCS_OCCURRENCE_SOURCE
        .find("fn admitted_binding(")
        .expect("binding dispatch must remain present");
    let dispatch_end = LCS_OCCURRENCE_SOURCE[dispatch_start..]
        .find("/// Derive one modeled tristimulus")
        .map(|offset| dispatch_start + offset)
        .expect("modeled derivation docs must delimit the dispatch source gate");
    let dispatch_source = &LCS_OCCURRENCE_SOURCE[dispatch_start..dispatch_end];
    assert!(
        !dispatch_source.contains("_ =>"),
        "closed F0 profile/transform dispatch must not silently fall back",
    );
}

#[test]
fn f0_modeled_derivation_mints_no_observation_or_bounded_evidence() {
    for forbidden in [
        "BoundEvidence",
        "NumericalDecisionEvidenceV1",
        "CanonicalFiniteBoundedEvidenceV1",
        "SourceOverCertificateV1",
        "GlowCompositeCertificateV1",
        "RendererCapability",
        "RenderObservation",
        "crate::constraints",
        "crate::wcag22_evidence",
    ] {
        assert!(
            !LCS_OCCURRENCE_SOURCE.contains(forbidden),
            "deterministic signal lowering cannot mint `{forbidden}`",
        );
    }
}
