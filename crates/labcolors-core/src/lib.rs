// BEGIN WCAG22_SOURCE_ROUTES_V1
const _: () = (); // First-item proof anchor; moving it fails verify_wcag22_q55.py.
pub mod numerics;
pub(crate) mod srgb8;
pub mod wcag22;
#[doc(hidden)]
pub mod wcag22_evidence;
// END WCAG22_SOURCE_ROUTES_V1

pub(crate) mod clean_set;
#[cfg_attr(test, allow(dead_code))]
pub(crate) mod composition;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the contextual family definition is staged before its offline proof kernel"
    )
)]
pub(crate) mod contextual_region;
mod family;
mod family_artifact;
mod family_definition_binding;
pub(crate) mod field_effect;
#[cfg(test)]
mod field_effect_tests;
#[allow(dead_code)]
pub(crate) mod incremental_runtime;
pub(crate) mod spaces;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the alpha/backdrop TQ substrate is staged before its R-10 field consumer"
    )
)]
pub(crate) mod technical_quality;

pub use srgb8::Srgb8;

pub(crate) mod accent_balance;
pub mod alpha;
pub(crate) mod appearance;
#[allow(dead_code)]
pub(crate) mod config;
pub(crate) mod constraints;
#[allow(dead_code)]
pub(crate) mod corridor_representation;
#[allow(dead_code)]
pub(crate) mod glow;
pub mod hash;
#[allow(dead_code)]
pub(crate) mod ladder;
pub mod lcs;
#[expect(
    dead_code,
    reason = "F0 colour-identity internals are exposed only through typed Program evidence"
)]
pub(crate) mod lcs_occurrence;
pub(crate) mod lpc;
#[allow(dead_code)]
pub(crate) mod material;
pub mod neutral;
pub mod numerical_plan;
#[allow(dead_code)]
mod output_bindings;
#[expect(
    dead_code,
    reason = "the output-profile firewall is intentionally internal to registered profiles"
)]
pub(crate) mod output_projection;
pub(crate) mod point_representation;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the full-support recheck remains a private verified engine"
    )
)]
pub(crate) mod point_support;
#[cfg(feature = "private-fixture")]
#[doc(hidden)]
mod private_fixture;
#[expect(
    dead_code,
    reason = "the Program internals remain private; terminal C7c exposes only program_wire wrappers"
)]
#[deny(missing_docs)]
pub(crate) mod program;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "generic Program machinery is used only through the public program_wire wrappers"
    )
)]
pub(crate) mod program_session;
pub mod program_wire;
pub mod recheck;
pub(crate) mod relation;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "release-registry internals are projected only through typed Program evidence"
    )
)]
pub(crate) mod release_registry;
#[allow(dead_code)] // R-07 PR-A: restorative auto types staged before runtime integration in PR-C
pub(crate) mod restorative_auto;
pub mod scale;
#[allow(dead_code)]
pub(crate) mod semantic;
pub(crate) mod sha256;
#[cfg_attr(test, allow(dead_code))]
pub mod solve;

pub mod curve;

#[cfg(test)]
pub(crate) mod exposure_support;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod golden_tests;

#[cfg(test)]
mod appearance_graph_tests;

#[cfg(test)]
mod appearance_replay_tests;

#[cfg(test)]
mod lcs_occurrence_tests;

#[cfg(test)]
mod output_projection_tests;

#[cfg(test)]
mod program_session_tests;

#[cfg(test)]
mod program_lcs_integration_tests;

#[cfg(test)]
mod program_joint_integration_tests;

#[cfg(test)]
mod program_point_causality_tests;

#[cfg(test)]
mod program_mixed_evaluator_tests;

#[cfg(test)]
mod program_identity_tests;

#[cfg(test)]
mod program_boundary_tests;

#[cfg(test)]
mod program_api_tests;
#[cfg(test)]
mod program_clean_set_tests;
#[cfg(test)]
mod program_relation_tests;

#[cfg(test)]
mod program_category_relation_tests;

#[cfg(test)]
mod program_v5_exit_gate_tests;

#[cfg(test)]
mod program_family_tests;

#[cfg(test)]
mod release_registry_tests;

#[cfg(test)]
mod generic_boundary_tests;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "raw observation ownership is used only through the staged Program session"
    )
)]
pub(crate) mod observation;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "ReportArenaPoolV1 is staged V7 infrastructure before PR4 wires it into ProgramSession"
    )
)]
pub(crate) mod report_arena;

#[cfg(test)]
mod observation_tests;

#[cfg(test)]
mod observation_differential_oracle_tests;

#[cfg(test)]
mod point_support_tests;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the generic Session engine is used only through the staged Program owner"
    )
)]
pub(crate) mod session;

#[cfg(test)]
mod session_tests;

pub(crate) mod joint;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the authored selection release materialises the joint order from V5c-2"
    )
)]
pub(crate) mod selection_release;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "R-07 PR-A restorative-auto types are staged before upstream TQ substrates land"
    )
)]
#[cfg(test)]
mod selection_release_tests;

#[cfg(test)]
mod selection_release_materialisation_tests;

#[cfg(test)]
mod constraint_tests;

#[cfg(test)]
mod family_artifact_tests;

#[cfg(test)]
mod family_definition_binding_tests;

#[cfg(test)]
mod clean_set_tests;

#[cfg(test)]
mod contextual_region_tests;

#[cfg(test)]
mod contextual_region_formula_tests;

#[cfg(test)]
mod wcag22_tests;

#[cfg(test)]
mod lcs_hue_dimensionality_tests;

// Built-in-showcase behaviour tests, relocated in-crate (ADR-0001): the
// built-in `Role`/`RoleTable`/`resolve_set` cluster is now `#[cfg(test)]`-only,
// so these tests РІР‚вЂќ which exercise it as the byte-identity oracle РІР‚вЂќ must live
// inside the crate to see it (integration tests only see the public API).

