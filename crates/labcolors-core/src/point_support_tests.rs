use crate::Srgb8;
use crate::appearance::{
    EncodedPointPaintV1, EncodedPointPaintValueV1, OccurrenceId, PaintId,
    PhysicalProgramIdentityV1, SurfaceInputPortId,
};
use crate::composition::{AdmittedOpacityV1, CompositionProfileV1};
use crate::lcs_occurrence::ColorSignal;
use crate::observation::{
    OBSERVATION_ARENA_SLOT_COUNT_V1, ObservationPayloadInput, ObservationStreamId,
    ObservationUpdateInput, ObservedScenarioSetInput, Revision, ScenarioId, ScenarioInput,
    SurfaceInputBinding,
};
use crate::point_support::{
    CompiledPointSupportRecheckV1, PointSupportCompileErrorV1, PointSupportCriterionAggregateV1,
    PointSupportCriterionAssessmentV1, PointSupportCriterionRequirementV1,
    PointSupportDropFractionV1, PointSupportExactAggregateV1, PointSupportExactAssessmentV1,
    PointSupportOccurrenceRequirementV1, PointSupportStabilityAggregateV1,
    PointSupportStabilityAnchorV1, PointSupportStabilityAssessmentV1,
    PointSupportStabilityDecisionV1, PointSupportStabilityPolicyV1,
};
use crate::session::{Session, SessionPlanV1, SessionState};
use crate::session_tests::CommitSessionUpdateForTest as _;
use crate::wcag22::Wcag22CriterionV1;

const STREAM: ObservationStreamId = ObservationStreamId::new(31);
const SURFACE_A: SurfaceInputPortId = SurfaceInputPortId::new(21);
const SURFACE_B: SurfaceInputPortId = SurfaceInputPortId::new(22);
const OCCURRENCE_A: OccurrenceId = OccurrenceId::new(11);
const OCCURRENCE_B: OccurrenceId = OccurrenceId::new(12);
const PAINT_A: PaintId = PaintId::new(41);
const PAINT_B: PaintId = PaintId::new(42);

fn paint(id: PaintId, source: [u8; 3], opacity: f64) -> EncodedPointPaintV1 {
    EncodedPointPaintV1::from_value(
        id,
        EncodedPointPaintValueV1::from_admitted(
            Srgb8::new(source),
            AdmittedOpacityV1::new(opacity).unwrap(),
        ),
    )
}

fn occurrence(
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    paint: EncodedPointPaintV1,
    exact: Option<[u8; 3]>,
    criterion: PointSupportCriterionRequirementV1,
    stability: PointSupportStabilityPolicyV1,
) -> PointSupportOccurrenceRequirementV1 {
    PointSupportOccurrenceRequirementV1::new(
        occurrence,
        surface,
        paint,
        exact.map(Srgb8::new),
        criterion,
        stability,
    )
}

fn compiled(
    occurrences: Vec<PointSupportOccurrenceRequirementV1>,
) -> CompiledPointSupportRecheckV1 {
    CompiledPointSupportRecheckV1::new(CompositionProfileV1::EncodedSrgb8SourceOverV1, occurrences)
        .unwrap()
}

