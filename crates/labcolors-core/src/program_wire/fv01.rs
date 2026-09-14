//! FV-01 public projection of the attachment-owned materialization contract.
//!
//! The implementation and constructor authority remain inside
//! `program::attachment`. This module only publishes the provisional pre-1.0
//! types needed by host/WASM adapters, preventing a second source of truth.

pub use crate::program::attachment::fv01::{
    AppearanceSurroundV1, AttachedMaterializationAuthorityErrorV1,
    AttachedMaterializationAuthorityV1, AttachedPointSinkAdmissionErrorV1,
    AttachedPointSinkErrorV1, AttachedPointSinkHostIntentV1, AttachedPointSinkHostPatchEntryV1,
    AttachedPointSinkHostV1, RendererProvenanceV1,
};
