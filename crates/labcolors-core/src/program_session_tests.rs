use std::cell::Cell;
use std::convert::Infallible;

use crate::Srgb8;
use crate::appearance::{
    ColorInputId, OccurrenceId, OpacityInputId, PaintId, SurfaceId, SurfaceInputPortId,
};
use crate::constraints::{
    ExactSrgb8IdentityV1, PointEvaluatorV1, PointInvocation, ProgramTestEvaluationErrorV1,
    ProgramTestEvaluatorV1, ProgramTestInvocationV1, Wcag22Srgb8V1, arm_program_test_failure_once,
    program_test_evaluation_count, reset_program_test_evaluation_count,
};
use crate::program_session::{
    ColorInput, CompiledProgram, CompositionProfile, ConstraintAssessment, ConstraintId,
    ConstraintInvocation, ConstraintOutcome, ConstraintReportEntry, ConstraintSet, HardModeV1,
    Occurrence, OpacityInput, OutputBinding, OutputSlotId, Paint, Program, ProgramCompileError,
    ReportModeV1, SessionState, SessionUpdateError, Surface, SurfaceSignal, SurfaceUpdate,
    canonical_surface_input_port_sequence_matches, check_render_node_count,
};
use crate::wcag22::{Wcag22ApplicableDecisionV1, Wcag22CriterionV1};

const COLOR: ColorInputId = ColorInputId::new(1);
const SURFACE_PORT: SurfaceInputPortId = SurfaceInputPortId::new(2);
const OPACITY: OpacityInputId = OpacityInputId::new(3);
const SOLID: PaintId = PaintId::new(10);
const TRANSLUCENT: PaintId = PaintId::new(11);
const BACKDROP: SurfaceId = SurfaceId::new(20);
const VISIBLE_SURFACE: SurfaceId = SurfaceId::new(21);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(30);
const OUTPUT: OutputSlotId = OutputSlotId::new(40);
const REQUIRED: ConstraintId = ConstraintId::new(50);

fn base_program<Evaluation>(
    opacity: f64,
    against: SurfaceId,
    constraints: ConstraintSet<PointInvocation<Evaluation>>,
    outputs: Vec<OutputBinding>,
    evaluator: Evaluation,
) -> Program<Evaluation>
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        vec![SURFACE_PORT],
        vec![OpacityInput::new(OPACITY, opacity)],
        vec![
            Paint::Solid {
                id: SOLID,
                color: COLOR,
            },
            Paint::Opacity {
                id: TRANSLUCENT,
                source: SOLID,
                opacity: OPACITY,
            },
        ],
        vec![
            Surface::Input {
                id: BACKDROP,
                input: SURFACE_PORT,
            },
            Surface::FromOccurrence {
                id: VISIBLE_SURFACE,
                occurrence: OCCURRENCE,
            },
        ],
        vec![Occurrence::new(
            OCCURRENCE,
            TRANSLUCENT,
            against,
            CompositionProfile::EncodedSrgb8SourceOverV1,
        )],
        constraints,
        outputs,
        evaluator,
    )
}

fn exact_program(
    hard: Vec<ConstraintInvocation<Srgb8, HardModeV1>>,
    report_only: Vec<ConstraintInvocation<Srgb8, ReportModeV1>>,
) -> Program<ExactSrgb8IdentityV1> {
    base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(hard, report_only),
        vec![OutputBinding::new(OUTPUT, TRANSLUCENT)],
        ExactSrgb8IdentityV1,
    )
}

fn exact_required(expected: Srgb8) -> Program<ExactSrgb8IdentityV1> {
    exact_program(
        vec![ConstraintInvocation::hard(REQUIRED, OCCURRENCE, expected)],
        vec![],
    )
}

fn compiled_exact(expected: Srgb8) -> CompiledProgram<ExactSrgb8IdentityV1> {
    exact_required(expected).compile().unwrap()
}

fn wcag_program(
    hard: Vec<ConstraintInvocation<Wcag22CriterionV1, HardModeV1>>,
    report_only: Vec<ConstraintInvocation<Wcag22CriterionV1, ReportModeV1>>,
) -> Program<Wcag22Srgb8V1> {
    base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(hard, report_only),
        vec![OutputBinding::new(OUTPUT, TRANSLUCENT)],
        Wcag22Srgb8V1,
    )
}

fn compile_error<Evaluation>(program: Program<Evaluation>) -> ProgramCompileError
where
    Evaluation: PointEvaluatorV1,
    PointInvocation<Evaluation>: Copy,
{
    match program.compile() {
        Ok(_) => panic!("invalid declaration compiled"),
        Err(error) => error,
    }
}

fn update_error<State, Error>(result: Result<State, Error>) -> Error {
    match result {
        Ok(_) => panic!("invalid update committed"),
        Err(error) => error,
    }
}

struct ReadProbe<const N: usize> {
    values: [Srgb8; N],
    reads: Cell<[usize; N]>,
}

impl<const N: usize> ReadProbe<N> {
    fn new(values: [Srgb8; N]) -> Self {
        Self {
            values,
            reads: Cell::new([0; N]),
        }
    }

