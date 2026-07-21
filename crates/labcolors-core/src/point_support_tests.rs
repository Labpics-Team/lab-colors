use crate::point_support::{
    CompiledPointSupportPlanV1, PointSupportActionV1, PointSupportAdmissionErrorV1,
    PointSupportCriterionAggregateV1, PointSupportCriterionAssessmentV1,
    PointSupportCriterionRequirementV1, PointSupportDropFractionV1,
    PointSupportEvaluationErrorV1, PointSupportOccurrenceV1, PointSupportPlanErrorV1,
    PointSupportPlanRevisionV1, PointSupportStabilityAggregateV1,
    PointSupportStabilityAnchorV1, PointSupportStabilityAssessmentV1,
    PointSupportStabilityDecisionV1, PointSupportStabilityPolicyV1,
};
use crate::wcag22::{Wcag22ApplicableDecisionV1, Wcag22CriterionV1};
use crate::{
    OccurrenceId, ObservationSchemaV1, ObservationStreamId, ObservedScenarioSetInput, Revision,
    ScenarioId, ScenarioInput, Srgb8, SurfaceInputBinding, SurfaceInputPortId,
    admit_observation_snapshot_v1,
};

const STREAM: ObservationStreamId = ObservationStreamId::new(31);
const SURFACE: SurfaceInputPortId = SurfaceInputPortId::new(21);
const OCCURRENCE_A: OccurrenceId = OccurrenceId::new(11);
const OCCURRENCE_B: OccurrenceId = OccurrenceId::new(12);

fn schema() -> ObservationSchemaV1 {
    ObservationSchemaV1::try_new(vec![SURFACE]).unwrap()
}

fn observation(
    schema: &ObservationSchemaV1,
    revision: u64,
    samples: impl IntoIterator<Item = (u32, [u8; 3])>,
) -> crate::RevisionBoundObservationV1 {
    let scenarios = samples
        .into_iter()
        .map(|(id, backdrop)| {
            ScenarioInput::new(
                ScenarioId::new(id),
                vec![SurfaceInputBinding::new(SURFACE, Srgb8::new(backdrop))],
            )
        })
        .collect();
    admit_observation_snapshot_v1(
        schema.clone(),
        STREAM,
        Revision::new(revision),
        ObservedScenarioSetInput::new(scenarios),
    )
    .unwrap()
}

fn occurrence(
    id: OccurrenceId,
    source: [u8; 3],
    opacity: f64,
    criterion: PointSupportCriterionRequirementV1,
    baseline_backdrop: [u8; 3],
    anchor: PointSupportStabilityAnchorV1,
    drop_basis_points: u32,
) -> PointSupportOccurrenceV1 {
    PointSupportOccurrenceV1::try_new(
        id,
        SURFACE,
        Srgb8::new(source),
        opacity,
        criterion,
        PointSupportStabilityPolicyV1::RetainBaselineReferenceSurplus {
            baseline_backdrop: Srgb8::new(baseline_backdrop),
            anchor,
            drop_fraction: PointSupportDropFractionV1::try_from_basis_points(drop_basis_points)
                .unwrap(),
        },
    )
    .unwrap()
}

fn bound(
    schema: &ObservationSchemaV1,
    revision: u64,
    occurrences: Vec<PointSupportOccurrenceV1>,
) -> crate::BoundPointSupportPlanV1 {
    CompiledPointSupportPlanV1::try_new(
        PointSupportPlanRevisionV1::new(revision),
        occurrences,
    )
    .unwrap()
    .bind(schema)
    .unwrap()
}