#[test]
fn point_support_observation_arenas_share_the_canonical_schema_for_the_session_lifetime() {
    let requirements = compiled(vec![occurrence(
        OCCURRENCE_A,
        SURFACE_A,
        paint(PAINT_A, [0; 3], 1.0),
        Some([0; 3]),
        PointSupportCriterionRequirementV1::NotRequested,
        PointSupportStabilityPolicyV1::Disabled,
    )]);

    assert_eq!(
        requirements.observation_schema(&()).strong_count_for_test(),
        1,
    );

    let schema_ptr = requirements.observation_schema(&()).backing_ptr_for_test();
    let mut session = Session::new(STREAM, requirements);
    let session_schema_handle_count = 1 + OBSERVATION_ARENA_SLOT_COUNT_V1;
    assert_eq!(
        session
            .plan()
            .observation_schema(&())
            .strong_count_for_test(),
        session_schema_handle_count,
    );

    let (report_schema_ptr, report_backing_ptr) = match session
        .commit(observed_update(1, [(1, vec![(SURFACE_A, [0; 3])])]))
        .unwrap()
    {
        SessionState::Ready { current } => (
            current.report().observation().schema_ptr_for_test(),
            current.report().observation().backing_ptr_for_test(),
        ),
        _ => panic!("the exact point-support requirement must verify"),
    };
    assert_eq!(report_schema_ptr, schema_ptr);
    assert_eq!(
        session
            .plan()
            .observation_schema(&())
            .strong_count_for_test(),
        session_schema_handle_count,
    );

    let idempotent_report_backing_ptr = match session
        .commit(observed_update(1, [(1, vec![(SURFACE_A, [0; 3])])]))
        .unwrap()
    {
        SessionState::Ready { current } => current.report().observation().backing_ptr_for_test(),
        _ => panic!("an exact replay must retain the verified report"),
    };
    assert_eq!(idempotent_report_backing_ptr, report_backing_ptr);
    assert_eq!(
        session
            .plan()
            .observation_schema(&())
            .strong_count_for_test(),
        session_schema_handle_count,
    );

    let observation_clone = match session.state() {
        SessionState::Ready { current } => current.report().observation().clone(),
        _ => panic!("the verified report must remain current"),
    };
    assert_eq!(observation_clone.schema_ptr_for_test(), schema_ptr);
    assert_eq!(
        session
            .plan()
            .observation_schema(&())
            .strong_count_for_test(),
        session_schema_handle_count,
        "cloning an observation shares its backing, not the schema Rc directly",
    );
    drop(observation_clone);

    let schema_probe = session.plan().observation_schema(&()).clone();
    assert_eq!(
        schema_probe.strong_count_for_test(),
        session_schema_handle_count + 1,
    );
    drop(session);
    assert_eq!(schema_probe.backing_ptr_for_test(), schema_ptr);
    assert_eq!(
        schema_probe.strong_count_for_test(),
        1,
        "dropping the Session must release its plan and all persistent observation arenas",
    );
}

fn observed_update(
    revision: u64,
    scenarios: impl IntoIterator<Item = (u32, Vec<(SurfaceInputPortId, [u8; 3])>)>,
) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: scenarios
                .into_iter()
                .map(|(id, bindings)| ScenarioInput {
                    id: ScenarioId::new(id),
                    bindings: bindings
                        .into_iter()
                        .map(|(port, value)| {
                            SurfaceInputBinding::new(
                                port,
                                ColorSignal::from_srgb8(Srgb8::new(value)),
                            )
                        })
                        .collect(),
                })
                .collect(),
        }),
    }
}

#[test]
fn multi_paint_declared_order_and_direct_provenance_are_preserved() {
    // Deliberately declare occurrence B before lower-ID occurrence A.
    let requirements = compiled(vec![
        occurrence(
            OCCURRENCE_B,
            SURFACE_B,
            paint(PAINT_B, [0; 3], 1.0),
            None,
            PointSupportCriterionRequirementV1::Required(Wcag22CriterionV1::Sc143TextDefault),
            PointSupportStabilityPolicyV1::Disabled,
        ),
        occurrence(
            OCCURRENCE_A,
            SURFACE_A,
            paint(PAINT_A, [255; 3], 1.0),
            None,
            PointSupportCriterionRequirementV1::Required(Wcag22CriterionV1::Sc143TextDefault),
            PointSupportStabilityPolicyV1::Disabled,
        ),
    ]);
    assert_eq!(requirements.surface_schema(), &[SURFACE_A, SURFACE_B]);
    assert_eq!(SURFACE_A.value(), 21);
    assert_eq!(OCCURRENCE_A.value(), 11);

    let mut session = Session::new(STREAM, requirements);
    let SessionState::Ready { current } = session
        .commit(observed_update(
            1,
            [(9, vec![(SURFACE_A, [0; 3]), (SURFACE_B, [255; 3])])],
        ))
        .unwrap()
    else {
        panic!("both independently required WCAG occurrences must pass");
    };
    let report = current.report();
    assert_eq!(
        report.exact_aggregate(),
        PointSupportExactAggregateV1::NotRequested
    );
    assert_eq!(
        report.criterion_aggregate(),
        PointSupportCriterionAggregateV1::AllRequiredPass
    );
    let cells: Vec<_> = report.cells().collect();
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].case_index(), 0);
    assert_eq!(cells[0].occurrence(), OCCURRENCE_B);
    assert_eq!(cells[0].occurrence_index(), 0);
    assert_eq!(cells[0].surface(), SURFACE_B);
    assert_eq!(cells[0].paint().id(), PAINT_B);
    assert_eq!(cells[0].composition().backdrop_rgb(), [255; 3]);
    assert_eq!(cells[1].occurrence(), OCCURRENCE_A);
    assert_eq!(cells[1].occurrence_index(), 1);
    assert_eq!(cells[1].surface(), SURFACE_A);
    assert_eq!(cells[1].paint().id(), PAINT_A);
    assert_eq!(cells[1].composition().backdrop_rgb(), [0; 3]);
    assert_eq!(cells[0].provenance(), &[ScenarioId::new(9)]);
    assert_eq!(cells[1].provenance(), &[ScenarioId::new(9)]);
}

