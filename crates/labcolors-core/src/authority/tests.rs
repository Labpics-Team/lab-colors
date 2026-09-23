use super::*;
use core::mem::size_of;

fn descriptor(id: AuthorityIdV1, seed: u8) -> AuthorityDescriptorV1 {
    AuthorityDescriptorV1::from_verified_owner(
        id,
        [seed; 32],
        [seed.wrapping_add(1); 32],
        [seed.wrapping_add(2); 32],
    )
}

fn permit<'a>(
    owner: &'a AuthorityOwnerCurrentV1,
    next: AuthorityDescriptorV1,
) -> AuthorityAdmissionPermitV1<'a> {
    issue_permit(
        owner,
        next,
        true,
        RendererObservationRequirementV1::NotRequired,
        ProgramRendererProvenanceV1::Unverified,
    )
    .expect("fresh owner proof must issue the exact permit")
}

#[test]
fn state_has_exactly_three_fixed_slots() {
    assert_eq!(
        size_of::<AuthorityStateV1>(),
        3 * size_of::<Option<AuthorityDescriptorV1>>()
    );
}

#[test]
fn three_lanes_are_independent_and_missing_never_falls_back() {
    let tq = descriptor(AuthorityIdV1::TechnicalQuality, 1);
    let cc = descriptor(AuthorityIdV1::CleanConvention, 10);
    let tq_owner = AuthorityOwnerCurrentV1::new(tq);
    let mut state = AuthorityStateV1::new();

    assert_eq!(
        state.admit(
            tq,
            AuthorityExpectedCurrentV1::Vacant,
            permit(&tq_owner, tq)
        ),
        Ok(AuthorityAdmissionOutcomeV1::Installed)
    );
    assert_eq!(state.read(AuthorityIdV1::TechnicalQuality), Some(tq));
    assert_eq!(state.read(AuthorityIdV1::CleanConvention), None);
    assert_eq!(state.require(cc), Err(AuthorityRequireErrorV1::Missing));
}

#[test]
fn exact_duplicate_wins_over_stale_vacant_expected() {
    let value = descriptor(AuthorityIdV1::TechnicalQuality, 2);
    let owner = AuthorityOwnerCurrentV1::new(value);
    let mut state = AuthorityStateV1::new();

    assert_eq!(
        state.admit(
            value,
            AuthorityExpectedCurrentV1::Vacant,
            permit(&owner, value)
        ),
        Ok(AuthorityAdmissionOutcomeV1::Installed)
    );
    assert_eq!(
        state.admit(
            value,
            AuthorityExpectedCurrentV1::Vacant,
            permit(&owner, value)
        ),
        Ok(AuthorityAdmissionOutcomeV1::DuplicateNoop)
    );
    assert_eq!(state.read(value.id()), Some(value));
}

#[test]
fn exact_same_lane_replacement_commits_once() {
    let first = descriptor(AuthorityIdV1::TechnicalQuality, 3);
    let second = descriptor(AuthorityIdV1::TechnicalQuality, 4);
    let mut owner = AuthorityOwnerCurrentV1::new(first);
    let mut state = AuthorityStateV1::new();

    state
        .admit(
            first,
            AuthorityExpectedCurrentV1::Vacant,
            permit(&owner, first),
        )
        .unwrap();
    owner.advance_verified(second).unwrap();
    assert_eq!(
        state.admit(
            second,
            AuthorityExpectedCurrentV1::Exact(first),
            permit(&owner, second),
        ),
        Ok(AuthorityAdmissionOutcomeV1::Replaced)
    );
    assert_eq!(state.read(first.id()), Some(second));
}

#[test]
fn stale_expected_replacement_is_typed_and_non_mutating() {
    let first = descriptor(AuthorityIdV1::TechnicalQuality, 5);
    let current = descriptor(AuthorityIdV1::TechnicalQuality, 6);
    let next = descriptor(AuthorityIdV1::TechnicalQuality, 7);
    let mut owner = AuthorityOwnerCurrentV1::new(first);
    let mut state = AuthorityStateV1::new();

    state
        .admit(
            first,
            AuthorityExpectedCurrentV1::Vacant,
            permit(&owner, first),
        )
        .unwrap();
    owner.advance_verified(current).unwrap();
    state
        .admit(
            current,
            AuthorityExpectedCurrentV1::Exact(first),
            permit(&owner, current),
        )
        .unwrap();
    owner.advance_verified(next).unwrap();

    assert_eq!(
        state.admit(
            next,
            AuthorityExpectedCurrentV1::Exact(first),
            permit(&owner, next),
        ),
        Err(AuthorityAdmissionErrorV1::ExpectedReleaseMismatch)
    );
    assert_eq!(state.read(first.id()), Some(current));
}

#[test]
fn cross_lane_expected_is_rejected_without_mutation() {
    let current = descriptor(AuthorityIdV1::TechnicalQuality, 8);
    let next = descriptor(AuthorityIdV1::TechnicalQuality, 9);
    let foreign = descriptor(AuthorityIdV1::CleanConvention, 8);
    let mut owner = AuthorityOwnerCurrentV1::new(current);
    let mut state = AuthorityStateV1::new();

    state
        .admit(
            current,
            AuthorityExpectedCurrentV1::Vacant,
            permit(&owner, current),
        )
        .unwrap();
    owner.advance_verified(next).unwrap();
    assert_eq!(
        state.admit(
            next,
            AuthorityExpectedCurrentV1::Exact(foreign),
            permit(&owner, next),
        ),
        Err(AuthorityAdmissionErrorV1::ExpectedAuthorityMismatch)
    );
    assert_eq!(state.read(current.id()), Some(current));
}

