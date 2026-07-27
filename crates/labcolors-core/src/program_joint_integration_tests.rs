use crate::Srgb8;
use crate::appearance::{OccurrenceId, PaintId, SurfaceId, SurfaceInputPortId};
use crate::constraints::{
    ApplicableWcag22EvaluationErrorV1, CountingProgramWcag22Srgb8V1, ExactSrgb8IdentityV1,
    FinalRecheckMutantProgramEvaluatorV1, HardDecision, ProgramConstraintContentV1,
    ProgramPointAssessmentErrorV1, ProgramPointEvaluatorContentV1, ProgramPointOccurrenceV1,
    ProgramVisiblePointBindingV1, ProgramVisiblePointPassEvidence,
    ProgramVisiblePointViolationEvidence, Wcag22Srgb8V1, assess_program_point_hard,
};
use crate::joint::FiniteJointOrderErrorV1;
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextId, AppearanceContextSchemaReleaseId,
    BackgroundLuminanceRatio, ColorSignal, IEC_SRGB_D65_XYZ_FRAME_V1, SurroundProfileId,
};
use crate::observation::{
    ObservationGroupId, ObservationHeadViewV1, ObservationPayloadInput, ObservationStreamId,
    ObservationUpdateInput, ObservedScenarioSetInput, Revision, ScenarioId, ScenarioInput,
    SurfaceInputBinding,
};
use crate::program_session::{
    CompositionProfile, ConstraintId, ConstraintInvocation, ConstraintSet,
    DeclaredJointSelectionV1, HardModeV1, JointCandidateStateV1, ObservationGroup, Occurrence,
    OutputBinding, OutputSlotId, Paint, Program, ProgramCompileError,
    ProgramConstraintEvaluatorSetV1, ProgramConstraintSubjectV1, ProgramSessionEvaluationError,
    ReportModeV1, Source, SourceId, Surface, Target, TargetCandidateChoiceV1, TargetCandidateId,
    TargetCandidateV1, TargetDomainV1, TargetId, checked_program_evaluation_cell_counts_for_test,
    fail_program_preflight_reservation_for_test, program_preflight_failure_remaining_for_test,
};
use crate::session::{SessionState, SessionUpdateError};
use crate::session_tests::CommitSessionUpdateForTest as _;
use crate::wcag22::Wcag22CriterionV1;

const SOURCE: SourceId = SourceId::new(1);
const SURFACE_PORT: SurfaceInputPortId = SurfaceInputPortId::new(2);
const PAINT: PaintId = PaintId::new(10);
const BACKDROP: SurfaceId = SurfaceId::new(20);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(30);
const OUTPUT: OutputSlotId = OutputSlotId::new(40);
const TARGET: TargetId = TargetId::new(50);
const FIRST: TargetCandidateId = TargetCandidateId::new(60);
const SECOND: TargetCandidateId = TargetCandidateId::new(61);
const GROUP: ObservationGroupId = ObservationGroupId::new(70);
const STREAM: ObservationStreamId = ObservationStreamId::new(80);
const UPPER_SOURCE: SourceId = SourceId::new(81);
const UPPER_PAINT: PaintId = PaintId::new(82);
const DERIVED_SURFACE: SurfaceId = SurfaceId::new(83);
const UPPER_OCCURRENCE: OccurrenceId = OccurrenceId::new(84);
const UPPER_OUTPUT: OutputSlotId = OutputSlotId::new(85);
const UPPER_TARGET: TargetId = TargetId::new(86);
const UPPER_FIRST: TargetCandidateId = TargetCandidateId::new(87);
const UPPER_SECOND: TargetCandidateId = TargetCandidateId::new(88);
const HARD_INVOCATION: Srgb8 = Srgb8::new([0xDD; 3]);
const DIAGNOSTIC_INVOCATION: Srgb8 = Srgb8::new([0xEE; 3]);

/// A premature report invocation becomes an evaluator error, so this hostile
/// test double detects diagnostic authority leakage instead of merely counting
/// extra calls to a pure evaluator.
#[derive(Debug, Clone)]
struct ReportSelectionIsolationEvaluatorSetV1 {
    control: std::rc::Rc<ReportSelectionIsolationControlV1>,
}

#[derive(Debug)]
struct ReportSelectionIsolationControlV1 {
    selected: Srgb8,
    report_invocation: Wcag22CriterionV1,
    selected_non_report_calls: std::cell::Cell<usize>,
    report_calls: std::cell::Cell<usize>,
    calls: std::cell::RefCell<Vec<Srgb8>>,
}

impl ReportSelectionIsolationEvaluatorSetV1 {
    fn new(selected: Srgb8, report_invocation: Wcag22CriterionV1) -> Self {
        Self {
            control: std::rc::Rc::new(ReportSelectionIsolationControlV1 {
                selected,
                report_invocation,
                selected_non_report_calls: std::cell::Cell::new(0),
                report_calls: std::cell::Cell::new(0),
                calls: std::cell::RefCell::new(Vec::new()),
            }),
        }
    }

    fn report_calls(&self) -> usize {
        self.control.report_calls.get()
    }

    fn calls(&self) -> Vec<Srgb8> {
        self.control.calls.borrow().clone()
    }
}

impl ProgramConstraintEvaluatorSetV1 for ReportSelectionIsolationEvaluatorSetV1 {
    type Invocation = Wcag22CriterionV1;
    type PassEvidence = ProgramVisiblePointPassEvidence<Wcag22Srgb8V1>;
    type ViolationEvidence = ProgramVisiblePointViolationEvidence<Wcag22Srgb8V1>;
    type Error = ApplicableWcag22EvaluationErrorV1;

    fn assess(
        &self,
        point: ProgramPointOccurrenceV1,
        invocation: Self::Invocation,
    ) -> Result<
        HardDecision<Self::PassEvidence, Self::ViolationEvidence>,
        ProgramPointAssessmentErrorV1<Self::Error>,
    > {
        let visible = Srgb8::new(point.target().encoded().visible());
        self.control.calls.borrow_mut().push(visible);
        if invocation == self.control.report_invocation {
            // The selected state must complete its search hit and fresh hard
            // recheck before the report-only phase may execute.
            if self.control.selected_non_report_calls.get() < 2 {
                return Err(ProgramPointAssessmentErrorV1::Evaluator(
                    ApplicableWcag22EvaluationErrorV1::CriterionMismatch {
                        requested: invocation,
                        evaluated: Wcag22CriterionV1::Sc143TextLargeScale,
                    },
                ));
            }
            self.control
                .report_calls
                .set(self.control.report_calls.get() + 1);
        } else if visible == self.control.selected {
            self.control
                .selected_non_report_calls
                .set(self.control.selected_non_report_calls.get() + 1);
        }
        assess_program_point_hard(point, &Wcag22Srgb8V1, invocation)
    }

    fn pass_binding(evidence: &Self::PassEvidence) -> ProgramVisiblePointBindingV1 {
        *evidence.binding()
    }

    fn violation_binding(evidence: &Self::ViolationEvidence) -> ProgramVisiblePointBindingV1 {
        *evidence.binding()
    }

    fn constraint_content(&self, invocation: Self::Invocation) -> ProgramConstraintContentV1 {
        Wcag22Srgb8V1.program_constraint_content_v1(invocation)
    }
}

#[derive(Debug, Clone, Copy)]
struct DiagnosticPoisonPassV1(ProgramVisiblePointBindingV1);

#[derive(Debug, Clone, Copy)]
struct DiagnosticPoisonViolationV1(ProgramVisiblePointBindingV1);

/// The first diagnostic poisons every later hard decision. A complete conflict
/// is therefore possible only when all state × case hard evidence is frozen
/// before any report-only invocation runs.
#[derive(Debug, Clone, Default)]
struct CrossStateDiagnosticPoisonEvaluatorSetV1 {
    control: std::rc::Rc<CrossStateDiagnosticPoisonControlV1>,
}

#[derive(Debug, Default)]
struct CrossStateDiagnosticPoisonControlV1 {
    poisoned: std::cell::Cell<bool>,
    hard_calls: std::cell::Cell<usize>,
    first_report_after_hard_calls: std::cell::Cell<Option<usize>>,
}

