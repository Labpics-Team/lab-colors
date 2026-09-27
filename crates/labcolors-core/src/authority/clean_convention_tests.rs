use super::super::test_support::{Host, point_attachment_for};
use super::*;
use crate::program_wire::ProgramScenarioV1;
use crate::test_support::{AllocatorEvents, measured_allocator_events};

fn selection() -> CleanConventionSelectionV1 {
    CleanConventionSelectionV1::select(
        EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1,
        CleanConventionScopeV1::ModeledSrgb8Point,
        CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate,
    )
    .unwrap()
}

fn observe(attachment: &mut ProgramAttachmentV1<Host>, revision: u64, backdrop: Srgb8) {
    attachment
        .update_observed(revision, &[ProgramScenarioV1::new(1, vec![backdrop])])
        .unwrap();
}

fn ready(
    source: Srgb8,
    opacity: f64,
    backdrop: Srgb8,
    composite: Srgb8,
) -> ProgramAttachmentV1<Host> {
    let mut attachment = point_attachment_for(source, opacity, composite);
    observe(&mut attachment, 1, backdrop);
    assert_eq!(
        attachment
            .current_materialization_authority()
            .unwrap()
            .terminal_composite(),
        composite
    );
    attachment
}

#[test]
fn explicit_selection_rejects_foreign_release_scope_and_unearned_admission() {
    use CleanConventionAdmissionKindV1 as Kind;
    use CleanConventionScopeV1 as Scope;
    let release = EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1;
    let accepted = selection();
    assert_eq!(accepted.release(), release);
    assert_eq!(accepted.scope(), Scope::ModeledSrgb8Point);
    assert_eq!(accepted.admission(), Kind::DeclaredPackagePolicyCandidate);
    for index in 0..release.len() {
        let mut foreign = release;
        foreign[index] ^= 1;
        assert_eq!(
            CleanConventionSelectionV1::select(
                foreign,
                Scope::ModeledSrgb8Point,
                Kind::DeclaredPackagePolicyCandidate
            ),
            Err(CleanConventionErrorV1::UnsupportedRelease)
        );
    }
    for scope in [Scope::WholeField, Scope::ObservedPoint] {
        assert_eq!(
            CleanConventionSelectionV1::select(
                release,
                scope,
                Kind::DeclaredPackagePolicyCandidate
            ),
            Err(CleanConventionErrorV1::UnsupportedScope)
        );
    }
    for kind in [Kind::ProductionAuto, Kind::HumanAction] {
        assert_eq!(
            CleanConventionSelectionV1::select(release, Scope::ModeledSrgb8Point, kind),
            Err(CleanConventionErrorV1::UnsupportedAdmission)
        );
    }
}

/// Белый слой принят множеством, но его точная смесь [128,128,129] отклонена.
/// Хорошее TQ не компенсирует провал конвенции, никакая AUTH-ячейка не меняется.
#[test]
fn accepted_source_does_not_admit_rejected_terminal_composite() {
    let composite = Srgb8::new([128, 128, 129]);
    let attachment = ready(Srgb8::new([255; 3]), 0.5, Srgb8::new([1, 1, 3]), composite);
    let mut state = AuthorityStateV1::new();
    state
        .admit_modeled_point_technical_quality(&attachment, AuthorityExpectedCurrentV1::Vacant)
        .unwrap();
    let before = state;
    let result = state.admit_modeled_point_clean_convention(
        &attachment,
        Some(selection()),
        AuthorityExpectedCurrentV1::Vacant,
    );
    assert!(
        matches!(result, Err(CleanConventionErrorV1::RejectedByConvention { composite: got, .. }) if got == composite)
    );
    assert!(state == before);
    assert_eq!(state.read(AuthorityIdV1::CleanConvention), None);
}

/// Отклонённый исходный [128,128,129] после композиции становится нейтральным.
/// Reject-all и проверка source вместо final оба нарушают этот разрешённый случай.
#[test]
fn rejected_source_can_admit_accepted_terminal_composite_without_other_authorities() {
    let attachment = ready(
        Srgb8::new([128, 128, 129]),
        0.5,
        Srgb8::new([128, 128, 127]),
        Srgb8::new([128; 3]),
    );
    let mut state = AuthorityStateV1::new();
    let receipt = state
        .admit_modeled_point_clean_convention(
            &attachment,
            Some(selection()),
            AuthorityExpectedCurrentV1::Vacant,
        )
        .unwrap();
    assert_eq!(receipt.outcome(), AuthorityAdmissionOutcomeV1::Installed);
    assert_eq!(receipt.selection(), selection());
    assert_eq!(
        receipt.selection().admission(),
        CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate
    );
    assert_eq!(
        receipt.materialization().terminal_composite(),
        Srgb8::new([128; 3])
    );
    assert_eq!(
        receipt.materialization().renderer_provenance(),
        ProgramRendererProvenanceV1::Unverified
    );
    assert_eq!(receipt.descriptor().id(), AuthorityIdV1::CleanConvention);
    assert_eq!(
        state.read(AuthorityIdV1::CleanConvention),
        Some(receipt.descriptor())
    );
    assert_eq!(state.read(AuthorityIdV1::TechnicalQuality), None);
    assert_eq!(state.read(AuthorityIdV1::HumanCleanEvidence), None);
}