// Reference checks for the deepest colour-science layers (sRGB EOTF & matrices,
// Ottosson Oklab, CAT16/CIECAM16 adapt, Hellwig-2022 H-K, WCAG linearise). These
// reach `pub(crate)` transforms an integration test in `tests/` cannot see; the
// public-API-reachable checks live in `tests/reference_vectors.rs`, with source
// and oracle scope beside each test.
#[cfg(test)]
mod reference_vectors_deep;

// AccentCurve golden snapshots are in-crate because their built-in showcase
// anchors are `#[cfg(test)]`-only.

pub use alpha::composite_over_encoded;
pub use curve::{ColorCurve, CurvePosition, CurvePositionError};
pub use hash::fnv1a_32;
// Solver curve uses deprecated LcsColor per F-01 design
#[allow(deprecated)]
pub use lcs::LcsColor;
pub use numerical_plan::{
    CompiledInvocationIdV1, CompiledNumericalInvocationV1, CompiledNumericalPlanV1,
    NUMERICAL_PLAN_SCHEMA_VERSION_V1, NumericalExecutionModeV1, NumericalPlanChecksumV1,
    NumericalPlanErrorV1, compile_numerical_plan_v1,
};
pub use numerics::{
    LegacyPlatformDependentV1, NUMERICAL_CAPABILITY_SCHEMA_VERSION_V2, NumericalArtifactIdV2,
    NumericalBoundStatusV2, NumericalCapabilityChecksumV2, NumericalCapabilityManifestV2,
    NumericalCompatibilityReleaseIdV1, NumericalDecisionEvidenceV1, NumericalDecisionV1,
    NumericalErrorBoundIdV2, NumericalEvidenceClassV2, NumericalFallbackStatusV1,
    NumericalIndeterminacyV1, NumericalProofIdV2, NumericalRegistryCoverageV2,
    NumericalRuntimeAttestationIdV2, NumericalSiteCapabilityV2, NumericalSiteIdV1,
    NumericalSiteIdV2, NumericalSiteRecordV2, OutwardIntervalV1, ReferenceProfileIdV1,
    StableNumericalOutcomeV2, numerical_capability_manifest_v2, numerical_registry_v2,
};
pub use recheck::{
    measure_contrast, recheck_against, recheck_against_multi, recheck_against_multi_u32,
    recheck_against_u32,
};
pub use wcag22_evidence::CanonicalFiniteBoundedEvidenceV1;
// The built-in v1 showcase (`Role`/`RoleTable`/`resolve`/`resolve_set`) is no
// longer part of the production API (ADR-0001): the agnostic engine ships
// only the string-keyed `resolve_named_set` path. It survives ONLY as the
// `#[cfg(test)]` byte-identity oracle for the named path, re-exported crate-wide
// so the in-crate showcase tests keep their `crate::РІР‚В¦` spellings.
pub use solve::{
    BgInput, ChromaPolicy, Contract, Gamut, Hue, SolveFailure, SolveFailureBoundary,
    SolveFailureCategory, SolveJob, Solved, solve, solve_many,
};
pub use spaces::oklch::{css_alpha_value, oklch_css_from_hex, oklch_from_hex};
pub use spaces::srgb::srgb_encoded_from_hex;
pub use spaces::vc::ViewingConditions;

/// Р С™Р С•Р СР С—Р С‘Р В»Р С‘РЎР‚РЎС“Р ВµРЎвЂљ rust-Р В±Р В»Р С•Р С”Р С‘ package-local README Р С”Р В°Р С” doctest-РЎвЂ№: Р С•Р С—РЎС“Р В±Р В»Р С‘Р С”Р С•Р Р†Р В°Р Р…Р Р…РЎвЂ№Р в„–
/// crate Р С•Р В±РЎРЏР В·Р В°Р Р… Р Р…Р ВµРЎРѓРЎвЂљР С‘ Р С‘ Р С‘РЎРѓР С—Р С•Р В»Р Р…РЎРЏРЎвЂљРЎРЉ РЎРѓР С•Р В±РЎРѓРЎвЂљР Р†Р ВµР Р…Р Р…РЎС“РЎР‹ Р Т‘Р С•Р С”РЎС“Р СР ВµР Р…РЎвЂљР В°РЎвЂ Р С‘РЎР‹ Р В±Р ВµР В· РЎвЂћР В°Р в„–Р В»Р С•Р Р† Р Р†РЎвЂ№РЎв‚¬Р Вµ
/// package root. Р СћР С‘Р С— РЎРѓРЎС“РЎвЂ°Р ВµРЎРѓРЎвЂљР Р†РЎС“Р ВµРЎвЂљ РЎвЂљР С•Р В»РЎРЉР С”Р С• Р С—Р С•Р Т‘ `--test`, Р Р† Р В±Р С‘Р Р…Р В°РЎР‚РЎРЉ Р Р…Р Вµ Р Р†РЎвЂ¦Р С•Р Т‘Р С‘РЎвЂљ.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

/// Р вЂ”Р В°Р С”РЎР‚РЎвЂ№РЎвЂљР В°РЎРЏ РЎвЂћР С‘Р В·Р С‘РЎвЂЎР ВµРЎРѓР С”Р В°РЎРЏ РЎвЂљР С•Р С—Р С•Р В»Р С•Р С–Р С‘РЎРЏ Р С‘ recipe-Р Р†Р В°РЎР‚Р С‘Р В°Р Р…РЎвЂљРЎвЂ№ РІР‚вЂќ Р Т‘Р ВµРЎвЂљР В°Р В»Р С‘ resolver-Р В°, Р В° Р Р…Р Вµ
/// extension points. Р СџРЎС“Р В±Р В»Р С‘РЎвЂЎР Р…РЎвЂ№Р в„– API Р Р…Р Вµ РЎР‚Р В°РЎРѓР С”РЎР‚РЎвЂ№Р Р†Р В°Р ВµРЎвЂљ client-authored topology.
///
/// ```compile_fail
/// use labcolors_core::accent_balance::accent_balanced;
/// ```
///
/// ```compile_fail
/// use labcolors_core::accent_surface::derive_accent_surface_ramp;
/// ```
#[cfg(doctest)]
pub struct InternalAccentRecipes;

