// BEGIN WCAG22_SOURCE_ROUTES_V1
const _: () = (); // First-item proof anchor; moving it fails verify_wcag22_q55.py.
pub mod numerics;
pub(crate) mod srgb8;
pub mod wcag22;
#[doc(hidden)]
pub mod wcag22_evidence;
// END WCAG22_SOURCE_ROUTES_V1

pub(crate) mod composition;
pub(crate) mod spaces;

pub use appearance::SourceOverCertificateV1;
pub use composition::CompositionProfileV1;
pub use srgb8::Srgb8;

pub(crate) mod accent_balance;
pub mod alpha;
pub(crate) mod analog;
pub(crate) mod appearance;
pub mod config;
pub(crate) mod constraints;
pub mod glow;
pub mod hash;
pub mod ladder;
pub mod lcs;
pub(crate) mod lpc;
pub mod material;
pub mod neutral;
pub mod numerical_plan;
pub(crate) mod pair;
pub mod scale;
pub mod semantic;
pub mod solve;
pub(crate) mod wcag;

pub mod curve;

#[cfg(test)]
pub(crate) mod exposure_support;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod golden_tests;

#[cfg(test)]
mod agnostic_gates;

#[cfg(test)]
mod appearance_graph_tests;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "private F2 raw admission is production-compiled before its package bridge exists"
    )
)]
pub(crate) mod observation;

#[cfg(test)]
mod observation_tests;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "private F2 exact recheck is production-compiled before its package bridge exists"
    )
)]
pub(crate) mod recheck;

#[cfg(test)]
mod recheck_tests;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "private F2 Session is production-compiled before C8c package integration exists"
    )
)]
pub(crate) mod session;

#[cfg(test)]
mod session_tests;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "private V2a joint selection is production-compiled before Pair lowering or a public Program exists"
    )
)]
pub(crate) mod joint;

#[cfg(test)]
mod joint_tests;

#[cfg(test)]
mod constraint_tests;

#[cfg(test)]
mod wcag22_tests;

#[cfg(test)]
mod one_levelness_tests;

#[cfg(test)]
mod lcs_hue_dimensionality_tests;

// Built-in-showcase behaviour tests, relocated in-crate (ADR-0001): the
// built-in `Role`/`RoleTable`/`resolve_set` cluster is now `#[cfg(test)]`-only,
// so these tests — which exercise it as the byte-identity oracle — must live
// inside the crate to see it (integration tests only see the public API).
#[cfg(test)]
mod continuity_tests;

#[cfg(test)]
mod dim_tinted_tests;

#[cfg(test)]
mod pair_label_tests;

#[cfg(test)]
mod r3_byte_identity_tests;

// Reference checks for the deepest colour-science layers (sRGB EOTF & matrices,
// Ottosson Oklab, CAT16/CIECAM16 adapt, Hellwig-2022 H-K, WCAG linearise). These
// reach `pub(crate)` transforms an integration test in `tests/` cannot see; the
// public-API-reachable checks live in `tests/reference_vectors.rs`, with source
// and oracle scope beside each test.
#[cfg(test)]
mod reference_vectors_deep;

// AccentCurve golden snapshots are in-crate because their built-in showcase
// anchors are `#[cfg(test)]`-only.
#[cfg(test)]
mod accent_golden_tests;

