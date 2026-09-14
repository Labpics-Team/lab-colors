//! Public Program wire/runtime boundary and FV-01 attached materialization seam.
//!
//! The facade keeps one public Program owner while separating the established
//! wire/runtime implementation from the provisional attached-authority slice.
//! Internal modules remain private; consumers only see the re-exported contract.

mod fv01;
mod runtime;

pub(crate) use fv01::AttachedMaterializationCaseProofV1;
pub use fv01::{
    AppearanceSurroundV1, AttachedMaterializationAuthorityErrorV1,
    AttachedMaterializationAuthorityV1, AttachedMaterializationCaseV1,
    AttachedPointSinkAdmissionErrorV1, AttachedPointSinkErrorV1, AttachedPointSinkHostIntentV1,
    AttachedPointSinkHostPatchEntryV1, AttachedPointSinkHostV1, RendererProvenanceV1,
};
pub use runtime::*;