/// Р СћР С•РЎвЂЎР Р…РЎвЂ№Р Вµ Р Р†РЎвЂ№РЎвЂ¦Р С•Р Т‘Р Р…РЎвЂ№Р Вµ Р В±Р В°Р в„–РЎвЂљРЎвЂ№ Р Р…Р Вµ Р Т‘Р С•Р С”Р В°Р В·РЎвЂ№Р Р†Р В°РЎР‹РЎвЂљ Р С—Р ВµРЎР‚РЎвЂ Р ВµР С—РЎвЂљР С‘Р Р†Р Р…РЎС“РЎР‹ Р Р†Р С‘Р Т‘Р С‘Р СР С•РЎРѓРЎвЂљРЎРЉ Р С•РЎвЂљРЎвЂљР ВµР Р…Р С”Р В°.
/// Р Р€Р Т‘Р В°Р В»РЎвЂР Р…Р Р…РЎвЂ№Р в„– Р Р†Р ВµРЎР‚Р Т‘Р С‘Р С”РЎвЂљ Р Р…Р ВµР В»РЎРЉР В·РЎРЏ Р Р†Р С•РЎРѓРЎРѓРЎвЂљР В°Р Р…Р В°Р Р†Р В»Р С‘Р Р†Р В°РЎвЂљРЎРЉ Р С‘Р В· Р С”Р В°Р С”Р С•Р в„–-Р В»Р С‘Р В±Р С• РЎвЂћР С•РЎР‚Р СРЎвЂ№ РЎР‚Р ВµР В·РЎС“Р В»РЎРЉРЎвЂљР В°РЎвЂљР В°.
///
/// ```compile_fail
/// use labcolors_core::Resolved;
///
/// fn inferred_verdict(result: &Resolved) -> bool {
///     match result {
///         Resolved::Color { hue_vanished, .. } => *hue_vanished,
///         _ => false,
///     }
/// }
/// ```
///
/// ```compile_fail
/// use labcolors_core::Resolved;
///
/// fn inferred_verdict(result: &Resolved) -> bool {
///     match result {
///         Resolved::Material(material) => material.hue_vanished(),
///         _ => false,
///     }
/// }
/// ```
#[cfg(doctest)]
pub struct NoHueVisibilityVerdict;

/// Р Р€РЎРѓРЎвЂљР В°РЎР‚Р ВµР Р†РЎв‚¬Р С‘Р Вµ compatibility-Р В°Р В»Р С‘Р В°РЎРѓРЎвЂ№ Р Р…Р Вµ Р Р†РЎвЂ¦Р С•Р Т‘РЎРЏРЎвЂљ Р Р† breaking-РЎР‚Р ВµР В»Р С‘Р В· Р Т‘Р С• Р С”Р В»Р С‘Р ВµР Р…РЎвЂљР С•Р Р†.
/// Р вЂўР Т‘Р С‘Р Р…РЎРѓРЎвЂљР Р†Р ВµР Р…Р Р…РЎвЂ№Р в„– SSOT РІР‚вЂќ РЎвЂљР С‘Р С—Р С‘Р В·Р С‘РЎР‚Р С•Р Р†Р В°Р Р…Р Р…РЎвЂ№Р Вµ РЎРѓРЎвЂљР В°РЎвЂљРЎС“РЎРѓРЎвЂ№ Р С‘ РЎРЏР Р†Р Р…Р С• Р Р…Р В°Р В·Р Р†Р В°Р Р…Р Р…РЎвЂ№Р Вµ Р С‘Р В·Р СР ВµРЎР‚Р ВµР Р…Р С‘РЎРЏ.
///
/// ```compile_fail
/// use labcolors_core::Resolved;
///
/// fn old_measurement_alias(result: &Resolved) -> Option<f64> {
///     match result {
///         Resolved::Glow(glow) => Some(glow.achieved_dj()),
///         _ => None,
///     }
/// }
/// ```
///
/// ```compile_fail
/// use labcolors_core::Resolved;
///
/// fn old_boolean_alias(result: &Resolved) -> Option<bool> {
///     match result {
///         Resolved::Glow(glow) => Some(glow.degraded()),
///         _ => None,
///     }
/// }
/// ```
///
/// ```compile_fail
/// use labcolors_core::Resolved;
///
/// fn old_boolean_alias(result: &Resolved) -> Option<bool> {
///     match result {
///         Resolved::Material(material) => Some(material.guaranteed()),
///         _ => None,
///     }
/// }
/// ```
#[cfg(doctest)]
pub struct NoCompatibilityAliases;

/// Р СљР ВµРЎвЂљРЎР‚Р С‘Р С”Р В° Р С—Р С•Р Р†Р ВµРЎР‚РЎвЂ¦Р Р…Р С•РЎРѓРЎвЂљР С‘ Р Р…Р Вµ РЎРѓР СР ВµРЎв‚¬Р С‘Р Р†Р В°Р ВµРЎвЂљ Р С”Р С•Р С•РЎР‚Р Т‘Р С‘Р Р…Р В°РЎвЂљРЎвЂ№ Р Р…Р ВµРЎРѓР С•Р Р†Р СР ВµРЎРѓРЎвЂљР С‘Р СРЎвЂ№РЎвЂ¦ appearance-
/// Р С—РЎР‚Р С•РЎРѓРЎвЂљРЎР‚Р В°Р Р…РЎРѓРЎвЂљР Р† Р С‘ Р Р…Р Вµ Р С—РЎС“Р В±Р В»Р С‘Р С”РЎС“Р ВµРЎвЂљ РЎР‚Р ВµР В·РЎС“Р В»РЎРЉРЎвЂљР В°РЎвЂљ Р С”Р В°Р С” LPC. Р вЂРЎС“Р Т‘РЎС“РЎвЂ°Р ВµР СРЎС“ Р С—РЎР‚Р С‘Р СР С‘РЎвЂљР С‘Р Р†РЎС“ РЎР‚Р В°РЎРѓРЎРѓРЎвЂљР С•РЎРЏР Р…Р С‘РЎРЏ
/// Р Р…РЎС“Р В¶Р Р…РЎвЂ№ РЎРѓР С•Р В±РЎРѓРЎвЂљР Р†Р ВµР Р…Р Р…РЎвЂ№Р Вµ Р Т‘Р С•Р С—РЎС“РЎвЂ°Р ВµР Р…Р Р…РЎвЂ№Р Вµ Р СР С•Р Т‘Р ВµР В»РЎРЉ, Р С‘Р СРЎРЏ, Р Т‘Р С•Р СР ВµР Р… Р С‘ Р Р…Р ВµР В·Р В°Р Р†Р С‘РЎРѓР С‘Р СРЎвЂ№Р в„– oracle.
///
/// ```compile_fail
/// use labcolors_core::lpc::lpc_surface;
/// ```
///
/// ```compile_fail
/// use labcolors_core::lpc::lpc_surface_with_vc;
/// ```
#[cfg(doctest)]
pub struct NoHybridLpcSurfaceMetric;

