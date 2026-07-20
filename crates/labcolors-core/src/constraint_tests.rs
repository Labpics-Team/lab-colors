use proptest::prelude::*;

use crate::Srgb8;
use crate::appearance::{
    AppearanceBindings, AppearanceGraphSpec, ColorInputId, CompositionProfileV1, OccurrenceId,
    OccurrenceSpec, OpacityInputId, PaintId, PaintSpec, PointOpacityOverSurfaceV1, SurfaceId,
    SurfaceInputPortId, SurfaceSpec, VisiblePointBindingV1,
};
use crate::constraints::{
    ApplicableWcag22EvaluationErrorV1, ApplicableWcag22MeasurementV1, ClassifiedMeasurement,
    Evaluator, ExactIdentityPassV1, ExactPassEvidenceV1, ExactSrgb8IdentityV1,
    ExactViolationEvidenceV1, HardDecision, VisiblePointPassEvidence,
    VisiblePointViolationEvidence, Wcag22PassV1, Wcag22Srgb8V1, Wcag22ViolationV1,
    assess_visible_point_hard,
};
use crate::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22CriterionV1, Wcag22ProfileIdV1,
    evaluate_wcag22_srgb8, wcag22_profile_v1,
};

fn point_occurrence(
    source: [u8; 3],
    opacity: f64,
    backdrop: [u8; 3],
) -> crate::appearance::ResolvedOccurrence {
    PointOpacityOverSurfaceV1::evaluate(source, opacity, backdrop)
        .unwrap_or_else(|error| panic!("valid point occurrence rejected: {}", error.message()))
}

type Wcag22HardResultV1 = Result<
    HardDecision<
        VisiblePointPassEvidence<Wcag22Srgb8V1>,
        VisiblePointViolationEvidence<Wcag22Srgb8V1>,
    >,
    ApplicableWcag22EvaluationErrorV1,
>;

fn wcag_outcome(
    source: [u8; 3],
    opacity: f64,
    backdrop: [u8; 3],
    criterion: Wcag22CriterionV1,
) -> Wcag22HardResultV1 {
    let occurrence = point_occurrence(source, opacity, backdrop);
    assess_visible_point_hard(&occurrence, &Wcag22Srgb8V1, criterion)
}

fn wcag_parts(
    outcome: &Wcag22HardResultV1,
) -> Result<
    (
        &ApplicableWcag22MeasurementV1,
        &VisiblePointBindingV1,
        &Wcag22CriterionV1,
        &Wcag22ProfileIdV1,
    ),
    &ApplicableWcag22EvaluationErrorV1,
> {
    match outcome {
        Ok(HardDecision::Pass(evidence)) => Ok((
            evidence.measurement().value(),
            evidence.binding(),
            evidence.invocation(),
            evidence.release(),
        )),
        Ok(HardDecision::Violation(evidence)) => Ok((
            evidence.measurement().value(),
            evidence.binding(),
            evidence.invocation(),
            evidence.release(),
        )),
        Err(error) => Err(error),
    }
}

#[test]
fn wcag_808080_on_white_is_a_typed_hard_violation() {
    let occurrence = point_occurrence([0x80; 3], 1.0, [0xFF; 3]);
    let Ok(HardDecision::Violation(violation)) = assess_visible_point_hard(
        &occurrence,
        &Wcag22Srgb8V1,
        Wcag22CriterionV1::Sc143TextDefault,
    ) else {
        panic!("#808080/#FFFFFF must violate SC 1.4.3 default text");
    };

    fn requires_wcag_violation(
        _: &ClassifiedMeasurement<ApplicableWcag22MeasurementV1, Wcag22ViolationV1>,
    ) {
    }
    let retained = violation.measurement().value();
    assert_eq!(retained.decision(), Wcag22ApplicableDecisionV1::Fail);
    assert_eq!(retained.measurement().foreground, [0x80; 3]);
    assert_eq!(retained.measurement().background, [0xFF; 3]);
    requires_wcag_violation(violation.measurement());
}