#[test]
fn report_binds_plan_observation_occurrence_and_recomposed_alpha() {
    let schema = schema();
    let plan = bound(
        &schema,
        70,
        vec![occurrence(
            OCCURRENCE_A,
            [255; 3],
            0.4,
            PointSupportCriterionRequirementV1::Required(
                Wcag22CriterionV1::Sc1411UiComponentOrState,
            ),
            [0; 3],
            PointSupportStabilityAnchorV1::Ratio3,
            10_000,
        )],
    );
    let report = plan
        .evaluate(observation(&schema, 9, [(44, [255; 3])]))
        .unwrap();

    assert_eq!(report.plan_revision(), PointSupportPlanRevisionV1::new(70));
    assert_eq!(report.observation().stream(), STREAM);
    assert_eq!(report.observation().revision(), Revision::new(9));
    assert_eq!(
        report.criterion_aggregate(),
        PointSupportCriterionAggregateV1::RequiredFailure
    );
    assert_eq!(
        report.stability_aggregate(),
        PointSupportStabilityAggregateV1::NotRetained
    );
    assert_eq!(
        report.action(),
        PointSupportActionV1::ReconciliationRequired
    );

    let cell = &report.cells()[0];
    assert_eq!(cell.occurrence(), OCCURRENCE_A);
    assert_eq!(cell.surface(), SURFACE);
    assert_eq!(report.provenance(0), Some(&[ScenarioId::new(44)][..]));
    let composition = cell.composition();
    assert_eq!(composition.subject_rgb(), [255; 3]);
    assert_eq!(composition.subject_opacity(), 0.4);
    assert_eq!(composition.backdrop_rgb(), [255; 3]);
    assert_eq!(composition.output_rgb(), [255; 3]);
    assert_eq!(composition.replay(), composition.output_rgb());

    let PointSupportCriterionAssessmentV1::Required(criterion) = cell.criterion() else {
        panic!("required criterion assessment missing");
    };
    assert_eq!(
        criterion.criterion(),
        Wcag22CriterionV1::Sc1411UiComponentOrState
    );
    assert_eq!(criterion.decision(), Wcag22ApplicableDecisionV1::Fail);

    let PointSupportStabilityAssessmentV1::Evaluated(stability) = cell.stability() else {
        panic!("enabled stability evidence missing");
    };
    assert_eq!(stability.baseline_composition().output_rgb(), [0x66; 3]);
    assert_eq!(stability.baseline_composition().replay(), [0x66; 3]);
    assert_eq!(stability.current_measurement().foreground, [255; 3]);
    assert_eq!(stability.current_measurement().background, [255; 3]);
    assert_eq!(stability.anchor(), PointSupportStabilityAnchorV1::Ratio3);
    assert_eq!(stability.drop_fraction(), PointSupportDropFractionV1::ALL);
    assert!(stability.current_surplus().numerator() < 0);
    assert_eq!(
        stability.decision(),
        PointSupportStabilityDecisionV1::NotRetained
    );

    let vc = crate::spaces::vc::ViewingConditions::srgb();
    let wrong =
        crate::semantic::recheck_against_u32(0x00FF_FFFF, &[0x0066_6666], &vc).unwrap()[0].1;
    assert!(wrong > 3.0, "old composite-as-opaque must produce the opposite result");
}