impl CrossStateDiagnosticPoisonEvaluatorSetV1 {
    fn hard_calls_before_first_report(&self) -> Option<usize> {
        self.control.first_report_after_hard_calls.get()
    }
}

impl ProgramConstraintEvaluatorSetV1 for CrossStateDiagnosticPoisonEvaluatorSetV1 {
    type Invocation = Srgb8;
    type PassEvidence = DiagnosticPoisonPassV1;
    type ViolationEvidence = DiagnosticPoisonViolationV1;
    type Error = core::convert::Infallible;

    fn assess(
        &self,
        point: ProgramPointOccurrenceV1,
        invocation: Self::Invocation,
    ) -> Result<
        HardDecision<Self::PassEvidence, Self::ViolationEvidence>,
        ProgramPointAssessmentErrorV1<Self::Error>,
    > {
        let binding = point.binding();
        if invocation == DIAGNOSTIC_INVOCATION {
            if self.control.first_report_after_hard_calls.get().is_none() {
                self.control
                    .first_report_after_hard_calls
                    .set(Some(self.control.hard_calls.get()));
            }
            self.control.poisoned.set(true);
            return Ok(HardDecision::Pass(DiagnosticPoisonPassV1(binding)));
        }

        self.control
            .hard_calls
            .set(self.control.hard_calls.get() + 1);
        if self.control.poisoned.get() {
            Ok(HardDecision::Pass(DiagnosticPoisonPassV1(binding)))
        } else {
            Ok(HardDecision::Violation(DiagnosticPoisonViolationV1(
                binding,
            )))
        }
    }

    fn pass_binding(evidence: &Self::PassEvidence) -> ProgramVisiblePointBindingV1 {
        evidence.0
    }

    fn violation_binding(evidence: &Self::ViolationEvidence) -> ProgramVisiblePointBindingV1 {
        evidence.0
    }

    fn constraint_content(&self, invocation: Self::Invocation) -> ProgramConstraintContentV1 {
        ExactSrgb8IdentityV1.program_constraint_content_v1(invocation)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinalViolationDiagnosticErrorV1 {
    DiagnosticInvoked,
}

/// The hard evaluator passes candidate search and rejects the fresh selected
/// state recheck. Its diagnostic branch fails loudly, so a leaked diagnostic
/// invocation would mask the authoritative final-recheck verdict.
#[derive(Debug, Clone, Default)]
struct FinalViolationDiagnosticErrorEvaluatorSetV1 {
    control: std::rc::Rc<FinalViolationDiagnosticErrorControlV1>,
}

#[derive(Debug, Default)]
struct FinalViolationDiagnosticErrorControlV1 {
    hard_calls: std::cell::Cell<usize>,
    diagnostic_calls: std::cell::Cell<usize>,
}

impl FinalViolationDiagnosticErrorEvaluatorSetV1 {
    fn diagnostic_calls(&self) -> usize {
        self.control.diagnostic_calls.get()
    }
}

impl ProgramConstraintEvaluatorSetV1 for FinalViolationDiagnosticErrorEvaluatorSetV1 {
    type Invocation = Srgb8;
    type PassEvidence = DiagnosticPoisonPassV1;
    type ViolationEvidence = DiagnosticPoisonViolationV1;
    type Error = FinalViolationDiagnosticErrorV1;

    fn assess(
        &self,
        point: ProgramPointOccurrenceV1,
        invocation: Self::Invocation,
    ) -> Result<
        HardDecision<Self::PassEvidence, Self::ViolationEvidence>,
        ProgramPointAssessmentErrorV1<Self::Error>,
    > {
        if invocation == DIAGNOSTIC_INVOCATION {
            self.control
                .diagnostic_calls
                .set(self.control.diagnostic_calls.get() + 1);
            return Err(ProgramPointAssessmentErrorV1::Evaluator(
                FinalViolationDiagnosticErrorV1::DiagnosticInvoked,
            ));
        }

        let hard_call = self.control.hard_calls.get();
        self.control.hard_calls.set(hard_call + 1);
        let binding = point.binding();
        if hard_call == 0 {
            Ok(HardDecision::Pass(DiagnosticPoisonPassV1(binding)))
        } else {
            Ok(HardDecision::Violation(DiagnosticPoisonViolationV1(
                binding,
            )))
        }
    }

    fn pass_binding(evidence: &Self::PassEvidence) -> ProgramVisiblePointBindingV1 {
        evidence.0
    }

    fn violation_binding(evidence: &Self::ViolationEvidence) -> ProgramVisiblePointBindingV1 {
        evidence.0
    }

    fn constraint_content(&self, invocation: Self::Invocation) -> ProgramConstraintContentV1 {
        ExactSrgb8IdentityV1.program_constraint_content_v1(invocation)
    }
}

#[derive(Debug)]
struct PanicOnceEvaluatorControlV1 {
    armed: std::cell::Cell<bool>,
    calls: std::cell::Cell<usize>,
}

/// Паникует только на первой реальной оценке: тест отличает unwind после
/// получения evaluation-arena lease от более ранней паники.
#[derive(Debug, Clone)]
struct PanicOnceEvaluatorSetV1 {
    control: std::rc::Rc<PanicOnceEvaluatorControlV1>,
}

impl PanicOnceEvaluatorSetV1 {
    fn new() -> Self {
        Self {
            control: std::rc::Rc::new(PanicOnceEvaluatorControlV1 {
                armed: std::cell::Cell::new(true),
                calls: std::cell::Cell::new(0),
            }),
        }
    }

    fn calls(&self) -> usize {
        self.control.calls.get()
    }
}

impl ProgramConstraintEvaluatorSetV1 for PanicOnceEvaluatorSetV1 {
    type Invocation = Wcag22CriterionV1;
    type PassEvidence = ProgramVisiblePointPassEvidence<Wcag22Srgb8V1>;
    type ViolationEvidence = ProgramVisiblePointViolationEvidence<Wcag22Srgb8V1>;
    type Error = ApplicableWcag22EvaluationErrorV1;

    fn assess(
        &self,
        point: ProgramPointOccurrenceV1,
        invocation: Self::Invocation,
    ) -> Result<
        HardDecision<Self::PassEvidence, Self::ViolationEvidence>,
        ProgramPointAssessmentErrorV1<Self::Error>,
    > {
        self.control.calls.set(self.control.calls.get() + 1);
        if self.control.armed.replace(false) {
            panic!("hostile evaluator panic after the arena lease was acquired");
        }
        assess_program_point_hard(point, &Wcag22Srgb8V1, invocation)
    }

    fn pass_binding(evidence: &Self::PassEvidence) -> ProgramVisiblePointBindingV1 {
        *evidence.binding()
    }

    fn violation_binding(evidence: &Self::ViolationEvidence) -> ProgramVisiblePointBindingV1 {
        *evidence.binding()
    }

    fn constraint_content(&self, invocation: Self::Invocation) -> ProgramConstraintContentV1 {
        Wcag22Srgb8V1.program_constraint_content_v1(invocation)
    }
}

fn appearance_context() -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        IEC_SRGB_D65_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(64.0).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        SurroundProfileId::AverageV1,
    )
}

fn signal(value: u8) -> ColorSignal {
    ColorSignal::from_srgb8(Srgb8::new([value; 3]))
}

fn candidate(id: TargetCandidateId, value: u8) -> TargetCandidateV1 {
    TargetCandidateV1::new(id, signal(value))
}

fn state(candidate: TargetCandidateId) -> JointCandidateStateV1 {
    JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(TARGET, candidate)])
}

fn target(candidates: Vec<TargetCandidateV1>) -> Target {
    Target::finite(TARGET, SOURCE, candidates)
}