#[test]
fn permit_must_bind_the_exact_descriptor() {
    let first = descriptor(AuthorityIdV1::TechnicalQuality, 11);
    let other = descriptor(AuthorityIdV1::TechnicalQuality, 12);
    let owner = AuthorityOwnerCurrentV1::new(first);
    let mut state = AuthorityStateV1::new();

    assert_eq!(
        state.admit(
            other,
            AuthorityExpectedCurrentV1::Vacant,
            permit(&owner, first)
        ),
        Err(AuthorityAdmissionErrorV1::PermitDescriptorMismatch)
    );
    assert_eq!(state.read(first.id()), None);
}

#[test]
fn saved_descriptor_cannot_mint_after_owner_current_advances() {
    let old = descriptor(AuthorityIdV1::TechnicalQuality, 13);
    let new = descriptor(AuthorityIdV1::TechnicalQuality, 14);
    let mut owner = AuthorityOwnerCurrentV1::new(old);
    owner.advance_verified(new).unwrap();

    assert!(matches!(
        issue_permit(
            &owner,
            old,
            true,
            RendererObservationRequirementV1::NotRequired,
            ProgramRendererProvenanceV1::Unverified,
        ),
        Err(AuthorityPermitErrorV1::ReleaseMismatch)
    ));
}

#[test]
fn owner_proof_and_all_identity_axes_fail_closed() {
    let base = descriptor(AuthorityIdV1::TechnicalQuality, 15);
    let owner = AuthorityOwnerCurrentV1::new(base);

    assert!(matches!(
        issue_permit(
            &owner,
            base,
            false,
            RendererObservationRequirementV1::NotRequired,
            ProgramRendererProvenanceV1::Unverified,
        ),
        Err(AuthorityPermitErrorV1::MissingProof)
    ));

    let foreign = descriptor(AuthorityIdV1::CleanConvention, 15);
    assert!(matches!(
        issue_permit(
            &owner,
            foreign,
            true,
            RendererObservationRequirementV1::NotRequired,
            ProgramRendererProvenanceV1::Unverified,
        ),
        Err(AuthorityPermitErrorV1::AuthorityMismatch)
    ));

    let release = AuthorityDescriptorV1::from_verified_owner(
        base.id,
        [99; 32],
        *base.applicability.as_bytes(),
        *base.provenance.as_bytes(),
    );
    assert!(matches!(
        issue_permit(
            &owner,
            release,
            true,
            RendererObservationRequirementV1::NotRequired,
            ProgramRendererProvenanceV1::Unverified,
        ),
        Err(AuthorityPermitErrorV1::ReleaseMismatch)
    ));

    let applicability = AuthorityDescriptorV1::from_verified_owner(
        base.id,
        *base.release.as_bytes(),
        [98; 32],
        *base.provenance.as_bytes(),
    );
    assert!(matches!(
        issue_permit(
            &owner,
            applicability,
            true,
            RendererObservationRequirementV1::NotRequired,
            ProgramRendererProvenanceV1::Unverified,
        ),
        Err(AuthorityPermitErrorV1::ApplicabilityMismatch)
    ));

    let provenance = AuthorityDescriptorV1::from_verified_owner(
        base.id,
        *base.release.as_bytes(),
        *base.applicability.as_bytes(),
        [97; 32],
    );
    assert!(matches!(
        issue_permit(
            &owner,
            provenance,
            true,
            RendererObservationRequirementV1::NotRequired,
            ProgramRendererProvenanceV1::Unverified,
        ),
        Err(AuthorityPermitErrorV1::ProvenanceMismatch)
    ));
}

#[test]
fn unverified_renderer_never_satisfies_observed_requirement() {
    let value = descriptor(AuthorityIdV1::TechnicalQuality, 16);
    let owner = AuthorityOwnerCurrentV1::new(value);

    assert!(matches!(
        issue_permit(
            &owner,
            value,
            true,
            RendererObservationRequirementV1::Required,
            ProgramRendererProvenanceV1::Unverified,
        ),
        Err(AuthorityPermitErrorV1::RendererObservationRequired)
    ));
}

#[test]
fn require_distinguishes_every_binding_mismatch() {
    let value = descriptor(AuthorityIdV1::HumanCleanEvidence, 20);
    let owner = AuthorityOwnerCurrentV1::new(value);
    let mut state = AuthorityStateV1::new();
    state
        .admit(
            value,
            AuthorityExpectedCurrentV1::Vacant,
            permit(&owner, value),
        )
        .unwrap();

    let changed_release = AuthorityDescriptorV1::from_verified_owner(
        value.id,
        [21; 32],
        *value.applicability.as_bytes(),
        *value.provenance.as_bytes(),
    );
    assert_eq!(
        state.require(changed_release),
        Err(AuthorityRequireErrorV1::ReleaseMismatch)
    );

    let changed_applicability = AuthorityDescriptorV1::from_verified_owner(
        value.id,
        *value.release.as_bytes(),
        [22; 32],
        *value.provenance.as_bytes(),
    );
    assert_eq!(
        state.require(changed_applicability),
        Err(AuthorityRequireErrorV1::ApplicabilityMismatch)
    );

    let changed_provenance = AuthorityDescriptorV1::from_verified_owner(
        value.id,
        *value.release.as_bytes(),
        *value.applicability.as_bytes(),
        [23; 32],
    );
    assert_eq!(
        state.require(changed_provenance),
        Err(AuthorityRequireErrorV1::ProvenanceMismatch)
    );
}