#[test]
fn wcag_black_on_white_is_a_typed_hard_pass() {
    let occurrence = point_occurrence([0x00; 3], 1.0, [0xFF; 3]);
    let Ok(HardDecision::Pass(pass)) = assess_visible_point_hard(
        &occurrence,
        &Wcag22Srgb8V1,
        Wcag22CriterionV1::Sc143TextDefault,
    ) else {
        panic!("#000000/#FFFFFF must pass SC 1.4.3 default text");
    };

    fn requires_wcag_pass(_: &ClassifiedMeasurement<ApplicableWcag22MeasurementV1, Wcag22PassV1>) {}
    assert_eq!(
        pass.measurement().value().decision(),
        Wcag22ApplicableDecisionV1::Pass
    );
    requires_wcag_pass(pass.measurement());
}

#[test]
fn wcag_pass_and_violation_payloads_are_type_incompatible() {
    assert_ne!(
        core::any::TypeId::of::<Wcag22PassV1>(),
        core::any::TypeId::of::<Wcag22ViolationV1>()
    );
}

#[test]
fn exact_mismatch_is_total_evaluation_and_constraint_violation() {
    let occurrence = point_occurrence([128, 128, 128], 1.0, [255, 255, 255]);
    let Ok(HardDecision::Violation(violation)) = assess_visible_point_hard(
        &occurrence,
        &ExactSrgb8IdentityV1,
        Srgb8::new([127, 128, 128]),
    ) else {
        panic!("exact mismatch was incorrectly classified as pass");
    };

    fn requires_violation(_: ExactViolationEvidenceV1) {}
    assert_eq!(violation.target(), Srgb8::new([127, 128, 128]));
    assert_eq!(violation.actual(), Srgb8::new([128; 3]));
    requires_violation(violation);
}

#[test]
fn exact_match_is_refined_to_the_distinct_pass_type() {
    let occurrence = point_occurrence([128, 128, 128], 1.0, [255, 255, 255]);
    let Ok(HardDecision::Pass(pass)) =
        assess_visible_point_hard(&occurrence, &ExactSrgb8IdentityV1, Srgb8::new([128; 3]))
    else {
        panic!("exact equality was incorrectly refined as a violation");
    };

    fn requires_pass(_: ExactPassEvidenceV1) {}
    fn requires_pass_classification(
        _: &crate::constraints::ClassifiedMeasurement<Srgb8, ExactIdentityPassV1>,
    ) {
    }
    assert_eq!(*pass.invocation(), Srgb8::new([128; 3]));
    assert_eq!(pass.binding().occurrence().output_rgb(), [128; 3]);
    requires_pass_classification(pass.measurement());
    requires_pass(pass);
}

#[test]
fn same_raw_808080_measurement_is_classified_only_against_the_invocation() {
    let occurrence = point_occurrence([0x80; 3], 1.0, [0xFF; 3]);
    let Ok(HardDecision::Pass(pass)) =
        assess_visible_point_hard(&occurrence, &ExactSrgb8IdentityV1, Srgb8::new([0x80; 3]))
    else {
        panic!("#808080 must pass its identical invocation");
    };
    let Ok(HardDecision::Violation(violation)) = assess_visible_point_hard(
        &occurrence,
        &ExactSrgb8IdentityV1,
        Srgb8::new([0x7F, 0x80, 0x80]),
    ) else {
        panic!("one-byte target mismatch must be a violation");
    };

    assert_eq!(pass.actual(), Srgb8::new([0x80; 3]));
    assert_eq!(violation.actual(), pass.actual());
    assert_ne!(violation.target(), pass.target());
}

#[test]
fn exact_evaluator_emits_only_raw_actual_measurement() {
    let occurrence = point_occurrence([0x80; 3], 1.0, [0xFF; 3]);
    let modeled = occurrence.modeled_srgb8_point();
    let first: Result<Srgb8, core::convert::Infallible> =
        ExactSrgb8IdentityV1.evaluate(&modeled, &Srgb8::new([0x80; 3]));
    let second: Result<Srgb8, core::convert::Infallible> =
        ExactSrgb8IdentityV1.evaluate(&modeled, &Srgb8::new([0x7F, 0x80, 0x80]));

    assert_eq!(first.unwrap(), Srgb8::new([0x80; 3]));
    assert_eq!(second.unwrap(), Srgb8::new([0x80; 3]));
}

