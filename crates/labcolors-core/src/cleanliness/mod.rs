//! Alpha cleanliness assessment and evidence aggregation (R-05).
//!
//! This module is staged before its primary consumer (R-06 field/attachment lift).
//! All types are `pub(crate)` with `#[expect(dead_code)]` per V7 staging convention.

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "alpha assessment types are staged for R-05 PR1; consumed by PR2 aggregation and R-06"
    )
)]
pub(crate) mod alpha_assessment;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "alpha aggregation policy is staged for R-05 PR2; consumed by R-06 field lift"
    )
)]
pub(crate) mod alpha_aggregation;
