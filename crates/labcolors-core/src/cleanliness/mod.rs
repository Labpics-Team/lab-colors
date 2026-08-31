//! Alpha cleanliness assessment and evidence aggregation (R-05).
//!
//! This module is staged before its primary consumer (R-06 field/attachment lift).
//! All types are `pub(crate)` with `#[expect(dead_code)]` per V7 staging convention.

#[allow(dead_code)] // V1 cleanliness submodules staged for R-05/R-06 consumers.
pub(crate) mod alpha_assessment;

#[allow(dead_code)] // V1 cleanliness submodules staged for R-05/R-06 consumers.
pub(crate) mod alpha_aggregation;
