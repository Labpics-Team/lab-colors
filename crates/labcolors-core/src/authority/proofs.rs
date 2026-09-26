//! Полные символьные входы для процессного AUTH-контракта, не модель вместо Rust.
//! Проверяется переход из любого состояния с правильным владельцем каждой ячейки.
//! Истинность доказательства TQ/CC/HCE и криптографические свойства здесь не выводятся.

use super::*;

fn any_id() -> AuthorityIdV1 {
    match kani::any::<u8>() {
        0 => AuthorityIdV1::TechnicalQuality,
        1 => AuthorityIdV1::CleanConvention,
        _ => AuthorityIdV1::HumanCleanEvidence,
    }
}

fn any_descriptor(id: AuthorityIdV1) -> AuthorityDescriptorV1 {
    AuthorityDescriptorV1::from_verified_owner(id, kani::any(), kani::any(), kani::any())
}

fn any_slot(id: AuthorityIdV1) -> Option<AuthorityDescriptorV1> {
    if kani::any() {
        Some(any_descriptor(id))
    } else {
        None
    }
}

fn any_state() -> AuthorityStateV1 {
    AuthorityStateV1 {
        slots: [
            any_slot(AuthorityIdV1::TechnicalQuality),
            any_slot(AuthorityIdV1::CleanConvention),
            any_slot(AuthorityIdV1::HumanCleanEvidence),
        ],
    }
}

// Oracle не вызывает production slot(), read(), require() или validator:
// контракт задаёт три независимые ячейки и равенство полного кортежа.
fn contract_slot(id: AuthorityIdV1) -> usize {
    match id {
        AuthorityIdV1::TechnicalQuality => 0,
        AuthorityIdV1::CleanConvention => 1,
        AuthorityIdV1::HumanCleanEvidence => 2,
    }
}

fn same_binding(a: AuthorityDescriptorV1, b: AuthorityDescriptorV1) -> bool {
    a.id == b.id
        && a.release.0 == b.release.0
        && a.applicability.0 == b.applicability.0
        && a.provenance.0 == b.provenance.0
}

#[kani::proof]
#[kani::unwind(33)]
fn auth_permit_iff_exact_current_proof() {
    let current = any_descriptor(any_id());
    let next = any_descriptor(any_id());
    let owner = AuthorityOwnerCurrentV1::new(current);
    let proof_present: bool = kani::any();
    let needs_observation: bool = kani::any();
    let requirement = if needs_observation {
        RendererObservationRequirementV1::Required
    } else {
        RendererObservationRequirementV1::NotRequired
    };
    let result = issue_permit(
        &owner,
        next,
        proof_present,
        requirement,
        ProgramRendererProvenanceV1::Unverified,
    );
    // V1 не имеет конструктора Observed: требование наблюдения не выполнимо.
    let allowed = proof_present && same_binding(current, next) && !needs_observation;
    assert!(
        result.is_ok() == allowed,
        "permit must match the full current proof binding"
    );
    if let Ok(permit) = &result {
        assert!(same_binding(permit.descriptor, next));
        assert!(same_binding(permit.owner_current.descriptor, current));
    }
    kani::cover!(result.is_ok(), "valid permit remains reachable");
    kani::cover!(
        matches!(result, Err(AuthorityPermitErrorV1::MissingProof)),
        "missing proof"
    );
    kani::cover!(
        matches!(result, Err(AuthorityPermitErrorV1::AuthorityMismatch)),
        "foreign lane"
    );
    kani::cover!(
        matches!(result, Err(AuthorityPermitErrorV1::ReleaseMismatch)),
        "stale release"
    );
    kani::cover!(
        matches!(result, Err(AuthorityPermitErrorV1::ApplicabilityMismatch)),
        "foreign applicability"
    );
    kani::cover!(
        matches!(result, Err(AuthorityPermitErrorV1::ProvenanceMismatch)),
        "foreign provenance"
    );
    kani::cover!(
        matches!(
            result,
            Err(AuthorityPermitErrorV1::RendererObservationRequired)
        ),
        "unobserved renderer"
    );
}

