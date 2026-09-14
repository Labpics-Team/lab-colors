//! FV-01 public-surface sentinel.
//!
//! The materialization node is not allowed to mint authority from detached
//! snapshots, Paint values, or CSS strings. The opaque authority lives on the
//! Program wire seam, while the host owns only the synchronous effect port.

use labcolors_core::program_wire::{
    AttachedMaterializationAuthorityV1, AttachedPointSinkHostIntentV1, AttachedPointSinkHostV1,
};

struct ExternalHost;

impl AttachedPointSinkHostV1 for ExternalHost {
    type Error = ();

    fn try_install(
        &mut self,
        intent: AttachedPointSinkHostIntentV1<'_>,
    ) -> Result<(), Self::Error> {
        match intent {
            AttachedPointSinkHostIntentV1::SetAll {
                binding_epoch,
                expected_sequence,
                desired_sequence,
                patch,
                ..
            } => {
                assert_ne!(binding_epoch, 0);
                assert!(desired_sequence > expected_sequence);
                assert!(!patch.is_empty());
            }
            AttachedPointSinkHostIntentV1::RevokeAll {
                binding_epoch,
                expected_sequence,
                desired_sequence,
                ..
            } => {
                assert_ne!(binding_epoch, 0);
                assert!(desired_sequence > expected_sequence);
            }
            AttachedPointSinkHostIntentV1::ConfirmExact {
                binding_epoch,
                patch,
                ..
            } => {
                assert_ne!(binding_epoch, 0);
                let _ = patch;
            }
        }
        Ok(())
    }
}

#[test]
fn attached_materialization_authority_is_a_public_opaque_capability() {
    fn accepts_capability(_: &AttachedMaterializationAuthorityV1) {}
    let _ = accepts_capability as fn(&AttachedMaterializationAuthorityV1);
}

#[test]
fn external_consumers_can_implement_only_the_typed_host_effect_port() {
    fn accepts_host<H: AttachedPointSinkHostV1>(_host: H) {}
    accepts_host(ExternalHost);
}