    fn read(&self, index: usize) -> Srgb8 {
        let mut reads = self.reads.get();
        reads[index] += 1;
        self.reads.set(reads);
        self.values[index]
    }

    fn reads(&self) -> [usize; N] {
        self.reads.get()
    }
}

fn exact_outcome(
    assessment: &ConstraintAssessment<ExactSrgb8IdentityV1, HardModeV1>,
) -> (Srgb8, Srgb8) {
    match assessment.outcome() {
        ConstraintOutcome::Pass(evidence) => (evidence.target(), evidence.actual()),
        ConstraintOutcome::Violation(evidence) => (evidence.target(), evidence.actual()),
    }
}

#[test]
fn authored_modes_are_marker_typed_and_values_preserve_exact_ids() {
    let hard = ConstraintInvocation::hard(REQUIRED, OCCURRENCE, Srgb8::new([0x80; 3]));
    let report =
        ConstraintInvocation::report_only(ConstraintId::new(51), OCCURRENCE, Srgb8::new([0x81; 3]));
    let set = ConstraintSet::new(vec![hard], vec![report]);
    assert_eq!(set.hard()[0].id(), REQUIRED);
    assert_eq!(set.hard()[0].target(), OCCURRENCE);
    assert_eq!(*set.hard()[0].invocation(), Srgb8::new([0x80; 3]));
    assert_eq!(set.report_only()[0].id(), ConstraintId::new(51));

    let output = OutputBinding::new(OUTPUT, TRANSLUCENT);
    assert_eq!(output.output(), OUTPUT);
    assert_eq!(output.paint(), TRANSLUCENT);
    assert_eq!(ConstraintId::new(7).value(), 7);
    assert_eq!(OutputSlotId::new(8).value(), 8);

    let color = ColorInput::new(COLOR, Srgb8::new([1, 2, 3]));
    assert_eq!(color.id(), COLOR);
    assert_eq!(color.value(), Srgb8::new([1, 2, 3]));
    let opacity = OpacityInput::new(OPACITY, 0.375);
    assert_eq!(opacity.id(), OPACITY);
    assert_eq!(opacity.value(), 0.375);

    let occurrence = Occurrence::new(
        OCCURRENCE,
        TRANSLUCENT,
        BACKDROP,
        CompositionProfile::EncodedSrgb8SourceOverV1,
    );
    assert_eq!(occurrence.id(), OCCURRENCE);
    assert_eq!(occurrence.subject(), TRANSLUCENT);
    assert_eq!(occurrence.against(), BACKDROP);
    assert_eq!(
        occurrence.composition(),
        CompositionProfile::EncodedSrgb8SourceOverV1
    );

    let signal = SurfaceSignal::new(SURFACE_PORT, Srgb8::new([4, 5, 6]));
    assert_eq!(signal.input(), SURFACE_PORT);
    assert_eq!(signal.value(), Srgb8::new([4, 5, 6]));
}

#[test]
fn empty_domains_have_stable_precedence() {
    let empty_surface = Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        vec![],
        vec![],
        vec![Paint::Solid {
            id: SOLID,
            color: COLOR,
        }],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            SOLID,
            BACKDROP,
            CompositionProfile::EncodedSrgb8SourceOverV1,
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0; 3]),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, SOLID)],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(empty_surface),
        ProgramCompileError::EmptySurfaceSchema
    );

    let empty_occurrence = Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        vec![SURFACE_PORT],
        vec![],
        vec![Paint::Solid {
            id: SOLID,
            color: COLOR,
        }],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0; 3]),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, SOLID)],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(empty_occurrence),
        ProgramCompileError::EmptyOccurrenceSet
    );

    let empty_constraints = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::<Srgb8>::new(vec![], vec![]),
        vec![],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(empty_constraints),
        ProgramCompileError::EmptyConstraintSet
    );

    let empty_outputs = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
            vec![],
        ),
        vec![],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(empty_outputs),
        ProgramCompileError::EmptyOutputSet
    );
}

#[test]
fn physical_errors_precede_constraint_and_output_errors() {
    let missing_surface = SurfaceId::new(999);
    let duplicate = ConstraintId::new(77);
    let program = base_program(
        0.5,
        missing_surface,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                duplicate,
                OccurrenceId::new(998),
                Srgb8::new([0; 3]),
            )],
            vec![ConstraintInvocation::report_only(
                duplicate,
                OccurrenceId::new(997),
                Srgb8::new([0; 3]),
            )],
        ),
        vec![
            OutputBinding::new(OUTPUT, PaintId::new(996)),
            OutputBinding::new(OUTPUT, PaintId::new(995)),
        ],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(program),
        ProgramCompileError::MissingOccurrenceBackdrop {
            occurrence: OCCURRENCE,
            surface: missing_surface,
        }
    );
}