fn point_program<Evaluation>(
    source_signal: ColorSignal,
    target: Target,
    hard: Vec<
        ConstraintInvocation<
            <Evaluation as ProgramConstraintEvaluatorSetV1>::Invocation,
            HardModeV1,
        >,
    >,
    report_only: Vec<
        ConstraintInvocation<
            <Evaluation as ProgramConstraintEvaluatorSetV1>::Invocation,
            ReportModeV1,
        >,
    >,
    evaluator: Evaluation,
) -> Program<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    <Evaluation as ProgramConstraintEvaluatorSetV1>::Invocation: Copy,
{
    Program::new(
        vec![Source::new(SOURCE, source_signal)],
        vec![target],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: PAINT,
            target: TARGET,
        }],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            BACKDROP,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            appearance_context(),
        )],
        ConstraintSet::new(hard, report_only),
        vec![OutputBinding::new(OUTPUT, PAINT)],
        evaluator,
    )
}

fn program(
    hard: Vec<ConstraintInvocation<Wcag22CriterionV1, HardModeV1>>,
    report_only: Vec<ConstraintInvocation<Wcag22CriterionV1, ReportModeV1>>,
    candidates: Vec<TargetCandidateV1>,
    order: Vec<JointCandidateStateV1>,
) -> Program<Wcag22Srgb8V1> {
    point_program(
        signal(0),
        target(candidates),
        hard,
        report_only,
        Wcag22Srgb8V1,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(order))
}

fn update(revision: u64, backdrop: u8) -> ObservationUpdateInput {
    update_cases(revision, &[backdrop])
}

fn update_cases(revision: u64, backdrops: &[u8]) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: backdrops
                .iter()
                .copied()
                .enumerate()
                .map(|(index, backdrop)| ScenarioInput {
                    id: ScenarioId::new(u32::try_from(index + 1).unwrap()),
                    bindings: vec![SurfaceInputBinding::new(SURFACE_PORT, signal(backdrop))],
                })
                .collect(),
        }),
    }
}

fn paired_state(
    lower: TargetCandidateId,
    upper: TargetCandidateId,
    reverse_choices: bool,
) -> JointCandidateStateV1 {
    let mut choices = vec![
        TargetCandidateChoiceV1::new(TARGET, lower),
        TargetCandidateChoiceV1::new(UPPER_TARGET, upper),
    ];
    if reverse_choices {
        choices.reverse();
    }
    JointCandidateStateV1::new(choices)
}

fn nested_two_target_program(
    reverse_targets: bool,
    reverse_choices: bool,
) -> Program<Wcag22Srgb8V1> {
    let lower = Target::finite(
        TARGET,
        SOURCE,
        vec![candidate(FIRST, 0x00), candidate(SECOND, 0xFF)],
    );
    let upper = Target::finite(
        UPPER_TARGET,
        UPPER_SOURCE,
        vec![candidate(UPPER_FIRST, 0x55), candidate(UPPER_SECOND, 0xFF)],
    );
    let mut targets = vec![lower, upper];
    if reverse_targets {
        targets.reverse();
    }

    Program::new(
        vec![
            Source::new(SOURCE, signal(0)),
            Source::new(UPPER_SOURCE, signal(0)),
        ],
        targets,
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![
            Paint::Solid {
                id: PAINT,
                target: TARGET,
            },
            Paint::Solid {
                id: UPPER_PAINT,
                target: UPPER_TARGET,
            },
        ],
        vec![
            Surface::Input {
                id: BACKDROP,
                input: SURFACE_PORT,
            },
            Surface::FromOccurrence {
                id: DERIVED_SURFACE,
                occurrence: OCCURRENCE,
            },
        ],
        vec![
            Occurrence::new(
                OCCURRENCE,
                PAINT,
                BACKDROP,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            ),
            Occurrence::new(
                UPPER_OCCURRENCE,
                UPPER_PAINT,
                DERIVED_SURFACE,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            ),
        ],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                ConstraintId::new(1),
                UPPER_OCCURRENCE,
                Wcag22CriterionV1::Sc143TextLargeScale,
            )],
            vec![],
        ),
        vec![
            OutputBinding::new(OUTPUT, PAINT),
            OutputBinding::new(UPPER_OUTPUT, UPPER_PAINT),
        ],
        Wcag22Srgb8V1,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![
        paired_state(FIRST, UPPER_FIRST, reverse_choices),
        paired_state(SECOND, UPPER_FIRST, reverse_choices),
        paired_state(FIRST, UPPER_SECOND, reverse_choices),
        paired_state(SECOND, UPPER_SECOND, reverse_choices),
    ]))
}

#[derive(Clone, Copy)]
struct AlphaRenamedJointIds {
    lower_source: SourceId,
    upper_source: SourceId,
    lower_target: TargetId,
    upper_target: TargetId,
    lower_first: TargetCandidateId,
    lower_second: TargetCandidateId,
    upper_first: TargetCandidateId,
    upper_second: TargetCandidateId,
}

fn alpha_renamed_nested_program(
    ids: AlphaRenamedJointIds,
    permute_declarations: bool,
) -> Program<Wcag22Srgb8V1> {
    let mut sources = vec![
        Source::new(ids.lower_source, signal(0)),
        Source::new(ids.upper_source, signal(0)),
    ];
    let mut lower_candidates = vec![
        candidate(ids.lower_first, 0x00),
        candidate(ids.lower_second, 0xFF),
    ];
    let mut upper_candidates = vec![
        candidate(ids.upper_first, 0x55),
        candidate(ids.upper_second, 0xFF),
    ];
    if permute_declarations {
        sources.reverse();
        lower_candidates.reverse();
        upper_candidates.reverse();
    }
    let mut targets = vec![
        Target::finite(ids.lower_target, ids.lower_source, lower_candidates),
        Target::finite(ids.upper_target, ids.upper_source, upper_candidates),
    ];
    if permute_declarations {
        targets.reverse();
    }

    let state = |lower, upper| {
        let mut choices = vec![
            TargetCandidateChoiceV1::new(ids.lower_target, lower),
            TargetCandidateChoiceV1::new(ids.upper_target, upper),
        ];
        if permute_declarations {
            choices.reverse();
        }
        JointCandidateStateV1::new(choices)
    };

    Program::new(
        sources,
        targets,
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![
            Paint::Solid {
                id: PAINT,
                target: ids.lower_target,
            },
            Paint::Solid {
                id: UPPER_PAINT,
                target: ids.upper_target,
            },
        ],
        vec![
            Surface::Input {
                id: BACKDROP,
                input: SURFACE_PORT,
            },
            Surface::FromOccurrence {
                id: DERIVED_SURFACE,
                occurrence: OCCURRENCE,
            },
        ],
        vec![
            Occurrence::new(
                OCCURRENCE,
                PAINT,
                BACKDROP,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            ),
            Occurrence::new(
                UPPER_OCCURRENCE,
                UPPER_PAINT,
                DERIVED_SURFACE,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            ),
        ],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                ConstraintId::new(1),
                UPPER_OCCURRENCE,
                Wcag22CriterionV1::Sc143TextLargeScale,
            )],
            vec![],
        ),
        vec![
            OutputBinding::new(OUTPUT, PAINT),
            OutputBinding::new(UPPER_OUTPUT, UPPER_PAINT),
        ],
        Wcag22Srgb8V1,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![
        state(ids.lower_first, ids.upper_first),
        state(ids.lower_second, ids.upper_first),
        state(ids.lower_first, ids.upper_second),
        state(ids.lower_second, ids.upper_second),
    ]))
}

#[test]
fn authored_finite_target_values_keep_only_opaque_identity_and_explicit_policy() {
    let first = candidate(FIRST, 0x66);
    assert_eq!(TARGET.value(), 50);
    assert_eq!(FIRST.value(), 60);
    assert_eq!(first.id(), FIRST);
    assert_eq!(first.signal(), signal(0x66));

    let target = target(vec![first]);
    assert_eq!(target.id(), TARGET);
    assert_eq!(target.source(), SOURCE);
    let TargetDomainV1::Finite(candidates) = target.domain() else {
        panic!("target must retain its explicit finite domain");
    };
    assert_eq!(candidates, &[first]);

    let choice = TargetCandidateChoiceV1::new(TARGET, FIRST);
    assert_eq!(choice.target(), TARGET);
    assert_eq!(choice.candidate(), FIRST);
    let state = JointCandidateStateV1::new(vec![choice]);
    assert_eq!(state.choices(), &[choice]);
    let order = DeclaredJointSelectionV1::new(vec![state.clone()]);
    assert_eq!(order.states(), &[state]);
}