#[test]
fn duplicate_raw_scenarios_share_one_physical_case_without_cartesian_expansion() {
    let requirements = compiled(vec![
        occurrence(
            OCCURRENCE_A,
            SURFACE_A,
            paint(PAINT_A, [0; 3], 0.5),
            Some([128; 3]),
            PointSupportCriterionRequirementV1::NotRequested,
            PointSupportStabilityPolicyV1::Disabled,
        ),
        occurrence(
            OCCURRENCE_B,
            SURFACE_B,
            paint(PAINT_B, [255; 3], 0.5),
            Some([128; 3]),
            PointSupportCriterionRequirementV1::NotRequested,
            PointSupportStabilityPolicyV1::Disabled,
        ),
    ]);
    let mut session = Session::new(STREAM, requirements);

    crate::composition::reset_source_over_evaluation_count();
    let SessionState::Failed { cause, previous } = session
        .commit(observed_update(
            1,
            [
                // Same complete physical tuple, deliberately repeated with
                // non-canonical IDs and binding order.
                (90, vec![(SURFACE_B, [0; 3]), (SURFACE_A, [255; 3])]),
                (10, vec![(SURFACE_A, [255; 3]), (SURFACE_B, [0; 3])]),
                // A second anti-correlated tuple must remain one whole case;
                // it must not be crossed with either value from the first.
                (50, vec![(SURFACE_B, [255; 3]), (SURFACE_A, [0; 3])]),
            ],
        ))
        .unwrap()
    else {
        panic!("the second physical case violates both required exact identities");
    };
    assert!(previous.is_none());

    let report = cause.report();
    assert_eq!(report.observation().physical_case_count(), 2);
    assert_eq!(
        report.observation().physical_values(0),
        Some(
            &[
                ColorSignal::from_srgb8(Srgb8::new([0; 3])),
                ColorSignal::from_srgb8(Srgb8::new([255; 3])),
            ][..]
        )
    );
    assert_eq!(
        report.observation().physical_values(1),
        Some(
            &[
                ColorSignal::from_srgb8(Srgb8::new([255; 3])),
                ColorSignal::from_srgb8(Srgb8::new([0; 3])),
            ][..]
        )
    );
    assert_eq!(
        report.observation().provenance(0),
        Some(&[ScenarioId::new(50)][..])
    );
    assert_eq!(
        report.observation().provenance(1),
        Some(&[ScenarioId::new(10), ScenarioId::new(90)][..])
    );

    let cells: Vec<_> = report.cells().collect();
    assert_eq!(
        cells.len(),
        4,
        "two cases times two occurrences, not six raw cells"
    );
    assert_eq!(
        crate::composition::source_over_evaluation_count(),
        4,
        "compose exactly once per (unique physical case, occurrence)"
    );

    assert_eq!(cells[0].case_index(), 0);
    assert_eq!(cells[0].occurrence(), OCCURRENCE_A);
    assert_eq!(cells[0].composition().backdrop_rgb(), [0; 3]);
    assert_eq!(cells[0].provenance(), &[ScenarioId::new(50)]);
    assert!(matches!(
        cells[0].exact(),
        PointSupportExactAssessmentV1::RequiredFailure(_)
    ));
    assert_eq!(cells[1].case_index(), 0);
    assert_eq!(cells[1].occurrence(), OCCURRENCE_B);
    assert_eq!(cells[1].composition().backdrop_rgb(), [255; 3]);
    assert_eq!(cells[1].provenance(), &[ScenarioId::new(50)]);
    assert!(matches!(
        cells[1].exact(),
        PointSupportExactAssessmentV1::RequiredFailure(_)
    ));

    for (cell, occurrence, backdrop) in [
        (cells[2], OCCURRENCE_A, [255; 3]),
        (cells[3], OCCURRENCE_B, [0; 3]),
    ] {
        assert_eq!(cell.case_index(), 1);
        assert_eq!(cell.occurrence(), occurrence);
        assert_eq!(cell.composition().backdrop_rgb(), backdrop);
        assert_eq!(
            cell.provenance(),
            &[ScenarioId::new(10), ScenarioId::new(90)]
        );
        assert!(matches!(
            cell.exact(),
            PointSupportExactAssessmentV1::RequiredPass(_)
        ));
    }
    assert_eq!(
        report.exact_aggregate(),
        PointSupportExactAggregateV1::RequiredFailure,
        "one unique violating case fails the whole recheck"
    );
}