#[test]
fn wcag_reads_final_visible_occurrence_in_measurement_order() {
    let report = wcag_outcome(
        [0, 0, 0],
        0.5,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextDefault,
    );

    let (measurement, binding, invocation, _) = wcag_parts(&report)
        .expect("proof-bound WCAG evaluator must measure every admitted sRGB8 pair");
    assert_eq!(measurement.measurement().foreground, [128, 128, 128]);
    assert_eq!(measurement.measurement().background, [255, 255, 255]);
    assert_eq!(measurement.decision(), Wcag22ApplicableDecisionV1::Fail);
    assert_eq!(invocation, &Wcag22CriterionV1::Sc143TextDefault);
    assert_eq!(
        binding.program_occurrence().occurrence(),
        OccurrenceId::new(0)
    );

    let subject_decision = evaluate_wcag22_srgb8(
        [0, 0, 0],
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextDefault,
    )
    .expect("control pair is in domain");
    assert!(matches!(
        subject_decision,
        Wcag22AssessmentV1::Evaluated {
            decision: Wcag22ApplicableDecisionV1::Pass,
            ..
        }
    ));
}

#[test]
fn modeled_target_preserves_chromatic_channel_order() {
    let occurrence = point_occurrence([0x12, 0x34, 0xAB], 1.0, [0xF0, 0x40, 0x20]);
    let target = occurrence.modeled_srgb8_point();

    assert_eq!(target.visible(), [0x12, 0x34, 0xAB]);
    assert_eq!(target.backdrop(), [0xF0, 0x40, 0x20]);
}

#[test]
fn modeled_target_has_no_binding_or_source_capability() {
    let source = include_str!("appearance.rs");
    let (_, target_tail) = source
        .split_once("pub(crate) struct ModeledSrgb8PointOccurrence {")
        .expect("modeled target declaration");
    let (target_surface, _) = target_tail
        .split_once("/// Physical identity modeled point-occurrence")
        .expect("binding declaration follows modeled target");

    for forbidden in [
        "ResolvedOccurrence",
        "VisiblePointBindingV1",
        "SourceOverCertificateV1",
        "fn binding",
        "certificate:",
    ] {
        assert!(
            !target_surface.contains(forbidden),
            "evaluator target leaked forbidden capability {forbidden}"
        );
    }
}

