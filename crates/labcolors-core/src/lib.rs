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
pub(crate) mod field_technical_quality;
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
// so these tests вЂ” which exercise it as the byte-identity oracle вЂ” must live
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
// so the in-crate showcase tests keep their `crate::вЂ¦` spellings.
pub use solve::{
    BgInput, ChromaPolicy, Contract, Gamut, Hue, SolveFailure, SolveFailureBoundary,
    SolveFailureCategory, SolveJob, Solved, solve, solve_many,
};
pub use spaces::oklch::{css_alpha_value, oklch_css_from_hex, oklch_from_hex};
pub use spaces::srgb::srgb_encoded_from_hex;
pub use spaces::vc::ViewingConditions;

/// РљРѕРјРїРёР»РёСЂСѓРµС‚ rust-Р±Р»РѕРєРё package-local README РєР°Рє doctest-С‹: РѕРїСѓР±Р»РёРєРѕРІР°РЅРЅС‹Р№
/// crate РѕР±СЏР·Р°РЅ РЅРµСЃС‚Рё Рё РёСЃРїРѕР»РЅСЏС‚СЊ СЃРѕР±СЃС‚РІРµРЅРЅСѓСЋ РґРѕРєСѓРјРµРЅС‚Р°С†РёСЋ Р±РµР· С„Р°Р№Р»РѕРІ РІС‹С€Рµ
/// package root. РўРёРї СЃСѓС‰РµСЃС‚РІСѓРµС‚ С‚РѕР»СЊРєРѕ РїРѕРґ `--test`, РІ Р±РёРЅР°СЂСЊ РЅРµ РІС…РѕРґРёС‚.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

/// Р—Р°РєСЂС‹С‚Р°СЏ С„РёР·РёС‡РµСЃРєР°СЏ С‚РѕРїРѕР»РѕРіРёСЏ Рё recipe-РІР°СЂРёР°РЅС‚С‹ вЂ” РґРµС‚Р°Р»Рё resolver-Р°, Р° РЅРµ
/// extension points. РџСѓР±Р»РёС‡РЅС‹Р№ API РЅРµ СЂР°СЃРєСЂС‹РІР°РµС‚ client-authored topology.
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

/// РўРѕС‡РЅС‹Рµ РІС‹С…РѕРґРЅС‹Рµ Р±Р°Р№С‚С‹ РЅРµ РґРѕРєР°Р·С‹РІР°СЋС‚ РїРµСЂС†РµРїС‚РёРІРЅСѓСЋ РІРёРґРёРјРѕСЃС‚СЊ РѕС‚С‚РµРЅРєР°.
/// РЈРґР°Р»С‘РЅРЅС‹Р№ РІРµСЂРґРёРєС‚ РЅРµР»СЊР·СЏ РІРѕСЃСЃС‚Р°РЅР°РІР»РёРІР°С‚СЊ РёР· РєР°РєРѕР№-Р»РёР±Рѕ С„РѕСЂРјС‹ СЂРµР·СѓР»СЊС‚Р°С‚Р°.
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

/// РЈСЃС‚Р°СЂРµРІС€РёРµ compatibility-Р°Р»РёР°СЃС‹ РЅРµ РІС…РѕРґСЏС‚ РІ breaking-СЂРµР»РёР· РґРѕ РєР»РёРµРЅС‚РѕРІ.
/// Р•РґРёРЅСЃС‚РІРµРЅРЅС‹Р№ SSOT вЂ” С‚РёРїРёР·РёСЂРѕРІР°РЅРЅС‹Рµ СЃС‚Р°С‚СѓСЃС‹ Рё СЏРІРЅРѕ РЅР°Р·РІР°РЅРЅС‹Рµ РёР·РјРµСЂРµРЅРёСЏ.
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

