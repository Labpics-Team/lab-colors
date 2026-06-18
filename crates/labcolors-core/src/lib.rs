pub(crate) mod spaces;

pub mod background;
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

pub use background::{
    BackgroundEntry, BackgroundError, BackgroundRole, ColorScheme, ContrastMode, ThemeContext,
    background_input, resolve_background, resolve_background_set,
};
pub use curve::ColorCurve;
pub use lcs::LcsColor;
pub use semantic::{
    Resolved, Role, RoleChroma, RoleSpec, RoleTable, TextAnchor, measure_contrast, recheck_against,
    resolve, resolve_set,
};
pub use solve::{
    BgInput, ChromaPolicy, Contract, Floor, Gamut, Hue, SolveJob, Solved, TypographicContext,
    Unreachable, solve, solve_many,
};
pub use spaces::vc::ViewingConditions;
