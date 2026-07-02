pub(crate) mod spaces;

pub mod accent;
pub mod alpha;
pub mod cleanliness;
pub mod config;
pub mod lcs;
pub mod lpc;
pub(crate) mod lut;
pub mod neutral;
pub mod scale;
pub mod semantic;
pub mod sentiment;
pub mod solve;
pub(crate) mod wcag;

pub mod curve;

pub(crate) mod greyfast;

pub(crate) mod chromafast;

#[cfg(test)]
mod golden_tests;

pub use accent::Accent;
pub use cleanliness::{
    DefectContext, Theme, confidence as cleanliness_confidence,
    confidence_from_hex as cleanliness_confidence_from_hex, drab, drab_in_context,
    muddiness_from_hex, muddiness_from_linear_srgb, muddiness_in_context, muddiness_oklch, n_pure,
};
pub use config::{
    Brand, ConfigError, NeutralAnchors, NeutralConfig, NeutralTint, PaletteFamily, RoleRecipe,
    SentimentCategory, SentimentsConfig, ThemeConfig, ThemesConfig, VcPreset, labui_reference,
};
pub use curve::ColorCurve;
pub use lcs::LcsColor;
pub use semantic::{
    NamedRoleTable, Resolved, Role, RoleChroma, RoleSpec, RoleTable, TextAnchor, measure_contrast,
    recheck_against, resolve, resolve_named_set, resolve_set,
};
pub use solve::{
    BgInput, ChromaPolicy, Contract, Floor, Gamut, Hue, SolveJob, Solved, TypographicContext,
    Unreachable, solve, solve_many,
};
pub use spaces::vc::ViewingConditions;