#[test]
fn constraint_and_output_error_precedence_is_canonical() {
    let duplicate = ConstraintId::new(77);
    let missing_occurrence = OccurrenceId::new(999);
    let duplicate_constraints = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                duplicate,
                missing_occurrence,
                Srgb8::new([0; 3]),
            )],
            vec![ConstraintInvocation::report_only(
                duplicate,
                OCCURRENCE,
                Srgb8::new([0; 3]),
            )],
        ),
        vec![
            OutputBinding::new(OUTPUT, PaintId::new(998)),
            OutputBinding::new(OUTPUT, TRANSLUCENT),
        ],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(duplicate_constraints),
        ProgramCompileError::DuplicateConstraint {
            constraint: duplicate,
        }
    );

    let missing_constraint = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                missing_occurrence,
                Srgb8::new([0; 3]),
            )],
            vec![],
        ),
        vec![
            OutputBinding::new(OUTPUT, TRANSLUCENT),
            OutputBinding::new(OUTPUT, PaintId::new(998)),
        ],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(missing_constraint),
        ProgramCompileError::MissingConstraintOccurrence {
            constraint: REQUIRED,
            occurrence: missing_occurrence,
        }
    );

    let duplicate_output = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
            vec![],
        ),
        vec![
            OutputBinding::new(OUTPUT, PaintId::new(998)),
            OutputBinding::new(OUTPUT, TRANSLUCENT),
        ],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(duplicate_output),
        ProgramCompileError::DuplicateOutputSlot { output: OUTPUT }
    );

    let missing_paint = PaintId::new(998);
    let missing_output = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, missing_paint)],
        ExactSrgb8IdentityV1,
    );
    assert_eq!(
        compile_error(missing_output),
        ProgramCompileError::MissingOutputPaint {
            output: OUTPUT,
            paint: missing_paint,
        }
    );
}

#[test]
fn compile_canonicalizes_constraints_and_outputs_independent_of_mode_lists() {
    let low = ConstraintId::new(1);
    let high = ConstraintId::new(9);
    let output_low = OutputSlotId::new(2);
    let output_high = OutputSlotId::new(8);
    let compiled = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                high,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
            vec![ConstraintInvocation::report_only(
                low,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
        ),
        vec![
            OutputBinding::new(output_high, TRANSLUCENT),
            OutputBinding::new(output_low, TRANSLUCENT),
        ],
        ExactSrgb8IdentityV1,
    )
    .compile()
    .unwrap();
    assert_eq!(compiled.surface_input_ports(), &[SURFACE_PORT]);
    assert_eq!(
        compiled.constraint_ids().collect::<Vec<_>>(),
        vec![low, high]
    );
    assert_eq!(
        compiled.outputs().collect::<Vec<_>>(),
        vec![(output_low, TRANSLUCENT), (output_high, TRANSLUCENT)]
    );
    let owner = compiled.into_owner();
    assert_eq!(
        owner.constraint_ids().unwrap().collect::<Vec<_>>(),
        vec![low, high]
    );
    assert_eq!(
        owner.outputs().unwrap().collect::<Vec<_>>(),
        vec![(output_low, TRANSLUCENT), (output_high, TRANSLUCENT)]
    );
}

#[test]
fn wcag_report_only_uses_visible_808080_but_emits_black_half_alpha_paint() {
    let criterion = Wcag22CriterionV1::Sc143TextDefault;
    let program = wcag_program(
        vec![],
        vec![ConstraintInvocation::report_only(
            REQUIRED, OCCURRENCE, criterion,
        )],
    );
    let owner = program.compile().unwrap().into_owner();
    let mut session = owner.attach().unwrap();
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
            .map(|_| ())
    });
    assert!(result.is_ok());
    assert_eq!(allocations, 0);
    let SessionState::Ready { current } = session.state() else {
        panic!("report-only violation must commit Ready");
    };
    assert!(
        current
            .surfaces()
            .eq([SurfaceSignal::new(SURFACE_PORT, Srgb8::new([0xff; 3]))])
    );

    let output = current.output(OUTPUT).expect("compiled output slot");
    assert_eq!(output.output(), OUTPUT);
    assert_eq!(output.paint(), TRANSLUCENT);
    assert_eq!(output.value().source(), Srgb8::new([0; 3]));
    assert_eq!(output.value().straight_alpha(), 0.5);
    assert_eq!(output.value().straight_alpha_bits(), 0.5f64.to_bits());

    let mut report = current.report();
    let Some(ConstraintReportEntry::ReportOnly(assessment)) = report.next() else {
        panic!("WCAG diagnostic must retain report-only mode");
    };
    assert_eq!(assessment.constraint(), REQUIRED);
    assert_eq!(assessment.target(), OCCURRENCE);
    let ConstraintOutcome::Violation(evidence) = assessment.outcome() else {
        panic!("#808080 on white must violate text-default WCAG");
    };
    let applicable = evidence.measurement().value();
    assert_eq!(applicable.criterion(), criterion);
    assert_eq!(applicable.decision(), Wcag22ApplicableDecisionV1::Fail);
    assert_eq!(applicable.measurement().foreground, [0x80; 3]);
    assert_eq!(applicable.measurement().background, [0xff; 3]);
    assert!(report.next().is_none());
}

