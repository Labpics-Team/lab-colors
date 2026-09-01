//! Alpha cleanliness assessment and evidence aggregation (R-05).
//!
//! This module is staged before its primary consumer (R-06 field/attachment lift).
//! All types are `pub(crate)` with `#[expect(dead_code)]` per V7 staging convention.

/// Staged for R-09 alpha backdrop integration. Types are complete and tested;
/// no production consumer exists yet — the field attachment pass (R-06) will
/// wire these into the cleanliness audit pipeline.
#[allow(
    dead_code,
    reason = "Staged for R-09 alpha backdrop; consumer lands in R-06 field attachment"
)]
pub(crate) mod alpha_assessment;

/// Staged for R-09 alpha backdrop aggregation. Paired with alpha_assessment;
/// both modules are adopted atomically when the field attachment pass lands.
#[allow(
    dead_code,
    reason = "Staged for R-09 alpha backdrop; consumer lands in R-06 field attachment"
)]
pub(crate) mod alpha_aggregation;
