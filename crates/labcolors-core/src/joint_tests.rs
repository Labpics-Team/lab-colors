use crate::Srgb8;
use crate::appearance::{EncodedPointPaintV1, PaintId, SurfaceInputPortId};
use crate::composition::AdmittedOpacityV1;
use crate::joint::{
    CandidateOrdinalV1, CandidateSetErrorV1, DeclaredTotalOrderV1, HardFeasibilityV1,
    JointCandidateSetV1, JointCandidateTupleV1, JointConstraintDecisionV1, JointConstraintIdV1,
    JointHardConstraintV1, JointPointProgramIdentityV1, JointPointProgramV1, JointProgramErrorV1,
    JointReportErrorV1, JointVisibleTargetV1, SelectionPolicyErrorV1, checked_joint_cardinality,
};
use crate::observation::{
    ObservationPayloadInput, ObservationSnapshot, ObservationState, ObservationStreamId,
    ObservationUpdateInput, ObservedScenarioSetInput, Revision, RevisionBoundObservationV1,
    ScenarioId, ScenarioInput, SurfaceInputBinding,
};

const ROOT: SurfaceInputPortId = SurfaceInputPortId::new(7);
const LOWER: PaintId = PaintId::new(11);
const UPPER: PaintId = PaintId::new(12);
const STREAM: ObservationStreamId = ObservationStreamId::new(3);

fn paint(id: PaintId, bytes: [u8; 3], opacity: f64) -> EncodedPointPaintV1 {
    EncodedPointPaintV1::from_admitted(
        id,
        Srgb8::new(bytes),
        AdmittedOpacityV1::new(opacity).unwrap(),
    )
}

fn candidate(ordinal: u32, lower: ([u8; 3], f64), upper: ([u8; 3], f64)) -> JointCandidateTupleV1 {
    JointCandidateTupleV1::new(
        CandidateOrdinalV1::new(ordinal),
        paint(LOWER, lower.0, lower.1),
        paint(UPPER, upper.0, upper.1),
    )
}

fn candidates(values: Vec<JointCandidateTupleV1>) -> JointCandidateSetV1 {
    JointCandidateSetV1::new(values).unwrap()
}

fn program(constraints: Vec<JointHardConstraintV1>) -> JointPointProgramV1 {
    JointPointProgramV1::new(ROOT, LOWER, UPPER, constraints).unwrap()
}

fn exact_upper(id: u32, target: [u8; 3]) -> JointHardConstraintV1 {
    JointHardConstraintV1::exact(
        JointConstraintIdV1::new(id),
        JointVisibleTargetV1::Upper,
        Srgb8::new(target),
    )
}

fn exact_lower(id: u32, target: [u8; 3]) -> JointHardConstraintV1 {
    JointHardConstraintV1::exact(
        JointConstraintIdV1::new(id),
        JointVisibleTargetV1::Lower,
        Srgb8::new(target),
    )
}

fn observation(revision: u64, cases: Vec<(u32, [u8; 3])>) -> RevisionBoundObservationV1 {
    let mut state = ObservationState::new(STREAM, vec![ROOT]).unwrap();
    state
        .apply(ObservationUpdateInput {
            stream: STREAM,
            revision: Revision::new(revision),
            payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
                scenarios: cases
                    .into_iter()
                    .map(|(id, value)| ScenarioInput {
                        id: ScenarioId::new(id),
                        bindings: vec![SurfaceInputBinding {
                            port: ROOT,
                            value: Srgb8::new(value),
                        }],
                    })
                    .collect(),
            }),
        })
        .unwrap();
    match state.snapshot() {
        ObservationSnapshot::Ready { observation } => observation,
        snapshot => panic!("expected Ready, got {snapshot:?}"),
    }
}