/// РњРµС‚СЂРёРєР° РїРѕРІРµСЂС…РЅРѕСЃС‚Рё РЅРµ СЃРјРµС€РёРІР°РµС‚ РєРѕРѕСЂРґРёРЅР°С‚С‹ РЅРµСЃРѕРІРјРµСЃС‚РёРјС‹С… appearance-
/// РїСЂРѕСЃС‚СЂР°РЅСЃС‚РІ Рё РЅРµ РїСѓР±Р»РёРєСѓРµС‚ СЂРµР·СѓР»СЊС‚Р°С‚ РєР°Рє LPC. Р‘СѓРґСѓС‰РµРјСѓ РїСЂРёРјРёС‚РёРІСѓ СЂР°СЃСЃС‚РѕСЏРЅРёСЏ
/// РЅСѓР¶РЅС‹ СЃРѕР±СЃС‚РІРµРЅРЅС‹Рµ РґРѕРїСѓС‰РµРЅРЅС‹Рµ РјРѕРґРµР»СЊ, РёРјСЏ, РґРѕРјРµРЅ Рё РЅРµР·Р°РІРёСЃРёРјС‹Р№ oracle.
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

/// РќРµРїРѕР»РЅС‹Рµ СЃРєР°Р»СЏСЂРЅС‹Рµ С„СѓРЅРєС†РёРё РЅРµ РѕР±СЂР°Р·СѓСЋС‚ РїСѓР±Р»РёС‡РЅС‹Р№ РєРѕРЅС‚СЂР°РєС‚ LPC. РџРѕР»Рµ `lc`
/// РїРµСЂРµС…РѕРґРЅРѕРіРѕ resolver вЂ” С‚РѕР»СЊРєРѕ Р·Р°С„РёРєСЃРёСЂРѕРІР°РЅРЅР°СЏ candidate-РєРѕРѕСЂРґРёРЅР°С‚Р°. LPC
/// СЃС‚Р°РЅРѕРІРёС‚СЃСЏ РїСѓР±Р»РёС‡РЅС‹Рј СѓС‚РІРµСЂР¶РґРµРЅРёРµРј Р»РёС€СЊ С‡РµСЂРµР· РІРµСЂСЃРёРѕРЅРёСЂРѕРІР°РЅРЅС‹Р№ СЂРµРµСЃС‚СЂ
/// evaluators СЃ РёРґРµРЅС‚РёС‡РЅРѕСЃС‚СЏРјРё СЃС‚РёРјСѓР»Р°, РєРѕРЅС‚РµРєСЃС‚Р°, РїСЂРёРјРµРЅРёРјРѕСЃС‚Рё Рё evidence.
///
/// ```compile_fail
/// use labcolors_core::lpc::lpc;
/// ```
#[cfg(doctest)]
pub struct NoPrematureScalarLpcApi;

/// РљР°РЅРґРёРґР°С‚ Program РѕСЃС‚Р°С‘С‚СЃСЏ РІРЅСѓС‚СЂРµРЅРЅРёРј РґРѕ Р·Р°РІРµСЂС€РµРЅРёСЏ terminal C7c: РЅРµРїРѕР»РЅСѓСЋ
/// emission/attachment/transaction РїРѕРІРµСЂС…РЅРѕСЃС‚СЊ РЅРµР»СЊР·СЏ СЃР»СѓС‡Р°Р№РЅРѕ РѕРїСѓР±Р»РёРєРѕРІР°С‚СЊ.
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
/// Р•РґРёРЅСЃС‚РІРµРЅРЅС‹Р№ РґРѕРїСѓС‰РµРЅРЅС‹Р№ РґРѕ C7c seam вЂ” РїСЂРѕРІРµСЂРєР° РєР°РЅРѕРЅРёС‡РµСЃРєРёС… wire-Р±Р°Р№С‚РѕРІ
/// (`program_wire`): РѕРЅР° РЅРµ РІС‹РґР°С‘С‚ runtime-authority. Р’РЅСѓС‚СЂРµРЅРЅРѕСЃС‚Рё wire-РјРѕРґСѓР»СЏ
/// РѕСЃС‚Р°СЋС‚СЃСЏ Р·Р°РєСЂС‹С‚С‹РјРё:
///
/// ```compile_fail
/// use labcolors_core::program::wire::decode_program_wire_v1;
/// ```
#[cfg(doctest)]
pub struct NoPrematureProgramApi;

