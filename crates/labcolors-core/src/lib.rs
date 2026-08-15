// BEGIN WCAG22_SOURCE_ROUTES_V1
const _: () = (); // First-item proof anchor; moving it fails verify_wcag22_q55.py.
pub mod numerics;
pub(crate) mod srgb8;
pub mod wcag22;
#[doc(hidden)]
pub mod wcag22_evidence;
// END WCAG22_SOURCE_ROUTES_V1

pub(crate) mod clean_set;
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
pub(crate) mod spaces;

pub use srgb8::Srgb8;

pub(crate) mod accent_balance;
pub mod alpha;
pub(crate) mod appearance;
pub mod config;
pub(crate) mod constraints;
pub(crate) mod corridor_representation;
pub mod glow;
pub mod hash;
pub mod ladder;
pub mod lcs;
#[expect(
    dead_code,
    reason = "F0 colour-identity internals are exposed only through typed Program evidence"
)]
pub(crate) mod lcs_occurrence;
pub(crate) mod lpc;
pub mod material;
pub mod neutral;
pub mod numerical_plan;
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
    reason = "the complete Program candidate remains private until terminal C7c"
)]
#[deny(missing_docs)]
pub(crate) mod program;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "generic Program machinery is used only through the staged concrete module"
    )
)]
pub(crate) mod program_session;
pub(crate) mod relation;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "release-registry internals are projected only through typed Program evidence"
    )
)]
pub(crate) mod release_registry;
pub mod scale;
pub mod semantic;
pub(crate) mod sha256;
pub mod solve;

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
pub use semantic::{
    Floor, GlowIndeterminateResolved, NamedRoleTable, ResolveSetError, ResolveSetErrorKind, Resolved,
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
    BgInput, ChromaPolicy, Contract, Gamut, Hue, SolveFailure, SolveFailureBoundary,
    SolveFailureCategory, SolveJob, Solved, solve, solve_many,
};
pub use spaces::oklch::{css_alpha_value, oklch_css_from_hex, oklch_from_hex};
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

/// Кандидат Program остаётся внутренним до завершения terminal C7c: неполную
/// emission/attachment/transaction поверхность нельзя случайно опубликовать.
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
#[cfg(doctest)]
pub struct NoPrematureProgramApi;

/// C8d recheck и F2 observation остаются деталями одной приватной Session;
/// они не могут стать дополнительными public authoring/runtime roots.
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

/// Слой семейств закрыт намеренно, и условие его открытия — контракт, а не
/// готовность кода.
///
/// Загрузчик артефакта полон: он допускает байты только против доверенного
/// сертификата, сверяя его побайтово, и пересчитывает payload-дайджест,
/// receipt, semantic release и канонический образ. Но **аутентификации он не
/// выполняет**. Весь периметр — подлинность записи сертификата, которую
/// предъявляет вызывающий: подписи и якоря доверия в ядре отсутствуют
/// сознательно. Адрес определения — свободное поле сертификата, публично
/// вычислимое из `(releases, pipeline, region)`, поэтому самосогласованный
/// сертификат над произвольным образом минтится кем угодно, и загрузчик
/// примет его вместе с соответствующим артефактом.
///
/// Отсюда условие публикации: поверхность нельзя открывать, пока публичный
/// контракт прямо не скажет, что доверие к записи обеспечивает вызывающий, а
/// не ядро. Экспорт с именем, обещающим проверку, которой нет, — это худший
/// вид молчаливого допущения, потому что он выглядит гарантией.
///
/// Второе условие — то, ради чего слой существует: ядро владеет механизмом
/// проверки, а не перечнем семейств. Встроенный реестр «имя → сертификат»
/// вернул бы именованные роли внутрь ядра, то есть рецепты, от которых
/// система ушла осознанно. Сегодня такого перечня нет: семейство — данные,
/// приходящие извне.
///
/// `family_definition_binding` — часть именно этого механизма, а не исключение
/// из него: он сравнивает адрес спрошенного региона с адресом в доверенной
/// записи и не хранит ни одного имени семейства. Поэтому он закрыт тем же
/// гейтом и по той же причине. Гейт от него не сужается: слой закрыт целиком,
/// а второе условие сдвигает не публикацию, а полноту механизма — первое
/// условие (ядро не аутентифицирует запись) им не затрагивается и остаётся
/// невыполненным.
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