#[test]
fn linked_candidate_is_selected_only_after_upper_sees_lower_visible_surface() {
    let observed = observation(1, vec![(1, [0; 3])]);
    let domain = candidates(vec![
        candidate(0, ([0; 3], 1.0), ([255; 3], 0.5)),
        candidate(1, ([128; 3], 1.0), ([255; 3], 0.5)),
    ]);
    let report = program(vec![exact_upper(1, [192; 3])])
        .evaluate(domain, observed)
        .unwrap();

    assert_eq!(
        report.program_identity(),
        JointPointProgramIdentityV1::TwoPaintDerivedSurfaceExactPointV1
    );
    assert_eq!(report.executions().len(), 2);
    assert!(
        report
            .executions()
            .iter()
            .all(|execution| execution.derived_surface_is_exact())
    );
    assert_eq!(report.executions()[0].ordinal(), CandidateOrdinalV1::new(0));
    assert_eq!(report.executions()[0].case_index(), 0);
    assert_eq!(report.executions()[0].lower_paint().id(), LOWER);
    assert_eq!(report.executions()[0].upper_paint().id(), UPPER);
    assert_eq!(report.executions()[0].lower_visible(), Srgb8::new([0; 3]));
    assert_eq!(report.executions()[0].upper_visible(), Srgb8::new([128; 3]));
    assert_eq!(report.executions()[1].upper_visible(), Srgb8::new([192; 3]));
    assert_eq!(report.cells()[0].ordinal(), CandidateOrdinalV1::new(0));
    assert_eq!(report.cells()[0].constraint(), JointConstraintIdV1::new(1));
    assert_eq!(report.cells()[0].target_kind(), JointVisibleTargetV1::Upper);
    assert_eq!(report.cells()[0].case_index(), 0);
    assert_eq!(report.cells()[0].decision().target(), Srgb8::new([192; 3]));
    assert!(matches!(
        report.cells()[0].decision(),
        JointConstraintDecisionV1::Violation(_)
    ));
    assert!(matches!(
        report.cells()[1].decision(),
        JointConstraintDecisionV1::Pass(_)
    ));

    let HardFeasibilityV1::NonEmpty(feasible) = report.classify() else {
        panic!("second joint tuple must be feasible");
    };
    assert_eq!(feasible.feasible(), &[CandidateOrdinalV1::new(1)]);
    let policy = DeclaredTotalOrderV1::new(
        feasible.candidate_set(),
        vec![CandidateOrdinalV1::new(0), CandidateOrdinalV1::new(1)],
    )
    .unwrap();
    let selected = feasible.select(policy);
    assert_eq!(selected.ordinal(), CandidateOrdinalV1::new(1));
    let verified = selected.recheck().unwrap();
    assert_eq!(verified.ordinal(), CandidateOrdinalV1::new(1));
    assert_eq!(verified.fresh_executions().len(), 1);
    assert_eq!(verified.fresh_cells().len(), 1);
}

#[test]
fn every_unique_physical_case_must_pass_without_worst_or_average_reduction() {
    let observed = observation(2, vec![(1, [0; 3]), (2, [255; 3])]);
    let domain = candidates(vec![candidate(0, ([0; 3], 0.5), ([255; 3], 0.5))]);
    let report = program(vec![exact_upper(1, [128; 3])])
        .evaluate(domain, observed)
        .unwrap();

    assert_eq!(report.executions().len(), 2);
    assert_eq!(report.cells().len(), 2);
    assert_eq!(report.cells()[0].decision().actual(), Srgb8::new([128; 3]));
    assert_eq!(report.cells()[1].decision().actual(), Srgb8::new([192; 3]));
    let HardFeasibilityV1::Infeasible(report) = report.classify() else {
        panic!("one violated case must exclude the whole tuple");
    };
    assert_eq!(
        report
            .cells()
            .iter()
            .filter(|cell| !cell.decision().is_pass())
            .count(),
        1
    );
}

#[test]
fn full_report_does_not_short_circuit_after_first_violation() {
    let observed = observation(3, vec![(1, [0; 3])]);
    let domain = candidates(vec![candidate(0, ([0; 3], 1.0), ([255; 3], 0.5))]);
    crate::composition::reset_source_over_evaluation_count();
    let report = program(vec![
        exact_upper(1, [0; 3]),
        exact_upper(2, [128; 3]),
        exact_lower(3, [0; 3]),
    ])
    .evaluate(domain, observed)
    .unwrap();

    assert_eq!(crate::composition::source_over_evaluation_count(), 2);
    assert_eq!(report.executions().len(), 1);
    assert_eq!(report.cells().len(), 3);
    assert_eq!(
        report
            .cells()
            .iter()
            .filter(|cell| cell.decision().is_pass())
            .count(),
        2
    );
    assert_eq!(
        report
            .cells()
            .iter()
            .filter(|cell| !cell.decision().is_pass())
            .count(),
        1
    );
}