/// C8d recheck Рё F2 observation РѕСЃС‚Р°СЋС‚СЃСЏ РґРµС‚Р°Р»СЏРјРё РѕРґРЅРѕР№ РїСЂРёРІР°С‚РЅРѕР№ Session;
/// РѕРЅРё РЅРµ РјРѕРіСѓС‚ СЃС‚Р°С‚СЊ РґРѕРїРѕР»РЅРёС‚РµР»СЊРЅС‹РјРё public authoring/runtime roots.
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

/// РЎР»РѕР№ СЃРµРјРµР№СЃС‚РІ Р·Р°РєСЂС‹С‚ РЅР°РјРµСЂРµРЅРЅРѕ, Рё СѓСЃР»РѕРІРёРµ РµРіРѕ РѕС‚РєСЂС‹С‚РёСЏ вЂ” РєРѕРЅС‚СЂР°РєС‚, Р° РЅРµ
/// РіРѕС‚РѕРІРЅРѕСЃС‚СЊ РєРѕРґР°.
///
/// Р—Р°РіСЂСѓР·С‡РёРє Р°СЂС‚РµС„Р°РєС‚Р° РїРѕР»РѕРЅ: РѕРЅ РґРѕРїСѓСЃРєР°РµС‚ Р±Р°Р№С‚С‹ С‚РѕР»СЊРєРѕ РїСЂРѕС‚РёРІ РґРѕРІРµСЂРµРЅРЅРѕРіРѕ
/// СЃРµСЂС‚РёС„РёРєР°С‚Р°, СЃРІРµСЂСЏСЏ РµРіРѕ РїРѕР±Р°Р№С‚РѕРІРѕ, Рё РїРµСЂРµСЃС‡РёС‚С‹РІР°РµС‚ payload-РґР°Р№РґР¶РµСЃС‚,
/// receipt, semantic release Рё РєР°РЅРѕРЅРёС‡РµСЃРєРёР№ РѕР±СЂР°Р·. РќРѕ **Р°СѓС‚РµРЅС‚РёС„РёРєР°С†РёРё РѕРЅ РЅРµ
/// РІС‹РїРѕР»РЅСЏРµС‚**. Р’РµСЃСЊ РїРµСЂРёРјРµС‚СЂ вЂ” РїРѕРґР»РёРЅРЅРѕСЃС‚СЊ Р·Р°РїРёСЃРё СЃРµСЂС‚РёС„РёРєР°С‚Р°, РєРѕС‚РѕСЂСѓСЋ
/// РїСЂРµРґСЉСЏРІР»СЏРµС‚ РІС‹Р·С‹РІР°СЋС‰РёР№: РїРѕРґРїРёСЃРё Рё СЏРєРѕСЂСЏ РґРѕРІРµСЂРёСЏ РІ СЏРґСЂРµ РѕС‚СЃСѓС‚СЃС‚РІСѓСЋС‚
/// СЃРѕР·РЅР°С‚РµР»СЊРЅРѕ. РђРґСЂРµСЃ РѕРїСЂРµРґРµР»РµРЅРёСЏ вЂ” СЃРІРѕР±РѕРґРЅРѕРµ РїРѕР»Рµ СЃРµСЂС‚РёС„РёРєР°С‚Р°, РїСѓР±Р»РёС‡РЅРѕ
/// РІС‹С‡РёСЃР»РёРјРѕРµ РёР· `(releases, pipeline, region)`, РїРѕСЌС‚РѕРјСѓ СЃР°РјРѕСЃРѕРіР»Р°СЃРѕРІР°РЅРЅС‹Р№
/// СЃРµСЂС‚РёС„РёРєР°С‚ РЅР°Рґ РїСЂРѕРёР·РІРѕР»СЊРЅС‹Рј РѕР±СЂР°Р·РѕРј РјРёРЅС‚РёС‚СЃСЏ РєРµРј СѓРіРѕРґРЅРѕ, Рё Р·Р°РіСЂСѓР·С‡РёРє
/// РїСЂРёРјРµС‚ РµРіРѕ РІРјРµСЃС‚Рµ СЃ СЃРѕРѕС‚РІРµС‚СЃС‚РІСѓСЋС‰РёРј Р°СЂС‚РµС„Р°РєС‚РѕРј.
///
/// РћС‚СЃСЋРґР° СѓСЃР»РѕРІРёРµ РїСѓР±Р»РёРєР°С†РёРё: РїРѕРІРµСЂС…РЅРѕСЃС‚СЊ РЅРµР»СЊР·СЏ РѕС‚РєСЂС‹РІР°С‚СЊ, РїРѕРєР° РїСѓР±Р»РёС‡РЅС‹Р№
/// РєРѕРЅС‚СЂР°РєС‚ РїСЂСЏРјРѕ РЅРµ СЃРєР°Р¶РµС‚, С‡С‚Рѕ РґРѕРІРµСЂРёРµ Рє Р·Р°РїРёСЃРё РѕР±РµСЃРїРµС‡РёРІР°РµС‚ РІС‹Р·С‹РІР°СЋС‰РёР№, Р°
/// РЅРµ СЏРґСЂРѕ. Р­РєСЃРїРѕСЂС‚ СЃ РёРјРµРЅРµРј, РѕР±РµС‰Р°СЋС‰РёРј РїСЂРѕРІРµСЂРєСѓ, РєРѕС‚РѕСЂРѕР№ РЅРµС‚, вЂ” СЌС‚Рѕ С…СѓРґС€РёР№
/// РІРёРґ РјРѕР»С‡Р°Р»РёРІРѕРіРѕ РґРѕРїСѓС‰РµРЅРёСЏ, РїРѕС‚РѕРјСѓ С‡С‚Рѕ РѕРЅ РІС‹РіР»СЏРґРёС‚ РіР°СЂР°РЅС‚РёРµР№.
///
/// Р’С‚РѕСЂРѕРµ СѓСЃР»РѕРІРёРµ вЂ” С‚Рѕ, СЂР°РґРё С‡РµРіРѕ СЃР»РѕР№ СЃСѓС‰РµСЃС‚РІСѓРµС‚: СЏРґСЂРѕ РІР»Р°РґРµРµС‚ РјРµС…Р°РЅРёР·РјРѕРј
/// РїСЂРѕРІРµСЂРєРё, Р° РЅРµ РїРµСЂРµС‡РЅРµРј СЃРµРјРµР№СЃС‚РІ. Р’СЃС‚СЂРѕРµРЅРЅС‹Р№ СЂРµРµСЃС‚СЂ В«РёРјСЏ в†’ СЃРµСЂС‚РёС„РёРєР°С‚В»
/// РІРµСЂРЅСѓР» Р±С‹ РёРјРµРЅРѕРІР°РЅРЅС‹Рµ СЂРѕР»Рё РІРЅСѓС‚СЂСЊ СЏРґСЂР°, С‚Рѕ РµСЃС‚СЊ СЂРµС†РµРїС‚С‹, РѕС‚ РєРѕС‚РѕСЂС‹С…
/// СЃРёСЃС‚РµРјР° СѓС€Р»Р° РѕСЃРѕР·РЅР°РЅРЅРѕ. РЎРµРіРѕРґРЅСЏ С‚Р°РєРѕРіРѕ РїРµСЂРµС‡РЅСЏ РЅРµС‚: СЃРµРјРµР№СЃС‚РІРѕ вЂ” РґР°РЅРЅС‹Рµ,
/// РїСЂРёС…РѕРґСЏС‰РёРµ РёР·РІРЅРµ.
///
/// `family_definition_binding` вЂ” С‡Р°СЃС‚СЊ РёРјРµРЅРЅРѕ СЌС‚РѕРіРѕ РјРµС…Р°РЅРёР·РјР°, Р° РЅРµ РёСЃРєР»СЋС‡РµРЅРёРµ
/// РёР· РЅРµРіРѕ: РѕРЅ СЃСЂР°РІРЅРёРІР°РµС‚ Р°РґСЂРµСЃ СЃРїСЂРѕС€РµРЅРЅРѕРіРѕ СЂРµРіРёРѕРЅР° СЃ Р°РґСЂРµСЃРѕРј РІ РґРѕРІРµСЂРµРЅРЅРѕР№
/// Р·Р°РїРёСЃРё Рё РЅРµ С…СЂР°РЅРёС‚ РЅРё РѕРґРЅРѕРіРѕ РёРјРµРЅРё СЃРµРјРµР№СЃС‚РІР°. РџРѕСЌС‚РѕРјСѓ РѕРЅ Р·Р°РєСЂС‹С‚ С‚РµРј Р¶Рµ
/// РіРµР№С‚РѕРј Рё РїРѕ С‚РѕР№ Р¶Рµ РїСЂРёС‡РёРЅРµ. Р“РµР№С‚ РѕС‚ РЅРµРіРѕ РЅРµ СЃСѓР¶Р°РµС‚СЃСЏ: СЃР»РѕР№ Р·Р°РєСЂС‹С‚ С†РµР»РёРєРѕРј,
/// Р° РІС‚РѕСЂРѕРµ СѓСЃР»РѕРІРёРµ СЃРґРІРёРіР°РµС‚ РЅРµ РїСѓР±Р»РёРєР°С†РёСЋ, Р° РїРѕР»РЅРѕС‚Сѓ РјРµС…Р°РЅРёР·РјР° вЂ” РїРµСЂРІРѕРµ
/// СѓСЃР»РѕРІРёРµ (СЏРґСЂРѕ РЅРµ Р°СѓС‚РµРЅС‚РёС„РёС†РёСЂСѓРµС‚ Р·Р°РїРёСЃСЊ) РёРј РЅРµ Р·Р°С‚СЂР°РіРёРІР°РµС‚СЃСЏ Рё РѕСЃС‚Р°С‘С‚СЃСЏ
/// РЅРµРІС‹РїРѕР»РЅРµРЅРЅС‹Рј.
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

