//! Public Program wire/runtime boundary and FV-01 attached materialization seam.
//!
//! The facade keeps one public Program owner while separating the established
//! wire/runtime implementation from the provisional attached-authority slice.
//! Internal modules remain private; consumers only see the re-exported contract.

mod attached;
#[cfg(test)]
mod attached_tests;
mod fv01;
mod runtime;

pub use attached::*;
pub use fv01::{
    AppearanceSurroundV1, AttachedMaterializationAuthorityErrorV1,
    AttachedMaterializationAuthorityV1, AttachedMaterializationCaseV1,
    AttachedPointSinkAdmissionErrorV1, AttachedPointSinkErrorV1, AttachedPointSinkHostIntentV1,
    AttachedPointSinkHostPatchEntryV1, AttachedPointSinkHostV1, RendererProvenanceV1,
};
pub(crate) use fv01::{
    AttachedMaterializationAuthorityPartsV1, AttachedMaterializationCaseProofV1,
};
pub use runtime::*;

impl<HostError: core::fmt::Debug> core::fmt::Debug for AttachedProgramUpdateErrorV1<HostError> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ResourceExhausted => f.write_str("ResourceExhausted"),
            Self::Update => f.write_str("Update"),
            Self::SinkPrepare(error) => f.debug_tuple("SinkPrepare").field(error).finish(),
            Self::SinkInstall(error) => f.debug_tuple("SinkInstall").field(error).finish(),
            Self::Authority(error) => f.debug_tuple("Authority").field(error).finish(),
            Self::InternalInvariant => f.write_str("InternalInvariant"),
        }
    }
}

#[cfg(test)]
mod public_contract_tests {
    use super::*;

    #[test]
    fn attached_update_errors_support_debug_diagnostics() {
        fn assert_debug<T: core::fmt::Debug>() {}
        assert_debug::<AttachedProgramUpdateErrorV1<()>>();
    }
}
