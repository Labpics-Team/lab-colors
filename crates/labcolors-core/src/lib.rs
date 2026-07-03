pub(crate) mod spaces;

pub(crate) mod accent;
pub mod alpha;
pub mod cleanliness;
pub mod config;
pub mod glow;
pub mod ladder;
pub mod lcs;
pub mod lpc;
pub(crate) mod lut;
pub mod neutral;
pub mod pair;
pub mod scale;
pub mod semantic;
pub mod sentiment;
pub mod solve;
pub(crate) mod wcag;

pub mod curve;

#[cfg(test)]
mod golden_tests;

#[cfg(test)]
mod agnostic_gates;

#[cfg(test)]
mod one_levelness_tests;

// Built-in-showcase behaviour tests, relocated in-crate (ADR-0001 PR-c): the
// built-in `Role`/`RoleTable`/`resolve_set` cluster is now `#[cfg(test)]`-only,
// so these tests — which exercise it as the byte-identity oracle — must live
// inside the crate to see it (integration tests only see the public API).
#[cfg(test)]
mod continuity_tests;

#[cfg(test)]
mod dim_tinted_tests;

#[cfg(test)]
mod r3_byte_identity_tests;

// AccentCurve/SentimentCurve golden snapshots, relocated in-crate (ADR-0001
// PR-c): the `Sentiment` enum is now `#[cfg(test)]`-only, and the golden uses
// the crate-private `SentimentCurve::from_sentiment` helper, so this test must
// live inside the crate to see them.
#[cfg(test)]
mod accent_golden_tests;

pub use alpha::composite_over_encoded;
pub use cleanliness::{
    DefectContext, Theme, drab, drab_in_context, muddiness_from_hex, muddiness_from_linear_srgb,
    muddiness_in_context, muddiness_oklch, n_pure,
};
pub use config::{
    Brand, ConfigError, LadderSource, NeutralAnchors, NeutralConfig, NeutralPick, NeutralTint,
    PaletteFamily, RoleRecipe, SentimentCategory, SentimentsConfig, ThemeConfig, ThemesConfig,
    VcPreset,
};
pub use curve::ColorCurve;
pub use glow::{
    GLOW_BASE_DJ, GLOW_BLOOM_DJ, GLOW_SUBTLE_DJ, GlowSolve, glow_layers_from_source,
    screen_layer_over_encoded, solve_screen_alpha_for_dj,
};
pub use ladder::{LadderPosition, LadderTint, ThemeAnchors};
pub use lcs::LcsColor;
pub use semantic::{
    NamedRoleTable, Resolved, RoleChroma, RoleSpec, TextAnchor, TranslucentResolved,
    measure_contrast, recheck_against, resolve_named_set,
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
pub use spaces::oklch::{oklch_css_from_hex, oklch_from_hex};
pub use spaces::p3::{p3_css_from_hex, p3_from_hex};
pub use spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};
pub use spaces::vc::ViewingConditions;

/// Компилирует rust-блоки корневого README как doctest-ы: их API-примеры
/// обязаны собираться на каждом `cargo test --doc`, иначе README тихо
/// разъедется с кодом. Тип существует только под `--test`, в бинарь не входит.
#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
pub struct ReadmeDoctests;