#[test]
fn evaluator_unwind_does_not_consume_the_reusable_evaluation_arena() {
    let evaluator = PanicOnceEvaluatorSetV1::new();
    let probe = evaluator.clone();
    let compiled = point_program(
        signal(0),
        Target::fixed(TARGET, SOURCE),
        vec![ConstraintInvocation::hard(
            ConstraintId::new(1),
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextDefault,
        )],
        vec![],
        evaluator,
    )
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = session.commit(update(1, 0xFF));
    }));
    assert!(first.is_err(), "the hostile evaluator must exercise unwind");
    assert_eq!(
        probe.calls(),
        1,
        "the panic must originate inside the evaluator, after arena acquisition",
    );

    let SessionState::Ready { current } = session
        .commit(update(1, 0xFF))
        .expect("unwind must return the arena slot for the lawful retry")
    else {
        panic!("opaque black on white must remain a normal verified outcome");
    };
    assert_eq!(current.outputs()[0].source_signal(), signal(0));
    assert!(
        probe.calls() > 1,
        "the retry must reach the evaluator again"
    );
}

#[test]
fn terminal_safety_rejects_an_output_outside_every_assessment_cone() {
    let error = match Program::new(
        vec![
            Source::new(SOURCE, signal(0)),
            Source::new(UPPER_SOURCE, signal(0xFF)),
        ],
        vec![
            Target::fixed(TARGET, SOURCE),
            Target::fixed(UPPER_TARGET, UPPER_SOURCE),
        ],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![
            Paint::Solid {
                id: PAINT,
                target: TARGET,
            },
            Paint::Solid {
                id: UPPER_PAINT,
                target: UPPER_TARGET,
            },
        ],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            BACKDROP,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            appearance_context(),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                ConstraintId::new(1),
                OCCURRENCE,
                Wcag22CriterionV1::Sc143TextLargeScale,
            )],
            vec![],
        ),
        vec![OutputBinding::new(UPPER_OUTPUT, UPPER_PAINT)],
        Wcag22Srgb8V1,
    )
    .compile()
    {
        Ok(_) => panic!("unassessed output must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        ProgramCompileError::UnassessedOutput {
            output: UPPER_OUTPUT,
            paint: UPPER_PAINT,
        }
    );
}

#[test]
fn terminal_safety_rejects_an_unconstrained_finite_target() {
    let error = match Program::new(
        vec![
            Source::new(SOURCE, signal(0)),
            Source::new(UPPER_SOURCE, signal(0xFF)),
        ],
        vec![
            target(vec![candidate(FIRST, 0), candidate(SECOND, 0xFF)]),
            Target::fixed(UPPER_TARGET, UPPER_SOURCE),
        ],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: UPPER_PAINT,
            target: UPPER_TARGET,
        }],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            UPPER_OCCURRENCE,
            UPPER_PAINT,
            BACKDROP,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            appearance_context(),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                ConstraintId::new(1),
                UPPER_OCCURRENCE,
                Wcag22CriterionV1::Sc143TextLargeScale,
            )],
            vec![],
        ),
        vec![OutputBinding::new(UPPER_OUTPUT, UPPER_PAINT)],
        Wcag22Srgb8V1,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![
        state(FIRST),
        state(SECOND),
    ]))
    .compile()
    {
        Ok(_) => panic!("unconstrained finite target must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        ProgramCompileError::UnconstrainedTarget { target: TARGET }
    );
}

#[test]
fn independent_finite_target_components_are_rejected_before_global_product_search() {
    let error = match Program::new(
        vec![
            Source::new(SOURCE, signal(0)),
            Source::new(UPPER_SOURCE, signal(0)),
        ],
        vec![
            target(vec![candidate(FIRST, 0), candidate(SECOND, 0xFF)]),
            Target::finite(
                UPPER_TARGET,
                UPPER_SOURCE,
                vec![candidate(UPPER_FIRST, 0x55), candidate(UPPER_SECOND, 0xFF)],
            ),
        ],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![
            Paint::Solid {
                id: PAINT,
                target: TARGET,
            },
            Paint::Solid {
                id: UPPER_PAINT,
                target: UPPER_TARGET,
            },
        ],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![
            Occurrence::new(
                OCCURRENCE,
                PAINT,
                BACKDROP,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            ),
            Occurrence::new(
                UPPER_OCCURRENCE,
                UPPER_PAINT,
                BACKDROP,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            ),
        ],
        ConstraintSet::new(
            vec![
                ConstraintInvocation::hard(
                    ConstraintId::new(1),
                    OCCURRENCE,
                    Wcag22CriterionV1::Sc143TextLargeScale,
                ),
                ConstraintInvocation::hard(
                    ConstraintId::new(2),
                    UPPER_OCCURRENCE,
                    Wcag22CriterionV1::Sc143TextLargeScale,
                ),
            ],
            vec![],
        ),
        vec![
            OutputBinding::new(OUTPUT, PAINT),
            OutputBinding::new(UPPER_OUTPUT, UPPER_PAINT),
        ],
        Wcag22Srgb8V1,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![
        paired_state(FIRST, UPPER_FIRST, false),
        paired_state(SECOND, UPPER_FIRST, false),
        paired_state(FIRST, UPPER_SECOND, false),
        paired_state(SECOND, UPPER_SECOND, false),
    ]))
    .compile()
    {
        Ok(_) => panic!("independent components must not enter one global product search"),
        Err(error) => error,
    };
    assert_eq!(error, ProgramCompileError::DisconnectedFiniteTargets);
}

#[test]
fn candidate_passing_one_hard_cell_and_failing_another_is_never_certified() {
    let compiled = program(
        vec![
            ConstraintInvocation::hard(
                ConstraintId::new(1),
                OCCURRENCE,
                Wcag22CriterionV1::Sc143TextLargeScale,
            ),
            ConstraintInvocation::hard(
                ConstraintId::new(2),
                OCCURRENCE,
                Wcag22CriterionV1::Sc143TextDefault,
            ),
        ],
        vec![],
        vec![candidate(FIRST, 0x66), candidate(SECOND, 0xFF)],
        vec![state(FIRST), state(SECOND)],
    )
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let SessionState::Ready { current } = session.commit(update(1, 0x00)).unwrap() else {
        panic!("the later all-hard-feasible candidate must be selected");
    };
    assert_eq!(current.selected_state_index(), Some(1));
    assert_eq!(current.outputs().len(), 1);
    assert_eq!(current.outputs()[0].source_signal(), signal(0xFF));
    assert_eq!(current.report().cells().len(), 2);
    assert!(
        current
            .report()
            .cells()
            .iter()
            .all(|cell| cell.candidate_state_index() == 1 && !cell.result().is_violation())
    );
}

#[test]
fn report_only_violation_is_retained_but_does_not_select() {
    let compiled = program(
        vec![ConstraintInvocation::hard(
            ConstraintId::new(1),
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextLargeScale,
        )],
        vec![ConstraintInvocation::report_only(
            ConstraintId::new(2),
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextDefault,
        )],
        vec![candidate(FIRST, 0x66), candidate(SECOND, 0xFF)],
        vec![state(FIRST), state(SECOND)],
    )
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let SessionState::Ready { current } = session.commit(update(1, 0x00)).unwrap() else {
        panic!("report-only failure must not reject the first hard-feasible state");
    };
    assert_eq!(current.selected_state_index(), Some(0));
    assert_eq!(current.outputs()[0].source_signal(), signal(0x66));
    let cells = current.report().cells();
    assert_eq!(cells.len(), 2);
    assert!(!cells[0].result().is_violation());
    assert!(cells[0].is_hard());
    assert!(cells[1].result().is_violation());
    assert!(!cells[1].is_hard());
}