#[test]
fn exact_wcag_and_stability_are_independent_axes_and_baseline_binds_once() {
    let drop_all = PointSupportDropFractionV1::try_from_basis_points(10_000).unwrap();
    crate::composition::reset_source_over_evaluation_count();
    let requirements = compiled(vec![occurrence(
        OCCURRENCE_A,
        SURFACE_A,
        paint(PAINT_A, [255; 3], 0.4),
        Some([255; 3]),
        PointSupportCriterionRequirementV1::Required(Wcag22CriterionV1::Sc1411UiComponentOrState),
        PointSupportStabilityPolicyV1::RetainBaselineReferenceSurplus {
            baseline_backdrop: Srgb8::new([0; 3]),
            anchor: PointSupportStabilityAnchorV1::Ratio3,
            drop_fraction: drop_all,
        },
    )]);
    assert_eq!(
        crate::composition::source_over_evaluation_count(),
        1,
        "the baseline is composed exactly once at compile/bind"
    );

    let mut session = Session::new(STREAM, requirements);
    let SessionState::Failed { cause, previous } = session
        .commit(observed_update(1, [(44, vec![(SURFACE_A, [255; 3])])]))
        .unwrap()
    else {
        panic!("WCAG and stability must fail even though exact identity passes");
    };
    assert!(previous.is_none());
    assert_eq!(crate::composition::source_over_evaluation_count(), 2);
    let report = cause.report();
    assert_eq!(
        report.exact_aggregate(),
        PointSupportExactAggregateV1::AllRequiredPass
    );
    assert_eq!(
        report.criterion_aggregate(),
        PointSupportCriterionAggregateV1::RequiredFailure
    );
    assert_eq!(
        report.stability_aggregate(),
        PointSupportStabilityAggregateV1::NotRetained
    );
    assert_eq!(
        report.composition_profile(),
        CompositionProfileV1::EncodedSrgb8SourceOverV1
    );
    assert_eq!(
        report.composition_profile().key(),
        "encoded-srgb8-source-over-v1"
    );
    assert_eq!(
        report.physical_program(),
        PhysicalProgramIdentityV1::InputOpacityOverSurfaceEncodedSrgb8V1
    );
    let cell = report.cells().next().unwrap();
    assert_eq!(cell.provenance(), &[ScenarioId::new(44)]);
    let exact = cell.exact();
    assert!(matches!(
        exact,
        PointSupportExactAssessmentV1::RequiredPass(_)
    ));
    assert_eq!(exact.invocation(), Some(Srgb8::new([255; 3])));
    assert_eq!(exact.actual(), Some(Srgb8::new([255; 3])));
    let PointSupportCriterionAssessmentV1::Required(criterion) = cell.criterion() else {
        panic!("required criterion evidence missing");
    };
    assert_eq!(
        criterion.criterion(),
        Wcag22CriterionV1::Sc1411UiComponentOrState
    );
    let PointSupportStabilityAssessmentV1::Evaluated(stability) = cell.stability() else {
        panic!("enabled stability evidence missing");
    };
    assert_eq!(
        stability.decision(),
        PointSupportStabilityDecisionV1::NotRetained
    );
    assert_eq!(stability.anchor(), PointSupportStabilityAnchorV1::Ratio3);
    assert_eq!(stability.drop_fraction(), drop_all);
    assert!(stability.current_surplus().numerator() < 0);
    assert_ne!(stability.current_surplus().denominator(), 0);
    assert_eq!(stability.baseline_composition().backdrop_rgb(), [0; 3]);
    assert_eq!(cell.composition().profile(), report.composition_profile());
    assert!(report.first_exact_failure().is_none());
    assert_eq!(
        report.first_required_failure().unwrap().occurrence(),
        OCCURRENCE_A
    );
    assert_eq!(
        report.first_stability_failure().unwrap().occurrence(),
        OCCURRENCE_A
    );

    session
        .commit(observed_update(2, [(45, vec![(SURFACE_A, [255; 3])])]))
        .unwrap();
    assert_eq!(
        crate::composition::source_over_evaluation_count(),
        3,
        "a new revision recomposes the current occurrence, never the baseline"
    );
}

