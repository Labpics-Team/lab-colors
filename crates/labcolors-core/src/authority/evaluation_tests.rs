use super::super::clean_convention::{CleanConventionAdmissionKindV1, CleanConventionScopeV1};
use super::super::test_support::{Host, point_attachment_for};
use super::*;
use crate::Srgb8;
use crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2;
use crate::program_wire::{
    ProgramMaterializationAuthorityErrorV1, ProgramRendererProvenanceV1, ProgramScenarioV1,
};
use crate::test_support::{AllocatorEvents, measured_allocator_events};

fn profile() -> PointQualityProfileV1 {
    PointQualityProfileV1::declared_point(
        CleanConventionSelectionV1::select(
            EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2,
            CleanConventionScopeV1::ModeledSrgb8Point,
            CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate,
        )
        .unwrap(),
    )
}

fn observed(attachment: &mut ProgramAttachmentV1<Host>, revision: u64, backdrop: [u8; 3]) {
    attachment
        .update_observed(
            revision,
            &[ProgramScenarioV1::new(1, vec![Srgb8::new(backdrop)])],
        )
        .unwrap();
}

fn ready(source: [u8; 3], backdrop: [u8; 3], composite: [u8; 3]) -> ProgramAttachmentV1<Host> {
    let mut attachment = point_attachment_for(Srgb8::new(source), 0.5, Srgb8::new(composite));
    observed(&mut attachment, 1, backdrop);
    assert_eq!(
        attachment
            .current_materialization_authority()
            .unwrap()
            .terminal_composite(),
        Srgb8::new(composite)
    );
    attachment
}

fn neutral() -> ProgramAttachmentV1<Host> {
    ready([64; 3], [128; 3], [96; 3])
}

fn install_tq(state: &mut AuthorityStateV1, attachment: &ProgramAttachmentV1<Host>) {
    let expected = state.read(AuthorityIdV1::TechnicalQuality).map_or(
        AuthorityExpectedCurrentV1::Vacant,
        AuthorityExpectedCurrentV1::Exact,
    );
    state
        .admit_modeled_point_technical_quality(attachment, expected)
        .unwrap();
}

fn install_cc(state: &mut AuthorityStateV1, attachment: &ProgramAttachmentV1<Host>) {
    let expected = state.read(AuthorityIdV1::CleanConvention).map_or(
        AuthorityExpectedCurrentV1::Vacant,
        AuthorityExpectedCurrentV1::Exact,
    );
    state
        .admit_modeled_point_clean_convention(attachment, Some(profile().convention()), expected)
        .unwrap();
}

fn admitted(attachment: &ProgramAttachmentV1<Host>) -> AuthorityStateV1 {
    let mut state = AuthorityStateV1::new();
    install_tq(&mut state, attachment);
    install_cc(&mut state, attachment);
    state
}

#[test]
fn explicit_profile_and_both_installed_authorities_are_required() {
    let attachment = neutral();
    let full = admitted(&attachment);
    assert_eq!(
        full.evaluate_declared_modeled_point(&attachment, None),
        Err(PointEvaluationErrorV1::ProfileRequired)
    );
    for presence in [1_u8, 2, 0] {
        let mut state = AuthorityStateV1::new();
        if presence == 1 {
            install_tq(&mut state, &attachment);
        }
        if presence == 2 {
            install_cc(&mut state, &attachment);
        }
        let before = state;
        let missing = if presence == 1 {
            AuthorityIdV1::CleanConvention
        } else {
            AuthorityIdV1::TechnicalQuality
        };
        assert_eq!(
            state.evaluate_declared_modeled_point(&attachment, Some(profile())),
            Err(PointEvaluationErrorV1::RequiredAuthority {
                authority: missing,
                cause: AuthorityRequireErrorV1::Missing
            })
        );
        assert!(state == before);
    }
    // Полезный соседний случай не зависит от отсутствующего HCE.
    let result = full
        .evaluate_declared_modeled_point(&attachment, Some(profile()))
        .unwrap();
    assert_eq!(
        result.human_evidence(),
        HumanEvidenceEvaluationV1::NotRequested
    );
    assert_eq!(full.read(AuthorityIdV1::HumanCleanEvidence), None);
}

