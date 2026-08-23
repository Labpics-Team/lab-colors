//! Versioned composition-law proof types for the R3c/R3d restorative auto chain.
//! These model mathematical guarantees about source-over composition correctness,
//! base acceptance, reject absence, and whole-field coverage over owned domains.
//!
//! All types are content-addressed via SHA-256 digest where applicable.
//! Independent versioning from composition.rs point-arithmetic layer.
//!
//! V7 staged: pub(crate), #[allow(dead_code)], no unwrap/unsafe.

mod base_accepted;
mod law_proof;
mod no_reject;
mod owned_reference;
mod whole_field;

#[cfg(test)]
mod tests;

#[allow(
    unused_imports,
    reason = "R-08 PR-A staged before PR-C field proof engine"
)]
pub(crate) use base_accepted::{BaseAcceptedOnSupportCertificateV1, BaseAcceptedOnSupportError};
#[allow(
    unused_imports,
    reason = "R-08 PR-A staged before PR-C field proof engine"
)]
pub(crate) use law_proof::{
    CompositionLawProofError, CompositionLawProofV1, CompositionLawVerificationMethodV1,
};
#[allow(
    unused_imports,
    reason = "R-08 PR-A staged before PR-D human-clean lift"
)]
pub(crate) use no_reject::{NoIntroducedRejectCertificateV1, NoIntroducedRejectError};
#[allow(
    unused_imports,
    reason = "R-08 PR-A staged before PR-C field proof engine"
)]
pub(crate) use owned_reference::{OwnedCompositionReferenceError, OwnedCompositionReferenceV1};
#[allow(
    unused_imports,
    reason = "R-08 PR-A staged before PR-C field proof engine"
)]
pub(crate) use whole_field::{
    WholeFieldCoverageError, WholeFieldCoverageMethodV1, WholeFieldCoverageProofV1,
};