#[test]
fn all_four_wcag_criterion_identities_survive_the_full_support_path() {
    let criteria = [
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22CriterionV1::Sc143TextLargeScale,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
        Wcag22CriterionV1::Sc1411GraphicalObject,
    ];
    let shared_paint = paint(PAINT_A, [0; 3], 1.0);
    let requirements = compiled(
        criteria
            .iter()
            .copied()
            .enumerate()
            .map(|(index, criterion)| {
                occurrence(
                    OccurrenceId::new(index as u32 + 1),
                    SURFACE_A,
                    shared_paint,
                    None,
                    PointSupportCriterionRequirementV1::Required(criterion),
                    PointSupportStabilityPolicyV1::Disabled,
                )
            })
            .collect(),
    );
    let mut session = Session::new(STREAM, requirements);
    let SessionState::Ready { current } = session
        .commit(observed_update(1, [(1, vec![(SURFACE_A, [255; 3])])]))
        .unwrap()
    else {
        panic!("black on white passes every admitted WCAG criterion");
    };
    let observed: Vec<_> = current
        .report()
        .cells()
        .map(|cell| {
            let PointSupportCriterionAssessmentV1::Required(assessment) = cell.criterion() else {
                panic!("required assessment missing");
            };
            assessment.criterion()
        })
        .collect();
    assert_eq!(observed, criteria);
}

