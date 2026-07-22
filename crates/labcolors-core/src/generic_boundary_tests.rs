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
