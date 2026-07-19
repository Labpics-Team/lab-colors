use proptest::prelude::*;

use crate::appearance::{
    AppearanceBindings, AppearanceGraphSpec, ColorInputId, CompositionProfileV1, OccurrenceId,
    OccurrenceSpec, OpacityInputId, PaintId, PaintSpec, PointOpacityOverSurfaceV1, SurfaceId,
    SurfaceSpec,
};
use crate::constraints::{BoundAssessment, Wcag22Srgb8V1, assess};
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

fn wcag_assessment(
    source: [u8; 3],
    opacity: f64,
    backdrop: [u8; 3],
    criterion: Wcag22CriterionV1,
) -> BoundAssessment<crate::appearance::VisiblePointBindingV1, Wcag22ProfileIdV1, Wcag22AssessmentV1>
{
    let occurrence = point_occurrence(source, opacity, backdrop);
    assess(&occurrence, &Wcag22Srgb8V1, criterion)
        .expect("proof-bound WCAG evaluator must decide every admitted sRGB8 pair")
}

#[test]
fn wcag_reads_final_visible_occurrence_in_measurement_order() {
    let report = wcag_assessment(
        [0, 0, 0],
        0.5,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextDefault,
    );

    let Wcag22AssessmentV1::Evaluated {
        measurement,
        decision,
        ..
    } = report.assessment()
    else {
        panic!("required invocation cannot become NotEvaluated");
    };
    assert_eq!(measurement.foreground, [128, 128, 128]);
    assert_eq!(measurement.background, [255, 255, 255]);
    assert_eq!(*decision, Wcag22ApplicableDecisionV1::Fail);
    assert_eq!(
        report.binding().certificate().occurrence(),
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
    let backdrop = ColorInputId::new(1);
    let opacity = OpacityInputId::new(0);
    let solid = PaintId::new(0);
    let translucent = PaintId::new(1);
    let backdrop_surface = SurfaceId::new(0);
    let first_surface = SurfaceId::new(1);
    let second_surface = SurfaceId::new(2);
    let first = OccurrenceId::new(0);
    let second = OccurrenceId::new(1);
    let graph = AppearanceGraphSpec::new(
        vec![source, backdrop],
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
                color: backdrop,
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
            vec![(source, [0, 64, 255]), (backdrop, [255, 255, 255])],
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
    let first_report = assess(
        &first,
        &Wcag22Srgb8V1,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
    )
    .expect("first assessment");
    let second_report = assess(
        &second,
        &Wcag22Srgb8V1,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
    )
    .expect("second assessment");

    assert_eq!(first_report.assessment(), second_report.assessment());
    assert_ne!(first_report.binding(), second_report.binding());
}

#[test]
fn same_ids_and_final_pair_do_not_erase_subject_or_alpha_provenance() {
    let transparent_black = wcag_assessment(
        [0, 0, 0],
        0.0,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let opaque_white = wcag_assessment(
        [255, 255, 255],
        1.0,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextDefault,
    );

    assert_eq!(transparent_black.assessment(), opaque_white.assessment());
    assert_ne!(transparent_black.binding(), opaque_white.binding());
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
            let report = assess(&occurrence, &Wcag22Srgb8V1, criterion)
                .expect("finite WCAG table must decide every admitted pair");
            let standalone = evaluate_wcag22_srgb8(final_visible, final_backdrop, criterion)
                .expect("same admitted pair must be decided by standalone evaluator");

            prop_assert_eq!(target.visible(), final_visible);
            prop_assert_eq!(target.backdrop(), final_backdrop);
            prop_assert_eq!(report.assessment(), &standalone);
            prop_assert_eq!(report.binding(), &occurrence.visible_point_binding());
        }
    }
}

#[test]
fn required_criterion_is_not_replaced_by_one_hardcoded_threshold() {
    let text = wcag_assessment(
        [138, 138, 138],
        1.0,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let large_text = wcag_assessment(
        [138, 138, 138],
        1.0,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextLargeScale,
    );

    assert!(matches!(
        text.assessment(),
        Wcag22AssessmentV1::Evaluated {
            decision: Wcag22ApplicableDecisionV1::Fail,
            ..
        }
    ));
    assert!(matches!(
        large_text.assessment(),
        Wcag22AssessmentV1::Evaluated {
            decision: Wcag22ApplicableDecisionV1::Pass,
            ..
        }
    ));
}

#[test]
fn bound_report_release_matches_assessment_and_registry() {
    let report = wcag_assessment(
        [0, 0, 0],
        1.0,
        [255, 255, 255],
        Wcag22CriterionV1::Sc143TextDefault,
    );
    let Wcag22AssessmentV1::Evaluated { profile_id, .. } = report.assessment() else {
        panic!("required invocation cannot become NotEvaluated");
    };

    assert_eq!(report.release(), profile_id);
    assert_eq!(*report.release(), wcag22_profile_v1().profile_id);
}

#[test]
fn wcag_adapter_contains_delegation_not_a_second_formula() {
    let source = include_str!("constraints/wcag22.rs");
    assert_eq!(source.matches("evaluate_wcag22_srgb8(").count(), 1);
    for duplicated_math in ["4.5", "3.0", "luminance", "Q55", "powf"] {
        assert!(
            !source.contains(duplicated_math),
            "adapter duplicated WCAG math marker {duplicated_math}"
        );
    }
}