#[test]
fn wholly_inactive_plan_is_rejected_but_an_inactive_composition_cell_is_allowed() {
    let inactive = occurrence(
        OCCURRENCE_A,
        SURFACE_A,
        paint(PAINT_A, [0; 3], 1.0),
        None,
        PointSupportCriterionRequirementV1::NotRequested,
        PointSupportStabilityPolicyV1::Disabled,
    );
    assert_eq!(
        CompiledPointSupportRecheckV1::new(
            CompositionProfileV1::EncodedSrgb8SourceOverV1,
            vec![inactive],
        ),
        Err(PointSupportCompileErrorV1::InactivePlan)
    );

    let active = occurrence(
        OCCURRENCE_A,
        SURFACE_A,
        paint(PAINT_A, [0; 3], 1.0),
        Some([0; 3]),
        PointSupportCriterionRequirementV1::NotRequested,
        PointSupportStabilityPolicyV1::Disabled,
    );
    let mixed = CompiledPointSupportRecheckV1::new(
        CompositionProfileV1::EncodedSrgb8SourceOverV1,
        vec![
            inactive,
            occurrence(
                OCCURRENCE_B,
                SURFACE_B,
                paint(PAINT_B, [255; 3], 1.0),
                Some([255; 3]),
                PointSupportCriterionRequirementV1::NotRequested,
                PointSupportStabilityPolicyV1::Disabled,
            ),
        ],
    )
    .expect("one active axis makes the whole full-support plan meaningful");
    assert_eq!(mixed.surface_schema(), &[SURFACE_A, SURFACE_B]);
    let mut mixed_session = Session::new(STREAM, mixed);
    let SessionState::Ready { current } = mixed_session
        .commit(observed_update(
            1,
            [(1, vec![(SURFACE_A, [17; 3]), (SURFACE_B, [3; 3])])],
        ))
        .unwrap()
    else {
        panic!("the active exact axis passes; the composition-only cell is not a failure");
    };
    let cells: Vec<_> = current.report().cells().collect();
    assert_eq!(cells.len(), 2);
    assert!(matches!(
        cells[0].exact(),
        PointSupportExactAssessmentV1::NotRequested
    ));
    assert_eq!(cells[0].composition().backdrop_rgb(), [17; 3]);

    assert_eq!(
        CompiledPointSupportRecheckV1::new(
            CompositionProfileV1::EncodedSrgb8SourceOverV1,
            vec![active, active],
        ),
        Err(PointSupportCompileErrorV1::DuplicateOccurrence(
            OCCURRENCE_A
        ))
    );

    let drifted_paint = occurrence(
        OCCURRENCE_B,
        SURFACE_B,
        paint(PAINT_A, [255; 3], 1.0),
        Some([255; 3]),
        PointSupportCriterionRequirementV1::NotRequested,
        PointSupportStabilityPolicyV1::Disabled,
    );
    assert_eq!(
        CompiledPointSupportRecheckV1::new(
            CompositionProfileV1::EncodedSrgb8SourceOverV1,
            vec![active, drifted_paint],
        ),
        Err(PointSupportCompileErrorV1::PaintDefinitionMismatch(PAINT_A))
    );
}

#[test]
fn drop_fraction_is_closed_and_exact() {
    let none = PointSupportDropFractionV1::try_from_basis_points(0).unwrap();
    assert_eq!(none, PointSupportDropFractionV1::NONE);
    assert_eq!(none.basis_points(), 0);
    let all = PointSupportDropFractionV1::try_from_basis_points(10_000).unwrap();
    assert_eq!(all, PointSupportDropFractionV1::ALL);
    assert_eq!(all.basis_points(), 10_000);
    assert_eq!(
        PointSupportDropFractionV1::try_from_basis_points(10_001),
        Err(PointSupportCompileErrorV1::DropFractionOutsideBasisPointRange)
    );
}

#[test]
fn every_stability_anchor_survives_compile_evaluate_and_typed_evidence() {
    let anchors = [
        PointSupportStabilityAnchorV1::Identity1,
        PointSupportStabilityAnchorV1::Ratio3,
        PointSupportStabilityAnchorV1::Ratio4Point5,
    ];
    let shared_paint = paint(PAINT_A, [0; 3], 1.0);
    let requirements = compiled(
        anchors
            .iter()
            .copied()
            .enumerate()
            .map(|(index, anchor)| {
                occurrence(
                    OccurrenceId::new(index as u32 + 100),
                    SURFACE_A,
                    shared_paint,
                    None,
                    PointSupportCriterionRequirementV1::NotRequested,
                    PointSupportStabilityPolicyV1::RetainBaselineReferenceSurplus {
                        baseline_backdrop: Srgb8::new([255; 3]),
                        anchor,
                        drop_fraction: PointSupportDropFractionV1::NONE,
                    },
                )
            })
            .collect(),
    );
    let mut session = Session::new(STREAM, requirements);
    let SessionState::Ready { current } = session
        .commit(observed_update(1, [(91, vec![(SURFACE_A, [255; 3])])]))
        .unwrap()
    else {
        panic!("unchanged black-on-white reference must retain every declared anchor");
    };
    assert_eq!(
        current.report().stability_aggregate(),
        PointSupportStabilityAggregateV1::AllRetained
    );
    let observed: Vec<_> = current
        .report()
        .cells()
        .map(|cell| {
            let PointSupportStabilityAssessmentV1::Evaluated(evidence) = cell.stability() else {
                panic!("every declared stability policy must produce evidence");
            };
            assert_eq!(
                evidence.decision(),
                PointSupportStabilityDecisionV1::Retained
            );
            evidence.anchor()
        })
        .collect();
    assert_eq!(observed, anchors);
}