#[test]
fn report_only_assessment_admits_output_but_cannot_override_explicit_selection_order() {
    let compiled = program(
        vec![],
        vec![ConstraintInvocation::report_only(
            ConstraintId::new(1),
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextDefault,
        )],
        vec![candidate(FIRST, 0x66), candidate(SECOND, 0xFF)],
        vec![state(FIRST), state(SECOND)],
    )
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let SessionState::Ready { current } = session.commit(update(1, 0x00)).unwrap() else {
        panic!("report-only assessment must not create hard infeasibility");
    };
    assert_eq!(current.selected_state_index(), Some(0));
    assert_eq!(current.outputs()[0].source_signal(), signal(0x66));
    assert!(current.report().cells()[0].result().is_violation());
    assert!(!current.report().cells()[0].is_hard());
}

#[test]
fn no_feasible_joint_state_commits_no_output_and_retains_previous_certificate() {
    let compiled = program(
        vec![ConstraintInvocation::hard(
            ConstraintId::new(1),
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextLargeScale,
        )],
        vec![],
        vec![candidate(FIRST, 0xAA), candidate(SECOND, 0xFF)],
        vec![state(FIRST), state(SECOND)],
    )
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let SessionState::Ready { current } = session.commit(update(1, 0x00)).unwrap() else {
        panic!("control update must certify the first state");
    };
    assert_eq!(current.outputs()[0].source_signal(), signal(0xAA));

    let SessionState::Failed { cause, previous } = session.commit(update(2, 0xFF)).unwrap() else {
        panic!("all-hard-infeasible domain must become Conflict/Failed");
    };
    assert_eq!(cause.considered_state_count(), 2);
    let previous = previous
        .as_ref()
        .expect("last certificate must be retained");
    assert_eq!(previous.outputs()[0].source_signal(), signal(0xAA));
    assert_eq!(cause.report().cells().len(), 2);
    assert!(
        cause
            .report()
            .cells()
            .iter()
            .all(|cell| { cell.is_hard() && cell.result().is_violation() })
    );
}

#[test]
fn candidate_declaration_permutation_preserves_explicit_physical_order() {
    let make = |candidates| {
        program(
            vec![ConstraintInvocation::hard(
                ConstraintId::new(1),
                OCCURRENCE,
                Wcag22CriterionV1::Sc143TextLargeScale,
            )],
            vec![],
            candidates,
            vec![state(FIRST), state(SECOND)],
        )
        .compile()
        .unwrap()
    };
    let first = make(vec![candidate(FIRST, 0x66), candidate(SECOND, 0xFF)]);
    let reversed = make(vec![candidate(SECOND, 0xFF), candidate(FIRST, 0x66)]);
    let mut first_session = first.instantiate(STREAM).unwrap();
    let mut reversed_session = reversed.instantiate(STREAM).unwrap();

    let SessionState::Ready { current: first } = first_session.commit(update(1, 0x00)).unwrap()
    else {
        panic!("first program must certify");
    };
    let SessionState::Ready { current: reversed } =
        reversed_session.commit(update(1, 0x00)).unwrap()
    else {
        panic!("permuted program must certify");
    };
    assert_eq!(
        first.selected_state_index(),
        reversed.selected_state_index()
    );
    assert_eq!(first.outputs(), reversed.outputs());
}

#[test]
fn nested_two_target_selection_ignores_target_and_choice_declaration_order() {
    let run = |reverse_targets, reverse_choices| {
        let compiled = nested_two_target_program(reverse_targets, reverse_choices)
            .compile()
            .unwrap();
        let mut session = compiled.instantiate(STREAM).unwrap();
        let SessionState::Ready { current } = session.commit(update(1, 0x00)).unwrap() else {
            panic!("the second explicitly ordered joint tuple must certify");
        };
        (
            current.selected_state_index(),
            current
                .outputs()
                .iter()
                .map(|output| (output.output(), output.source_signal()))
                .collect::<Vec<_>>(),
        )
    };

    let canonical = run(false, false);
    let permuted = run(true, true);
    assert_eq!(canonical, permuted);
    assert_eq!(canonical.0, Some(1));
    assert_eq!(
        canonical.1,
        vec![(OUTPUT, signal(0xFF)), (UPPER_OUTPUT, signal(0x55))]
    );
}

#[test]
fn bijective_source_target_and_candidate_renaming_preserves_joint_evidence() {
    let canonical_ids = AlphaRenamedJointIds {
        lower_source: SourceId::new(100),
        upper_source: SourceId::new(200),
        lower_target: TargetId::new(300),
        upper_target: TargetId::new(400),
        lower_first: TargetCandidateId::new(500),
        lower_second: TargetCandidateId::new(600),
        upper_first: TargetCandidateId::new(700),
        upper_second: TargetCandidateId::new(800),
    };
    // Every authored namespace is alpha-renamed. Source and Target order is
    // reversed across logical dimensions, while both candidate domains also
    // reverse numeric order. Declaration and per-state choice order are then
    // independently permuted; only the explicit logical state order remains.
    let renamed_ids = AlphaRenamedJointIds {
        lower_source: SourceId::new(920),
        upper_source: SourceId::new(110),
        lower_target: TargetId::new(840),
        upper_target: TargetId::new(230),
        lower_first: TargetCandidateId::new(760),
        lower_second: TargetCandidateId::new(650),
        upper_first: TargetCandidateId::new(540),
        upper_second: TargetCandidateId::new(430),
    };

    let run = |program: Program<Wcag22Srgb8V1>| {
        let compiled = program.compile().unwrap();
        let mut session = compiled.instantiate(STREAM).unwrap();
        let SessionState::Ready { current } = session.commit(update(1, 0x00)).unwrap() else {
            panic!("the second logical state must certify after the first is rejected");
        };
        assert_eq!(current.selected_state_index(), Some(1));

        let outputs = current
            .outputs()
            .iter()
            .map(|output| (output.output(), output.paint(), output.source_signal()))
            .collect::<Vec<_>>();
        let cells = current
            .report()
            .cells()
            .iter()
            .map(|cell| {
                (
                    cell.candidate_state_index(),
                    cell.case_index(),
                    cell.constraint(),
                    cell.subject(),
                    cell.is_hard(),
                    cell.result().is_violation(),
                )
            })
            .collect::<Vec<_>>();
        let observation = current.report().observation();
        let physical_cases = (0..observation.physical_case_count())
            .map(|case_index| {
                (
                    observation.physical_values(case_index).unwrap().to_vec(),
                    observation.provenance(case_index).unwrap().to_vec(),
                )
            })
            .collect::<Vec<_>>();

        (
            current.selected_state_index(),
            outputs,
            cells,
            observation.stream(),
            observation.revision(),
            physical_cases,
        )
    };

    let canonical = run(alpha_renamed_nested_program(canonical_ids, false));
    let renamed = run(alpha_renamed_nested_program(renamed_ids, true));
    assert_eq!(canonical, renamed);
    assert_eq!(
        canonical.1,
        vec![
            (OUTPUT, canonical.1[0].1, signal(0xFF),),
            (UPPER_OUTPUT, canonical.1[1].1, signal(0x55),),
        ]
    );
}

#[test]
fn rejected_state_runs_once_and_selected_state_runs_fresh_recheck_twice() {
    let evaluator = CountingProgramWcag22Srgb8V1::default();
    let calls = evaluator.clone();
    let compiled = Program::new(
        vec![Source::new(SOURCE, signal(0))],
        vec![target(vec![
            candidate(FIRST, 0x55),
            candidate(SECOND, 0xFF),
        ])],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: PAINT,
            target: TARGET,
        }],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            BACKDROP,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            appearance_context(),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                ConstraintId::new(1),
                OCCURRENCE,
                Wcag22CriterionV1::Sc143TextLargeScale,
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, PAINT)],
        evaluator,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![
        state(FIRST),
        state(SECOND),
    ]))
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let SessionState::Ready { current } = session.commit(update(1, 0x00)).unwrap() else {
        panic!("the second candidate must certify after a fresh full recheck");
    };
    assert_eq!(current.selected_state_index(), Some(1));
    assert_eq!(
        calls.calls(),
        vec![
            Srgb8::new([0x55; 3]),
            Srgb8::new([0xFF; 3]),
            Srgb8::new([0xFF; 3]),
        ]
    );
}