/// Р СњР ВµР С—Р С•Р В»Р Р…РЎвЂ№Р Вµ РЎРѓР С”Р В°Р В»РЎРЏРЎР‚Р Р…РЎвЂ№Р Вµ РЎвЂћРЎС“Р Р…Р С”РЎвЂ Р С‘Р С‘ Р Р…Р Вµ Р С•Р В±РЎР‚Р В°Р В·РЎС“РЎР‹РЎвЂљ Р С—РЎС“Р В±Р В»Р С‘РЎвЂЎР Р…РЎвЂ№Р в„– Р С”Р С•Р Р…РЎвЂљРЎР‚Р В°Р С”РЎвЂљ LPC. Р СџР С•Р В»Р Вµ `lc`
/// Р С—Р ВµРЎР‚Р ВµРЎвЂ¦Р С•Р Т‘Р Р…Р С•Р С–Р С• resolver РІР‚вЂќ РЎвЂљР С•Р В»РЎРЉР С”Р С• Р В·Р В°РЎвЂћР С‘Р С”РЎРѓР С‘РЎР‚Р С•Р Р†Р В°Р Р…Р Р…Р В°РЎРЏ candidate-Р С”Р С•Р С•РЎР‚Р Т‘Р С‘Р Р…Р В°РЎвЂљР В°. LPC
/// РЎРѓРЎвЂљР В°Р Р…Р С•Р Р†Р С‘РЎвЂљРЎРѓРЎРЏ Р С—РЎС“Р В±Р В»Р С‘РЎвЂЎР Р…РЎвЂ№Р С РЎС“РЎвЂљР Р†Р ВµРЎР‚Р В¶Р Т‘Р ВµР Р…Р С‘Р ВµР С Р В»Р С‘РЎв‚¬РЎРЉ РЎвЂЎР ВµРЎР‚Р ВµР В· Р Р†Р ВµРЎР‚РЎРѓР С‘Р С•Р Р…Р С‘РЎР‚Р С•Р Р†Р В°Р Р…Р Р…РЎвЂ№Р в„– РЎР‚Р ВµР ВµРЎРѓРЎвЂљРЎР‚
/// evaluators РЎРѓ Р С‘Р Т‘Р ВµР Р…РЎвЂљР С‘РЎвЂЎР Р…Р С•РЎРѓРЎвЂљРЎРЏР СР С‘ РЎРѓРЎвЂљР С‘Р СРЎС“Р В»Р В°, Р С”Р С•Р Р…РЎвЂљР ВµР С”РЎРѓРЎвЂљР В°, Р С—РЎР‚Р С‘Р СР ВµР Р…Р С‘Р СР С•РЎРѓРЎвЂљР С‘ Р С‘ evidence.
///
/// ```compile_fail
/// use labcolors_core::lpc::lpc;
/// ```
#[cfg(doctest)]
pub struct NoPrematureScalarLpcApi;

/// Р С™Р В°Р Р…Р Т‘Р С‘Р Т‘Р В°РЎвЂљ Program Р С•РЎРѓРЎвЂљР В°РЎвЂРЎвЂљРЎРѓРЎРЏ Р Р†Р Р…РЎС“РЎвЂљРЎР‚Р ВµР Р…Р Р…Р С‘Р С Р Т‘Р С• Р В·Р В°Р Р†Р ВµРЎР‚РЎв‚¬Р ВµР Р…Р С‘РЎРЏ terminal C7c: Р Р…Р ВµР С—Р С•Р В»Р Р…РЎС“РЎР‹
/// emission/attachment/transaction Р С—Р С•Р Р†Р ВµРЎР‚РЎвЂ¦Р Р…Р С•РЎРѓРЎвЂљРЎРЉ Р Р…Р ВµР В»РЎРЉР В·РЎРЏ РЎРѓР В»РЎС“РЎвЂЎР В°Р в„–Р Р…Р С• Р С•Р С—РЎС“Р В±Р В»Р С‘Р С”Р С•Р Р†Р В°РЎвЂљРЎРЉ.
///
/// ```compile_fail
/// use labcolors_core::program;
/// ```
///
/// ```compile_fail
/// use labcolors_core::package_bridge;
/// ```
///
/// ```compile_fail
/// use labcolors_core::package_bridge::PackageProgramDraftV1;
/// ```
///
/// ```compile_fail
/// use labcolors_core::program::PackageProgramDraftV1;
/// ```
///
/// ```compile_fail
/// use labcolors_core::DraftV1;
/// ```
///
/// Р вЂўР Т‘Р С‘Р Р…РЎРѓРЎвЂљР Р†Р ВµР Р…Р Р…РЎвЂ№Р в„– Р Т‘Р С•Р С—РЎС“РЎвЂ°Р ВµР Р…Р Р…РЎвЂ№Р в„– Р Т‘Р С• C7c seam РІР‚вЂќ Р С—РЎР‚Р С•Р Р†Р ВµРЎР‚Р С”Р В° Р С”Р В°Р Р…Р С•Р Р…Р С‘РЎвЂЎР ВµРЎРѓР С”Р С‘РЎвЂ¦ wire-Р В±Р В°Р в„–РЎвЂљР С•Р Р†
/// (`program_wire`): Р С•Р Р…Р В° Р Р…Р Вµ Р Р†РЎвЂ№Р Т‘Р В°РЎвЂРЎвЂљ runtime-authority. Р вЂ™Р Р…РЎС“РЎвЂљРЎР‚Р ВµР Р…Р Р…Р С•РЎРѓРЎвЂљР С‘ wire-Р СР С•Р Т‘РЎС“Р В»РЎРЏ
/// Р С•РЎРѓРЎвЂљР В°РЎР‹РЎвЂљРЎРѓРЎРЏ Р В·Р В°Р С”РЎР‚РЎвЂ№РЎвЂљРЎвЂ№Р СР С‘:
///
/// ```compile_fail
/// use labcolors_core::program::wire::decode_program_wire_v1;
/// ```
#[cfg(doctest)]
pub struct NoPrematureProgramApi;

