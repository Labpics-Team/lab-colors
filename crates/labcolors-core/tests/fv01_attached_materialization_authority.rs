//! FV-01 public-surface sentinel.
//!
//! The materialization node is not allowed to mint authority from detached
//! snapshots, Paint values, or CSS strings. The opaque authority lives on the
//! Program wire seam, while the host owns only the synchronous effect port.

use labcolors_core::program_wire::{
    AttachedMaterializationAuthorityErrorV1, AttachedMaterializationAuthorityV1,
    AttachedMaterializationCaseV1, AttachedPointSinkHostIntentV1, AttachedPointSinkHostV1,
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
fn one_authority_projects_many_exact_physical_cases_without_minting_more_authorities() {
    fn accepts_case(_: AttachedMaterializationCaseV1) {}
    fn visits_cases(authority: &AttachedMaterializationAuthorityV1) {
        assert_eq!(authority.case_count(), authority.cases().len());
        for case in authority.cases() {
            accepts_case(case);
        }
    }
    let _ = visits_cases as fn(&AttachedMaterializationAuthorityV1);
}

#[test]
fn authority_refusals_are_typed_and_the_family_remains_open_for_pre_1_0_evolution() {
    fn classifies(error: AttachedMaterializationAuthorityErrorV1) -> &'static str {
        match error {
            AttachedMaterializationAuthorityErrorV1::PublishedRevisionMismatch => "revision",
            AttachedMaterializationAuthorityErrorV1::ProgramIdentityMismatch => "identity",
            AttachedMaterializationAuthorityErrorV1::ForeignOwnerGeneration => "owner",
            AttachedMaterializationAuthorityErrorV1::ForeignBindingEpoch => "epoch",
            AttachedMaterializationAuthorityErrorV1::MissingExactPointAbsenceProof
            | AttachedMaterializationAuthorityErrorV1::EmptyFinalOwnedDomain => "absence-proof",
            AttachedMaterializationAuthorityErrorV1::NonTerminalRoot
            | AttachedMaterializationAuthorityErrorV1::RootConsumedDownstream => "terminality",
            AttachedMaterializationAuthorityErrorV1::ResourceExhausted => "resource",
            _ => "future",
        }
    }
    let _ = classifies as fn(AttachedMaterializationAuthorityErrorV1) -> &'static str;
}

#[test]
fn external_consumers_can_implement_only_the_typed_host_effect_port() {
    fn accepts_host<H: AttachedPointSinkHostV1>(_host: H) {}
    accepts_host(ExternalHost);
}