#[test]
fn report_evaluator_error_cannot_poison_candidate_search_or_change_selection() {
    let report_invocation = Wcag22CriterionV1::Sc143TextDefault;
    let evaluator =
        ReportSelectionIsolationEvaluatorSetV1::new(Srgb8::new([0xFF; 3]), report_invocation);
    let probe = evaluator.clone();
    let compiled = point_program(
        signal(0),
        target(vec![candidate(FIRST, 0x55), candidate(SECOND, 0xFF)]),
        vec![ConstraintInvocation::hard(
            ConstraintId::new(1),
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextLargeScale,
        )],
        vec![ConstraintInvocation::report_only(
            ConstraintId::new(2),
            OCCURRENCE,
            report_invocation,
        )],
        evaluator,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![
        state(FIRST),
        state(SECOND),
    ]))
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let SessionState::Ready { current } = session.commit(update(1, 0x00)).unwrap() else {
        panic!("diagnostics without selection authority cannot poison hard candidate search");
    };
    assert_eq!(current.selected_state_index(), Some(1));
    assert_eq!(current.report().cells().len(), 2);
    assert!(current.report().cells()[0].is_hard());
    assert!(!current.report().cells()[0].result().is_violation());
    assert!(!current.report().cells()[1].is_hard());
    assert!(!current.report().cells()[1].result().is_violation());
    assert_eq!(probe.report_calls(), 1);
    assert_eq!(
        probe.calls(),
        vec![
            Srgb8::new([0x55; 3]),
            Srgb8::new([0xFF; 3]),
            Srgb8::new([0xFF; 3]),
            Srgb8::new([0xFF; 3]),
        ],
    );
}

#[test]
fn hard_conflict_runs_report_only_once_in_the_exhaustive_full_pass() {
    let evaluator = CountingProgramWcag22Srgb8V1::default();
    let calls = evaluator.clone();
    let compiled = point_program(
        signal(0),
        target(vec![candidate(FIRST, 0xAA), candidate(SECOND, 0xFF)]),
        vec![ConstraintInvocation::hard(
            ConstraintId::new(1),
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextLargeScale,
        )],
        vec![ConstraintInvocation::report_only(
            ConstraintId::new(2),
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextDefault,
        )],
        evaluator,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![
        state(FIRST),
        state(SECOND),
    ]))
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let SessionState::Failed { cause, previous } = session.commit(update(1, 0xFF)).unwrap() else {
        panic!("both states must fail the hard large-text criterion on white");
    };
    assert!(previous.is_none());
    assert_eq!(cause.considered_state_count(), 2);
    assert_eq!(cause.report().cells().len(), 4);
    assert_eq!(
        calls.calls(),
        vec![
            Srgb8::new([0xAA; 3]),
            Srgb8::new([0xFF; 3]),
            Srgb8::new([0xAA; 3]),
            Srgb8::new([0xFF; 3]),
            Srgb8::new([0xAA; 3]),
            Srgb8::new([0xFF; 3]),
        ],
        "hard search and the exhaustive all-state hard phase must precede the complete diagnostic phase",
    );
}

#[test]
fn fixed_program_without_finite_targets_executes_one_complete_evidence_pass() {
    let evaluator = CountingProgramWcag22Srgb8V1::default();
    let calls = evaluator.clone();
    let compiled = point_program(
        signal(0xFF),
        Target::fixed(TARGET, SOURCE),
        vec![],
        vec![ConstraintInvocation::report_only(
            ConstraintId::new(1),
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextDefault,
        )],
        evaluator,
    )
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let SessionState::Ready { current } = session.commit(update(1, 0x00)).unwrap() else {
        panic!("a fixed Program must retain diagnostics in its sole complete pass");
    };
    assert_eq!(current.selected_state_index(), None);
    assert_eq!(current.report().cells().len(), 1);
    assert!(!current.report().cells()[0].is_hard());
    assert!(!current.report().cells()[0].result().is_violation());
    assert_eq!(calls.calls(), vec![Srgb8::new([0xFF; 3])]);
}

#[test]
fn fixed_hard_conflict_still_collects_report_only_evidence() {
    let evaluator = CountingProgramWcag22Srgb8V1::default();
    let calls = evaluator.clone();
    let report = ConstraintId::new(1);
    let hard = ConstraintId::new(2);
    let compiled = point_program(
        signal(0xAA),
        Target::fixed(TARGET, SOURCE),
        vec![ConstraintInvocation::hard(
            hard,
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextLargeScale,
        )],
        vec![ConstraintInvocation::report_only(
            report,
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextDefault,
        )],
        evaluator,
    )
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let SessionState::Failed { cause, previous } = session.commit(update(1, 0xFF)).unwrap() else {
        panic!("a fixed hard conflict must retain its complete diagnostic report");
    };
    assert!(previous.is_none());
    assert_eq!(cause.considered_state_count(), 1);
    assert_eq!(
        cause
            .report()
            .cells()
            .iter()
            .map(|cell| (
                cell.constraint(),
                cell.is_hard(),
                cell.result().is_violation(),
            ))
            .collect::<Vec<_>>(),
        vec![(report, false, true), (hard, true, true)],
    );
    assert_eq!(
        calls.calls(),
        vec![Srgb8::new([0xAA; 3]), Srgb8::new([0xAA; 3])],
        "fixed evidence executes the hard phase before report-only diagnostics",
    );
}