#[test]
fn wcag_hard_violation_commits_conflict_with_full_report_and_no_current_output() {
    let program = wcag_program(
        vec![ConstraintInvocation::hard(
            REQUIRED,
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextDefault,
        )],
        vec![],
    );
    let owner = program.compile().unwrap().into_owner();
    let mut session = owner.attach().unwrap();
    let SessionState::Conflict { current, previous } = session
        .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap()
    else {
        panic!("mandatory WCAG violation must commit Conflict");
    };
    assert!(previous.is_none());
    assert_eq!(current.revision(), 1);
    assert!(
        current
            .surfaces()
            .eq([SurfaceSignal::new(SURFACE_PORT, Srgb8::new([0xff; 3]))])
    );
    let entries = current.report().collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    let ConstraintReportEntry::Hard(assessment) = entries[0] else {
        panic!("mandatory result lost its hard type");
    };
    assert!(matches!(
        assessment.outcome(),
        ConstraintOutcome::Violation(_)
    ));
}

#[test]
fn all_constraints_run_before_hard_gate_and_report_order_is_canonical() {
    let low_hard = ConstraintId::new(1);
    let middle_report = ConstraintId::new(2);
    let high_hard = ConstraintId::new(3);
    let program = exact_program(
        vec![
            ConstraintInvocation::hard(high_hard, OCCURRENCE, Srgb8::new([0x80; 3])),
            ConstraintInvocation::hard(low_hard, OCCURRENCE, Srgb8::new([0x81; 3])),
        ],
        vec![ConstraintInvocation::report_only(
            middle_report,
            OCCURRENCE,
            Srgb8::new([0x80; 3]),
        )],
    );
    let owner = program.compile().unwrap().into_owner();
    let mut session = owner.attach().unwrap();
    let SessionState::Conflict { current, .. } = session
        .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap()
    else {
        panic!("one hard violation must gate the complete result");
    };
    let entries = current.report().collect::<Vec<_>>();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.constraint())
            .collect::<Vec<_>>(),
        vec![low_hard, middle_report, high_hard]
    );
    assert!(entries.iter().all(|entry| entry.target() == OCCURRENCE));
    assert!(matches!(entries[0], ConstraintReportEntry::Hard(_)));
    assert!(matches!(entries[1], ConstraintReportEntry::ReportOnly(_)));
    assert!(matches!(entries[2], ConstraintReportEntry::Hard(_)));
    let ConstraintReportEntry::Hard(last) = entries[2] else {
        unreachable!()
    };
    assert_eq!(
        exact_outcome(last),
        (Srgb8::new([0x80; 3]), Srgb8::new([0x80; 3]))
    );
}

#[test]
fn report_only_violation_never_gates_terminal_paint() {
    let program = exact_program(
        vec![ConstraintInvocation::hard(
            REQUIRED,
            OCCURRENCE,
            Srgb8::new([0x80; 3]),
        )],
        vec![ConstraintInvocation::report_only(
            ConstraintId::new(51),
            OCCURRENCE,
            Srgb8::new([0x81; 3]),
        )],
    );
    let owner = program.compile().unwrap().into_owner();
    let mut session = owner.attach().unwrap();
    let SessionState::Ready { current } = session
        .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap()
    else {
        panic!("report-only violation cannot gate");
    };
    assert_eq!(current.outputs().count(), 1);
    assert!(matches!(
        current.report().nth(1),
        Some(ConstraintReportEntry::ReportOnly(assessment))
            if matches!(assessment.outcome(), ConstraintOutcome::Violation(_))
    ));
}

#[test]
fn output_slot_renaming_changes_routing_not_physical_paint() {
    fn resolve(output: OutputSlotId) -> crate::program_session::OutputValueV1 {
        let program = base_program(
            0.5,
            BACKDROP,
            ConstraintSet::new(
                vec![ConstraintInvocation::hard(
                    REQUIRED,
                    OCCURRENCE,
                    Srgb8::new([0x80; 3]),
                )],
                vec![],
            ),
            vec![OutputBinding::new(output, TRANSLUCENT)],
            ExactSrgb8IdentityV1,
        );
        let owner = program.compile().unwrap().into_owner();
        let mut session = owner.attach().unwrap();
        let SessionState::Ready { current } = session
            .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
            .unwrap()
        else {
            unreachable!()
        };
        current.outputs().next().unwrap()
    }

    let left = resolve(OutputSlotId::new(1));
    let renamed = resolve(OutputSlotId::new(999));
    assert_ne!(left.output(), renamed.output());
    assert_eq!(left.paint(), renamed.paint());
    assert_eq!(left.value(), renamed.value());
    assert_eq!(left.value().source(), Srgb8::new([0; 3]));
    assert_eq!(left.value().straight_alpha_bits(), 0.5f64.to_bits());
}

