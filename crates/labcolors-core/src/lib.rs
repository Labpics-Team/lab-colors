pub(crate) mod spaces;

pub mod lcs;
pub mod lpc;
pub mod neutral;
pub mod scale;
pub mod sentiment;
pub mod solve;

pub mod curve;

#[cfg(test)]
mod golden_tests;

pub use curve::ColorCurve;
pub use lcs::LcsColor;
pub use solve::{
    BgInput, ChromaPolicy, Contract, Gamut, Hue, Solved, TypographicContext, Unreachable, solve,
};
pub use spaces::vc::ViewingConditions;