#[test]
fn exhaustive_conflict_freezes_every_state_case_hard_cell_before_any_diagnostic() {
    let evaluator = CrossStateDiagnosticPoisonEvaluatorSetV1::default();
    let probe = evaluator.clone();
    let hard = ConstraintId::new(1);
    let report = ConstraintId::new(2);
    let compiled = point_program(
        signal(0),
        target(vec![candidate(FIRST, 0xAA), candidate(SECOND, 0xBB)]),
        vec![ConstraintInvocation::hard(
            hard,
            OCCURRENCE,
            HARD_INVOCATION,
        )],
        vec![ConstraintInvocation::report_only(
            report,
            OCCURRENCE,
            DIAGNOSTIC_INVOCATION,
        )],
        evaluator,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![
        state(FIRST),
        state(SECOND),
    ]))
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let SessionState::Failed { cause, previous } =
        session.commit(update_cases(1, &[0x00, 0xFF])).unwrap()
    else {
        panic!("diagnostics cannot poison hard evidence in a later case or state");
    };
    assert!(previous.is_none());
    assert_eq!(cause.considered_state_count(), 2);
    assert_eq!(probe.hard_calls_before_first_report(), Some(8));
    assert_eq!(cause.report().cells().len(), 8);
    assert_eq!(
        cause
            .report()
            .cells()
            .iter()
            .map(|cell| (
                cell.candidate_state_index(),
                cell.case_index(),
                cell.constraint(),
                cell.is_hard(),
                cell.result().is_violation(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (0, 0, hard, true, true),
            (0, 0, report, false, false),
            (0, 1, hard, true, true),
            (0, 1, report, false, false),
            (1, 0, hard, true, true),
            (1, 0, report, false, false),
            (1, 1, hard, true, true),
            (1, 1, report, false, false),
        ],
    );
}

#[test]
fn successful_search_allocations_do_not_scale_with_rejected_states() {
    let compile = |candidates, order| {
        program(
            vec![ConstraintInvocation::hard(
                ConstraintId::new(1),
                OCCURRENCE,
                Wcag22CriterionV1::Sc143TextLargeScale,
            )],
            vec![],
            candidates,
            order,
        )
        .compile()
        .unwrap()
    };
    let direct = compile(vec![candidate(SECOND, 0xFF)], vec![state(SECOND)]);
    let after_rejection = compile(
        vec![candidate(FIRST, 0x55), candidate(SECOND, 0xFF)],
        vec![state(FIRST), state(SECOND)],
    );
    let mut direct_session = direct.instantiate(STREAM).unwrap();
    let mut rejected_session = after_rejection.instantiate(STREAM).unwrap();

    let (_, direct_allocations) = crate::test_support::measured_allocations(|| {
        let SessionState::Ready { .. } = direct_session.commit(update(1, 0x00)).unwrap() else {
            panic!("direct candidate must certify");
        };
    });
    let (_, rejected_allocations) = crate::test_support::measured_allocations(|| {
        let SessionState::Ready { current } = rejected_session.commit(update(1, 0x00)).unwrap()
        else {
            panic!("later candidate must certify");
        };
        assert_eq!(current.selected_state_index(), Some(1));
    });
    assert_eq!(rejected_allocations, direct_allocations);
}

#[test]
fn evaluation_cell_cardinality_checks_both_products_without_a_numeric_cap() {
    assert_eq!(
        checked_program_evaluation_cell_counts_for_test(3, 2, 4),
        Some((6, 24))
    );
    assert_eq!(
        checked_program_evaluation_cell_counts_for_test(usize::MAX, 2, 1),
        None
    );
    assert_eq!(
        checked_program_evaluation_cell_counts_for_test(usize::MAX, 1, 2),
        None
    );
}

#[test]
fn every_required_joint_arena_reservation_is_fail_before_work_and_retryable() {
    // Joint-оценка без point-causal evidence имеет две непустые координаты
    // arena: constraint cells и committed outputs.
    const FIRST_UNUSED_RESERVATION_INDEX: usize = 2;

    for reservation_index in 0..=FIRST_UNUSED_RESERVATION_INDEX {
        let evaluator = CountingProgramWcag22Srgb8V1::default();
        let calls = evaluator.clone();
        let compiled = Program::new(
            vec![Source::new(SOURCE, signal(0))],
            vec![target(vec![
                candidate(FIRST, 0x55),
                candidate(SECOND, 0xFF),
            ])],
            ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
            vec![],
            vec![Paint::Solid {
                id: PAINT,
                target: TARGET,
            }],
            vec![Surface::Input {
                id: BACKDROP,
                input: SURFACE_PORT,
            }],
            vec![Occurrence::new(
                OCCURRENCE,
                PAINT,
                BACKDROP,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            )],
            ConstraintSet::new(
                vec![ConstraintInvocation::hard(
                    ConstraintId::new(1),
                    OCCURRENCE,
                    Wcag22CriterionV1::Sc143TextLargeScale,
                )],
                vec![],
            ),
            vec![OutputBinding::new(OUTPUT, PAINT)],
            evaluator,
        )
        .with_joint_selection(DeclaredJointSelectionV1::new(vec![
            state(FIRST),
            state(SECOND),
        ]))
        .compile()
        .unwrap();
        let mut session = compiled.instantiate(STREAM).unwrap();

        let result = {
            let _failure = fail_program_preflight_reservation_for_test(reservation_index);
            session.commit(update(1, 0x00))
        };
        if reservation_index == FIRST_UNUSED_RESERVATION_INDEX {
            assert!(
                matches!(result, Ok(SessionState::Ready { .. })),
                "the first unused reservation index must leave the Session Ready"
            );
            assert!(!calls.calls().is_empty());
            continue;
        }
        let error = match result {
            Ok(_) => panic!("injected preflight failure must abort the update"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            SessionUpdateError::Plan(ProgramSessionEvaluationError::ResourceExhausted)
        );
        assert!(calls.calls().is_empty());
        assert!(matches!(session.state(), SessionState::Waiting));
        assert_eq!(session.raw_head(), ObservationHeadViewV1::Empty);

        let SessionState::Ready { current } = session
            .commit(update(1, 0x00))
            .expect("a failed preflight must return the arena for an exact retry")
        else {
            panic!("the retry must select the first certifying joint state");
        };
        assert_eq!(current.selected_state_index(), Some(1));
        assert!(!calls.calls().is_empty());
    }
}

#[test]
fn every_fallible_fixed_preflight_reservation_precedes_evaluator_work() {
    const FIRST_UNUSED_RESERVATION_INDEX: usize = 2;

    for reservation_index in 0..=FIRST_UNUSED_RESERVATION_INDEX {
        let evaluator = CountingProgramWcag22Srgb8V1::default();
        let calls = evaluator.clone();
        let compiled = Program::new(
            vec![Source::new(SOURCE, signal(0xFF))],
            vec![Target::fixed(TARGET, SOURCE)],
            ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
            vec![],
            vec![Paint::Solid {
                id: PAINT,
                target: TARGET,
            }],
            vec![Surface::Input {
                id: BACKDROP,
                input: SURFACE_PORT,
            }],
            vec![Occurrence::new(
                OCCURRENCE,
                PAINT,
                BACKDROP,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                appearance_context(),
            )],
            ConstraintSet::new(
                vec![ConstraintInvocation::hard(
                    ConstraintId::new(1),
                    OCCURRENCE,
                    Wcag22CriterionV1::Sc143TextLargeScale,
                )],
                vec![],
            ),
            vec![OutputBinding::new(OUTPUT, PAINT)],
            evaluator,
        )
        .compile()
        .unwrap();
        let mut session = compiled.instantiate(STREAM).unwrap();

        let result = {
            let _failure = fail_program_preflight_reservation_for_test(reservation_index);
            session.commit(update(1, 0x00))
        };
        if reservation_index == FIRST_UNUSED_RESERVATION_INDEX {
            assert!(
                matches!(result, Ok(SessionState::Ready { .. })),
                "the first unused reservation index must leave the Session Ready"
            );
            assert!(!calls.calls().is_empty());
            continue;
        }
        let error = match result {
            Ok(_) => panic!("injected fixed preflight failure must abort the update"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            SessionUpdateError::Plan(ProgramSessionEvaluationError::ResourceExhausted)
        );
        assert!(calls.calls().is_empty());
        assert!(matches!(session.state(), SessionState::Waiting));
    }
}

#[test]
fn warmed_program_preflight_still_visits_every_nonempty_coordinate() {
    const NONEMPTY_COORDINATE_COUNT: usize = 2;

    let evaluator = CountingProgramWcag22Srgb8V1::default();
    let calls = evaluator.clone();
    let compiled = counting_fixed_program(evaluator);
    let mut session = compiled.instantiate(STREAM).unwrap();
    assert!(matches!(
        session.commit(update(1, 0x00)).unwrap(),
        SessionState::Ready { .. }
    ));
    assert!(matches!(
        session.commit(update(2, 0x00)).unwrap(),
        SessionState::Ready { .. }
    ));
    let calls_before = calls.calls().len();

    let _failure = fail_program_preflight_reservation_for_test(NONEMPTY_COORDINATE_COUNT);
    assert!(matches!(
        session.commit(update(3, 0x00)).unwrap(),
        SessionState::Ready { .. }
    ));
    assert_eq!(program_preflight_failure_remaining_for_test(), Some(0));
    assert!(calls.calls().len() > calls_before);
}

fn counting_fixed_program(
    evaluator: CountingProgramWcag22Srgb8V1,
) -> crate::program_session::CompiledProgram<CountingProgramWcag22Srgb8V1> {
    Program::new(
        vec![Source::new(SOURCE, signal(0xFF))],
        vec![Target::fixed(TARGET, SOURCE)],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: PAINT,
            target: TARGET,
        }],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            BACKDROP,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            appearance_context(),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                ConstraintId::new(1),
                OCCURRENCE,
                Wcag22CriterionV1::Sc143TextLargeScale,
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, PAINT)],
        evaluator,
    )
    .compile()
    .unwrap()
}

#[test]
fn expired_program_generation_precedes_composition_and_evaluation_without_allocation() {
    let evaluator = CountingProgramWcag22Srgb8V1::default();
    let calls = evaluator.clone();
    let compiled = counting_fixed_program(evaluator);
    let mut session = compiled.instantiate(STREAM).unwrap();

    crate::composition::reset_source_over_evaluation_count();
    let SessionState::Ready { current } = session.commit(update(1, 0x00)).unwrap() else {
        panic!("control generation must certify");
    };
    assert_eq!(current.report().observation().revision(), Revision::new(1));
    assert_eq!(calls.calls().len(), 1);
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);

    drop(compiled);
    let expired_update = update(2, 0x00);
    let (error, allocations) = crate::test_support::measured_allocations(|| {
        session.commit(expired_update).map(|_| ()).unwrap_err()
    });
    assert_eq!(error, SessionUpdateError::OwnerExpired);
    assert_eq!(allocations, 0);
    assert_eq!(calls.calls().len(), 1);
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    assert_eq!(session.raw_head().revision(), Some(Revision::new(1)));
    let SessionState::Ready { current } = session.state() else {
        panic!("expiry must retain the previous committed state");
    };
    assert_eq!(current.report().observation().revision(), Revision::new(1));
}