#[test]
fn missing_selection_and_non_ready_materialization_cannot_mint_authority() {
    let mut attachment = point_attachment_for(Srgb8::new([64; 3]), 0.5, Srgb8::new([96; 3]));
    let mut state = AuthorityStateV1::new();
    let vacant = AuthorityExpectedCurrentV1::Vacant;
    assert_eq!(
        state.admit_modeled_point_clean_convention(&attachment, Some(selection()), vacant),
        Err(CleanConventionErrorV1::Materialization(
            ProgramMaterializationAuthorityErrorV1::NotReady
        ))
    );
    observe(&mut attachment, 1, Srgb8::new([128; 3]));
    assert_eq!(
        state.admit_modeled_point_clean_convention(&attachment, None, vacant),
        Err(CleanConventionErrorV1::SelectionRequired)
    );
    let receipt = state
        .admit_modeled_point_clean_convention(&attachment, Some(selection()), vacant)
        .unwrap();
    let before = state;
    attachment.update_unknown(2, 1).unwrap();
    assert_eq!(
        state.admit_modeled_point_clean_convention(
            &attachment,
            Some(selection()),
            AuthorityExpectedCurrentV1::Exact(receipt.descriptor())
        ),
        Err(CleanConventionErrorV1::Materialization(
            ProgramMaterializationAuthorityErrorV1::NotReady
        ))
    );
    assert!(
        state == before,
        "отзыв материализации не превращает старую квитанцию в новый успех"
    );
    observe(&mut attachment, 3, Srgb8::new([128; 3]));
    let recovered = state
        .admit_modeled_point_clean_convention(
            &attachment,
            Some(selection()),
            AuthorityExpectedCurrentV1::Exact(receipt.descriptor()),
        )
        .unwrap();
    assert_eq!(recovered.outcome(), AuthorityAdmissionOutcomeV1::Replaced);
    let before_dispose = state;
    attachment.dispose(|| Ok::<(), ()>(())).unwrap();
    assert_eq!(
        state.admit_modeled_point_clean_convention(&attachment, Some(selection()), vacant),
        Err(CleanConventionErrorV1::Materialization(
            ProgramMaterializationAuthorityErrorV1::NotReady
        ))
    );
    assert!(state == before_dispose);
}

#[test]
fn current_revision_exact_cas_and_independent_authority_lanes_are_preserved() {
    let mut attachment = ready(
        Srgb8::new([64; 3]),
        0.5,
        Srgb8::new([128; 3]),
        Srgb8::new([96; 3]),
    );
    let mut state = AuthorityStateV1::new();
    state
        .admit_modeled_point_technical_quality(&attachment, AuthorityExpectedCurrentV1::Vacant)
        .unwrap();
    let tq = state.read(AuthorityIdV1::TechnicalQuality).unwrap();
    let first = state
        .admit_modeled_point_clean_convention(
            &attachment,
            Some(selection()),
            AuthorityExpectedCurrentV1::Vacant,
        )
        .unwrap();
    let duplicate = state
        .admit_modeled_point_clean_convention(
            &attachment,
            Some(selection()),
            AuthorityExpectedCurrentV1::Vacant,
        )
        .unwrap();
    assert_eq!(
        duplicate.outcome(),
        AuthorityAdmissionOutcomeV1::DuplicateNoop
    );
    assert_eq!(first.descriptor(), duplicate.descriptor());
    observe(&mut attachment, 2, Srgb8::new([128; 3]));
    let before = state;
    assert_eq!(
        state.admit_modeled_point_clean_convention(
            &attachment,
            Some(selection()),
            AuthorityExpectedCurrentV1::Vacant
        ),
        Err(CleanConventionErrorV1::Authority(
            AuthorityAdmissionErrorV1::ExpectedCurrentRequired
        ))
    );
    assert_eq!(
        state.admit_modeled_point_clean_convention(
            &attachment,
            Some(selection()),
            AuthorityExpectedCurrentV1::Exact(tq)
        ),
        Err(CleanConventionErrorV1::Authority(
            AuthorityAdmissionErrorV1::ExpectedAuthorityMismatch
        ))
    );
    assert!(state == before);
    let second = state
        .admit_modeled_point_clean_convention(
            &attachment,
            Some(selection()),
            AuthorityExpectedCurrentV1::Exact(first.descriptor()),
        )
        .unwrap();
    assert_eq!(second.outcome(), AuthorityAdmissionOutcomeV1::Replaced);
    assert_eq!(second.materialization().revision(), 2);
    assert_eq!(
        first.materialization().terminal_composite(),
        second.materialization().terminal_composite()
    );
    assert_eq!(
        first.descriptor().release_identity(),
        second.descriptor().release_identity()
    );
    assert_eq!(
        first.descriptor().applicability_identity(),
        second.descriptor().applicability_identity()
    );
    assert_ne!(
        first.descriptor().provenance_identity(),
        second.descriptor().provenance_identity()
    );
    observe(&mut attachment, 3, Srgb8::new([128; 3]));
    let before = state;
    assert_eq!(
        state.admit_modeled_point_clean_convention(
            &attachment,
            Some(selection()),
            AuthorityExpectedCurrentV1::Exact(first.descriptor())
        ),
        Err(CleanConventionErrorV1::Authority(
            AuthorityAdmissionErrorV1::ExpectedProvenanceMismatch
        ))
    );
    assert!(state == before);
    assert_eq!(state.read(AuthorityIdV1::TechnicalQuality), Some(tq));
    assert_eq!(state.read(AuthorityIdV1::HumanCleanEvidence), None);
}