/// C8d recheck Р С‘ F2 observation Р С•РЎРѓРЎвЂљР В°РЎР‹РЎвЂљРЎРѓРЎРЏ Р Т‘Р ВµРЎвЂљР В°Р В»РЎРЏР СР С‘ Р С•Р Т‘Р Р…Р С•Р в„– Р С—РЎР‚Р С‘Р Р†Р В°РЎвЂљР Р…Р С•Р в„– Session;
/// Р С•Р Р…Р С‘ Р Р…Р Вµ Р СР С•Р С–РЎС“РЎвЂљ РЎРѓРЎвЂљР В°РЎвЂљРЎРЉ Р Т‘Р С•Р С—Р С•Р В»Р Р…Р С‘РЎвЂљР ВµР В»РЎРЉР Р…РЎвЂ№Р СР С‘ public authoring/runtime roots.
///
/// ```compile_fail
/// use labcolors_core::point_support::CompiledPointSupportRecheckV1;
/// ```
///
/// ```compile_fail
/// use labcolors_core::observation::RevisionBoundObservationV1;
/// ```
///
/// ```compile_fail
/// use labcolors_core::session::Session;
/// ```
#[cfg(doctest)]
pub struct NoPrematurePointSupportApi;

/// Р РЋР В»Р С•Р в„– РЎРѓР ВµР СР ВµР в„–РЎРѓРЎвЂљР Р† Р В·Р В°Р С”РЎР‚РЎвЂ№РЎвЂљ Р Р…Р В°Р СР ВµРЎР‚Р ВµР Р…Р Р…Р С•, Р С‘ РЎС“РЎРѓР В»Р С•Р Р†Р С‘Р Вµ Р ВµР С–Р С• Р С•РЎвЂљР С”РЎР‚РЎвЂ№РЎвЂљР С‘РЎРЏ РІР‚вЂќ Р С”Р С•Р Р…РЎвЂљРЎР‚Р В°Р С”РЎвЂљ, Р В° Р Р…Р Вµ
/// Р С–Р С•РЎвЂљР С•Р Р†Р Р…Р С•РЎРѓРЎвЂљРЎРЉ Р С”Р С•Р Т‘Р В°.
///
/// Р вЂ”Р В°Р С–РЎР‚РЎС“Р В·РЎвЂЎР С‘Р С” Р В°РЎР‚РЎвЂљР ВµРЎвЂћР В°Р С”РЎвЂљР В° Р С—Р С•Р В»Р С•Р Р…: Р С•Р Р… Р Т‘Р С•Р С—РЎС“РЎРѓР С”Р В°Р ВµРЎвЂљ Р В±Р В°Р в„–РЎвЂљРЎвЂ№ РЎвЂљР С•Р В»РЎРЉР С”Р С• Р С—РЎР‚Р С•РЎвЂљР С‘Р Р† Р Т‘Р С•Р Р†Р ВµРЎР‚Р ВµР Р…Р Р…Р С•Р С–Р С•
/// РЎРѓР ВµРЎР‚РЎвЂљР С‘РЎвЂћР С‘Р С”Р В°РЎвЂљР В°, РЎРѓР Р†Р ВµРЎР‚РЎРЏРЎРЏ Р ВµР С–Р С• Р С—Р С•Р В±Р В°Р в„–РЎвЂљР С•Р Р†Р С•, Р С‘ Р С—Р ВµРЎР‚Р ВµРЎРѓРЎвЂЎР С‘РЎвЂљРЎвЂ№Р Р†Р В°Р ВµРЎвЂљ payload-Р Т‘Р В°Р в„–Р Т‘Р В¶Р ВµРЎРѓРЎвЂљ,
/// receipt, semantic release Р С‘ Р С”Р В°Р Р…Р С•Р Р…Р С‘РЎвЂЎР ВµРЎРѓР С”Р С‘Р в„– Р С•Р В±РЎР‚Р В°Р В·. Р СњР С• **Р В°РЎС“РЎвЂљР ВµР Р…РЎвЂљР С‘РЎвЂћР С‘Р С”Р В°РЎвЂ Р С‘Р С‘ Р С•Р Р… Р Р…Р Вµ
/// Р Р†РЎвЂ№Р С—Р С•Р В»Р Р…РЎРЏР ВµРЎвЂљ**. Р вЂ™Р ВµРЎРѓРЎРЉ Р С—Р ВµРЎР‚Р С‘Р СР ВµРЎвЂљРЎР‚ РІР‚вЂќ Р С—Р С•Р Т‘Р В»Р С‘Р Р…Р Р…Р С•РЎРѓРЎвЂљРЎРЉ Р В·Р В°Р С—Р С‘РЎРѓР С‘ РЎРѓР ВµРЎР‚РЎвЂљР С‘РЎвЂћР С‘Р С”Р В°РЎвЂљР В°, Р С”Р С•РЎвЂљР С•РЎР‚РЎС“РЎР‹
/// Р С—РЎР‚Р ВµР Т‘РЎР‰РЎРЏР Р†Р В»РЎРЏР ВµРЎвЂљ Р Р†РЎвЂ№Р В·РЎвЂ№Р Р†Р В°РЎР‹РЎвЂ°Р С‘Р в„–: Р С—Р С•Р Т‘Р С—Р С‘РЎРѓР С‘ Р С‘ РЎРЏР С”Р С•РЎР‚РЎРЏ Р Т‘Р С•Р Р†Р ВµРЎР‚Р С‘РЎРЏ Р Р† РЎРЏР Т‘РЎР‚Р Вµ Р С•РЎвЂљРЎРѓРЎС“РЎвЂљРЎРѓРЎвЂљР Р†РЎС“РЎР‹РЎвЂљ
/// РЎРѓР С•Р В·Р Р…Р В°РЎвЂљР ВµР В»РЎРЉР Р…Р С•. Р С’Р Т‘РЎР‚Р ВµРЎРѓ Р С•Р С—РЎР‚Р ВµР Т‘Р ВµР В»Р ВµР Р…Р С‘РЎРЏ РІР‚вЂќ РЎРѓР Р†Р С•Р В±Р С•Р Т‘Р Р…Р С•Р Вµ Р С—Р С•Р В»Р Вµ РЎРѓР ВµРЎР‚РЎвЂљР С‘РЎвЂћР С‘Р С”Р В°РЎвЂљР В°, Р С—РЎС“Р В±Р В»Р С‘РЎвЂЎР Р…Р С•
/// Р Р†РЎвЂ№РЎвЂЎР С‘РЎРѓР В»Р С‘Р СР С•Р Вµ Р С‘Р В· `(releases, pipeline, region)`, Р С—Р С•РЎРЊРЎвЂљР С•Р СРЎС“ РЎРѓР В°Р СР С•РЎРѓР С•Р С–Р В»Р В°РЎРѓР С•Р Р†Р В°Р Р…Р Р…РЎвЂ№Р в„–
/// РЎРѓР ВµРЎР‚РЎвЂљР С‘РЎвЂћР С‘Р С”Р В°РЎвЂљ Р Р…Р В°Р Т‘ Р С—РЎР‚Р С•Р С‘Р В·Р Р†Р С•Р В»РЎРЉР Р…РЎвЂ№Р С Р С•Р В±РЎР‚Р В°Р В·Р С•Р С Р СР С‘Р Р…РЎвЂљР С‘РЎвЂљРЎРѓРЎРЏ Р С”Р ВµР С РЎС“Р С–Р С•Р Т‘Р Р…Р С•, Р С‘ Р В·Р В°Р С–РЎР‚РЎС“Р В·РЎвЂЎР С‘Р С”
/// Р С—РЎР‚Р С‘Р СР ВµРЎвЂљ Р ВµР С–Р С• Р Р†Р СР ВµРЎРѓРЎвЂљР Вµ РЎРѓ РЎРѓР С•Р С•РЎвЂљР Р†Р ВµРЎвЂљРЎРѓРЎвЂљР Р†РЎС“РЎР‹РЎвЂ°Р С‘Р С Р В°РЎР‚РЎвЂљР ВµРЎвЂћР В°Р С”РЎвЂљР С•Р С.
///
/// Р С›РЎвЂљРЎРѓРЎР‹Р Т‘Р В° РЎС“РЎРѓР В»Р С•Р Р†Р С‘Р Вµ Р С—РЎС“Р В±Р В»Р С‘Р С”Р В°РЎвЂ Р С‘Р С‘: Р С—Р С•Р Р†Р ВµРЎР‚РЎвЂ¦Р Р…Р С•РЎРѓРЎвЂљРЎРЉ Р Р…Р ВµР В»РЎРЉР В·РЎРЏ Р С•РЎвЂљР С”РЎР‚РЎвЂ№Р Р†Р В°РЎвЂљРЎРЉ, Р С—Р С•Р С”Р В° Р С—РЎС“Р В±Р В»Р С‘РЎвЂЎР Р…РЎвЂ№Р в„–
/// Р С”Р С•Р Р…РЎвЂљРЎР‚Р В°Р С”РЎвЂљ Р С—РЎР‚РЎРЏР СР С• Р Р…Р Вµ РЎРѓР С”Р В°Р В¶Р ВµРЎвЂљ, РЎвЂЎРЎвЂљР С• Р Т‘Р С•Р Р†Р ВµРЎР‚Р С‘Р Вµ Р С” Р В·Р В°Р С—Р С‘РЎРѓР С‘ Р С•Р В±Р ВµРЎРѓР С—Р ВµРЎвЂЎР С‘Р Р†Р В°Р ВµРЎвЂљ Р Р†РЎвЂ№Р В·РЎвЂ№Р Р†Р В°РЎР‹РЎвЂ°Р С‘Р в„–, Р В°
/// Р Р…Р Вµ РЎРЏР Т‘РЎР‚Р С•. Р В­Р С”РЎРѓР С—Р С•РЎР‚РЎвЂљ РЎРѓ Р С‘Р СР ВµР Р…Р ВµР С, Р С•Р В±Р ВµРЎвЂ°Р В°РЎР‹РЎвЂ°Р С‘Р С Р С—РЎР‚Р С•Р Р†Р ВµРЎР‚Р С”РЎС“, Р С”Р С•РЎвЂљР С•РЎР‚Р С•Р в„– Р Р…Р ВµРЎвЂљ, РІР‚вЂќ РЎРЊРЎвЂљР С• РЎвЂ¦РЎС“Р Т‘РЎв‚¬Р С‘Р в„–
/// Р Р†Р С‘Р Т‘ Р СР С•Р В»РЎвЂЎР В°Р В»Р С‘Р Р†Р С•Р С–Р С• Р Т‘Р С•Р С—РЎС“РЎвЂ°Р ВµР Р…Р С‘РЎРЏ, Р С—Р С•РЎвЂљР С•Р СРЎС“ РЎвЂЎРЎвЂљР С• Р С•Р Р… Р Р†РЎвЂ№Р С–Р В»РЎРЏР Т‘Р С‘РЎвЂљ Р С–Р В°РЎР‚Р В°Р Р…РЎвЂљР С‘Р ВµР в„–.
///
/// Р вЂ™РЎвЂљР С•РЎР‚Р С•Р Вµ РЎС“РЎРѓР В»Р С•Р Р†Р С‘Р Вµ РІР‚вЂќ РЎвЂљР С•, РЎР‚Р В°Р Т‘Р С‘ РЎвЂЎР ВµР С–Р С• РЎРѓР В»Р С•Р в„– РЎРѓРЎС“РЎвЂ°Р ВµРЎРѓРЎвЂљР Р†РЎС“Р ВµРЎвЂљ: РЎРЏР Т‘РЎР‚Р С• Р Р†Р В»Р В°Р Т‘Р ВµР ВµРЎвЂљ Р СР ВµРЎвЂ¦Р В°Р Р…Р С‘Р В·Р СР С•Р С
/// Р С—РЎР‚Р С•Р Р†Р ВµРЎР‚Р С”Р С‘, Р В° Р Р…Р Вµ Р С—Р ВµРЎР‚Р ВµРЎвЂЎР Р…Р ВµР С РЎРѓР ВµР СР ВµР в„–РЎРѓРЎвЂљР Р†. Р вЂ™РЎРѓРЎвЂљРЎР‚Р С•Р ВµР Р…Р Р…РЎвЂ№Р в„– РЎР‚Р ВµР ВµРЎРѓРЎвЂљРЎР‚ Р’В«Р С‘Р СРЎРЏ РІвЂ вЂ™ РЎРѓР ВµРЎР‚РЎвЂљР С‘РЎвЂћР С‘Р С”Р В°РЎвЂљР’В»
/// Р Р†Р ВµРЎР‚Р Р…РЎС“Р В» Р В±РЎвЂ№ Р С‘Р СР ВµР Р…Р С•Р Р†Р В°Р Р…Р Р…РЎвЂ№Р Вµ РЎР‚Р С•Р В»Р С‘ Р Р†Р Р…РЎС“РЎвЂљРЎР‚РЎРЉ РЎРЏР Т‘РЎР‚Р В°, РЎвЂљР С• Р ВµРЎРѓРЎвЂљРЎРЉ РЎР‚Р ВµРЎвЂ Р ВµР С—РЎвЂљРЎвЂ№, Р С•РЎвЂљ Р С”Р С•РЎвЂљР С•РЎР‚РЎвЂ№РЎвЂ¦
/// РЎРѓР С‘РЎРѓРЎвЂљР ВµР СР В° РЎС“РЎв‚¬Р В»Р В° Р С•РЎРѓР С•Р В·Р Р…Р В°Р Р…Р Р…Р С•. Р РЋР ВµР С–Р С•Р Т‘Р Р…РЎРЏ РЎвЂљР В°Р С”Р С•Р С–Р С• Р С—Р ВµРЎР‚Р ВµРЎвЂЎР Р…РЎРЏ Р Р…Р ВµРЎвЂљ: РЎРѓР ВµР СР ВµР в„–РЎРѓРЎвЂљР Р†Р С• РІР‚вЂќ Р Т‘Р В°Р Р…Р Р…РЎвЂ№Р Вµ,
/// Р С—РЎР‚Р С‘РЎвЂ¦Р С•Р Т‘РЎРЏРЎвЂ°Р С‘Р Вµ Р С‘Р В·Р Р†Р Р…Р Вµ.
///
/// `family_definition_binding` РІР‚вЂќ РЎвЂЎР В°РЎРѓРЎвЂљРЎРЉ Р С‘Р СР ВµР Р…Р Р…Р С• РЎРЊРЎвЂљР С•Р С–Р С• Р СР ВµРЎвЂ¦Р В°Р Р…Р С‘Р В·Р СР В°, Р В° Р Р…Р Вµ Р С‘РЎРѓР С”Р В»РЎР‹РЎвЂЎР ВµР Р…Р С‘Р Вµ
/// Р С‘Р В· Р Р…Р ВµР С–Р С•: Р С•Р Р… РЎРѓРЎР‚Р В°Р Р†Р Р…Р С‘Р Р†Р В°Р ВµРЎвЂљ Р В°Р Т‘РЎР‚Р ВµРЎРѓ РЎРѓР С—РЎР‚Р С•РЎв‚¬Р ВµР Р…Р Р…Р С•Р С–Р С• РЎР‚Р ВµР С–Р С‘Р С•Р Р…Р В° РЎРѓ Р В°Р Т‘РЎР‚Р ВµРЎРѓР С•Р С Р Р† Р Т‘Р С•Р Р†Р ВµРЎР‚Р ВµР Р…Р Р…Р С•Р в„–
/// Р В·Р В°Р С—Р С‘РЎРѓР С‘ Р С‘ Р Р…Р Вµ РЎвЂ¦РЎР‚Р В°Р Р…Р С‘РЎвЂљ Р Р…Р С‘ Р С•Р Т‘Р Р…Р С•Р С–Р С• Р С‘Р СР ВµР Р…Р С‘ РЎРѓР ВµР СР ВµР в„–РЎРѓРЎвЂљР Р†Р В°. Р СџР С•РЎРЊРЎвЂљР С•Р СРЎС“ Р С•Р Р… Р В·Р В°Р С”РЎР‚РЎвЂ№РЎвЂљ РЎвЂљР ВµР С Р В¶Р Вµ
/// Р С–Р ВµР в„–РЎвЂљР С•Р С Р С‘ Р С—Р С• РЎвЂљР С•Р в„– Р В¶Р Вµ Р С—РЎР‚Р С‘РЎвЂЎР С‘Р Р…Р Вµ. Р вЂњР ВµР в„–РЎвЂљ Р С•РЎвЂљ Р Р…Р ВµР С–Р С• Р Р…Р Вµ РЎРѓРЎС“Р В¶Р В°Р ВµРЎвЂљРЎРѓРЎРЏ: РЎРѓР В»Р С•Р в„– Р В·Р В°Р С”РЎР‚РЎвЂ№РЎвЂљ РЎвЂ Р ВµР В»Р С‘Р С”Р С•Р С,
/// Р В° Р Р†РЎвЂљР С•РЎР‚Р С•Р Вµ РЎС“РЎРѓР В»Р С•Р Р†Р С‘Р Вµ РЎРѓР Т‘Р Р†Р С‘Р С–Р В°Р ВµРЎвЂљ Р Р…Р Вµ Р С—РЎС“Р В±Р В»Р С‘Р С”Р В°РЎвЂ Р С‘РЎР‹, Р В° Р С—Р С•Р В»Р Р…Р С•РЎвЂљРЎС“ Р СР ВµРЎвЂ¦Р В°Р Р…Р С‘Р В·Р СР В° РІР‚вЂќ Р С—Р ВµРЎР‚Р Р†Р С•Р Вµ
/// РЎС“РЎРѓР В»Р С•Р Р†Р С‘Р Вµ (РЎРЏР Т‘РЎР‚Р С• Р Р…Р Вµ Р В°РЎС“РЎвЂљР ВµР Р…РЎвЂљР С‘РЎвЂћР С‘РЎвЂ Р С‘РЎР‚РЎС“Р ВµРЎвЂљ Р В·Р В°Р С—Р С‘РЎРѓРЎРЉ) Р С‘Р С Р Р…Р Вµ Р В·Р В°РЎвЂљРЎР‚Р В°Р С–Р С‘Р Р†Р В°Р ВµРЎвЂљРЎРѓРЎРЏ Р С‘ Р С•РЎРѓРЎвЂљР В°РЎвЂРЎвЂљРЎРѓРЎРЏ
/// Р Р…Р ВµР Р†РЎвЂ№Р С—Р С•Р В»Р Р…Р ВµР Р…Р Р…РЎвЂ№Р С.
///
/// ```compile_fail
/// use labcolors_core::family_artifact;
/// ```
///
/// ```compile_fail
/// use labcolors_core::family;
/// ```
///
/// ```compile_fail
/// use labcolors_core::contextual_region;
/// ```
///
/// ```compile_fail
/// use labcolors_core::family_artifact::FamilyArtifactLoaderV1;
/// ```
///
/// ```compile_fail
/// use labcolors_core::family::FamilyDeclarationV2;
/// ```
///
/// ```compile_fail
/// use labcolors_core::contextual_region::ContextualRegionFamilyProviderV1;
/// ```
///
/// ```compile_fail
/// use labcolors_core::family_definition_binding;
/// ```
///
/// ```compile_fail
/// use labcolors_core::family_definition_binding::DefinitionBoundFamilyLoaderV1;
/// ```
#[cfg(doctest)]
pub struct NoPrematureFamilyArtifactApi;