#[kani::proof]
#[kani::unwind(33)]
fn auth_admission_is_exact_atomic_and_lane_local() {
    let before = any_state();
    let next = any_descriptor(any_id());
    let expected = if kani::any() {
        AuthorityExpectedCurrentV1::Vacant
    } else {
        AuthorityExpectedCurrentV1::Exact(any_descriptor(any_id()))
    };
    let owner = AuthorityOwnerCurrentV1::new(any_descriptor(any_id()));
    // Намеренно допускаем внутреннюю подделку permit: проверка admit тоже
    // обязана отвергать её, не полагаясь только на приватность конструктора.
    let permit_descriptor = any_descriptor(any_id());
    let permit = AuthorityAdmissionPermitV1 {
        owner_current: &owner,
        descriptor: permit_descriptor,
    };
    let slot = contract_slot(next.id);
    let current = before.slots[slot];
    let authorized = same_binding(owner.descriptor, next) && same_binding(permit_descriptor, next);
    let duplicate = current.is_some_and(|value| same_binding(value, next));
    let vacant = current.is_none() && matches!(expected, AuthorityExpectedCurrentV1::Vacant);
    let exact = match (current, expected) {
        (Some(value), AuthorityExpectedCurrentV1::Exact(expected)) => same_binding(value, expected),
        _ => false,
    };
    let allowed = authorized && (duplicate || vacant || exact);
    let mut after = before;
    let result = after.admit(next, expected, permit);
    assert!(
        result.is_ok() == allowed,
        "admission must obey exact authorization and CAS"
    );
    if allowed {
        let outcome = if duplicate {
            AuthorityAdmissionOutcomeV1::DuplicateNoop
        } else if vacant {
            AuthorityAdmissionOutcomeV1::Installed
        } else {
            AuthorityAdmissionOutcomeV1::Replaced
        };
        assert!(result == Ok(outcome));
        let mut expected_state = before;
        expected_state.slots[slot] = Some(next);
        assert!(
            after == expected_state,
            "only the addressed lane may change"
        );
    } else {
        assert!(after == before, "every refusal must preserve all lanes");
    }
    kani::cover!(
        result == Ok(AuthorityAdmissionOutcomeV1::Installed),
        "install"
    );
    kani::cover!(
        result == Ok(AuthorityAdmissionOutcomeV1::Replaced),
        "replace"
    );
    kani::cover!(
        result == Ok(AuthorityAdmissionOutcomeV1::DuplicateNoop),
        "duplicate"
    );
    kani::cover!(result.is_err(), "refusal");
}

#[kani::proof]
#[kani::unwind(33)]
fn auth_require_iff_full_binding() {
    let state = any_state();
    let expected = any_descriptor(any_id());
    let stored = state.slots[contract_slot(expected.id)];
    let allowed = stored.is_some_and(|value| same_binding(value, expected));
    let result = state.require(expected);
    assert!(
        result.is_ok() == allowed,
        "require must compare every byte of every identity"
    );
    assert!(
        state.read(expected.id) == stored,
        "read must address the contract lane"
    );
    assert!(AuthorityStateV1::new().require(expected) == Err(AuthorityRequireErrorV1::Missing));
    kani::cover!(result.is_ok(), "exact binding");
    kani::cover!(
        result == Err(AuthorityRequireErrorV1::Missing),
        "empty lane"
    );
    kani::cover!(
        result == Err(AuthorityRequireErrorV1::ReleaseMismatch),
        "release mismatch"
    );
    kani::cover!(
        result == Err(AuthorityRequireErrorV1::ApplicabilityMismatch),
        "applicability mismatch"
    );
    kani::cover!(
        result == Err(AuthorityRequireErrorV1::ProvenanceMismatch),
        "provenance mismatch"
    );
}

#[kani::proof]
#[kani::unwind(33)]
fn auth_issue_admit_require_composes() {
    let mut state = any_state();
    let next = any_descriptor(any_id());
    let owner = AuthorityOwnerCurrentV1::new(next);
    let expected = match state.slots[contract_slot(next.id)] {
        None => AuthorityExpectedCurrentV1::Vacant,
        Some(current) => AuthorityExpectedCurrentV1::Exact(current),
    };
    let permit = issue_permit(
        &owner,
        next,
        true,
        RendererObservationRequirementV1::NotRequired,
        ProgramRendererProvenanceV1::Unverified,
    )
    .expect("current verified owner must authorize");
    assert!(
        state.admit(next, expected, permit).is_ok(),
        "fresh owner authorization must commit"
    );
    assert!(
        state.require(next).is_ok(),
        "accepted binding must remain usable"
    );
    let committed = state;
    let duplicate = issue_permit(
        &owner,
        next,
        true,
        RendererObservationRequirementV1::NotRequired,
        ProgramRendererProvenanceV1::Unverified,
    )
    .expect("current owner remains current");
    assert!(
        state.admit(next, AuthorityExpectedCurrentV1::Vacant, duplicate)
            == Ok(AuthorityAdmissionOutcomeV1::DuplicateNoop)
    );
    assert!(state == committed, "duplicate must remain a true no-op");
    kani::cover!(true, "complete authorized path");
}
