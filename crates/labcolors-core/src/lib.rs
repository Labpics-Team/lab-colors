pub(crate) mod spaces;

pub(crate) mod accent;
pub mod accent_balance;
pub mod accent_surface;
pub mod alpha;
pub mod cleanliness;
pub mod color_quality;
pub mod config;
pub mod glow;
pub mod hash;
pub mod ladder;
pub mod lcs;
pub mod lpc;
pub mod material;
pub mod neutral;
pub mod pair;
pub mod scale;
pub mod semantic;
pub mod sentiment;
pub mod solve;
pub(crate) mod wcag;

pub mod curve;

#[cfg(test)]
pub(crate) mod exposure_support;

#[cfg(test)]
mod golden_tests;

#[cfg(test)]
mod agnostic_gates;

#[cfg(test)]
mod one_levelness_tests;

#[cfg(test)]
mod lcs_hue_dimensionality_tests;

// Built-in-showcase behaviour tests, relocated in-crate (ADR-0001 PR-c): the
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

// External published reference vectors for the deepest colour-science layers
// (sRGB EOTF & matrices, Ottosson Oklab, CAT16/CIECAM16 adapt, Hellwig-2022 H-K,
// WCAG linearise). These reach `pub(crate)` transforms an integration test in
// `tests/` cannot see; the public-API-reachable vectors live in
// `tests/reference_vectors.rs`. See `docs/verification-map.md`.
#[cfg(test)]
mod reference_vectors_deep;

// AccentCurve/SentimentCurve golden snapshots, relocated in-crate (ADR-0001
// PR-c): the `Sentiment` enum is now `#[cfg(test)]`-only, and the golden uses
// the crate-private `SentimentCurve::from_sentiment` helper, so this test must
// live inside the crate to see them.
#[cfg(test)]
mod accent_golden_tests;

pub use accent_surface::{
    AccentSurface, SurfaceMaterial, derive_accent_surface_ramp, render_surface,
};
pub use alpha::composite_over_encoded;
pub use cleanliness::{
    DefectContext, Theme, drab, drab_in_context, muddiness_from_hex, muddiness_from_linear_srgb,
    muddiness_in_context, muddiness_oklch, n_pure,
};
pub use color_quality::{
    AppearanceContextV1, AppearanceMode, AppearanceProfileV1, COLOR_QUALITY_AUDIT_MODEL_V1,
    ColorQualityAuditV1, ContextualSrgbGamutAnalysisV1, DisplayConditionsV1, MUTEDNESS_MODEL_V2,
    ModelProvenance, MutednessComparisonV2, MutednessRelationV2, SpatialContextV1,
    SrgbGamutGeometryV1, WARM_DARK_INTERACTION_MODEL_V2, WarmDarkInteractionReportV2,
    WarmDarkInteractionStatusV2, WarmDarkZeroReasonV2, analyze_srgb_gamut_hex_in_context_v1,
    analyze_srgb_gamut_hex_v1, analyze_srgb_gamut_linear_v1, analyze_srgb_gamut_oklch_v1,
    analyze_warm_dark_interaction_v2, audit_color_quality_v1, compare_mutedness_v2,
};
pub use config::{
    Brand, ConfigError, LadderSource, NeutralAnchors, NeutralConfig, NeutralPick, NeutralTint,
    PaletteFamily, RoleRecipe, SentimentCategory, SentimentsConfig, ThemeConfig, ThemesConfig,
    VcPreset,
};
pub use curve::ColorCurve;
pub use glow::{
    GLOW_BASE_DJ, GLOW_BLOOM_DJ, GLOW_SUBTLE_DJ, GlowLayers, GlowSolve, glow_layers_from_source,
    glow_layers_report_from_source, screen_layer_over_encoded, solve_screen_alpha_for_dj,
};
pub use hash::fnv1a_32;
pub use ladder::{LadderPosition, LadderTint, ThemeAnchors};
pub use lcs::LcsColor;
pub use material::{
    BackdropBox, MaterialAlpha, Pole, committed_pole_encoded, solve_material_alpha_encoded,
    solve_material_alpha_hex, worst_contrast_encoded,
};
pub use semantic::{
    NamedRoleTable, Resolved, RoleChroma, RoleSpec, TextAnchor, TranslucentResolved,
    measure_contrast, recheck_against, recheck_against_multi, resolve_named_set,
};
// The built-in v1 showcase (`Role`/`RoleTable`/`resolve`/`resolve_set`) is no
// longer part of the production API (ADR-0001 PR-c): the agnostic engine ships
// only the string-keyed `resolve_named_set` path. It survives ONLY as the
// `#[cfg(test)]` byte-identity oracle for the named path, re-exported crate-wide
// so the in-crate showcase tests keep their `crate::…` spellings.
#[cfg(test)]
pub(crate) use semantic::{Role, RoleTable, resolve_set};
pub use solve::{
    BgInput, ChromaPolicy, Contract, Floor, Gamut, Hue, SolveJob, Solved, TypographicContext,
    Unreachable, solve, solve_many,
};
pub use spaces::oklch::{
    OKLCH_POWERLESS_CHROMA, OklchCoordinates, oklch_css_from_hex, oklch_from_hex,
};
pub use spaces::p3::{p3_css_from_hex, p3_from_hex};
pub use spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};
pub use spaces::vc::ViewingConditions;

/// Компилирует rust-блоки корневого README как doctest-ы: их API-примеры
/// обязаны собираться на каждом `cargo test --doc`, иначе README тихо
/// разъедется с кодом. Тип существует только под `--test`, в бинарь не входит.
#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
pub struct ReadmeDoctests;