#[test]
fn evaluator_error_preserves_prior_snapshot_head_and_all_owned_evidence() {
    reset_program_test_evaluation_count();
    let first = ConstraintId::new(1);
    let failing = ConstraintId::new(2);
    let program = base_program(
        0.5,
        BACKDROP,
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                first,
                OCCURRENCE,
                ProgramTestInvocationV1::exact(Srgb8::new([0x80; 3])),
            )],
            vec![ConstraintInvocation::report_only(
                failing,
                OCCURRENCE,
                ProgramTestInvocationV1::fail_once_when_armed(Srgb8::new([0x80; 3])),
            )],
        ),
        vec![OutputBinding::new(OUTPUT, TRANSLUCENT)],
        ProgramTestEvaluatorV1,
    );
    let owner = program.compile().unwrap().into_owner();
    let mut session = owner.attach().unwrap();
    session
        .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap();
    let SessionState::Ready { current } = session.state() else {
        panic!("control update must be Ready");
    };
    let retained_storage = current.storage_pointers_for_test();
    let retained_output = current.output(OUTPUT);
    let retained_report_ids = current
        .report()
        .map(ConstraintReportEntry::constraint)
        .collect::<Vec<_>>();

    arm_program_test_failure_once();
    let error = update_error(session.update_canonical_present(2, 1, |_| Srgb8::new([0xff; 3])));
    assert_eq!(
        error,
        SessionUpdateError::Evaluator {
            constraint: failing,
            source: ProgramTestEvaluationErrorV1::Forced,
        }
    );
    let SessionState::Ready { current } = session.state() else {
        panic!("evaluator Err must leave the prior state exact");
    };
    assert_eq!(current.revision(), 1);
    assert_eq!(current.storage_pointers_for_test(), retained_storage);
    assert_eq!(current.output(OUTPUT), retained_output);
    assert_eq!(
        current
            .report()
            .map(ConstraintReportEntry::constraint)
            .collect::<Vec<_>>(),
        retained_report_ids
    );

    let SessionState::Ready { current } = session
        .update_canonical_present(2, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap()
    else {
        panic!("same incoming revision must be retryable after evaluator Err");
    };
    assert_eq!(current.revision(), 2);

    session
        .update_canonical_present(3, 1, |_| Srgb8::new([0; 3]))
        .unwrap();
    let SessionState::Conflict { current, previous } = session.state() else {
        panic!("black backdrop must create the retained Conflict control");
    };
    let retained_conflict = current.storage_pointers_for_test();
    let retained_previous = previous
        .as_ref()
        .expect("Conflict after Ready must retain full prior evidence")
        .storage_pointers_for_test();

    arm_program_test_failure_once();
    let error = update_error(session.update_canonical_present(4, 1, |_| Srgb8::new([0; 3])));
    assert_eq!(
        error,
        SessionUpdateError::Evaluator {
            constraint: failing,
            source: ProgramTestEvaluationErrorV1::Forced,
        }
    );
    let SessionState::Conflict { current, previous } = session.state() else {
        panic!("evaluator Err must preserve retained Conflict exactly");
    };
    assert_eq!(current.revision(), 3);
    assert_eq!(current.storage_pointers_for_test(), retained_conflict);
    assert_eq!(
        previous.as_ref().unwrap().storage_pointers_for_test(),
        retained_previous
    );

    let SessionState::Conflict { current, previous } = session
        .update_canonical_present(4, 1, |_| Srgb8::new([0; 3]))
        .unwrap()
    else {
        panic!("retry after evaluator Err must execute and commit Conflict");
    };
    assert_eq!(current.revision(), 4);
    assert_eq!(
        previous.as_ref().unwrap().storage_pointers_for_test(),
        retained_previous
    );
    assert_eq!(program_test_evaluation_count(), 12);
}