#[test]
fn candidate_and_constraint_declaration_permutations_are_canonical() {
    let observed = observation(4, vec![(1, [0; 3])]);
    let first = program(vec![exact_upper(9, [255; 3]), exact_lower(4, [0; 3])])
        .evaluate(
            candidates(vec![
                candidate(8, ([255; 3], 0.0), ([255; 3], 1.0)),
                candidate(2, ([0; 3], 1.0), ([255; 3], 1.0)),
            ]),
            observed.clone(),
        )
        .unwrap();
    let second = program(vec![exact_lower(4, [0; 3]), exact_upper(9, [255; 3])])
        .evaluate(
            candidates(vec![
                candidate(2, ([0; 3], 1.0), ([255; 3], 1.0)),
                candidate(8, ([255; 3], 0.0), ([255; 3], 1.0)),
            ]),
            observed,
        )
        .unwrap();

    assert_eq!(first, second);
}

#[test]
fn scenario_declaration_permutation_is_canonical() {
    let first = program(vec![exact_upper(1, [255; 3])])
        .evaluate(
            candidates(vec![candidate(0, ([17; 3], 0.5), ([255; 3], 1.0))]),
            observation(5, vec![(2, [255; 3]), (1, [0; 3])]),
        )
        .unwrap();
    let second = program(vec![exact_upper(1, [255; 3])])
        .evaluate(
            candidates(vec![candidate(0, ([17; 3], 0.5), ([255; 3], 1.0))]),
            observation(5, vec![(1, [0; 3]), (2, [255; 3])]),
        )
        .unwrap();

    assert_eq!(first, second);
}

#[test]
fn duplicate_provenance_does_not_repeat_physical_execution() {
    let observed = observation(6, vec![(9, [1, 2, 3]), (3, [1, 2, 3])]);
    let report = program(vec![exact_upper(1, [255; 3])])
        .evaluate(
            candidates(vec![candidate(0, ([0; 3], 1.0), ([255; 3], 1.0))]),
            observed,
        )
        .unwrap();

    assert_eq!(report.executions().len(), 1);
    assert_eq!(report.cells().len(), 1);
    assert_eq!(
        report.provenance(0).unwrap(),
        &[ScenarioId::new(3), ScenarioId::new(9)]
    );
}

#[test]
fn declared_policy_is_separate_from_report_and_is_the_only_tie_break() {
    let observed = observation(7, vec![(1, [0; 3])]);
    let make_report = || {
        program(vec![exact_upper(1, [42; 3])])
            .evaluate(
                candidates(vec![
                    candidate(7, ([1; 3], 1.0), ([42; 3], 1.0)),
                    candidate(4, ([250; 3], 1.0), ([42; 3], 1.0)),
                ]),
                observed.clone(),
            )
            .unwrap()
    };

    let HardFeasibilityV1::NonEmpty(first) = make_report().classify() else {
        panic!("both tuples must pass");
    };
    let first_policy = DeclaredTotalOrderV1::new(
        first.candidate_set(),
        vec![CandidateOrdinalV1::new(7), CandidateOrdinalV1::new(4)],
    )
    .unwrap();
    let HardFeasibilityV1::NonEmpty(second) = make_report().classify() else {
        panic!("both tuples must pass");
    };
    let second_policy = DeclaredTotalOrderV1::new(
        second.candidate_set(),
        vec![CandidateOrdinalV1::new(4), CandidateOrdinalV1::new(7)],
    )
    .unwrap();
    assert_eq!(
        first.select(first_policy).ordinal(),
        CandidateOrdinalV1::new(7)
    );
    assert_eq!(
        second.select(second_policy).ordinal(),
        CandidateOrdinalV1::new(4)
    );
}