#[test]
fn admission_is_allocation_free_and_does_not_change_the_attachment() {
    for (source, backdrop, composite, accepted) in [
        ([128, 128, 129], [128, 128, 127], [128; 3], true),
        ([255; 3], [1, 1, 3], [128, 128, 129], false),
    ] {
        let attachment = ready(
            Srgb8::new(source),
            0.5,
            Srgb8::new(backdrop),
            Srgb8::new(composite),
        );
        let before = attachment.current_render();
        let selected = Some(selection());
        let mut state = AuthorityStateV1::new();
        let (result, events) = measured_allocator_events(|| {
            state.admit_modeled_point_clean_convention(
                &attachment,
                selected,
                AuthorityExpectedCurrentV1::Vacant,
            )
        });
        assert_eq!(result.is_ok(), accepted);
        assert_eq!(events, AllocatorEvents::default());
        assert_eq!(attachment.current_render(), before);
    }
}

/// Независимая сборка объявленного бинарного конверта из читаемых полей.
/// Не вызывает сериализатор AUTH, поэтому потеря поля или pin меняет ожидаемый хеш.
#[test]
fn receipt_identity_binds_selection_technical_pin_and_every_materialization_coordinate() {
    let attachment = ready(
        Srgb8::new([128, 128, 129]),
        0.5,
        Srgb8::new([128, 128, 127]),
        Srgb8::new([128; 3]),
    );
    let receipt = AuthorityStateV1::new()
        .admit_modeled_point_clean_convention(
            &attachment,
            Some(selection()),
            AuthorityExpectedCurrentV1::Vacant,
        )
        .unwrap();
    let material = receipt.materialization();
    fn digest(bytes: &[u8]) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(bytes);
        *hasher.finalize().as_bytes()
    }
    let mut release = b"labcolors.cc.release.v1\0".to_vec();
    release.extend(EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1);
    release.push(1); // DeclaredPackagePolicyCandidate, не HumanAction.
    let release = digest(&release);
    let mut scope = b"labcolors.cc.applicability.v1\0".to_vec();
    scope.push(1); // modeled sRGB8 point.
    scope.extend(material.context().identity_bytes());
    scope.extend([1, 0]); // source-over, renderer Unverified.
    let scope = digest(&scope);
    let mut provenance = b"labcolors.cc.provenance.v1\0".to_vec();
    provenance.extend(release);
    provenance.extend(scope);
    provenance.extend(include_bytes!(
        "../../contracts/clean-set-srgb8-v1/receipt-v1.sha256"
    ));
    provenance.extend(b"modeled-point-v1\0");
    provenance.extend(material.content_identity());
    provenance.push(1);
    for value in [
        material.revision(),
        material.sink_stamp().sequence(),
        material.sink_stamp().binding_epoch(),
    ] {
        provenance.extend(value.to_be_bytes());
    }
    provenance.extend(material.presentation_root().to_be_bytes());
    provenance.extend(material.occurrence().to_be_bytes());
    provenance.extend(material.context().identity_bytes());
    provenance.push(0);
    provenance.extend(material.output().slot().to_be_bytes());
    provenance.extend(material.output().source().bytes());
    provenance.extend(material.output().opacity().to_bits().to_be_bytes());
    provenance.extend(material.terminal_composite().bytes());
    assert_eq!(receipt.descriptor().release_identity().as_bytes(), &release);
    assert_eq!(
        receipt.descriptor().applicability_identity().as_bytes(),
        &scope
    );
    assert_eq!(
        receipt.descriptor().provenance_identity().as_bytes(),
        &digest(&provenance)
    );
}