pub use alpha::composite_over_encoded;
pub use config::{
    Brand, ConfigError, LadderSource, NeutralAnchors, NeutralConfig, NeutralPick, NeutralTint,
    PaletteFamily, RoleRecipe, ThemeConfig, ThemesConfig, VcPreset,
};
pub use curve::{ColorCurve, CurvePosition, CurvePositionError};
pub use glow::{
    GLOW_BASE_DJ, GLOW_BLOOM_DJ, GLOW_COMPOSITE_PROFILE, GLOW_DIAGNOSTIC_PROFILE,
    GLOW_LAYER_RECIPE_PROFILE, GLOW_SUBTLE_DJ, GlowCompositeCertificateV1,
    GlowCompositeGuaranteeV1, GlowCompositeProfileV1, GlowConstraintLayer, GlowDecisionOutcomeV1,
    GlowDecisionProfileV1, GlowDiagnosticProfileV1, GlowLayerRecipeProfileV1, GlowSolve,
    GlowTargetStatus, glow_layers_from_source, screen_layer_over_encoded, screen_layer_over_srgb8,
    screen_point_is_exact_noop, solve_screen_alpha_for_dj,
};
pub use hash::fnv1a_32;
pub use ladder::{LadderPosition, LadderTint, ThemeAnchors};
pub use lcs::LcsColor;
pub use material::{
    BackdropBoundV1, BackdropBox, BackdropBoxErrorV1, EncodedRgbErrorV1, MaterialAlpha,
    MaterialAlphaGuaranteeV1, MaterialAlphaStatusV1, MaterialNumericalProfileV1,
    MaterialSolveErrorV1, Pole, RgbChannelV1, committed_pole_encoded, solve_material_alpha_encoded,
    solve_material_alpha_hex, worst_contrast_encoded,
};
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
    PointSupportAdmissionErrorV1, PointSupportCellV1, PointSupportCriterionAssessmentV1,
    PointSupportCriterionRequirementV1, PointSupportDropFractionV1, PointSupportEvaluationErrorV1,
    PointSupportOccurrenceV1, PointSupportReportV1, PointSupportStabilityAssessmentV1,
    PointSupportStabilityEvidenceV1, PointSupportStabilityPolicyV1, PointSupportStabilityProfileV1,
    PointSupportStatusV1, PointSupportWcag22AssessmentV1, evaluate_point_support_v1,
};
pub use semantic::{
    GlowIndeterminateResolved, NamedRoleTable, ResolveSetError, ResolveSetErrorKind, Resolved,
    RoleChroma, RoleFailure, RoleFailureCategory, RoleSpec, TextAnchor, TranslucentResolved,
    measure_contrast, recheck_against, recheck_against_multi, recheck_against_multi_u32,
    recheck_against_u32, resolve_named_set,
};
pub use wcag22_evidence::CanonicalFiniteBoundedEvidenceV1;
// The built-in v1 showcase (`Role`/`RoleTable`/`resolve`/`resolve_set`) is no
// longer part of the production API (ADR-0001): the agnostic engine ships
// only the string-keyed `resolve_named_set` path. It survives ONLY as the
// `#[cfg(test)]` byte-identity oracle for the named path, re-exported crate-wide
// so the in-crate showcase tests keep their `crate::…` spellings.
#[cfg(test)]
pub(crate) use semantic::{Role, RoleTable, resolve_set};
pub use solve::{
    BgInput, ChromaPolicy, Contract, Floor, Gamut, Hue, SolveFailure, SolveFailureBoundary,
    SolveFailureCategory, SolveJob, Solved, solve, solve_many,
};
pub use spaces::oklch::{css_alpha_value, oklch_css_from_hex, oklch_from_hex};
pub use spaces::p3::{p3_css_from_hex, p3_from_hex};
pub use spaces::srgb::srgb_encoded_from_hex;
pub use spaces::vc::ViewingConditions;

/// Компилирует rust-блоки package-local README как doctest-ы: опубликованный
/// crate обязан нести и исполнять собственную документацию без файлов выше
/// package root. Тип существует только под `--test`, в бинарь не входит.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

/// Закрытая физическая топология и recipe-варианты — детали resolver-а, а не
/// extension points. Публичный API не раскрывает client-authored topology.
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

/// Точные выходные байты не доказывают перцептивную видимость оттенка.
/// Удалённый вердикт нельзя восстанавливать из какой-либо формы результата.
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

/// Устаревшие compatibility-алиасы не входят в breaking-релиз до клиентов.
/// Единственный SSOT — типизированные статусы и явно названные измерения.
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

/// Метрика поверхности не смешивает координаты несовместимых appearance-
/// пространств и не публикует результат как LPC. Будущему примитиву расстояния
/// нужны собственные допущенные модель, имя, домен и независимый oracle.
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

/// Неполные скалярные функции не образуют публичный контракт LPC. Поле `lc`
/// переходного resolver — только зафиксированная candidate-координата. LPC
/// становится публичным утверждением лишь через версионированный реестр
/// evaluators с идентичностями стимула, контекста, применимости и evidence.
///
/// ```compile_fail
/// use labcolors_core::lpc::lpc;
/// ```
#[cfg(doctest)]
pub struct NoPrematureScalarLpcApi;

/// Frozen Pair frontend не публикует собственную физику или solver extension
/// point; Pair lowering остаётся private до общего Program cutover.
///
/// ```compile_fail
/// use labcolors_core::pair::pair_side;
/// ```
#[cfg(doctest)]
pub struct NoPublicPairRecipeApi;

/// Сырые `f64` не являются валидированным цветовым значением: публичная
/// сериализация идёт через [`Srgb8::to_hex`], где невалидное состояние уже
/// непредставимо. Внутренние formatter-ы с generated-finite precondition не
/// образуют public API: внешний `NaN` не может попасть в них через эту границу.
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
