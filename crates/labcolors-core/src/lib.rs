pub(crate) mod spaces;

pub mod lcs;
pub mod lpc;
pub mod neutral;
pub mod scale;
pub mod semantic;
pub mod sentiment;
pub mod solve;
pub(crate) mod wcag;

pub mod curve;

#[cfg(test)]
mod golden_tests;

pub use curve::ColorCurve;
pub use lcs::LcsColor;
pub use semantic::{
    Resolved, Role, RoleChroma, RoleSpec, RoleTable, TextAnchor, resolve, resolve_set,
};
pub use solve::{
    BgInput, ChromaPolicy, Contract, Floor, Gamut, Hue, Solved, TypographicContext, Unreachable,
    solve,
};
pub use spaces::vc::ViewingConditions;