#[test]
fn fresh_recheck_executes_the_selected_joint_program_again_on_the_same_revision() {
    let observed = observation(8, vec![(1, [0; 3]), (2, [255; 3])]);
    let report = program(vec![exact_upper(1, [17; 3])])
        .evaluate(
            candidates(vec![candidate(0, ([9; 3], 1.0), ([17; 3], 1.0))]),
            observed,
        )
        .unwrap();
    crate::composition::reset_source_over_evaluation_count();
    let HardFeasibilityV1::NonEmpty(feasible) = report.classify() else {
        panic!("opaque upper must pass on both roots");
    };
    let policy =
        DeclaredTotalOrderV1::new(feasible.candidate_set(), vec![CandidateOrdinalV1::new(0)])
            .unwrap();
    let selected = feasible.select(policy);
    let verified = selected.recheck().unwrap();

    assert_eq!(crate::composition::source_over_evaluation_count(), 4);
    assert_eq!(verified.report().observation().revision(), Revision::new(8));
    assert_eq!(verified.fresh_executions().len(), 2);
    assert_eq!(verified.fresh_cells().len(), 2);
    assert_eq!(verified.policy(), &[CandidateOrdinalV1::new(0)]);
}

#[test]
fn invalid_domains_and_policies_fail_before_compositing() {
    assert_eq!(
        JointCandidateSetV1::new(vec![]),
        Err(CandidateSetErrorV1::Empty)
    );
    assert_eq!(
        JointCandidateSetV1::new(vec![
            candidate(1, ([0; 3], 1.0), ([0; 3], 1.0)),
            candidate(1, ([1; 3], 1.0), ([1; 3], 1.0)),
        ]),
        Err(CandidateSetErrorV1::DuplicateOrdinal(
            CandidateOrdinalV1::new(1)
        ))
    );
    assert_eq!(
        JointCandidateSetV1::new(vec![
            candidate(1, ([0; 3], 1.0), ([0; 3], 1.0)),
            candidate(2, ([0; 3], 1.0), ([0; 3], 1.0)),
        ]),
        Err(CandidateSetErrorV1::DuplicatePhysicalTuple {
            first: CandidateOrdinalV1::new(1),
            second: CandidateOrdinalV1::new(2),
        })
    );
    assert_eq!(
        JointPointProgramV1::new(ROOT, LOWER, LOWER, vec![exact_upper(1, [0; 3])]),
        Err(JointProgramErrorV1::SamePaintIdentity(LOWER))
    );
    assert_eq!(
        JointPointProgramV1::new(ROOT, LOWER, UPPER, vec![]),
        Err(JointProgramErrorV1::EmptyHardConstraintSet)
    );

    let observed = observation(9, vec![(1, [0; 3])]);
    let wrong = JointCandidateSetV1::new(vec![JointCandidateTupleV1::new(
        CandidateOrdinalV1::new(0),
        paint(PaintId::new(999), [0; 3], 1.0),
        paint(UPPER, [0; 3], 1.0),
    )])
    .unwrap();
    crate::composition::reset_source_over_evaluation_count();
    assert!(matches!(
        program(vec![exact_upper(1, [0; 3])]).evaluate(wrong, observed),
        Err(JointReportErrorV1::CandidatePaintMismatch {
            stage: JointVisibleTargetV1::Lower,
            ..
        })
    ));
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    let domain = candidates(vec![candidate(0, ([0; 3], 1.0), ([0; 3], 1.0))]);
    assert_eq!(
        DeclaredTotalOrderV1::new(&domain, vec![]),
        Err(SelectionPolicyErrorV1::NotATotalOrder)
    );
    assert_eq!(
        DeclaredTotalOrderV1::new(
            &domain,
            vec![CandidateOrdinalV1::new(0), CandidateOrdinalV1::new(0)],
        ),
        Err(SelectionPolicyErrorV1::NotATotalOrder)
    );
}

#[test]
fn cardinality_overflow_is_rejected_by_preflight() {
    assert_eq!(
        checked_joint_cardinality(usize::MAX, 2, 1),
        Err(JointReportErrorV1::ResourceExhausted)
    );
    assert_eq!(
        checked_joint_cardinality(usize::MAX / 2 + 1, 2, 2),
        Err(JointReportErrorV1::ResourceExhausted)
    );
}
