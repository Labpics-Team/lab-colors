use super::*;
use crate::authority::test_support::{Host, point_attachment_for, point_wire};
use crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1 as RELEASE;
use crate::program_wire::{ProgramScenarioV1, compile_program_wire_v1};

fn ready(source: [u8; 3], background: [u8; 3], expected: [u8; 3]) -> ProgramAttachmentV1<Host> {
    let mut value = point_attachment_for(Srgb8::new(source), 0.5, Srgb8::new(expected));
    value
        .update_observed(
            1,
            &[ProgramScenarioV1::new(1, vec![Srgb8::new(background)])],
        )
        .unwrap();
    value
}

#[test]
fn public_report_equals_direct_owner_chain_on_the_same_attachment() {
    let attachment = ready([128, 128, 129], [128, 128, 127], [128; 3]);
    let before = attachment.current_render();
    let report = evaluate_declared_point_v1(&attachment, RELEASE).unwrap();
    let selection = CleanConventionSelectionV1::select(
        RELEASE,
        CleanConventionScopeV1::ModeledSrgb8Point,
        CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate,
    )
    .unwrap();
    let mut authority = AuthorityStateV1::new();
    authority
        .admit_modeled_point_technical_quality(&attachment, AuthorityExpectedCurrentV1::Vacant)
        .unwrap();
    authority
        .admit_modeled_point_clean_convention(
            &attachment,
            Some(selection),
            AuthorityExpectedCurrentV1::Vacant,
        )
        .unwrap();
    let evaluation = authority
        .evaluate_declared_modeled_point(
            &attachment,
            Some(PointQualityProfileV1::declared_point(selection)),
        )
        .unwrap();
    let direct = DeclaredPointCertificateV1::issue(&evaluation).unwrap();
    assert_eq!(report.certificate_bytes(), direct.as_bytes());
    assert_eq!(report.terminal_srgb8().bytes(), [128; 3]);
    assert_eq!(report.subject_identity(), &evaluation.subject_identity());
    assert_eq!(report.profile_identity(), &evaluation.profile().identity());
    assert_eq!(
        report.program_identity(),
        &evaluation.materialization().content_identity()
    );
    assert_eq!(report.convention_release(), &RELEASE);
    assert_eq!(report.revision(), 1);
    assert_eq!(before, attachment.current_render());
}

#[test]
fn public_entry_checks_final_composite_and_never_defaults_the_release() {
    let rejected = ready([255; 3], [1, 1, 3], [128, 128, 129]);
    let before = rejected.current_render();
    let err = evaluate_declared_point_v1(&rejected, RELEASE).unwrap_err();
    assert_eq!(
        (err.kind(), err.domain(), err.code()),
        (
            PointEvaluationFailureKindV1::Evaluation,
            "clean-convention",
            "rejected_by_convention"
        )
    );
    assert_eq!(before, rejected.current_render());
    let accepted = ready([128, 128, 129], [128, 128, 127], [128; 3]);
    let mut wrong = RELEASE;
    wrong[17] ^= 1;
    let err = evaluate_declared_point_v1(&accepted, wrong).unwrap_err();
    assert_eq!(err.kind(), PointEvaluationFailureKindV1::Unsupported);
    assert_eq!(err.code(), "unsupported_convention_release");
    assert!(evaluate_declared_point_v1(&accepted, RELEASE).is_ok());
}

#[test]
fn unknown_and_revoked_heads_fail_then_new_revision_recovers_without_reusing_the_report() {
    let mut attachment = point_attachment_for(Srgb8::new([64; 3]), 0.5, Srgb8::new([96; 3]));
    assert_eq!(
        evaluate_declared_point_v1(&attachment, RELEASE)
            .unwrap_err()
            .code(),
        "materialization_not_ready"
    );
    attachment
        .update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([128; 3])])])
        .unwrap();
    let first = evaluate_declared_point_v1(&attachment, RELEASE).unwrap();
    attachment.update_unknown(2, 1).unwrap();
    assert_eq!(
        evaluate_declared_point_v1(&attachment, RELEASE)
            .unwrap_err()
            .code(),
        "materialization_not_ready"
    );
    attachment
        .update_observed(3, &[ProgramScenarioV1::new(1, vec![Srgb8::new([128; 3])])])
        .unwrap();
    let recovered = evaluate_declared_point_v1(&attachment, RELEASE).unwrap();
    assert_eq!(first.terminal_srgb8(), recovered.terminal_srgb8());
    assert_ne!(first.subject_identity(), recovered.subject_identity());
    assert_ne!(first.certificate_bytes(), recovered.certificate_bytes());
    assert_eq!(recovered.revision(), 3);
    attachment.dispose(|| Ok::<_, ()>(())).unwrap();
    assert!(evaluate_declared_point_v1(&attachment, RELEASE).is_err());
}

#[test]
fn contradictory_or_ambiguous_observations_cannot_be_replaced_by_a_plausible_sample() {
    let mut a = point_attachment_for(Srgb8::new([64; 3]), 0.5, Srgb8::new([96; 3]));
    a.update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([0; 3])])])
        .unwrap();
    assert!(evaluate_declared_point_v1(&a, RELEASE).is_err());
    let mut b = compile_program_wire_v1(&point_wire(Srgb8::new([64; 3]), 0.5, Srgb8::new([96; 3])))
        .unwrap()
        .attach(7, 17, 91, 9, 8, Host::default())
        .unwrap();
    let snapshot = b
        .update_observed(
            1,
            &[
                ProgramScenarioV1::new(1, vec![Srgb8::new([128; 3])]),
                ProgramScenarioV1::new(2, vec![Srgb8::new([0; 3])]),
            ],
        )
        .unwrap();
    assert_eq!(snapshot.render().unwrap().terminal_composite(), None);
    let error = evaluate_declared_point_v1(&b, RELEASE).unwrap_err();
    assert_eq!(
        (error.kind(), error.domain(), error.code()),
        (
            PointEvaluationFailureKindV1::Evaluation,
            "technical-quality",
            "ambiguous_observation_cases"
        )
    );
}

/// Командные примеры остаются каноническими графами реального компилятора.
#[test]
fn documented_inputs_match_the_canonical_graph_and_release() {
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    for (example, source, expected) in [
        (
            include_str!("../../labcolors-evaluate-cli/examples/declared-point.json"),
            [128, 128, 129],
            [128; 3],
        ),
        (
            include_str!("../../labcolors-evaluate-cli/examples/rejected-point.json"),
            [255; 3],
            [128, 128, 129],
        ),
    ] {
        let wire = hex(&point_wire(Srgb8::new(source), 0.5, Srgb8::new(expected)));
        assert!(example.contains(&format!("\"programWireHex\": \"{wire}\"")));
        assert!(example.contains(&hex(&RELEASE)));
    }
}
