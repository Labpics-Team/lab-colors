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
pub(crate) mod evaluator_registry;
mod family;
mod family_artifact;
mod family_definition_binding;
pub(crate) mod field_effect;
#[cfg(test)]
mod field_effect_tests;
/// NUMERIC-01: independent numeric reference bounds with declared
/// applicability, error bounds and counterexamples.
#[allow(
    dead_code,
    reason = "NUMERIC-01 bounds staged before LOWER-01 consumer; validated by bound tests"
)]
pub(crate) mod numerics_bounds;
pub(crate) mod spaces;

pub(crate) mod cleanliness;

pub use srgb8::Srgb8;

pub mod alpha;
pub(crate) mod appearance;
pub mod authority;
pub mod certificate;
pub mod claims_manifest;
pub(crate) mod constraints;
#[cfg(feature = "ext09-extractor")]
pub mod exports_manifest;
pub mod hash;
pub mod lcs;
#[expect(
    dead_code,
    reason = "F0 colour-identity internals are exposed only through typed Program evidence"
)]
pub(crate) mod lcs_occurrence;
pub(crate) mod lpc;
pub mod neutral;
#[expect(
    dead_code,
    reason = "the output-profile firewall is intentionally internal to registered profiles"
)]
pub(crate) mod output_projection;
// Независимая численная проверка границы инверсии alpha; не ветвь runtime.
#[cfg(test)]
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
pub(crate) mod restorative_auto;
pub mod scale;
pub(crate) mod sha256;
pub mod solve;
pub mod source_manifest;

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

pub mod compiled_dependency_plan;

#[cfg(feature = "ext09-extractor")]
#[expect(
    dead_code,
    reason = "CI build manifest extraction is staged before its offline proof consumer"
)]
pub(crate) mod ci_build_manifest;

#[cfg(test)]
mod compiled_dependency_plan_tests;

#[cfg(test)]
mod wcag22_tests;

#[cfg(test)]
mod lcs_hue_dimensionality_tests;

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
#[allow(deprecated)]
// LcsColor re-export retained for solver-curve compatibility during LCS freeze transition
pub use lcs::LcsColor;
pub use numerics::{
    NUMERICAL_CAPABILITY_SCHEMA_VERSION_V2, NumericalArtifactIdV2, NumericalBoundStatusV2,
    NumericalCapabilityChecksumV2, NumericalCapabilityManifestV2,
    NumericalCompatibilityReleaseIdV1, NumericalDecisionEvidenceV1, NumericalErrorBoundIdV2,
    NumericalEvidenceClassV2, NumericalFallbackStatusV1, NumericalProofIdV2,
    NumericalRegistryCoverageV2, NumericalRuntimeAttestationIdV2, NumericalSiteCapabilityV2,
    NumericalSiteIdV2, NumericalSiteRecordV2, StableNumericalOutcomeV2,
    numerical_capability_manifest_v2, numerical_registry_v2,
};
pub use recheck::{
    measure_contrast, recheck_against, recheck_against_multi, recheck_against_multi_u32,
    recheck_against_u32,
};
pub use solve::{
    BgInput, ChromaPolicy, Contract, Gamut, Hue, SolveFailure, SolveFailureBoundary,
    SolveFailureCategory, SolveJob, Solved, solve, solve_many,
};
pub use spaces::oklch::{css_alpha_value, oklch_css_from_hex, oklch_from_hex};
pub use spaces::srgb::srgb_encoded_from_hex;
pub use spaces::vc::ViewingConditions;
pub use wcag22_evidence::CanonicalFiniteBoundedEvidenceV1;

/// Исполняемые примеры публичного README.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

/// Ролевые рецепты не доступны через публичный API.
/// ```compile_fail
/// use labcolors_core::accent_balance::accent_balanced;
/// ```
///
/// ```compile_fail
/// use labcolors_core::accent_surface::derive_accent_surface_ramp;
/// ```
///
#[cfg(doctest)]
pub struct InternalAccentRecipes;

/// Старый результат роли не может выдавать вердикт о видимости оттенка.
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
///
#[cfg(doctest)]
pub struct NoHueVisibilityVerdict;

/// Удалённые измерительные псевдонимы и флаги не возвращаются как API.
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
///
#[cfg(doctest)]
pub struct NoCompatibilityAliases;

/// Гибридная surface-метрика не является публичной LPC-способностью.
/// ```compile_fail
/// use labcolors_core::lpc::lpc_surface;
/// ```
///
/// ```compile_fail
/// use labcolors_core::lpc::lpc_surface_with_vc;
/// ```
///
#[cfg(doctest)]
pub struct NoHybridLpcSurfaceMetric;

/// Человеческая читаемость не представляется неподтверждённым скалярным API.
/// ```compile_fail
/// use labcolors_core::lpc::lpc;
/// ```
///
#[cfg(doctest)]
pub struct NoPrematureScalarLpcApi;

/// Внутренние типы Program не создают второго публичного входа рядом с ProgramWire.
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
/// ```compile_fail
/// use labcolors_core::program::wire::decode_program_wire_v1;
/// ```
///
#[cfg(doctest)]
pub struct NoPrematureProgramApi;

/// Владельцы сессии, наблюдений и внутренних доказательств остаются закрытыми.
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
///
#[cfg(doctest)]
pub struct NoPrematurePointSupportApi;

/// Внутренние загрузчики и семейства не обходят допуск канонической границы.
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
///
#[cfg(doctest)]
pub struct NoPrematureFamilyArtifactApi;

/// Сериализация внешнего цвета принимает проверенный Srgb8, не произвольный float.
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
///
#[cfg(doctest)]
pub struct NoRawFloatSrgbSerializer;