#[test]
fn drop_endpoints_do_not_weaken_the_independent_required_criterion() {
    let schema = schema();
    let retain_all = bound(
        &schema,
        1,
        vec![occurrence(
            OCCURRENCE_A,
            [0; 3],
            1.0,
            PointSupportCriterionRequirementV1::Required(Wcag22CriterionV1::Sc143TextDefault),
            [255; 3],
            PointSupportStabilityAnchorV1::Ratio4Point5,
            0,
        )],
    );
    let retained_loss = retain_all
        .evaluate(observation(&schema, 1, [(1, [0xFE; 3])]))
        .unwrap();
    assert_eq!(
        retained_loss.criterion_aggregate(),
        PointSupportCriterionAggregateV1::AllRequiredPass
    );
    assert_eq!(
        retained_loss.stability_aggregate(),
        PointSupportStabilityAggregateV1::NotRetained
    );

    let allow_all = bound(
        &schema,
        2,
        vec![occurrence(
            OCCURRENCE_A,
            [0; 3],
            1.0,
            PointSupportCriterionRequirementV1::Required(Wcag22CriterionV1::Sc143TextDefault),
            [255; 3],
            PointSupportStabilityAnchorV1::Ratio4Point5,
            10_000,
        )],
    );
    let allowed = allow_all
        .evaluate(observation(&schema, 2, [(1, [0x76; 3])]))
        .unwrap();
    assert_eq!(
        allowed.criterion_aggregate(),
        PointSupportCriterionAggregateV1::AllRequiredPass
    );
    assert_eq!(
        allowed.stability_aggregate(),
        PointSupportStabilityAggregateV1::AllRetained
    );
    assert_eq!(
        allowed.action(),
        PointSupportActionV1::NoReconciliationRequired
    );

    let required_failure = allow_all
        .evaluate(observation(&schema, 3, [(1, [0x74; 3])]))
        .unwrap();
    assert_eq!(
        required_failure.criterion_aggregate(),
        PointSupportCriterionAggregateV1::RequiredFailure
    );
    assert_eq!(
        required_failure.action(),
        PointSupportActionV1::ReconciliationRequired
    );
}

#[test]
fn no_requested_policies_is_not_mislabeled_stable() {
    let schema = schema();
    let occurrence = PointSupportOccurrenceV1::try_new(
        OCCURRENCE_A,
        SURFACE,
        Srgb8::new([255, 0, 0]),
        0.0,
        PointSupportCriterionRequirementV1::NotRequested,
        PointSupportStabilityPolicyV1::Disabled,
    )
    .unwrap();
    let plan = bound(&schema, 1, vec![occurrence]);
    let report = plan
        .evaluate(observation(&schema, 1, [(1, [0x12, 0x34, 0x56])]))
        .unwrap();

    assert_eq!(
        report.criterion_aggregate(),
        PointSupportCriterionAggregateV1::NotRequested
    );
    assert_eq!(
        report.stability_aggregate(),
        PointSupportStabilityAggregateV1::Disabled
    );
    assert_eq!(
        report.action(),
        PointSupportActionV1::NoReconciliationRequired
    );
    assert!(report.primary_failure_cell().is_none());
    assert_eq!(
        report.cells()[0].criterion(),
        &PointSupportCriterionAssessmentV1::NotRequested
    );
    assert_eq!(
        report.cells()[0].stability(),
        PointSupportStabilityAssessmentV1::Disabled
    );
}

#[test]
fn stability_is_bit_identical_for_every_criterion_identity() {
    let schema = schema();
    let criteria = [
        PointSupportCriterionRequirementV1::NotRequested,
        PointSupportCriterionRequirementV1::Required(Wcag22CriterionV1::Sc143TextDefault),
        PointSupportCriterionRequirementV1::Required(Wcag22CriterionV1::Sc143TextLargeScale),
        PointSupportCriterionRequirementV1::Required(
            Wcag22CriterionV1::Sc1411UiComponentOrState,
        ),
        PointSupportCriterionRequirementV1::Required(
            Wcag22CriterionV1::Sc1411GraphicalObject,
        ),
    ];
    let occurrences = criteria
        .into_iter()
        .enumerate()
        .map(|(index, criterion)| {
            occurrence(
                OccurrenceId::new(index as u32 + 1),
                [0; 3],
                1.0,
                criterion,
                [255; 3],
                PointSupportStabilityAnchorV1::Identity1,
                5_000,
            )
        })
        .collect();
    let plan = bound(&schema, 1, occurrences);
    let report = plan
        .evaluate(observation(&schema, 1, [(1, [0x80; 3])]))
        .unwrap();

    let stability: Vec<_> = report
        .cells()
        .iter()
        .map(|cell| {
            let PointSupportStabilityAssessmentV1::Evaluated(evidence) = cell.stability() else {
                panic!("enabled stability missing");
            };
            evidence
        })
        .collect();
    assert!(stability.windows(2).all(|window| window[0] == window[1]));

    let exact: Vec<_> = report
        .cells()
        .iter()
        .skip(1)
        .map(|cell| {
            let PointSupportCriterionAssessmentV1::Required(assessment) = cell.criterion() else {
                panic!("required assessment missing");
            };
            assessment.criterion()
        })
        .collect();
    assert_eq!(
        exact,
        [
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22CriterionV1::Sc143TextLargeScale,
            Wcag22CriterionV1::Sc1411UiComponentOrState,
            Wcag22CriterionV1::Sc1411GraphicalObject,
        ]
    );
}