/// РЎС‹СЂС‹Рµ `f64` РЅРµ СЏРІР»СЏСЋС‚СЃСЏ РІР°Р»РёРґРёСЂРѕРІР°РЅРЅС‹Рј С†РІРµС‚РѕРІС‹Рј Р·РЅР°С‡РµРЅРёРµРј: РїСѓР±Р»РёС‡РЅР°СЏ
/// СЃРµСЂРёР°Р»РёР·Р°С†РёСЏ РёРґС‘С‚ С‡РµСЂРµР· [`Srgb8::to_hex`], РіРґРµ РЅРµРІР°Р»РёРґРЅРѕРµ СЃРѕСЃС‚РѕСЏРЅРёРµ СѓР¶Рµ
/// РЅРµРїСЂРµРґСЃС‚Р°РІРёРјРѕ. Р’РЅСѓС‚СЂРµРЅРЅРёРµ formatter-С‹ СЃ generated-finite precondition РЅРµ
/// РѕР±СЂР°Р·СѓСЋС‚ public API: РІРЅРµС€РЅРёР№ `NaN` РЅРµ РјРѕР¶РµС‚ РїРѕРїР°СЃС‚СЊ РІ РЅРёС… С‡РµСЂРµР· СЌС‚Сѓ РіСЂР°РЅРёС†Сѓ.
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
