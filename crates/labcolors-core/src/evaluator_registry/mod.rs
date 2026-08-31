//! Evaluator registry metadata, LPC channel separation, and admission infrastructure.
//!
//! This module is staged under `#[expect(dead_code)]` because consumers
//! land in F-03 PR2–PR4. The types themselves are complete and tested;
//! only the integration into live evaluation paths is deferred.

#[expect(
    dead_code,
    reason = "F-03 PR1 metadata types are staged; consumers arrive in PR2-4"
)]
pub(crate) mod metadata;

#[expect(
    dead_code,
    reason = "F-03 PR1 LPC channel types are staged; consumers arrive in PR2-4"
)]
pub(crate) mod lpc_channel;

/// Staged for F-03 PR2–PR4 external profile admission. Types are complete and
/// tested; the registry wiring into live evaluation paths is deferred.
#[expect(
    dead_code,
    reason = "F-03 PR1 admission types are staged; consumers arrive in PR2-4"
)]
pub(crate) mod admission;