#[test]
fn equivalent_recompiled_owner_is_a_new_generation_and_cannot_revive_old_sessions() {
    let first_evaluator = CountingProgramWcag22Srgb8V1::default();
    let first_calls = first_evaluator.clone();
    let mut compiled = counting_fixed_program(first_evaluator);
    let first_content_identity = compiled.content_identity();
    let mut old_session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = old_session.commit(update(1, 0x00)).unwrap() else {
        panic!("the first owner must certify its admitted input");
    };
    assert_eq!(current.report().content_identity(), first_content_identity);

    let replacement_evaluator = CountingProgramWcag22Srgb8V1::default();
    let replacement_calls = replacement_evaluator.clone();
    compiled = counting_fixed_program(replacement_evaluator);
    assert_eq!(compiled.content_identity(), first_content_identity);
    assert!(matches!(
        old_session.commit(update(2, 0x00)),
        Err(SessionUpdateError::OwnerExpired),
    ));
    assert_eq!(first_calls.calls().len(), 1);
    assert!(replacement_calls.calls().is_empty());
    assert_eq!(old_session.raw_head().revision(), Some(Revision::new(1)));

    let mut replacement_session = compiled.instantiate(STREAM).unwrap();
    assert!(matches!(
        replacement_session.commit(update(1, 0x00)).unwrap(),
        SessionState::Ready { .. }
    ));
    assert_eq!(replacement_calls.calls().len(), 1);
    assert!(matches!(
        replacement_session.raw_head(),
        ObservationHeadViewV1::Observed(_)
    ));
}

#[test]
fn lower_id_diagnostic_cannot_consume_the_selected_state_final_recheck() {
    let evaluator = FinalRecheckMutantProgramEvaluatorV1::default();
    let control = evaluator.clone();
    let hard = ConstraintId::new(2);
    let compiled = point_program(
        signal(0),
        target(vec![candidate(FIRST, 0xFF)]),
        vec![ConstraintInvocation::hard(
            hard,
            OCCURRENCE,
            Srgb8::new([0xFF; 3]),
        )],
        vec![ConstraintInvocation::report_only(
            ConstraintId::new(1),
            OCCURRENCE,
            Srgb8::new([0xFF; 3]),
        )],
        evaluator,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![state(FIRST)]))
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    assert!(matches!(
        session.commit(update(1, 0x00)).unwrap(),
        SessionState::Ready { .. }
    ));
    control.arm();
    let error = match session.commit(update(2, 0x00)) {
        Ok(_) => panic!("a lower-ID diagnostic must not consume the hard final recheck"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        SessionUpdateError::Plan(ProgramSessionEvaluationError::FinalRecheckViolation {
            state_index: 0,
            case_index: 0,
            constraint: hard,
            subject: ProgramConstraintSubjectV1::ModeledOccurrence {
                occurrence: OCCURRENCE,
                context: appearance_context(),
            },
            hard_violation_count: 1,
        }),
    );
}

#[test]
fn diagnostic_error_cannot_mask_a_selected_state_final_recheck_violation() {
    let evaluator = FinalViolationDiagnosticErrorEvaluatorSetV1::default();
    let probe = evaluator.clone();
    let hard = ConstraintId::new(1);
    let compiled = point_program(
        signal(0),
        target(vec![candidate(FIRST, 0xFF)]),
        vec![ConstraintInvocation::hard(
            hard,
            OCCURRENCE,
            HARD_INVOCATION,
        )],
        vec![ConstraintInvocation::report_only(
            ConstraintId::new(2),
            OCCURRENCE,
            DIAGNOSTIC_INVOCATION,
        )],
        evaluator,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![state(FIRST)]))
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let error = match session.commit(update(1, 0x00)) {
        Ok(_) => panic!("a diagnostic error must not mask the hard final-recheck verdict"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        SessionUpdateError::Plan(ProgramSessionEvaluationError::FinalRecheckViolation {
            state_index: 0,
            case_index: 0,
            constraint: hard,
            subject: ProgramConstraintSubjectV1::ModeledOccurrence {
                occurrence: OCCURRENCE,
                context: appearance_context(),
            },
            hard_violation_count: 1,
        }),
    );
    assert_eq!(probe.diagnostic_calls(), 0);
}

#[test]
fn final_recheck_violation_is_typed_and_retains_the_previous_certificate() {
    let evaluator = FinalRecheckMutantProgramEvaluatorV1::default();
    let control = evaluator.clone();
    let compiled = Program::new(
        vec![Source::new(SOURCE, signal(0))],
        vec![target(vec![candidate(FIRST, 0xFF)])],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: PAINT,
            target: TARGET,
        }],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            BACKDROP,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            appearance_context(),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                ConstraintId::new(1),
                OCCURRENCE,
                Srgb8::new([0xFF; 3]),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, PAINT)],
        evaluator,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![state(FIRST)]))
    .compile()
    .unwrap();
    let mut session = compiled.instantiate(STREAM).unwrap();

    let SessionState::Ready { current } = session.commit(update(1, 0x00)).unwrap() else {
        panic!("control revision must certify before the mutant is armed");
    };
    assert_eq!(current.outputs()[0].source_signal(), signal(0xFF));

    control.arm();
    let error = match session.commit(update(2, 0x00)) {
        Ok(_) => panic!("a failing final recheck must not commit"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        SessionUpdateError::Plan(ProgramSessionEvaluationError::FinalRecheckViolation {
            state_index: 0,
            case_index: 0,
            constraint: ConstraintId::new(1),
            subject: ProgramConstraintSubjectV1::ModeledOccurrence {
                occurrence: OCCURRENCE,
                context: appearance_context(),
            },
            hard_violation_count: 1,
        })
    );
    let SessionState::Ready { current } = session.state() else {
        panic!("the previous certificate must remain the sole committed state");
    };
    assert_eq!(current.outputs()[0].source_signal(), signal(0xFF));
}

#[test]
fn duplicate_physical_candidate_signal_is_typed_and_declaration_order_invariant() {
    let compile = |candidates| match program(
        vec![ConstraintInvocation::hard(
            ConstraintId::new(1),
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextLargeScale,
        )],
        vec![],
        candidates,
        vec![state(FIRST), state(SECOND)],
    )
    .compile()
    {
        Ok(_) => panic!("duplicate physical candidate signals must not compile"),
        Err(error) => error,
    };
    let canonical = compile(vec![candidate(FIRST, 0x66), candidate(SECOND, 0x66)]);
    let permuted = compile(vec![candidate(SECOND, 0x66), candidate(FIRST, 0x66)]);
    let expected = ProgramCompileError::DuplicateTargetCandidateSignal {
        target: TARGET,
        first: FIRST,
        duplicate: SECOND,
        signal: signal(0x66),
    };
    assert_eq!(canonical, expected);
    assert_eq!(permuted, expected);
}

#[test]
fn duplicate_joint_tuple_is_rejected_before_runtime() {
    let error = match program(
        vec![ConstraintInvocation::hard(
            ConstraintId::new(1),
            OCCURRENCE,
            Wcag22CriterionV1::Sc143TextLargeScale,
        )],
        vec![],
        vec![candidate(FIRST, 0x66), candidate(SECOND, 0xFF)],
        vec![state(FIRST), state(FIRST)],
    )
    .compile()
    {
        Ok(_) => panic!("duplicate tuple must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        ProgramCompileError::InvalidJointOrder(FiniteJointOrderErrorV1::DuplicateTuple {
            first: 0,
            duplicate: 1,
        })
    );
}