#[test]
fn result_preserves_one_subject_profile_and_distinct_current_branches() {
    let attachment = neutral();
    let state = admitted(&attachment);
    let selected = profile();
    let result = state
        .evaluate_declared_modeled_point(&attachment, Some(selected))
        .unwrap();
    assert_eq!(result.profile(), selected);
    assert_eq!(
        result.profile().convention().admission(),
        CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate
    );
    assert_eq!(
        result.materialization(),
        attachment.current_materialization_authority().unwrap()
    );
    assert_eq!(
        result.materialization().renderer_provenance(),
        ProgramRendererProvenanceV1::Unverified
    );
    assert_eq!(result.technical().id(), AuthorityIdV1::TechnicalQuality);
    assert_eq!(result.convention().id(), AuthorityIdV1::CleanConvention);
    assert_eq!(
        Some(result.technical()),
        state.read(AuthorityIdV1::TechnicalQuality)
    );
    assert_eq!(
        Some(result.convention()),
        state.read(AuthorityIdV1::CleanConvention)
    );
    // Кодирование выбранного закона проверяется отдельно от данных текущей точки.
    let mut expected = Hasher::new();
    expected.update(b"labcolors.quality-profile.declared-modeled-srgb8-point.v1\0");
    expected.update(&EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2);
    expected.update(&[1, 1]);
    assert_eq!(result.profile().identity(), *expected.finalize().as_bytes());
}

#[test]
fn same_rgb_new_revision_rejects_each_stale_branch_and_recovers() {
    let mut attachment = neutral();
    let original = admitted(&attachment);
    let first = original
        .evaluate_declared_modeled_point(&attachment, Some(profile()))
        .unwrap();
    let old_composite = first.materialization().terminal_composite();
    let profile_id = first.profile().identity();
    observed(&mut attachment, 2, [128; 3]);
    assert_eq!(
        attachment
            .current_materialization_authority()
            .unwrap()
            .terminal_composite(),
        old_composite
    );
    // Оба старых require ещё успешны. Они не доказывают новую оценку.
    assert!(
        original
            .require(original.read(AuthorityIdV1::TechnicalQuality).unwrap())
            .is_ok()
    );
    assert!(
        original
            .require(original.read(AuthorityIdV1::CleanConvention).unwrap())
            .is_ok()
    );
    assert!(
        original
            .evaluate_declared_modeled_point(&attachment, Some(profile()))
            .is_err()
    );
    let mut only_tq = original;
    install_tq(&mut only_tq, &attachment);
    assert_eq!(
        only_tq.evaluate_declared_modeled_point(&attachment, Some(profile())),
        Err(PointEvaluationErrorV1::RequiredAuthority {
            authority: AuthorityIdV1::CleanConvention,
            cause: AuthorityRequireErrorV1::ProvenanceMismatch
        })
    );
    let mut only_cc = original;
    install_cc(&mut only_cc, &attachment);
    assert!(matches!(
        only_cc.evaluate_declared_modeled_point(&attachment, Some(profile())),
        Err(PointEvaluationErrorV1::RequiredAuthority {
            authority: AuthorityIdV1::TechnicalQuality,
            ..
        })
    ));
    install_cc(&mut only_tq, &attachment);
    let recovered = only_tq
        .evaluate_declared_modeled_point(&attachment, Some(profile()))
        .unwrap();
    assert_eq!(recovered.materialization().revision(), 2);
    assert_eq!(recovered.profile().identity(), profile_id);
}

#[test]
fn equal_colors_from_different_attachments_cannot_be_joined() {
    let a = neutral();
    let b = neutral();
    let ma = a.current_materialization_authority().unwrap();
    let mb = b.current_materialization_authority().unwrap();
    assert_eq!(ma.content_identity(), mb.content_identity());
    assert_eq!(ma.terminal_composite(), mb.terminal_composite());
    assert_eq!(ma.revision(), mb.revision());
    assert_ne!(
        ma.sink_stamp().binding_epoch(),
        mb.sink_stamp().binding_epoch()
    );
    let mut mixed = AuthorityStateV1::new();
    install_tq(&mut mixed, &a);
    install_cc(&mut mixed, &b);
    assert!(matches!(
        mixed.evaluate_declared_modeled_point(&a, Some(profile())),
        Err(PointEvaluationErrorV1::RequiredAuthority {
            authority: AuthorityIdV1::CleanConvention,
            ..
        })
    ));
    assert!(matches!(
        mixed.evaluate_declared_modeled_point(&b, Some(profile())),
        Err(PointEvaluationErrorV1::RequiredAuthority {
            authority: AuthorityIdV1::TechnicalQuality,
            ..
        })
    ));
    // Перепроверка обеих ветвей одного настоящего владельца восстанавливает успех.
    install_cc(&mut mixed, &a);
    assert!(
        mixed
            .evaluate_declared_modeled_point(&a, Some(profile()))
            .is_ok()
    );
}