/// Р РЋРЎвЂ№РЎР‚РЎвЂ№Р Вµ `f64` Р Р…Р Вµ РЎРЏР Р†Р В»РЎРЏРЎР‹РЎвЂљРЎРѓРЎРЏ Р Р†Р В°Р В»Р С‘Р Т‘Р С‘РЎР‚Р С•Р Р†Р В°Р Р…Р Р…РЎвЂ№Р С РЎвЂ Р Р†Р ВµРЎвЂљР С•Р Р†РЎвЂ№Р С Р В·Р Р…Р В°РЎвЂЎР ВµР Р…Р С‘Р ВµР С: Р С—РЎС“Р В±Р В»Р С‘РЎвЂЎР Р…Р В°РЎРЏ
/// РЎРѓР ВµРЎР‚Р С‘Р В°Р В»Р С‘Р В·Р В°РЎвЂ Р С‘РЎРЏ Р С‘Р Т‘РЎвЂРЎвЂљ РЎвЂЎР ВµРЎР‚Р ВµР В· [`Srgb8::to_hex`], Р С–Р Т‘Р Вµ Р Р…Р ВµР Р†Р В°Р В»Р С‘Р Т‘Р Р…Р С•Р Вµ РЎРѓР С•РЎРѓРЎвЂљР С•РЎРЏР Р…Р С‘Р Вµ РЎС“Р В¶Р Вµ
/// Р Р…Р ВµР С—РЎР‚Р ВµР Т‘РЎРѓРЎвЂљР В°Р Р†Р С‘Р СР С•. Р вЂ™Р Р…РЎС“РЎвЂљРЎР‚Р ВµР Р…Р Р…Р С‘Р Вµ formatter-РЎвЂ№ РЎРѓ generated-finite precondition Р Р…Р Вµ
/// Р С•Р В±РЎР‚Р В°Р В·РЎС“РЎР‹РЎвЂљ public API: Р Р†Р Р…Р ВµРЎв‚¬Р Р…Р С‘Р в„– `NaN` Р Р…Р Вµ Р СР С•Р В¶Р ВµРЎвЂљ Р С—Р С•Р С—Р В°РЎРѓРЎвЂљРЎРЉ Р Р† Р Р…Р С‘РЎвЂ¦ РЎвЂЎР ВµРЎР‚Р ВµР В· РЎРЊРЎвЂљРЎС“ Р С–РЎР‚Р В°Р Р…Р С‘РЎвЂ РЎС“.
///
/// ```compile_fail
/// use labcolors_core::hex_from_srgb_encoded;
/// ```
///
/// ```compile_fail
/// use labcolors_core::hex_from_srgb;
/// ```
///
/// ```
/// use labcolors_core::Srgb8;
/// assert_eq!(Srgb8::new([0x1A, 0x2B, 0x3C]).to_hex(), "#1A2B3C");
/// ```
#[cfg(doctest)]
pub struct NoRawFloatSrgbSerializer;

// Retired recipe oracle: in-crate tests only; never part of production API.
#[cfg(test)]
pub(crate) use semantic::{Role, RoleTable, resolve_set};