fn two_equal_physical_occurrences() -> [crate::appearance::ResolvedOccurrence; 2] {
    let source = ColorInputId::new(0);
    let backdrop = SurfaceInputPortId::new(1);
    let opacity = OpacityInputId::new(0);
    let solid = PaintId::new(0);
    let translucent = PaintId::new(1);
    let backdrop_surface = SurfaceId::new(0);
    let first_surface = SurfaceId::new(1);
    let second_surface = SurfaceId::new(2);
    let first = OccurrenceId::new(0);
    let second = OccurrenceId::new(1);
    let graph = AppearanceGraphSpec::new(
        vec![source],
        vec![backdrop],
        vec![opacity],
        vec![
            PaintSpec::Solid {
                id: solid,
                color: source,
            },
            PaintSpec::Opacity {
                id: translucent,
                source: solid,
                opacity,
            },
        ],
        vec![
            SurfaceSpec::Input {
                id: backdrop_surface,
                port: backdrop,
            },
            SurfaceSpec::FromOccurrence {
                id: first_surface,
                occurrence: first,
            },
            SurfaceSpec::FromOccurrence {
                id: second_surface,
                occurrence: second,
            },
        ],
        vec![
            OccurrenceSpec {
                id: first,
                subject: translucent,
                against: backdrop_surface,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
            OccurrenceSpec {
                id: second,
                subject: translucent,
                against: backdrop_surface,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
        ],
    )
    .compile()
    .expect("acyclic typed graph must compile");
    let evaluation = graph
        .evaluate(&AppearanceBindings::new(
            vec![(source, crate::Srgb8::new([0, 64, 255]))],
            vec![(backdrop, crate::Srgb8::new([255, 255, 255]))],
            vec![(opacity, 0.5)],
        ))
        .expect("complete bindings must evaluate");
    [
        *evaluation.occurrence(first).expect("first occurrence"),
        *evaluation.occurrence(second).expect("second occurrence"),
    ]
}

#[test]
fn equal_physics_under_distinct_occurrence_ids_keeps_distinct_bindings() {
    let [first, second] = two_equal_physical_occurrences();
    let first_report = assess_visible_point_hard(
        &first,
        &Wcag22Srgb8V1,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
    );
    let second_report = assess_visible_point_hard(
        &second,
        &Wcag22Srgb8V1,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
    );
    let (first_measurement, first_binding, _, _) =
        wcag_parts(&first_report).expect("first measurement failed");
    let (second_measurement, second_binding, _, _) =
        wcag_parts(&second_report).expect("second measurement failed");

    assert_eq!(first_measurement, second_measurement);
    assert_ne!(first_binding, second_binding);
}

#[test]
fn same_ids_and_final_pair_do_not_erase_subject_or_alpha_provenance() {
    let transparent_black = wcag_outcome(
        [0, 0, 0],
        0.0,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let opaque_white = wcag_outcome(
        [255, 255, 255],
        1.0,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let (transparent_measurement, transparent_binding, _, _) =
        wcag_parts(&transparent_black).expect("transparent occurrence must evaluate");
    let (opaque_measurement, opaque_binding, _, _) =
        wcag_parts(&opaque_white).expect("opaque occurrence must evaluate");

    assert_eq!(transparent_measurement, opaque_measurement);
    assert_ne!(transparent_binding, opaque_binding);
}

proptest! {
    #[test]
    fn bound_wcag_adapter_matches_standalone_final_pair(
        source in any::<[u8; 3]>(),
        backdrop in any::<[u8; 3]>(),
        opacity_step in 0_u16..=1024,
    ) {
        let opacity = f64::from(opacity_step) / 1024.0;
        let occurrence = point_occurrence(source, opacity, backdrop);
        let final_visible = occurrence.visible();
        let final_backdrop = occurrence.backdrop();
        let target = occurrence.modeled_srgb8_point();
        for criterion in Wcag22CriterionV1::ALL {
            let report = assess_visible_point_hard(&occurrence, &Wcag22Srgb8V1, criterion);
            let (bound, binding, invocation, release) = wcag_parts(&report)
                .map_err(|_| TestCaseError::fail("finite WCAG table rejected an admitted pair"))?;
            let standalone = evaluate_wcag22_srgb8(final_visible, final_backdrop, criterion)
                .expect("same admitted pair must be decided by standalone evaluator");
            let Wcag22AssessmentV1::Evaluated {
                profile_id,
                criterion: standalone_criterion,
                measurement,
                decision,
                evidence,
            } = standalone else {
                return Err(TestCaseError::fail("explicit criterion became report-only"));
            };

            prop_assert_eq!(target.visible(), final_visible);
            prop_assert_eq!(target.backdrop(), final_backdrop);
            prop_assert_eq!(bound.profile_id(), profile_id);
            prop_assert_eq!(bound.criterion(), standalone_criterion);
            prop_assert_eq!(bound.measurement(), &measurement);
            prop_assert_eq!(bound.decision(), decision);
            prop_assert_eq!(bound.evidence(), &evidence);
            prop_assert_eq!(invocation, &criterion);
            prop_assert_eq!(release, &profile_id);
            prop_assert_eq!(binding, &occurrence.visible_point_binding());
        }
    }
}

#[test]
fn required_criterion_is_not_replaced_by_one_hardcoded_threshold() {
    let text = wcag_outcome(
        [138, 138, 138],
        1.0,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let large_text = wcag_outcome(
        [138, 138, 138],
        1.0,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextLargeScale,
    );
    let (text, _, _, _) = wcag_parts(&text).expect("text measurement failed");
    let (large_text, _, _, _) = wcag_parts(&large_text).expect("large-text measurement failed");

    assert_eq!(text.decision(), Wcag22ApplicableDecisionV1::Fail);
    assert_eq!(large_text.decision(), Wcag22ApplicableDecisionV1::Pass);
}

#[test]
fn bound_report_release_matches_assessment_and_registry() {
    let report = wcag_outcome(
        [0, 0, 0],
        1.0,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let (measurement, _, _, release) =
        wcag_parts(&report).expect("required criterion must evaluate");

    assert_eq!(release, &measurement.profile_id());
    assert_eq!(*release, wcag22_profile_v1().profile_id);
}