#[test]
fn revoked_and_disposed_subjects_refuse_saved_success_without_side_effects() {
    let mut attachment = neutral();
    let mut state = admitted(&attachment);
    let before = state;
    let first = state
        .evaluate_declared_modeled_point(&attachment, Some(profile()))
        .unwrap();
    let old_tq = first.technical();
    let old_cc = first.convention();
    attachment.update_unknown(2, 71).unwrap();
    assert!(state.require(old_tq).is_ok() && state.require(old_cc).is_ok());
    let refusal = Err(PointEvaluationErrorV1::Technical(
        TechnicalQualityAdmissionErrorV1::Materialization(
            ProgramMaterializationAuthorityErrorV1::NotReady,
        ),
    ));
    assert_eq!(
        state.evaluate_declared_modeled_point(&attachment, Some(profile())),
        refusal
    );
    assert!(state == before);
    observed(&mut attachment, 3, [128; 3]);
    install_tq(&mut state, &attachment);
    install_cc(&mut state, &attachment);
    assert!(
        state
            .evaluate_declared_modeled_point(&attachment, Some(profile()))
            .is_ok()
    );
    let before = state;
    attachment.dispose(|| Ok::<(), ()>(())).unwrap();
    assert_eq!(
        state.evaluate_declared_modeled_point(&attachment, Some(profile())),
        refusal
    );
    assert!(state == before);
}

#[test]
fn final_composition_not_source_decides_convention_and_rejection_is_atomic() {
    let accepted = ready([128, 128, 129], [128, 128, 127], [128; 3]);
    let state = admitted(&accepted);
    let result = state
        .evaluate_declared_modeled_point(&accepted, Some(profile()))
        .unwrap();
    assert_eq!(
        result.materialization().output().source(),
        Srgb8::new([128, 128, 129])
    );
    assert_eq!(
        result.materialization().terminal_composite(),
        Srgb8::new([128; 3])
    );
    let rejected = ready([255; 3], [1, 1, 3], [128, 128, 129]);
    let before = state;
    assert!(
        matches!(state.evaluate_declared_modeled_point(&rejected, Some(profile())),
        Err(PointEvaluationErrorV1::Convention(CleanConventionErrorV1::RejectedByConvention { composite, .. }))
        if composite == Srgb8::new([128, 128, 129]))
    );
    assert!(state == before);
}

#[test]
fn successful_and_failed_evaluations_allocate_nothing_and_leave_sources_unchanged() {
    let attachment = neutral();
    let full = admitted(&attachment);
    for state in [AuthorityStateV1::new(), full] {
        let before = state;
        let render = attachment.current_render();
        let selected = Some(profile());
        let (result, events) = measured_allocator_events(|| {
            state.evaluate_declared_modeled_point(&attachment, selected)
        });
        assert_eq!(result.is_ok(), state == full);
        assert_eq!(events, AllocatorEvents::default());
        assert!(state == before);
        assert_eq!(attachment.current_render(), render);
    }
}

/// Все последовательности трёх событий из пяти видов, с двумя исходными AUTH.
/// Модель содержит только текущую ревизию/доступность, не повторяет кодировщик
/// или классификатор. Каждое префиксное состояние проверяется реальным EVAL.
#[test]
fn bounded_histories_preserve_joint_freshness_and_recovery() {
    let mut histories = 0;
    for initially_admitted in [false, true] {
        for history in 0..125_u32 {
            let mut attachment = neutral();
            let mut state = if initially_admitted {
                admitted(&attachment)
            } else {
                AuthorityStateV1::new()
            };
            let mut revision = 1_u64;
            let mut available = true;
            let mut tq = initially_admitted.then_some(1);
            let mut cc = tq;
            let mut remaining = history;
            for _ in 0..3 {
                let event = remaining % 5;
                remaining /= 5;
                match event {
                    0 => {}
                    1 if available => {
                        install_tq(&mut state, &attachment);
                        tq = Some(revision);
                    }
                    2 if available => {
                        install_cc(&mut state, &attachment);
                        cc = Some(revision);
                    }
                    1 | 2 => {}
                    3 => {
                        revision += 1;
                        observed(&mut attachment, revision, [128; 3]);
                        available = true;
                    }
                    _ => {
                        revision += 1;
                        attachment.update_unknown(revision, 71).unwrap();
                        available = false;
                    }
                }
                let before = state;
                let render = attachment.current_render();
                let result = state.evaluate_declared_modeled_point(&attachment, Some(profile()));
                assert_eq!(
                    result.is_ok(),
                    available && tq == Some(revision) && cc == Some(revision),
                    "история {history}, начальный допуск {initially_admitted}, событие {event}"
                );
                if let Ok(result) = result {
                    assert_eq!(result.materialization().revision(), revision);
                    assert_eq!(
                        result.materialization().terminal_composite(),
                        Srgb8::new([96; 3])
                    );
                    assert_eq!(
                        result.human_evidence(),
                        HumanEvidenceEvaluationV1::NotRequested
                    );
                }
                assert!(state == before);
                assert_eq!(attachment.current_render(), render);
            }
            histories += 1;
        }
    }
    assert_eq!(histories, 250);
}