#[test]
fn three_frames_preserve_verified_and_replace_complete_conflict_reports_without_clone() {
    let owner = compiled_exact(Srgb8::new([0x80; 3])).into_owner();
    let mut session = owner.attach().unwrap();
    session
        .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap();
    let SessionState::Ready { current } = session.state() else {
        unreachable!()
    };
    let verified_storage = current.storage_pointers_for_test();

    session
        .update_canonical_present(2, 1, |_| Srgb8::new([0; 3]))
        .unwrap();
    let SessionState::Conflict { current, previous } = session.state() else {
        panic!("black backdrop must violate exact #808080");
    };
    let conflict_two_storage = current.storage_pointers_for_test();
    assert_ne!(conflict_two_storage, verified_storage);
    assert_eq!(
        previous
            .as_ref()
            .expect("Conflict retains prior verified evidence")
            .storage_pointers_for_test(),
        verified_storage
    );

    session
        .update_canonical_present(3, 1, |_| Srgb8::new([0x20; 3]))
        .unwrap();
    let SessionState::Conflict { current, previous } = session.state() else {
        unreachable!()
    };
    let conflict_three_storage = current.storage_pointers_for_test();
    assert_ne!(conflict_three_storage, verified_storage);
    assert_ne!(conflict_three_storage, conflict_two_storage);
    assert_eq!(
        previous
            .as_ref()
            .expect("new Conflict must retain the one verified witness")
            .storage_pointers_for_test(),
        verified_storage
    );
    assert_eq!(current.report().count(), 1);

    session
        .update_canonical_present(4, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap();
    let SessionState::Ready { current } = session.state() else {
        panic!("Conflict must recover directly to a new verified Snapshot");
    };
    let recovered_storage = current.storage_pointers_for_test();
    assert_ne!(recovered_storage, verified_storage);
    assert_ne!(recovered_storage, conflict_three_storage);

    session
        .update_canonical_present(5, 1, |_| Srgb8::new([0; 3]))
        .unwrap();
    let SessionState::Conflict { previous, .. } = session.state() else {
        unreachable!()
    };
    assert_eq!(
        previous.as_ref().unwrap().storage_pointers_for_test(),
        recovered_storage
    );

    let SessionState::Stale {
        previous,
        current_unavailable,
    } = session
        .update(SurfaceUpdate::Unavailable {
            revision: 6,
            reason: 9,
        })
        .unwrap()
    else {
        panic!("Unknown after Conflict(previous) must retain that full Snapshot");
    };
    assert_eq!(previous.storage_pointers_for_test(), recovered_storage);
    assert_eq!(current_unavailable.revision(), 6);
    assert_eq!(current_unavailable.reason(), 9);
}

#[test]
fn same_revision_conflict_replay_is_idempotent_and_changed_payload_conflicts() {
    let owner = compiled_exact(Srgb8::new([0x80; 3])).into_owner();
    let mut session = owner.attach().unwrap();
    let black = Srgb8::new([0; 3]);
    session.update_canonical_present(1, 1, |_| black).unwrap();
    let SessionState::Conflict { current, .. } = session.state() else {
        unreachable!()
    };
    let storage = current.storage_pointers_for_test();
    crate::composition::reset_source_over_evaluation_count();

    assert!(session.update_canonical_present(1, 1, |_| black).is_ok());
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    let SessionState::Conflict { current, .. } = session.state() else {
        unreachable!()
    };
    assert_eq!(current.storage_pointers_for_test(), storage);

    let error = update_error(session.update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3])));
    assert_eq!(
        error,
        SessionUpdateError::<Infallible>::RevisionConflict { revision: 1 }
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
}

#[test]
fn three_port_same_revision_conflict_reads_every_value_without_execution_or_allocation() {
    const PORT_A: SurfaceInputPortId = SurfaceInputPortId::new(10);
    const PORT_B: SurfaceInputPortId = SurfaceInputPortId::new(20);
    const PORT_C: SurfaceInputPortId = SurfaceInputPortId::new(30);
    const SURFACE_A: SurfaceId = SurfaceId::new(110);
    const SURFACE_B: SurfaceId = SurfaceId::new(120);
    const SURFACE_C: SurfaceId = SurfaceId::new(130);
    let program = Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        vec![PORT_C, PORT_A, PORT_B],
        vec![OpacityInput::new(OPACITY, 0.5)],
        vec![
            Paint::Solid {
                id: SOLID,
                color: COLOR,
            },
            Paint::Opacity {
                id: TRANSLUCENT,
                source: SOLID,
                opacity: OPACITY,
            },
        ],
        vec![
            Surface::Input {
                id: SURFACE_C,
                input: PORT_C,
            },
            Surface::Input {
                id: SURFACE_A,
                input: PORT_A,
            },
            Surface::Input {
                id: SURFACE_B,
                input: PORT_B,
            },
        ],
        vec![Occurrence::new(
            OCCURRENCE,
            TRANSLUCENT,
            SURFACE_B,
            CompositionProfile::EncodedSrgb8SourceOverV1,
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, TRANSLUCENT)],
        ExactSrgb8IdentityV1,
    );
    let compiled = program.compile().unwrap();
    assert_eq!(compiled.surface_input_ports(), &[PORT_A, PORT_B, PORT_C]);
    let owner = compiled.into_owner();
    let mut session = owner.attach().unwrap();
    let committed = [
        Srgb8::new([0x10; 3]),
        Srgb8::new([0; 3]),
        Srgb8::new([0x30; 3]),
    ];
    session
        .update_canonical_present(5, 3, |index| committed[index])
        .unwrap();
    let SessionState::Conflict { current, .. } = session.state() else {
        unreachable!()
    };
    let storage = current.storage_pointers_for_test();

    crate::composition::reset_source_over_evaluation_count();
    let replay = ReadProbe::new(committed);
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update_canonical_present(5, 3, |index| replay.read(index))
            .map(|_| ())
    });
    assert!(result.is_ok());
    assert_eq!(replay.reads(), [1, 1, 1]);
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    let SessionState::Conflict { current, .. } = session.state() else {
        unreachable!()
    };
    assert_eq!(current.storage_pointers_for_test(), storage);

    let mut mismatched = committed;
    mismatched[0] = Srgb8::new([0xff; 3]);
    let mismatch = ReadProbe::new(mismatched);
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update_canonical_present(5, 3, |index| mismatch.read(index))
            .map(|_| ())
    });
    assert_eq!(
        result,
        Err(SessionUpdateError::<Infallible>::RevisionConflict { revision: 5 })
    );
    assert_eq!(mismatch.reads(), [1, 1, 1]);
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    let SessionState::Conflict { current, .. } = session.state() else {
        unreachable!()
    };
    assert_eq!(current.storage_pointers_for_test(), storage);

    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update(SurfaceUpdate::Unavailable {
                revision: 5,
                reason: 91,
            })
            .map(|_| ())
    });
    assert_eq!(
        result,
        Err(SessionUpdateError::<Infallible>::RevisionConflict { revision: 5 })
    );
    assert_eq!(allocations, 0);
    let SessionState::Conflict { current, .. } = session.state() else {
        unreachable!()
    };
    assert_eq!(current.storage_pointers_for_test(), storage);
}