#[test]
fn declaration_and_schema_failures_precede_composition() {
    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        PointSupportDropFractionV1::try_from_basis_points(10_001),
        Err(PointSupportAdmissionErrorV1::DropFractionOutsideBasisPointRange)
    );
    assert_eq!(
        CompiledPointSupportPlanV1::try_new(PointSupportPlanRevisionV1::new(1), vec![]),
        Err(PointSupportPlanErrorV1::EmptyOccurrences)
    );

    let disabled = PointSupportOccurrenceV1::try_new(
        OCCURRENCE_A,
        SURFACE,
        Srgb8::new([0; 3]),
        1.0,
        PointSupportCriterionRequirementV1::NotRequested,
        PointSupportStabilityPolicyV1::Disabled,
    )
    .unwrap();
    assert_eq!(
        CompiledPointSupportPlanV1::try_new(
            PointSupportPlanRevisionV1::new(1),
            vec![disabled, disabled],
        ),
        Err(PointSupportPlanErrorV1::DuplicateOccurrence(OCCURRENCE_A))
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    let schema = schema();
    let plan = bound(&schema, 1, vec![disabled]);
    let other_surface = SurfaceInputPortId::new(99);
    let other_schema = ObservationSchemaV1::try_new(vec![other_surface]).unwrap();
    let other_observation = admit_observation_snapshot_v1(
        other_schema,
        STREAM,
        Revision::new(1),
        ObservedScenarioSetInput::new(vec![ScenarioInput::new(
            ScenarioId::new(1),
            vec![SurfaceInputBinding::new(
                other_surface,
                Srgb8::new([255; 3]),
            )],
        )]),
    )
    .unwrap();
    assert_eq!(
        plan.evaluate(other_observation),
        Err(PointSupportEvaluationErrorV1::ObservationSchemaMismatch)
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
}

#[test]
fn first_witness_uses_canonical_case_then_declared_occurrence_order() {
    let schema = schema();
    let plan = bound(
        &schema,
        1,
        vec![
            occurrence(
                OCCURRENCE_B,
                [255; 3],
                1.0,
                PointSupportCriterionRequirementV1::Required(
                    Wcag22CriterionV1::Sc143TextDefault,
                ),
                [0; 3],
                PointSupportStabilityAnchorV1::Ratio4Point5,
                10_000,
            ),
            occurrence(
                OCCURRENCE_A,
                [255; 3],
                1.0,
                PointSupportCriterionRequirementV1::Required(
                    Wcag22CriterionV1::Sc143TextDefault,
                ),
                [0; 3],
                PointSupportStabilityAnchorV1::Ratio4Point5,
                10_000,
            ),
        ],
    );
    let report = plan
        .evaluate(observation(
            &schema,
            1,
            [(9, [255; 3]), (3, [0xFE; 3])],
        ))
        .unwrap();

    let first = report.first_required_failure_cell().unwrap();
    assert!(core::ptr::eq(first, &report.cells()[0]));
    assert_eq!(first.occurrence_index(), 0);
    assert_eq!(first.occurrence(), OCCURRENCE_B);
    assert_eq!(report.provenance(0), Some(&[ScenarioId::new(3)][..]));
}