#[test]
fn typed_schema_revision_and_lifetime_failures_are_atomic() {
    let mut owner = compiled_exact(Srgb8::new([0x80; 3])).into_owner();
    let mut session = owner.attach().unwrap();
    let wrong = [SurfaceSignal::new(
        SurfaceInputPortId::new(999),
        Srgb8::new([0xff; 3]),
    )];
    crate::composition::reset_source_over_evaluation_count();
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update(SurfaceUpdate::Present {
                revision: 1,
                surfaces: &wrong,
            })
            .map(|_| ())
    });
    assert_eq!(
        result,
        Err(SessionUpdateError::<Infallible>::SurfaceInputPortMismatch {
            index: 0,
            expected: SURFACE_PORT,
            actual: SurfaceInputPortId::new(999),
        })
    );
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert!(matches!(
        session.state(),
        SessionState::Waiting {
            current_unavailable: None
        }
    ));

    session
        .update_canonical_present(5, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap();
    let SessionState::Ready { current } = session.state() else {
        unreachable!()
    };
    let retained_storage = current.storage_pointers_for_test();
    let reads = Cell::new(0);
    crate::composition::reset_source_over_evaluation_count();
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update_canonical_present(4, 1, |_| {
                reads.set(reads.get() + 1);
                Srgb8::new([0; 3])
            })
            .map(|_| ())
    });
    assert_eq!(
        result,
        Err(SessionUpdateError::<Infallible>::RevisionOutOfOrder {
            current: 5,
            incoming: 4,
        })
    );
    assert_eq!(reads.get(), 0);
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    let SessionState::Ready { current } = session.state() else {
        unreachable!()
    };
    assert_eq!(current.revision(), 5);
    assert_eq!(current.storage_pointers_for_test(), retained_storage);

    owner.dispose();
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update(SurfaceUpdate::Unavailable {
                revision: 6,
                reason: 1,
            })
            .map(|_| ())
    });
    assert_eq!(
        result,
        Err(SessionUpdateError::<Infallible>::ProgramExpired)
    );
    assert_eq!(allocations, 0);
}

#[test]
fn replacement_revokes_old_sessions_and_compile_error_keeps_live_epoch() {
    let mut owner = compiled_exact(Srgb8::new([0x80; 3])).into_owner();
    let mut old = owner.attach().unwrap();
    assert_eq!(
        compile_error(base_program(
            1.25,
            BACKDROP,
            ConstraintSet::new(
                vec![ConstraintInvocation::hard(
                    REQUIRED,
                    OCCURRENCE,
                    Srgb8::new([0x80; 3]),
                )],
                vec![],
            ),
            vec![OutputBinding::new(OUTPUT, TRANSLUCENT)],
            ExactSrgb8IdentityV1,
        )),
        ProgramCompileError::OpacityOutOfDomain { input: OPACITY }
    );
    assert!(
        old.update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
            .is_ok()
    );

    owner.replace(compiled_exact(Srgb8::new([0x80; 3])));
    assert_eq!(
        update_error(old.update_canonical_present(2, 1, |_| Srgb8::new([0xff; 3]))),
        SessionUpdateError::<Infallible>::ProgramExpired
    );
}

#[test]
fn present_ready_conflict_stale_and_recovery_allocate_zero_after_attach() {
    let owner = compiled_exact(Srgb8::new([0x80; 3])).into_owner();
    let mut session = owner.attach().unwrap();
    let white = [SurfaceSignal::new(SURFACE_PORT, Srgb8::new([0xff; 3]))];
    let black = [SurfaceSignal::new(SURFACE_PORT, Srgb8::new([0; 3]))];

    for update in [
        SurfaceUpdate::Present {
            revision: 1,
            surfaces: &white,
        },
        SurfaceUpdate::Present {
            revision: 2,
            surfaces: &black,
        },
        SurfaceUpdate::Unavailable {
            revision: 3,
            reason: 7,
        },
        SurfaceUpdate::Present {
            revision: 4,
            surfaces: &white,
        },
    ] {
        let (result, allocations) =
            crate::test_support::measured_allocations(|| session.update(update).map(|_| ()));
        assert!(result.is_ok());
        assert_eq!(allocations, 0);
    }
}

#[test]
fn canonical_helpers_and_checked_cardinality_fail_closed() {
    assert_eq!(check_render_node_count(usize::MAX - 1, 1), Ok(()));
    assert_eq!(
        check_render_node_count(usize::MAX, 1),
        Err(ProgramCompileError::ResourceExhausted)
    );
    let canonical = [SurfaceInputPortId::new(1), SurfaceInputPortId::new(2)];
    assert!(canonical_surface_input_port_sequence_matches(
        [SurfaceInputPortId::new(1), SurfaceInputPortId::new(2)],
        &canonical,
    ));
    assert!(!canonical_surface_input_port_sequence_matches(
        [SurfaceInputPortId::new(2), SurfaceInputPortId::new(1)],
        &canonical,
    ));
    assert!(!canonical_surface_input_port_sequence_matches(
        [SurfaceInputPortId::new(1)],
        &canonical,
    ));
}

#[test]
fn callback_is_not_read_before_lifetime_cardinality_or_revision_admission() {
    let mut expired = {
        let owner = compiled_exact(Srgb8::new([0x80; 3])).into_owner();
        owner.attach().unwrap()
    };
    let reads = Cell::new(0);
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        expired
            .update_canonical_present(1, 1, |_| {
                reads.set(reads.get() + 1);
                Srgb8::new([0xff; 3])
            })
            .map(|_| ())
    });
    assert_eq!(
        result,
        Err(SessionUpdateError::<Infallible>::ProgramExpired)
    );
    assert_eq!(reads.get(), 0);
    assert_eq!(allocations, 0);

    let owner = compiled_exact(Srgb8::new([0x80; 3])).into_owner();
    let mut session = owner.attach().unwrap();
    for actual in [0, 2] {
        let reads = Cell::new(0);
        let (result, allocations) = crate::test_support::measured_allocations(|| {
            session
                .update_canonical_present(1, actual, |_| {
                    reads.set(reads.get() + 1);
                    Srgb8::new([0xff; 3])
                })
                .map(|_| ())
        });
        assert_eq!(
            result,
            Err(
                SessionUpdateError::<Infallible>::SurfaceInputPortLengthMismatch {
                    expected: 1,
                    actual,
                }
            )
        );
        assert_eq!(reads.get(), 0);
        assert_eq!(allocations, 0);
    }

    session
        .update(SurfaceUpdate::Unavailable {
            revision: 7,
            reason: 12,
        })
        .unwrap();
    let reads = Cell::new(0);
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update_canonical_present(7, 1, |_| {
                reads.set(reads.get() + 1);
                Srgb8::new([0xff; 3])
            })
            .map(|_| ())
    });
    assert_eq!(
        result,
        Err(SessionUpdateError::<Infallible>::RevisionConflict { revision: 7 })
    );
    assert_eq!(reads.get(), 0);
    assert_eq!(allocations, 0);
}

#[test]
fn same_paint_can_be_assessed_in_multiple_occurrences_but_is_one_terminal_output() {
    const OTHER_PORT: SurfaceInputPortId = SurfaceInputPortId::new(4);
    const OTHER_SURFACE: SurfaceId = SurfaceId::new(22);
    const OTHER_OCCURRENCE: OccurrenceId = OccurrenceId::new(31);
    let program = Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        vec![OTHER_PORT, SURFACE_PORT],
        vec![OpacityInput::new(OPACITY, 0.5)],
        vec![
            Paint::Solid {
                id: SOLID,
                color: COLOR,
            },
            Paint::Opacity {
                id: TRANSLUCENT,
                source: SOLID,
                opacity: OPACITY,
            },
        ],
        vec![
            Surface::Input {
                id: BACKDROP,
                input: SURFACE_PORT,
            },
            Surface::Input {
                id: OTHER_SURFACE,
                input: OTHER_PORT,
            },
        ],
        vec![
            Occurrence::new(
                OCCURRENCE,
                TRANSLUCENT,
                BACKDROP,
                CompositionProfile::EncodedSrgb8SourceOverV1,
            ),
            Occurrence::new(
                OTHER_OCCURRENCE,
                TRANSLUCENT,
                OTHER_SURFACE,
                CompositionProfile::EncodedSrgb8SourceOverV1,
            ),
        ],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                REQUIRED,
                OCCURRENCE,
                Srgb8::new([0x80; 3]),
            )],
            vec![ConstraintInvocation::report_only(
                ConstraintId::new(51),
                OTHER_OCCURRENCE,
                Srgb8::new([0; 3]),
            )],
        ),
        vec![OutputBinding::new(OUTPUT, TRANSLUCENT)],
        ExactSrgb8IdentityV1,
    );
    let owner = program.compile().unwrap().into_owner();
    assert_eq!(
        owner.surface_input_ports(),
        Some(&[SURFACE_PORT, OTHER_PORT][..])
    );
    let mut session = owner.attach().unwrap();
    let signals = [
        SurfaceSignal::new(SURFACE_PORT, Srgb8::new([0xff; 3])),
        SurfaceSignal::new(OTHER_PORT, Srgb8::new([0; 3])),
    ];
    let SessionState::Ready { current } = session
        .update(SurfaceUpdate::Present {
            revision: 1,
            surfaces: &signals,
        })
        .unwrap()
    else {
        panic!("both exact occurrence contracts should pass");
    };
    assert_eq!(current.report().count(), 2);
    let outputs = current.outputs().collect::<Vec<_>>();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].value().source(), Srgb8::new([0; 3]));
    assert_eq!(outputs[0].value().straight_alpha_bits(), 0.5f64.to_bits());
}
