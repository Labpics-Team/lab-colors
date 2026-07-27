use core::iter::FusedIterator;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

use crate::Srgb8;
use crate::appearance::{OccurrenceId, OpacityInputId, PaintId, SurfaceId, SurfaceInputPortId};
use crate::constraints::{
    ApplicableWcag22MeasurementV1, ExactConstraintIdentityV1, ExactIdentityCapabilityV1,
    ExactIdentityReleaseV1, ProgramVisiblePointBindingV1,
};
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextId, AppearanceContextSchemaReleaseId,
    BackgroundLuminanceRatio, ColorSignal, IEC_SRGB_D65_XYZ_FRAME_V1,
    MODELED_TRISTIMULUS_DERIVATION_CALLS, SurroundProfileId,
};
use crate::observation::{
    ObservationGroupId, ObservationPayloadInput, ObservationStreamId, ObservationUpdateInput,
    ObservedScenarioSetInput, Revision, ScenarioId, ScenarioInput, SurfaceInputBinding,
};
use crate::program::{
    AccessErrorV1, AssessmentV1, CertificateV1, ConflictCellV1, ConstraintModeV1,
    ConstraintSubjectV1, DeclaredSrgb8CleanSetViolationKindV1, ExactSrgb8EvidenceV1,
    ObservationHeadV1, ObservationV1, OperationV1, OutputSlotIdV1, OwnerV1, PhysicalPointV1,
    ProjectionV1, ScenarioV1, SessionV1, SignalV1, StateKindV1, SurroundV1, UpdateErrorKindV1,
    UpdateErrorV1, UpdateV1, VerdictV1, VerifiedCellV1, Wcag22Srgb8EvidenceV1,
};
use crate::program_session::{
    CORE_PROGRAM_ASSESSMENT_CALLS, CompiledCoreProgramV1, CompositionProfile, ConstraintId,
    ConstraintInvocation, ConstraintSet, CoreProgramConstraintInvocationV1,
    CoreProgramEvaluatorsV1, CoreProgramPassEvidenceV1, CoreProgramV1,
    CoreProgramViolationEvidenceV1, DeclaredJointSelectionV1, JointCandidateStateV1,
    ObservationGroup, Occurrence, OpacityInput, OutputBinding, OutputSlotId, Paint,
    PointPresentationRootV1, PointPresentationTargetV1, PresentationRootId, Program,
    ProgramConstraintCellV1, ProgramConstraintPassEvidenceV1, ProgramConstraintResultV1,
    ProgramConstraintSubjectV1, ProgramConstraintViolationEvidenceV1, Source, SourceId, Surface,
    Target, TargetCandidateChoiceV1, TargetCandidateId, TargetCandidateV1, TargetId,
};
use crate::session::SessionState;
use crate::wcag22::{Wcag22CriterionV1, wcag22_profile_v1};

const SOURCE: SourceId = SourceId::new(1);
const TARGET: TargetId = TargetId::new(2);
const PAINT: PaintId = PaintId::new(3);
const SURFACE: SurfaceId = SurfaceId::new(4);
const SURFACE_PORT: SurfaceInputPortId = SurfaceInputPortId::new(5);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(6);
const EXACT_CONSTRAINT: ConstraintId = ConstraintId::new(7);
const WCAG_CONSTRAINT: ConstraintId = ConstraintId::new(8);
const OUTPUT: OutputSlotId = OutputSlotId::new(9);
const SECOND_OUTPUT: OutputSlotId = OutputSlotId::new(19);
const GROUP: ObservationGroupId = ObservationGroupId::new(10);
const STREAM: ObservationStreamId = ObservationStreamId::new(11);
const CLEAN_CONSTRAINT: ConstraintId = ConstraintId::new(20);
const PRESENTATION_ROOT: PresentationRootId = PresentationRootId::new(21);

fn signal(bytes: [u8; 3]) -> ColorSignal {
    ColorSignal::from_srgb8(Srgb8::new(bytes))
}

fn context() -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        IEC_SRGB_D65_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(64.0).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        SurroundProfileId::AverageV1,
    )
}

fn observed_white() -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(1),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: vec![ScenarioInput {
                id: ScenarioId::new(1),
                bindings: vec![SurfaceInputBinding::new(SURFACE_PORT, signal([0xFF; 3]))],
            }],
        }),
    }
}

fn observed_backdrops(backdrops: &[[u8; 3]]) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(1),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: backdrops
                .iter()
                .enumerate()
                .map(|(index, backdrop)| ScenarioInput {
                    id: ScenarioId::new(index as u32 + 1),
                    bindings: vec![SurfaceInputBinding::new(SURFACE_PORT, signal(*backdrop))],
                })
                .collect(),
        }),
    }
}

fn finite_program(candidate_signals: [[u8; 3]; 2]) -> CompiledCoreProgramV1 {
    finite_program_with_outputs(candidate_signals, vec![OutputBinding::new(OUTPUT, PAINT)])
}

fn finite_program_with_outputs(
    candidate_signals: [[u8; 3]; 2],
    outputs: Vec<OutputBinding>,
) -> CompiledCoreProgramV1 {
    const FIRST: TargetCandidateId = TargetCandidateId::new(1);
    const SECOND: TargetCandidateId = TargetCandidateId::new(2);
    let program: CoreProgramV1 = Program::new(
        vec![Source::new(SOURCE, signal(candidate_signals[0]))],
        vec![Target::finite(
            TARGET,
            SOURCE,
            vec![
                TargetCandidateV1::new(FIRST, signal(candidate_signals[0])),
                TargetCandidateV1::new(SECOND, signal(candidate_signals[1])),
            ],
        )],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: PAINT,
            target: TARGET,
        }],
        vec![Surface::Input {
            id: SURFACE,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            SURFACE,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                WCAG_CONSTRAINT,
                OCCURRENCE,
                CoreProgramConstraintInvocationV1::Wcag22Srgb8(Wcag22CriterionV1::Sc143TextDefault),
            )],
            vec![ConstraintInvocation::report_only(
                EXACT_CONSTRAINT,
                OCCURRENCE,
                CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0; 3])),
            )],
        ),
        outputs,
        CoreProgramEvaluatorsV1,
    );
    program
        .with_joint_selection(DeclaredJointSelectionV1::new(vec![
            JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(TARGET, FIRST)]),
            JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(TARGET, SECOND)]),
        ]))
        .compile()
        .unwrap()
}

fn fixed_translucent_program() -> CompiledCoreProgramV1 {
    const OPACITY: OpacityInputId = OpacityInputId::new(12);
    const TRANSLUCENT_PAINT: PaintId = PaintId::new(13);
    Program::new(
        vec![Source::new(SOURCE, signal([0; 3]))],
        vec![Target::fixed(TARGET, SOURCE)],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![OpacityInput::new(OPACITY, 0.5)],
        vec![
            Paint::Solid {
                id: PAINT,
                target: TARGET,
            },
            Paint::Opacity {
                id: TRANSLUCENT_PAINT,
                source: PAINT,
                opacity: OPACITY,
            },
        ],
        vec![Surface::Input {
            id: SURFACE,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            TRANSLUCENT_PAINT,
            SURFACE,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::hard(
                EXACT_CONSTRAINT,
                OCCURRENCE,
                CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0x80; 3])),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, TRANSLUCENT_PAINT)],
        CoreProgramEvaluatorsV1,
    )
    .compile()
    .unwrap()
}

fn fixed_clean_set_program(source: [u8; 3]) -> CompiledCoreProgramV1 {
    let target = PointPresentationTargetV1::new(PRESENTATION_ROOT, OCCURRENCE);
    Program::new(
        vec![Source::new(SOURCE, signal(source))],
        vec![Target::fixed(TARGET, SOURCE)],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: PAINT,
            target: TARGET,
        }],
        vec![Surface::Input {
            id: SURFACE,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            SURFACE,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(),
        )],
        ConstraintSet::new(
            vec![],
            vec![ConstraintInvocation::declared_srgb8_clean_set_report_only(
                CLEAN_CONSTRAINT,
                target,
            )],
        ),
        vec![OutputBinding::new(OUTPUT, PAINT)],
        CoreProgramEvaluatorsV1,
    )
    .with_point_presentations(
        vec![PointPresentationRootV1::new(PRESENTATION_ROOT, OCCURRENCE)],
        vec![target],
    )
    .compile()
    .unwrap()
}

fn assert_public_observation_matches_core(
    public: ObservationV1<'_>,
    core: &crate::observation::RevisionBoundObservationV1,
) {
    assert_eq!(public.stream().value(), core.stream().value());
    assert_eq!(public.revision(), core.revision().value());
    assert_eq!(
        public
            .surface_input_ports()
            .map(|port| port.value())
            .collect::<Vec<_>>(),
        core.schema()
            .iter()
            .map(|port| port.value())
            .collect::<Vec<_>>(),
    );

    let public_cases = public
        .physical_cases()
        .map(|case| {
            let values = case
                .values()
                .map(|value| match value {
                    SignalV1::Iec61966Srgb8D65(value) => value,
                })
                .collect::<Vec<_>>();
            let provenance = case
                .provenance()
                .map(|scenario| scenario.value())
                .collect::<Vec<_>>();
            (values, provenance)
        })
        .collect::<Vec<_>>();
    let core_cases = (0..core.physical_case_count())
        .map(|case_index| {
            let values = core
                .physical_values(case_index)
                .unwrap()
                .iter()
                .map(|signal| signal.srgb8())
                .collect::<Vec<_>>();
            let provenance = core
                .provenance(case_index)
                .unwrap()
                .iter()
                .map(|scenario| scenario.value())
                .collect::<Vec<_>>();
            (values, provenance)
        })
        .collect::<Vec<_>>();
    assert_eq!(public_cases, core_cases);
}

fn assert_public_binding_matches_core(
    public: AssessmentV1<'_>,
    core: &ProgramVisiblePointBindingV1,
    expected_occurrence: OccurrenceId,
) -> (Srgb8, Srgb8) {
    let core_physical = core.physical();
    let core_occurrence = core_physical.occurrence();
    let core_program_occurrence = core_physical.program_occurrence();
    assert_eq!(core_program_occurrence.occurrence(), expected_occurrence);
    let public_binding = match public {
        AssessmentV1::ExactSrgb8(evidence) => evidence.binding(),
        AssessmentV1::Wcag22Srgb8(evidence) => evidence.binding(),
        AssessmentV1::DeclaredSrgb8CleanSet(_) => {
            panic!("fixture contains only occurrence-subject evaluators")
        }
    };
    let PhysicalPointV1::EncodedSrgb8SourceOver(public_physical) = public_binding.physical();
    assert_eq!(
        public_physical.subject_paint().value(),
        core_program_occurrence.subject().value()
    );
    assert_eq!(
        public_physical.backdrop_surface().value(),
        core_program_occurrence.backdrop_surface().value()
    );
    assert_eq!(
        public_physical.subject(),
        Srgb8::new(core_occurrence.subject_rgb())
    );
    assert_eq!(
        public_physical.opacity().to_bits(),
        core_occurrence.subject_opacity_bits()
    );
    assert_eq!(
        public_physical.backdrop(),
        Srgb8::new(core_occurrence.backdrop_rgb())
    );
    assert_eq!(
        public_physical.visible(),
        Srgb8::new(core_occurrence.output_rgb())
    );

    let public_context = public_binding.appearance_context();
    let core_context = core.context();
    assert_eq!(
        public_context.adapting_luminance_cd_m2().to_bits(),
        core_context.adapting_luminance_cd_m2().to_bits()
    );
    assert_eq!(
        public_context.background_luminance_ratio_yb_yw().to_bits(),
        core_context.background_luminance_ratio().to_bits()
    );
    let core_surround = match core_context.surround_profile() {
        SurroundProfileId::AverageV1 => SurroundV1::Average,
        SurroundProfileId::DimV1 => SurroundV1::Dim,
        SurroundProfileId::DarkV1 => SurroundV1::Dark,
    };
    assert_eq!(public_context.surround(), core_surround);

    (public_physical.visible(), public_physical.backdrop())
}

fn assert_public_assessment_matches_core(
    public: AssessmentV1<'_>,
    core: &ProgramConstraintResultV1<CoreProgramEvaluatorsV1>,
    expected_occurrence: OccurrenceId,
) {
    fn assert_exact_matches(
        public: ExactSrgb8EvidenceV1<'_>,
        verdict: VerdictV1,
        expected: Srgb8,
        actual: Srgb8,
        binding: &ProgramVisiblePointBindingV1,
        expected_occurrence: OccurrenceId,
    ) {
        assert_eq!(public.verdict(), verdict);
        assert_eq!(public.expected(), expected);
        let (visible, _) = assert_public_binding_matches_core(
            AssessmentV1::ExactSrgb8(public),
            binding,
            expected_occurrence,
        );
        assert_eq!(visible, actual);
    }

    fn assert_wcag_matches(
        public: Wcag22Srgb8EvidenceV1<'_>,
        verdict: VerdictV1,
        measurement: &ApplicableWcag22MeasurementV1,
        binding: &ProgramVisiblePointBindingV1,
        expected_occurrence: OccurrenceId,
    ) {
        assert_eq!(public.verdict(), verdict);
        assert_eq!(public.profile_id(), measurement.profile_id());
        assert_eq!(public.criterion(), measurement.criterion());
        assert_eq!(
            public.foreground_luminance(),
            measurement.measurement().foreground_luminance
        );
        assert_eq!(
            public.background_luminance(),
            measurement.measurement().background_luminance
        );
        assert_eq!(public.numerical_evidence(), measurement.evidence());
        let (visible, backdrop) = assert_public_binding_matches_core(
            AssessmentV1::Wcag22Srgb8(public),
            binding,
            expected_occurrence,
        );
        assert_eq!(visible, Srgb8::new(measurement.measurement().foreground));
        assert_eq!(backdrop, Srgb8::new(measurement.measurement().background));
    }

    match (public, core) {
        (
            AssessmentV1::ExactSrgb8(public),
            ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::ModeledOccurrence(
                CoreProgramPassEvidenceV1::ExactSrgb8(core),
            )),
        ) => assert_exact_matches(
            public,
            VerdictV1::Pass,
            core.target(),
            core.actual(),
            core.binding(),
            expected_occurrence,
        ),
        (
            AssessmentV1::ExactSrgb8(public),
            ProgramConstraintResultV1::Violation(
                ProgramConstraintViolationEvidenceV1::ModeledOccurrence(
                    CoreProgramViolationEvidenceV1::ExactSrgb8(core),
                ),
            ),
        ) => assert_exact_matches(
            public,
            VerdictV1::Violation,
            core.target(),
            core.actual(),
            core.binding(),
            expected_occurrence,
        ),
        (
            AssessmentV1::Wcag22Srgb8(public),
            ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::ModeledOccurrence(
                CoreProgramPassEvidenceV1::Wcag22Srgb8(core),
            )),
        ) => assert_wcag_matches(
            public,
            VerdictV1::Pass,
            core.measurement().value(),
            core.binding(),
            expected_occurrence,
        ),
        (
            AssessmentV1::Wcag22Srgb8(public),
            ProgramConstraintResultV1::Violation(
                ProgramConstraintViolationEvidenceV1::ModeledOccurrence(
                    CoreProgramViolationEvidenceV1::Wcag22Srgb8(core),
                ),
            ),
        ) => assert_wcag_matches(
            public,
            VerdictV1::Violation,
            core.measurement().value(),
            core.binding(),
            expected_occurrence,
        ),
        _ => panic!("public assessment family or verdict drifted from Core"),
    }
}

fn assert_verified_cell_matches_core(
    public: VerifiedCellV1<'_>,
    core: &ProgramConstraintCellV1<CoreProgramEvaluatorsV1>,
    selected_state_index: usize,
) {
    assert_eq!(core.candidate_state_index(), selected_state_index);
    assert_eq!(public.case_index(), core.case_index());
    assert_eq!(public.constraint().value(), core.constraint().value());
    let (occurrence, context) = match core.subject() {
        ProgramConstraintSubjectV1::ModeledOccurrence {
            occurrence,
            context,
        } => (occurrence, context),
        ProgramConstraintSubjectV1::PointPresentation { .. } => {
            panic!("fixture contains only occurrence-subject evaluators")
        }
    };
    assert_eq!(
        public.subject(),
        ConstraintSubjectV1::ModeledOccurrence {
            occurrence: crate::program::OccurrenceIdV1::new(occurrence.value()),
            context: crate::program::AppearanceContextV1::try_new(
                context.adapting_luminance_cd_m2(),
                context.background_luminance_ratio(),
                match context.surround_profile() {
                    SurroundProfileId::AverageV1 => SurroundV1::Average,
                    SurroundProfileId::DimV1 => SurroundV1::Dim,
                    SurroundProfileId::DarkV1 => SurroundV1::Dark,
                },
            )
            .unwrap(),
        },
    );
    assert_eq!(
        matches!(public.mode(), ConstraintModeV1::Hard),
        core.is_hard()
    );
    assert_public_assessment_matches_core(public.assessment(), core.result(), occurrence);
}

fn assert_conflict_cell_matches_core(
    public: ConflictCellV1<'_>,
    core: &ProgramConstraintCellV1<CoreProgramEvaluatorsV1>,
) {
    assert_eq!(public.state_index(), core.candidate_state_index());
    assert_eq!(public.case_index(), core.case_index());
    assert_eq!(public.constraint().value(), core.constraint().value());
    let (occurrence, context) = match core.subject() {
        ProgramConstraintSubjectV1::ModeledOccurrence {
            occurrence,
            context,
        } => (occurrence, context),
        ProgramConstraintSubjectV1::PointPresentation { .. } => {
            panic!("fixture contains only occurrence-subject evaluators")
        }
    };
    let ConstraintSubjectV1::ModeledOccurrence {
        occurrence: public_occurrence,
        context: public_context,
    } = public.subject()
    else {
        panic!("fixture contains only occurrence-subject evaluators");
    };
    assert_eq!(public_occurrence.value(), occurrence.value());
    assert_eq!(
        public_context.adapting_luminance_cd_m2().to_bits(),
        context.adapting_luminance_cd_m2().to_bits(),
    );
    assert_eq!(
        matches!(public.mode(), ConstraintModeV1::Hard),
        core.is_hard()
    );
    assert_public_assessment_matches_core(public.assessment(), core.result(), occurrence);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProjectionProbe {
    iterators: usize,
    certificates: usize,
    cases: usize,
    values: usize,
    provenance: usize,
    cells: usize,
    outputs: usize,
    operations: usize,
    exact_assessments: usize,
    wcag_assessments: usize,
    iterator_laws_hold: bool,
    checksum: u64,
}

impl ProjectionProbe {
    const fn new() -> Self {
        Self {
            iterators: 0,
            certificates: 0,
            cases: 0,
            values: 0,
            provenance: 0,
            cells: 0,
            outputs: 0,
            operations: 0,
            exact_assessments: 0,
            wcag_assessments: 0,
            iterator_laws_hold: true,
            checksum: 0,
        }
    }

    fn mix(&mut self, value: u64) {
        self.checksum = self.checksum.rotate_left(1) ^ value;
    }

    fn mix_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.mix(u64::from(*byte));
        }
    }

    fn mix_srgb8(&mut self, value: Srgb8) {
        self.mix_bytes(&value.bytes());
    }
}

fn consume_exact_fused<I>(
    mut iterator: I,
    probe: &mut ProjectionProbe,
    mut consume: impl FnMut(I::Item, &mut ProjectionProbe),
) where
    I: ExactSizeIterator + FusedIterator,
{
    probe.iterators += 1;
    let mut remaining = iterator.len();
    let initial_len = remaining;
    probe.iterator_laws_hold &= iterator.size_hint() == (remaining, Some(remaining));
    for _ in 0..initial_len {
        let Some(value) = iterator.next() else {
            probe.iterator_laws_hold = false;
            break;
        };
        remaining -= 1;
        probe.iterator_laws_hold &= iterator.len() == remaining;
        probe.iterator_laws_hold &= iterator.size_hint() == (remaining, Some(remaining));
        consume(value, probe);
    }
    probe.iterator_laws_hold &= remaining == 0;
    probe.iterator_laws_hold &= iterator.len() == 0;
    probe.iterator_laws_hold &= iterator.next().is_none();
    probe.iterator_laws_hold &= iterator.next().is_none();
}

fn consume_public_assessment(assessment: AssessmentV1<'_>, probe: &mut ProjectionProbe) {
    probe.mix(match assessment.verdict() {
        VerdictV1::Pass => 1,
        VerdictV1::Violation => 2,
    });
    let binding = match assessment {
        AssessmentV1::ExactSrgb8(evidence) => {
            probe.exact_assessments += 1;
            probe.mix_srgb8(evidence.expected());
            Some(evidence.binding())
        }
        AssessmentV1::Wcag22Srgb8(evidence) => {
            probe.wcag_assessments += 1;
            probe.mix_bytes(evidence.profile_id().key().as_bytes());
            probe.mix_bytes(evidence.criterion().key().as_bytes());
            probe.mix(evidence.foreground_luminance().lower());
            probe.mix(evidence.foreground_luminance().upper());
            probe.mix(evidence.background_luminance().lower());
            probe.mix(evidence.background_luminance().upper());
            probe.mix_bytes(evidence.numerical_evidence().class_key().as_bytes());
            Some(evidence.binding())
        }
        AssessmentV1::DeclaredSrgb8CleanSet(evidence) => {
            probe.mix(evidence.visible().map_or(0, |value| {
                let [r, g, b] = value.bytes();
                (u64::from(r) << 16) | (u64::from(g) << 8) | u64::from(b)
            }));
            probe.mix(match evidence.violation() {
                None => 0,
                Some(DeclaredSrgb8CleanSetViolationKindV1::FinalOwnedDomainAbsent) => 1,
                Some(DeclaredSrgb8CleanSetViolationKindV1::Rejected) => 2,
            });
            probe.mix(
                evidence
                    .rejected_blue_interval()
                    .map_or(0, |[lower, upper]| {
                        (u64::from(lower) << 8) | u64::from(upper)
                    }),
            );
            None
        }
    };

    let Some(binding) = binding else {
        return;
    };
    let PhysicalPointV1::EncodedSrgb8SourceOver(physical) = binding.physical();
    probe.mix(u64::from(physical.subject_paint().value()));
    probe.mix(u64::from(physical.backdrop_surface().value()));
    probe.mix_srgb8(physical.subject());
    probe.mix(physical.opacity().to_bits());
    probe.mix_srgb8(physical.backdrop());
    probe.mix_srgb8(physical.visible());

    let context = binding.appearance_context();
    probe.mix(context.adapting_luminance_cd_m2().to_bits());
    probe.mix(context.background_luminance_ratio_yb_yw().to_bits());
    probe.mix(match context.surround() {
        SurroundV1::Average => 1,
        SurroundV1::Dim => 2,
        SurroundV1::Dark => 3,
    });
}

#[test]
fn clean_set_projection_probe_binds_violation_kind_and_rejected_interval() {
    let owner = OwnerV1::from_compiled(fixed_clean_set_program([0, 200, 71]));
    let mut session = owner.instantiate(STREAM.value()).unwrap();
    let backdrop = [Srgb8::new([0; 3])];
    let scenarios = [ScenarioV1::new(1, &backdrop)];
    let projection = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Verified(certificate)) = projection.evidence().certificates().next()
    else {
        panic!("report-only clean-set rejection must retain a verified certificate");
    };
    let assessment = certificate.cells().next().unwrap().assessment();

    let mut actual = ProjectionProbe::new();
    consume_public_assessment(assessment, &mut actual);

    let mut expected = ProjectionProbe::new();
    expected.mix(2);
    expected.mix((u64::from(200_u8) << 8) | u64::from(71_u8));
    expected.mix(2);
    expected.mix((u64::from(71_u8) << 8) | u64::from(101_u8));
    assert_eq!(actual.checksum, expected.checksum);
}

fn consume_public_projection(projection: ProjectionV1<'_, '_>) -> ProjectionProbe {
    let view = projection.evidence();
    let mut probe = ProjectionProbe::new();
    probe.mix(match view.kind() {
        StateKindV1::Waiting => 1,
        StateKindV1::Ready => 2,
        StateKindV1::Failed => 3,
        StateKindV1::Stale => 4,
    });
    match view.observation_head() {
        ObservationHeadV1::Empty => probe.mix(0),
        ObservationHeadV1::Unknown {
            stream,
            revision,
            reason_id,
        } => {
            probe.mix(1);
            probe.mix(u64::from(stream.value()));
            probe.mix(revision);
            probe.mix(u64::from(reason_id));
        }
        ObservationHeadV1::Observed { stream, revision } => {
            probe.mix(2);
            probe.mix(u64::from(stream.value()));
            probe.mix(revision);
        }
    }
    probe.mix(
        view.cause_certificate_index()
            .map_or(0, |index| index as u64 + 1),
    );

    consume_exact_fused(view.certificates(), &mut probe, |certificate, probe| {
        probe.certificates += 1;
        probe.mix_bytes(certificate.content_identity().as_bytes());
        let observation = certificate.observation();
        probe.mix(u64::from(observation.stream().value()));
        probe.mix(observation.revision());
        consume_exact_fused(observation.surface_input_ports(), probe, |port, probe| {
            probe.mix(u64::from(port.value()));
        });
        consume_exact_fused(observation.physical_cases(), probe, |case, probe| {
            probe.cases += 1;
            consume_exact_fused(case.values(), probe, |value, probe| {
                probe.values += 1;
                let SignalV1::Iec61966Srgb8D65(value) = value;
                probe.mix_srgb8(value);
            });
            consume_exact_fused(case.provenance(), probe, |scenario, probe| {
                probe.provenance += 1;
                probe.mix(u64::from(scenario.value()));
            });
        });
        match certificate {
            CertificateV1::Verified(verified) => {
                probe.mix(
                    verified
                        .selected_state_index()
                        .map_or(0, |index| index as u64 + 1),
                );
                consume_exact_fused(verified.cells(), probe, |cell, probe| {
                    probe.cells += 1;
                    probe.mix(cell.case_index() as u64);
                    probe.mix(u64::from(cell.constraint().value()));
                    match cell.subject() {
                        ConstraintSubjectV1::ModeledOccurrence { occurrence, .. } => {
                            probe.mix(u64::from(occurrence.value()));
                        }
                        ConstraintSubjectV1::PointPresentation {
                            root,
                            occurrence,
                            terminal,
                        } => {
                            probe.mix(u64::from(root.value()));
                            probe.mix(u64::from(occurrence.value()));
                            probe.mix(u64::from(terminal.value()));
                        }
                    }
                    probe.mix(match cell.mode() {
                        ConstraintModeV1::Hard => 1,
                        ConstraintModeV1::ReportOnly => 2,
                    });
                    consume_public_assessment(cell.assessment(), probe);
                });
                consume_exact_fused(verified.outputs(), probe, |output, probe| {
                    probe.outputs += 1;
                    probe.mix(u64::from(output.output_slot().value()));
                    probe.mix(u64::from(output.paint().value()));
                    probe.mix_srgb8(output.source());
                    probe.mix(output.opacity().to_bits());
                });
            }
            CertificateV1::Conflict(conflict) => {
                probe.mix(conflict.considered_state_count() as u64);
                consume_exact_fused(conflict.cells(), probe, |cell, probe| {
                    probe.cells += 1;
                    probe.mix(cell.state_index() as u64);
                    probe.mix(cell.case_index() as u64);
                    probe.mix(u64::from(cell.constraint().value()));
                    match cell.subject() {
                        ConstraintSubjectV1::ModeledOccurrence { occurrence, .. } => {
                            probe.mix(u64::from(occurrence.value()));
                        }
                        ConstraintSubjectV1::PointPresentation {
                            root,
                            occurrence,
                            terminal,
                        } => {
                            probe.mix(u64::from(root.value()));
                            probe.mix(u64::from(occurrence.value()));
                            probe.mix(u64::from(terminal.value()));
                        }
                    }
                    probe.mix(match cell.mode() {
                        ConstraintModeV1::Hard => 1,
                        ConstraintModeV1::ReportOnly => 2,
                    });
                    consume_public_assessment(cell.assessment(), probe);
                });
            }
        }
    });
    consume_exact_fused(projection.operations(), &mut probe, |operation, probe| {
        probe.operations += 1;
        match operation {
            OperationV1::Set(set) => {
                probe.mix(1);
                probe.mix(u64::from(set.output_slot().value()));
                probe.mix_srgb8(set.source());
                probe.mix(set.opacity().to_bits());
                probe.mix_bytes(set.certificate().content_identity().as_bytes());
                probe.mix(set.certificate().observation().revision());
            }
            OperationV1::Remove(remove) => {
                probe.mix(2);
                probe.mix(u64::from(remove.output_slot().value()));
            }
        }
    });
    std::hint::black_box(probe)
}

fn assert_observed_head(head: ObservationHeadV1, expected_stream: u32, expected_revision: u64) {
    let ObservationHeadV1::Observed { stream, revision } = head else {
        panic!("raw evidence must remain a closed Observed payload");
    };
    assert_eq!(stream.value(), expected_stream);
    assert_eq!(revision, expected_revision);
}

fn assert_unknown_head(
    head: ObservationHeadV1,
    expected_stream: u32,
    expected_revision: u64,
    expected_reason: u32,
) {
    let ObservationHeadV1::Unknown {
        stream,
        revision,
        reason_id,
    } = head
    else {
        panic!("raw evidence must remain a closed Unknown payload");
    };
    assert_eq!(stream.value(), expected_stream);
    assert_eq!(revision, expected_revision);
    assert_eq!(reason_id, expected_reason);
}

#[test]
fn one_program_retains_typed_exact_and_wcag22_outcomes() {
    let declared_context = context();
    let program: CoreProgramV1 = Program::new(
        vec![Source::new(SOURCE, signal([0; 3]))],
        vec![Target::fixed(TARGET, SOURCE)],
        ObservationGroup::new(GROUP, vec![SURFACE_PORT]),
        vec![],
        vec![Paint::Solid {
            id: PAINT,
            target: TARGET,
        }],
        vec![Surface::Input {
            id: SURFACE,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            PAINT,
            SURFACE,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            declared_context,
        )],
        ConstraintSet::new(
            vec![
                ConstraintInvocation::hard(
                    EXACT_CONSTRAINT,
                    OCCURRENCE,
                    CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0; 3])),
                ),
                ConstraintInvocation::hard(
                    WCAG_CONSTRAINT,
                    OCCURRENCE,
                    CoreProgramConstraintInvocationV1::Wcag22Srgb8(
                        Wcag22CriterionV1::Sc143TextDefault,
                    ),
                ),
            ],
            vec![],
        ),
        vec![OutputBinding::new(OUTPUT, PAINT)],
        CoreProgramEvaluatorsV1,
    );
    let compiled = program.compile().unwrap();

    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = session.update(observed_white()).unwrap() else {
        panic!("opaque black on white must satisfy both authored hard constraints");
    };
    let [exact, wcag] = current.report().cells() else {
        panic!("one case times two heterogeneous constraints must produce two cells");
    };

    let ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::ModeledOccurrence(
        CoreProgramPassEvidenceV1::ExactSrgb8(evidence),
    )) = exact.result()
    else {
        panic!("the first cell must retain Exact-specific pass evidence");
    };
    assert_eq!(
        evidence.identity(),
        &ExactConstraintIdentityV1::FinalSrgb8IdentityV1,
    );
    assert_eq!(evidence.release(), &ExactIdentityReleaseV1::V1);
    assert_eq!(
        evidence.capability(),
        &ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1,
    );
    assert_eq!(evidence.binding().context(), declared_context);

    let ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::ModeledOccurrence(
        CoreProgramPassEvidenceV1::Wcag22Srgb8(evidence),
    )) = wcag.result()
    else {
        panic!("the second cell must retain WCAG22-specific pass evidence");
    };
    assert_eq!(evidence.release(), &wcag22_profile_v1().profile_id);
    assert_eq!(evidence.binding().context(), declared_context);
    assert_ne!(
        core::any::type_name_of_val(evidence.identity()),
        core::any::type_name_of_val(exact.result()),
    );
}

#[test]
fn fixed_public_certificate_retains_none_selection_and_nonunit_output_opacity() {
    let owner = OwnerV1::from_compiled(fixed_translucent_program());
    let mut session = owner.instantiate(STREAM.value()).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [ScenarioV1::new(1, &white)];
    let projection = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let Some(CertificateV1::Verified(certificate)) = projection.evidence().certificates().next()
    else {
        panic!("the exact translucent midpoint must be verified");
    };
    assert_eq!(certificate.selected_state_index(), None);
    let AssessmentV1::ExactSrgb8(assessment) = certificate.cells().next().unwrap().assessment()
    else {
        panic!("the fixed Program has one Exact certificate cell");
    };
    let PhysicalPointV1::EncodedSrgb8SourceOver(physical) = assessment.binding().physical();
    assert_eq!(physical.opacity().to_bits(), 0.5_f64.to_bits());
    assert_eq!(physical.visible(), Srgb8::new([0x80; 3]));
    assert_eq!(
        certificate.outputs().next().unwrap().opacity().to_bits(),
        physical.opacity().to_bits()
    );
    let Some(OperationV1::Set(set)) = projection.operations().next() else {
        panic!("Verified must emit one Set");
    };
    assert_eq!(set.opacity().to_bits(), physical.opacity().to_bits());
}

#[test]
fn mixed_families_select_only_a_state_that_passes_every_case_then_recheck_it() {
    let compiled = finite_program([[0x80; 3], [0; 3]]);
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Ready { current } = session
        .update(observed_backdrops(&[[0xFF; 3], [0x80; 3]]))
        .unwrap()
    else {
        panic!("the later black state must pass WCAG22 over both physical cases");
    };

    assert_eq!(current.selected_state_index(), Some(1));
    let cells = current.report().cells();
    assert_eq!(cells.len(), 4);
    assert_eq!(
        cells
            .iter()
            .map(|cell| (cell.case_index(), cell.constraint(), cell.is_hard()))
            .collect::<Vec<_>>(),
        vec![
            (0, EXACT_CONSTRAINT, false),
            (0, WCAG_CONSTRAINT, true),
            (1, EXACT_CONSTRAINT, false),
            (1, WCAG_CONSTRAINT, true),
        ],
    );
    for cell in [cells[0].result(), cells[2].result()] {
        assert!(matches!(
            cell,
            ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::ModeledOccurrence(
                CoreProgramPassEvidenceV1::ExactSrgb8(_)
            ))
        ));
    }
    for cell in [cells[1].result(), cells[3].result()] {
        assert!(matches!(
            cell,
            ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::ModeledOccurrence(
                CoreProgramPassEvidenceV1::Wcag22Srgb8(_)
            ))
        ));
    }
}

#[test]
fn mixed_family_conflict_is_exhaustive_and_keeps_report_only_non_gating() {
    let compiled = finite_program([[0x80; 3], [0xFF; 3]]);
    let mut session = compiled.instantiate(STREAM).unwrap();
    let SessionState::Failed { cause, previous } = session.update(observed_white()).unwrap() else {
        panic!("neither gray nor white satisfies default text contrast on white");
    };
    assert!(previous.is_none());
    assert_eq!(cause.considered_state_count(), 2);
    let cells = cause.report().cells();
    assert_eq!(cells.len(), 4);
    assert_eq!(
        cells
            .iter()
            .map(|cell| (
                cell.candidate_state_index(),
                cell.constraint(),
                cell.is_hard()
            ))
            .collect::<Vec<_>>(),
        vec![
            (0, EXACT_CONSTRAINT, false),
            (0, WCAG_CONSTRAINT, true),
            (1, EXACT_CONSTRAINT, false),
            (1, WCAG_CONSTRAINT, true),
        ],
    );
    assert!(cells.iter().all(|cell| cell.result().is_violation()));
    assert!(matches!(
        cells[0].result(),
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::ModeledOccurrence(
                CoreProgramViolationEvidenceV1::ExactSrgb8(_)
            )
        )
    ));
    assert!(matches!(
        cells[1].result(),
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::ModeledOccurrence(
                CoreProgramViolationEvidenceV1::Wcag22Srgb8(_)
            )
        )
    ));
}

#[test]
fn public_projection_preserves_every_exposed_ready_and_conflict_field_against_core() {
    let ready_core_owner = finite_program([[0x80; 3], [0; 3]]);
    let ready_identity = ready_core_owner.content_identity();
    let ready_public_owner = OwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    let mut ready_core_session = ready_core_owner.instantiate(STREAM).unwrap();
    let mut ready_public_session = ready_public_owner.instantiate(STREAM.value()).unwrap();
    let ready_backdrops = [[0xFF; 3], [0xFF; 3], [0x80; 3]];
    let SessionState::Ready {
        current: core_verified,
    } = ready_core_session
        .update(observed_backdrops(&ready_backdrops))
        .unwrap()
    else {
        panic!("black is the first state that passes both canonical physical cases");
    };
    let ready_white = [Srgb8::new([0xFF; 3])];
    let ready_gray = [Srgb8::new([0x80; 3])];
    let ready_scenarios = [
        ScenarioV1::new(1, &ready_white),
        ScenarioV1::new(2, &ready_white),
        ScenarioV1::new(3, &ready_gray),
    ];
    let public_ready = ready_public_owner
        .update(
            &mut ready_public_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &ready_scenarios,
            },
        )
        .unwrap();
    let mut public_certificates = public_ready.evidence().certificates();
    assert_eq!(public_certificates.len(), 1);
    let Some(CertificateV1::Verified(public_verified)) = public_certificates.next() else {
        panic!("Ready must retain exactly one Verified certificate");
    };
    assert!(public_certificates.next().is_none());
    assert!(public_certificates.next().is_none());
    assert_eq!(
        public_verified.content_identity().as_bytes(),
        ready_identity.as_bytes()
    );
    assert_public_observation_matches_core(
        public_verified.observation(),
        core_verified.report().observation(),
    );
    assert_eq!(
        public_verified.selected_state_index(),
        core_verified.selected_state_index()
    );
    let selected_state_index = core_verified.selected_state_index().unwrap();
    let mut public_cells = public_verified.cells();
    let mut core_cells = core_verified.report().cells().iter();
    assert_eq!(public_cells.len(), core_cells.len());
    while let (Some(public), Some(core)) = (public_cells.next(), core_cells.next()) {
        assert_verified_cell_matches_core(public, core, selected_state_index);
        assert_eq!(public_cells.len(), core_cells.len());
    }
    assert!(public_cells.next().is_none());
    assert!(public_cells.next().is_none());
    assert!(core_cells.next().is_none());

    let mut public_outputs = public_verified.outputs();
    let mut core_outputs = core_verified.outputs().iter();
    assert_eq!(public_outputs.len(), core_outputs.len());
    while let (Some(public), Some(core)) = (public_outputs.next(), core_outputs.next()) {
        assert_eq!(public.output_slot().value(), core.output().value());
        assert_eq!(public.paint().value(), core.paint().id().value());
        assert_eq!(public.source(), core.paint().source());
        assert_eq!(
            public.opacity().to_bits(),
            core.paint().opacity().value().to_bits()
        );
        assert_eq!(public_outputs.len(), core_outputs.len());
    }
    assert!(public_outputs.next().is_none());
    assert!(public_outputs.next().is_none());
    assert!(core_outputs.next().is_none());
    let mut ready_operations = public_ready.operations();
    assert_eq!(ready_operations.len(), core_verified.outputs().len());
    for core_output in core_verified.outputs() {
        let Some(OperationV1::Set(set)) = ready_operations.next() else {
            panic!("every certified output must become exactly one Set");
        };
        assert_eq!(set.output_slot().value(), core_output.output().value());
        assert_eq!(set.source(), core_output.paint().source());
        assert_eq!(
            set.opacity().to_bits(),
            core_output.paint().opacity().value().to_bits()
        );
        assert_eq!(
            set.certificate().content_identity(),
            public_verified.content_identity()
        );
        assert_eq!(
            set.certificate().observation().revision(),
            public_verified.observation().revision()
        );
    }
    assert!(ready_operations.next().is_none());
    assert!(ready_operations.next().is_none());

    let conflict_core_owner = finite_program([[0; 3], [0xFF; 3]]);
    let conflict_identity = conflict_core_owner.content_identity();
    let conflict_public_owner = OwnerV1::from_compiled(finite_program([[0; 3], [0xFF; 3]]));
    let mut conflict_core_session = conflict_core_owner.instantiate(STREAM).unwrap();
    let mut conflict_public_session = conflict_public_owner.instantiate(STREAM.value()).unwrap();
    let conflict_backdrops = [[0xFF; 3], [0; 3]];
    let SessionState::Failed {
        cause: core_conflict,
        previous: None,
    } = conflict_core_session
        .update(observed_backdrops(&conflict_backdrops))
        .unwrap()
    else {
        panic!("neither black nor white passes both opposing physical cases");
    };
    let conflict_white = [Srgb8::new([0xFF; 3])];
    let conflict_black = [Srgb8::new([0; 3])];
    let conflict_scenarios = [
        ScenarioV1::new(1, &conflict_white),
        ScenarioV1::new(2, &conflict_black),
    ];
    let public_failed = conflict_public_owner
        .update(
            &mut conflict_public_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &conflict_scenarios,
            },
        )
        .unwrap();
    let mut public_certificates = public_failed.evidence().certificates();
    assert_eq!(public_certificates.len(), 1);
    let Some(CertificateV1::Conflict(public_conflict)) = public_certificates.next() else {
        panic!("Failed without previous state must retain one Conflict certificate");
    };
    assert!(public_certificates.next().is_none());
    assert!(public_certificates.next().is_none());
    assert_eq!(
        public_conflict.content_identity().as_bytes(),
        conflict_identity.as_bytes()
    );
    assert_public_observation_matches_core(
        public_conflict.observation(),
        core_conflict.report().observation(),
    );
    assert_eq!(
        public_conflict.considered_state_count(),
        core_conflict.considered_state_count()
    );
    let core_passes = core_conflict
        .report()
        .cells()
        .iter()
        .filter(|cell| !cell.result().is_violation())
        .count();
    assert!(core_passes > 0);
    assert!(core_passes < core_conflict.report().cells().len());
    let mut public_cells = public_conflict.cells();
    let mut core_cells = core_conflict.report().cells().iter();
    assert_eq!(public_cells.len(), core_cells.len());
    while let (Some(public), Some(core)) = (public_cells.next(), core_cells.next()) {
        assert_conflict_cell_matches_core(public, core);
        assert_eq!(public_cells.len(), core_cells.len());
    }
    assert!(public_cells.next().is_none());
    assert!(public_cells.next().is_none());
    assert!(core_cells.next().is_none());
    let mut failed_operations = public_failed.operations();
    assert_eq!(failed_operations.len(), 1);
    assert!(matches!(
        failed_operations.next(),
        Some(OperationV1::Remove(_))
    ));
    assert!(failed_operations.next().is_none());
    assert!(failed_operations.next().is_none());
}

#[test]
fn committed_projection_is_zero_alloc_and_repeats_no_composite_transform_or_evaluator_dispatch() {
    crate::composition::reset_source_over_evaluation_count();
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(0));
    CORE_PROGRAM_ASSESSMENT_CALLS.with(|calls| calls.set(0));

    let owner = OwnerV1::from_compiled(finite_program([[0; 3], [0xFF; 3]]));
    let mut session = owner.instantiate(STREAM.value()).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let white_only = [ScenarioV1::new(1, &white)];
    owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &white_only,
            },
        )
        .unwrap();
    let Some(CertificateV1::Verified(ready_certificate)) = session.evidence().certificates().next()
    else {
        panic!("black must be selected for the white-only physical support");
    };
    assert_eq!(ready_certificate.selected_state_index(), Some(0));

    let ready_compositions = crate::composition::source_over_evaluation_count();
    let ready_derivations = MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get);
    let ready_assessments = CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get);
    assert!(ready_compositions > 0);
    assert_eq!(
        ready_derivations, 0,
        "the encoded-only evaluator set must not derive an LCS view",
    );
    assert!(ready_assessments > 0);
    let (ready_probe, ready_allocations) = crate::test_support::measured_allocations(|| {
        consume_public_projection(std::hint::black_box(owner.project(&session).unwrap()))
    });
    assert_eq!(ready_allocations, 0);
    assert!(ready_probe.iterator_laws_hold);
    assert_eq!(ready_probe.certificates, 1);
    assert_eq!(ready_probe.cases, 1);
    assert_eq!(ready_probe.values, 1);
    assert_eq!(ready_probe.provenance, 1);
    assert_eq!(ready_probe.cells, 2);
    assert_eq!(ready_probe.outputs, 1);
    assert_eq!(ready_probe.operations, 1);
    assert_eq!(ready_probe.exact_assessments, 1);
    assert_eq!(ready_probe.wcag_assessments, 1);
    assert_ne!(ready_probe.checksum, 0);
    assert_eq!(
        crate::composition::source_over_evaluation_count(),
        ready_compositions
    );
    assert_eq!(
        MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get),
        ready_derivations
    );
    assert_eq!(
        CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get),
        ready_assessments
    );

    owner
        .update(
            &mut session,
            UpdateV1::Unknown {
                revision: 2,
                reason_id: 7,
            },
        )
        .unwrap();
    let stale_compositions = crate::composition::source_over_evaluation_count();
    let stale_derivations = MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get);
    let stale_assessments = CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get);
    let (stale_probe, stale_allocations) = crate::test_support::measured_allocations(|| {
        consume_public_projection(std::hint::black_box(owner.project(&session).unwrap()))
    });
    assert_eq!(stale_allocations, 0);
    assert!(stale_probe.iterator_laws_hold);
    assert_eq!(stale_probe.certificates, 1);
    assert_eq!(stale_probe.cells, 2);
    assert_eq!(stale_probe.outputs, 1);
    assert_eq!(stale_probe.operations, 1);
    assert_ne!(stale_probe.checksum, ready_probe.checksum);
    assert_eq!(
        crate::composition::source_over_evaluation_count(),
        stale_compositions
    );
    assert_eq!(
        MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get),
        stale_derivations
    );
    assert_eq!(
        CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get),
        stale_assessments
    );

    let black = [Srgb8::new([0; 3])];
    let opposing_backdrops = [ScenarioV1::new(1, &white), ScenarioV1::new(2, &black)];
    owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 3,
                scenarios: &opposing_backdrops,
            },
        )
        .unwrap();
    let failed_compositions = crate::composition::source_over_evaluation_count();
    let failed_derivations = MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get);
    let failed_assessments = CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get);
    let (failed_probe, failed_allocations) = crate::test_support::measured_allocations(|| {
        consume_public_projection(std::hint::black_box(owner.project(&session).unwrap()))
    });
    assert_eq!(failed_allocations, 0);
    assert!(failed_probe.iterator_laws_hold);
    assert_eq!(failed_probe.certificates, 2);
    assert_eq!(failed_probe.cases, 3);
    assert_eq!(failed_probe.values, 3);
    assert_eq!(failed_probe.provenance, 3);
    assert_eq!(failed_probe.cells, 10);
    assert_eq!(failed_probe.outputs, 1);
    assert_eq!(failed_probe.operations, 1);
    assert_eq!(failed_probe.exact_assessments, 5);
    assert_eq!(failed_probe.wcag_assessments, 5);
    assert_ne!(failed_probe.checksum, stale_probe.checksum);
    assert_eq!(
        crate::composition::source_over_evaluation_count(),
        failed_compositions
    );
    assert_eq!(
        MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get),
        failed_derivations
    );
    assert_eq!(
        CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get),
        failed_assessments
    );
}

#[test]
fn observation_projection_is_invariant_under_every_scenario_permutation_and_keeps_provenance() {
    const PERMUTATIONS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    const IDS: [u32; 3] = [9, 4, 3];
    const BACKDROPS: [[u8; 3]; 3] = [[0xFF; 3], [0x80; 3], [0xFF; 3]];

    crate::composition::reset_source_over_evaluation_count();
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(0));
    CORE_PROGRAM_ASSESSMENT_CALLS.with(|calls| calls.set(0));

    let core_owner = finite_program([[0x80; 3], [0; 3]]);
    let content_identity = core_owner.content_identity();
    let public_owner = OwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    let mut core_session = core_owner.instantiate(STREAM).unwrap();
    let mut public_session = public_owner.instantiate(STREAM.value()).unwrap();
    let public_values = BACKDROPS.map(|value| [Srgb8::new(value)]);
    let mut first_public_backing = None;
    let mut evaluation_counts_after_first = None;

    for permutation in PERMUTATIONS {
        let core_update = ObservationUpdateInput {
            stream: STREAM,
            revision: Revision::new(1),
            payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
                scenarios: permutation
                    .iter()
                    .map(|index| ScenarioInput {
                        id: ScenarioId::new(IDS[*index]),
                        bindings: vec![SurfaceInputBinding::new(
                            SURFACE_PORT,
                            signal(BACKDROPS[*index]),
                        )],
                    })
                    .collect(),
            }),
        };
        let SessionState::Ready {
            current: core_verified,
        } = core_session.update(core_update).unwrap()
        else {
            panic!("black must pass both deduplicated physical cases");
        };
        let public_scenarios =
            permutation.map(|index| ScenarioV1::new(IDS[index], &public_values[index]));
        let public_state = public_owner
            .update(
                &mut public_session,
                UpdateV1::Observed {
                    revision: 1,
                    scenarios: &public_scenarios,
                },
            )
            .unwrap();
        let Some(CertificateV1::Verified(public_verified)) =
            public_state.evidence().certificates().next()
        else {
            panic!("the canonical observation must keep one Verified certificate");
        };
        assert_eq!(
            public_verified.content_identity().as_bytes(),
            content_identity.as_bytes()
        );
        assert_public_observation_matches_core(
            public_verified.observation(),
            core_verified.report().observation(),
        );

        let projected_cases = public_verified
            .observation()
            .physical_cases()
            .map(|case| {
                let values = case
                    .values()
                    .map(|value| match value {
                        SignalV1::Iec61966Srgb8D65(value) => value,
                    })
                    .collect::<Vec<_>>();
                let provenance = case
                    .provenance()
                    .map(|scenario| scenario.value())
                    .collect::<Vec<_>>();
                (values, provenance)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            projected_cases,
            [
                (vec![Srgb8::new([0x80; 3])], vec![4]),
                (vec![Srgb8::new([0xFF; 3])], vec![3, 9]),
            ]
        );

        let backing = CertificateV1::Verified(public_verified).observation_backing_ptr_for_test();
        match first_public_backing {
            None => {
                first_public_backing = Some(backing);
                evaluation_counts_after_first = Some((
                    crate::composition::source_over_evaluation_count(),
                    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get),
                    CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get),
                ));
            }
            Some(first) => {
                assert_eq!(backing, first);
                assert_eq!(
                    evaluation_counts_after_first,
                    Some((
                        crate::composition::source_over_evaluation_count(),
                        MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get),
                        CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get),
                    ))
                );
            }
        }
    }
}

#[test]
fn concrete_program_projects_ready_and_fail_closed_stale_operations() {
    let owner = OwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    assert_eq!(owner.surface_input_port_count(), 1);
    assert_eq!(
        owner.output_slots().collect::<Vec<_>>(),
        [OutputSlotIdV1::new(OUTPUT.value())]
    );

    let mut session = owner.instantiate(11).unwrap();
    let initial = session.evidence();
    assert_eq!(initial.kind(), StateKindV1::Waiting);
    assert_eq!(initial.observation_head(), ObservationHeadV1::Empty);
    assert_eq!(initial.certificates().len(), 0);
    assert_eq!(owner.project(&session).unwrap().operations().len(), 0);

    let white = [Srgb8::new([0xFF; 3])];
    let gray = [Srgb8::new([0x80; 3])];
    let scenarios = [ScenarioV1::new(2, &gray), ScenarioV1::new(1, &white)];
    let ready = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let ready_evidence = ready.evidence();
    assert_eq!(ready_evidence.kind(), StateKindV1::Ready);
    assert_observed_head(ready_evidence.observation_head(), STREAM.value(), 1);
    assert_eq!(ready_evidence.cause_certificate_index(), None);
    let certificates = ready_evidence.certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    assert!(matches!(certificates[0], CertificateV1::Verified(_)));
    assert_eq!(certificates[0].observation().revision(), 1);
    let ready_backing = certificates[0].observation_backing_ptr_for_test();
    let mut operations = ready.operations();
    let Some(OperationV1::Set(set)) = operations.next() else {
        panic!("Ready must emit one Set operation");
    };
    assert_eq!(set.output_slot(), OutputSlotIdV1::new(OUTPUT.value()));
    assert_eq!(set.source(), Srgb8::new([0; 3]));
    assert_eq!(set.opacity(), 1.0);
    assert_eq!(
        set.certificate().observation().revision(),
        certificates[0].observation().revision()
    );
    assert_eq!(
        set.certificate().content_identity(),
        certificates[0].content_identity()
    );
    assert_eq!(
        CertificateV1::Verified(set.certificate()).observation_backing_ptr_for_test(),
        ready_backing
    );
    assert!(operations.next().is_none());
    drop(operations);
    drop(certificates);

    let reordered = [ScenarioV1::new(1, &white), ScenarioV1::new(2, &gray)];
    let replay = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &reordered,
            },
        )
        .unwrap();
    let replay_certificate = replay.evidence().certificates().next().unwrap();
    assert_eq!(
        replay_certificate.observation_backing_ptr_for_test(),
        ready_backing,
        "scenario permutation at the same revision must be exact idempotence"
    );

    let changed_same_revision = [ScenarioV1::new(1, &white)];
    let error = match owner.update(
        &mut session,
        UpdateV1::Observed {
            revision: 1,
            scenarios: &changed_same_revision,
        },
    ) {
        Ok(_) => panic!("changed payload at the same revision must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), UpdateErrorKindV1::RevisionConflict);
    assert_eq!(session.evidence().kind(), StateKindV1::Ready);
    assert_observed_head(session.evidence().observation_head(), STREAM.value(), 1);

    let stale = owner
        .update(
            &mut session,
            UpdateV1::Unknown {
                revision: 2,
                reason_id: 7,
            },
        )
        .unwrap();
    let stale_evidence = stale.evidence();
    assert_eq!(stale_evidence.kind(), StateKindV1::Stale);
    assert_unknown_head(stale_evidence.observation_head(), STREAM.value(), 2, 7);
    let certificates = stale_evidence.certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    assert!(matches!(certificates[0], CertificateV1::Verified(_)));
    assert_eq!(certificates[0].observation().revision(), 1);
    assert_eq!(
        certificates[0].observation_backing_ptr_for_test(),
        ready_backing
    );
    let mut operations = stale.operations();
    let Some(OperationV1::Remove(remove)) = operations.next() else {
        panic!("Stale must remove an output that lacks current evidence");
    };
    assert_eq!(remove.output_slot(), OutputSlotIdV1::new(OUTPUT.value()));
    assert!(operations.next().is_none());
}

#[test]
fn unknown_replacement_session_revokes_an_existing_owner_output() {
    let owner = OwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [ScenarioV1::new(1, &white)];

    let mut first_session = owner.instantiate(11).unwrap();
    let ready = owner
        .update(
            &mut first_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let mut sink = None;
    for operation in ready.operations() {
        match operation {
            OperationV1::Set(set) => sink = Some((set.source(), set.opacity())),
            OperationV1::Remove(_) => sink = None,
        }
    }
    assert!(
        sink.is_some(),
        "the first Session must populate the shared sink"
    );
    drop(first_session);

    let mut replacement_session = owner.instantiate(12).unwrap();
    let unknown = owner
        .update(
            &mut replacement_session,
            UpdateV1::Unknown {
                revision: 1,
                reason_id: 7,
            },
        )
        .unwrap();
    assert_eq!(unknown.evidence().kind(), StateKindV1::Waiting);
    assert_unknown_head(unknown.evidence().observation_head(), 12, 1, 7);
    assert_eq!(unknown.evidence().certificates().len(), 0);

    let mut operations = unknown.operations();
    let Some(OperationV1::Remove(remove)) = operations.next() else {
        panic!("an explicit Unknown must revoke an existing owner output during handoff");
    };
    assert_eq!(remove.output_slot(), OutputSlotIdV1::new(OUTPUT.value()));
    sink = None;
    assert!(operations.next().is_none());
    assert!(
        sink.is_none(),
        "the replacement Session must not leave stale paint"
    );
}

#[test]
fn concrete_program_failed_always_removes_but_retains_previous_evidence() {
    let white = [Srgb8::new([0xFF; 3])];
    let black = [Srgb8::new([0; 3])];
    let white_only = [ScenarioV1::new(1, &white)];

    let owner = OwnerV1::from_compiled(finite_program([[0x80; 3], [0xFF; 3]]));
    let mut session = owner.instantiate(11).unwrap();
    let failed = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &white_only,
            },
        )
        .unwrap();
    let failed_evidence = failed.evidence();
    assert_eq!(failed_evidence.kind(), StateKindV1::Failed);
    assert_eq!(failed_evidence.cause_certificate_index(), Some(0));
    let certificates = failed_evidence.certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    assert!(matches!(certificates[0], CertificateV1::Conflict(_)));
    assert_eq!(certificates[0].observation().revision(), 1);
    let mut operations = failed.operations();
    let Some(OperationV1::Remove(remove)) = operations.next() else {
        panic!("Failed without previous evidence must emit one Remove operation");
    };
    assert_eq!(remove.output_slot(), OutputSlotIdV1::new(OUTPUT.value()));
    assert!(operations.next().is_none());

    let owner = OwnerV1::from_compiled(finite_program([[0; 3], [0xFF; 3]]));
    let mut session = owner.instantiate(12).unwrap();
    let previous = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &white_only,
            },
        )
        .unwrap();
    let previous_backing = previous
        .evidence()
        .certificates()
        .next()
        .unwrap()
        .observation_backing_ptr_for_test();
    let both = [ScenarioV1::new(1, &white), ScenarioV1::new(2, &black)];
    let failed = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 2,
                scenarios: &both,
            },
        )
        .unwrap();
    let failed_evidence = failed.evidence();
    assert_eq!(failed_evidence.kind(), StateKindV1::Failed);
    assert_eq!(failed_evidence.cause_certificate_index(), Some(0));
    let certificates = failed_evidence.certificates().collect::<Vec<_>>();
    assert_eq!(
        certificates
            .iter()
            .map(|certificate| match certificate {
                CertificateV1::Verified(value) => {
                    ("verified", value.observation().revision())
                }
                CertificateV1::Conflict(value) => {
                    ("conflict", value.observation().revision())
                }
            })
            .collect::<Vec<_>>(),
        [("conflict", 2), ("verified", 1)]
    );
    assert_eq!(
        certificates[1].observation_backing_ptr_for_test(),
        previous_backing
    );
    let mut operations = failed.operations();
    let Some(OperationV1::Remove(remove)) = operations.next() else {
        panic!("Failed must remove an output that violates the current context");
    };
    assert_eq!(remove.output_slot(), OutputSlotIdV1::new(OUTPUT.value()));
    assert!(operations.next().is_none());
}

#[test]
fn concrete_program_rejects_transport_shape_before_core_admission() {
    let owner = OwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    let mut session = owner.instantiate(11).unwrap();
    let empty_values = [];
    let malformed = [ScenarioV1::new(1, &empty_values)];
    let error = match owner.update(
        &mut session,
        UpdateV1::Observed {
            revision: 1,
            scenarios: &malformed,
        },
    ) {
        Ok(_) => panic!("schema-short public input must fail before Core admission"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), UpdateErrorKindV1::InvalidObservation);
    assert_eq!(session.evidence().kind(), StateKindV1::Waiting);
    assert_eq!(
        session.evidence().observation_head(),
        ObservationHeadV1::Empty
    );

    let white = [Srgb8::new([0xFF; 3])];
    let valid = [ScenarioV1::new(1, &white)];
    owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 2,
                scenarios: &valid,
            },
        )
        .unwrap();
    let duplicate = [ScenarioV1::new(7, &white), ScenarioV1::new(7, &white)];
    let error = match owner.update(
        &mut session,
        UpdateV1::Observed {
            revision: 1,
            scenarios: &duplicate,
        },
    ) {
        Ok(_) => panic!("duplicate scenario IDs must precede revision admission"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), UpdateErrorKindV1::InvalidObservation);
    assert_eq!(session.evidence().kind(), StateKindV1::Ready);
    assert_observed_head(session.evidence().observation_head(), 11, 2);
}

#[test]
fn same_content_foreign_owner_is_rejected_before_admission_without_work() {
    crate::composition::reset_source_over_evaluation_count();
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(0));
    CORE_PROGRAM_ASSESSMENT_CALLS.with(|calls| calls.set(0));

    let compiled_a = finite_program([[0x80; 3], [0; 3]]);
    let compiled_b = finite_program([[0x80; 3], [0; 3]]);
    assert_eq!(compiled_a.content_identity(), compiled_b.content_identity());
    let owner_a = OwnerV1::from_compiled(compiled_a);
    let owner_b = OwnerV1::from_compiled(compiled_b);
    let mut session = owner_a.instantiate(STREAM.value()).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let observed = [ScenarioV1::new(1, &white)];

    owner_a
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &observed,
            },
        )
        .unwrap();
    let before = session.evidence();
    assert_observed_head(before.observation_head(), STREAM.value(), 1);
    let before_backing = before
        .certificates()
        .next()
        .unwrap()
        .observation_backing_ptr_for_test();
    let counts = (
        crate::composition::source_over_evaluation_count(),
        MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get),
        CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get),
    );

    let (project_mismatch, project_allocations) = crate::test_support::measured_allocations(|| {
        matches!(
            owner_b.project(std::hint::black_box(&session)),
            Err(AccessErrorV1::OwnerMismatch)
        )
    });
    assert!(project_mismatch);
    assert_eq!(project_allocations, 0);

    let empty = [];
    let malformed = [ScenarioV1::new(2, &empty)];
    let (update_mismatch, update_allocations) = crate::test_support::measured_allocations(|| {
        matches!(
            owner_b.update(
                std::hint::black_box(&mut session),
                UpdateV1::Observed {
                    revision: 2,
                    scenarios: &malformed,
                },
            ),
            Err(error) if error.kind() == UpdateErrorKindV1::OwnerMismatch
        )
    });
    assert!(update_mismatch);
    assert_eq!(update_allocations, 0);
    assert_eq!(
        (
            crate::composition::source_over_evaluation_count(),
            MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get),
            CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get),
        ),
        counts
    );

    let after = session.evidence();
    assert_eq!(after.kind(), StateKindV1::Ready);
    assert_observed_head(after.observation_head(), STREAM.value(), 1);
    assert_eq!(
        after
            .certificates()
            .next()
            .unwrap()
            .observation_backing_ptr_for_test(),
        before_backing
    );
    assert!(owner_a.project(&session).is_ok());
}

#[test]
fn raw_observation_head_preserves_unknown_reason_independently_of_lifecycle() {
    let owner = OwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    let mut session = owner.instantiate(STREAM.value()).unwrap();
    assert_eq!(
        session.evidence().observation_head(),
        ObservationHeadV1::Empty
    );

    owner
        .update(
            &mut session,
            UpdateV1::Unknown {
                revision: 1,
                reason_id: u32::MAX,
            },
        )
        .unwrap();
    let unknown = session.evidence();
    assert_eq!(unknown.kind(), StateKindV1::Waiting);
    let ObservationHeadV1::Unknown {
        stream,
        revision,
        reason_id,
    } = unknown.observation_head()
    else {
        panic!("Unknown raw evidence must remain a closed Unknown payload");
    };
    assert_eq!(stream.value(), STREAM.value());
    assert_eq!(revision, 1);
    assert_eq!(reason_id, u32::MAX);

    let mut other_session = owner.instantiate(29).unwrap();
    owner
        .update(
            &mut other_session,
            UpdateV1::Unknown {
                revision: 1,
                reason_id: u32::MAX,
            },
        )
        .unwrap();
    assert_ne!(
        unknown.observation_head(),
        other_session.evidence().observation_head(),
        "equal revision and reason on different streams are distinct raw evidence"
    );

    let conflicting = match owner.update(
        &mut session,
        UpdateV1::Unknown {
            revision: 1,
            reason_id: 0,
        },
    ) {
        Ok(_) => panic!("same revision with another reason must conflict"),
        Err(error) => error,
    };
    assert_eq!(conflicting.kind(), UpdateErrorKindV1::RevisionConflict);
    assert_unknown_head(
        session.evidence().observation_head(),
        STREAM.value(),
        1,
        u32::MAX,
    );
}

#[test]
fn copied_raw_heads_outlive_owner_and_session_without_losing_provenance() {
    let (unknown, observed) = {
        let owner = OwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
        let mut session = owner.instantiate(u32::MAX).unwrap();
        owner
            .update(
                &mut session,
                UpdateV1::Unknown {
                    revision: 1,
                    reason_id: u32::MAX,
                },
            )
            .unwrap();
        let unknown = session.evidence().observation_head();

        let white = [Srgb8::new([0xFF; 3])];
        let scenarios = [ScenarioV1::new(1, &white)];
        owner
            .update(
                &mut session,
                UpdateV1::Observed {
                    revision: u64::MAX,
                    scenarios: &scenarios,
                },
            )
            .unwrap();
        let observed = session.evidence().observation_head();
        (unknown, observed)
    };

    assert_unknown_head(unknown, u32::MAX, 1, u32::MAX);
    assert_observed_head(observed, u32::MAX, u64::MAX);
}

#[test]
fn expired_owner_preserves_historical_evidence_but_equivalent_owner_has_no_authority() {
    let compiled_a = finite_program([[0x80; 3], [0; 3]]);
    let compiled_b = finite_program([[0x80; 3], [0; 3]]);
    assert_eq!(compiled_a.content_identity(), compiled_b.content_identity());
    let owner_a = OwnerV1::from_compiled(compiled_a);
    let owner_b = OwnerV1::from_compiled(compiled_b);
    let mut session = owner_a.instantiate(STREAM.value()).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let observed = [ScenarioV1::new(1, &white)];
    owner_a
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &observed,
            },
        )
        .unwrap();

    let certificate = session.evidence().certificates().next().unwrap();
    let backing = certificate.observation_backing_ptr_for_test();
    drop(owner_a);
    assert_eq!(certificate.observation().revision(), 1);
    assert_eq!(
        certificate.observation_backing_ptr_for_test(),
        backing,
        "historical evidence is Session-owned rather than owner-authorized"
    );

    assert!(matches!(
        owner_b.project(&session),
        Err(AccessErrorV1::OwnerMismatch)
    ));
    let mismatch = match owner_b.update(
        &mut session,
        UpdateV1::Unknown {
            revision: 2,
            reason_id: 9,
        },
    ) {
        Ok(_) => panic!("equivalent recompile must not revive another owner generation"),
        Err(error) => error,
    };
    assert_eq!(mismatch.kind(), UpdateErrorKindV1::OwnerMismatch);
    assert_eq!(
        session
            .evidence()
            .certificates()
            .next()
            .unwrap()
            .observation_backing_ptr_for_test(),
        backing
    );
}

#[test]
fn raw_head_and_evaluator_lifecycle_form_the_exact_reachable_product() {
    let owner = OwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    let mut session = owner.instantiate(STREAM.value()).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let black = [Srgb8::new([0; 3])];
    let white_only = [ScenarioV1::new(1, &white)];
    let opposing = [ScenarioV1::new(1, &white), ScenarioV1::new(2, &black)];

    owner
        .update(
            &mut session,
            UpdateV1::Unknown {
                revision: 1,
                reason_id: 3,
            },
        )
        .unwrap();
    owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 2,
                scenarios: &white_only,
            },
        )
        .unwrap();
    let ready = session.evidence();
    assert_eq!(ready.kind(), StateKindV1::Ready);
    assert_observed_head(ready.observation_head(), STREAM.value(), 2);

    owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 3,
                scenarios: &opposing,
            },
        )
        .unwrap();
    let failed = session.evidence();
    assert_eq!(failed.kind(), StateKindV1::Failed);
    assert_observed_head(failed.observation_head(), STREAM.value(), 3);
    assert_eq!(
        failed
            .certificates()
            .map(|certificate| certificate.observation().revision())
            .collect::<Vec<_>>(),
        [3, 2]
    );

    owner
        .update(
            &mut session,
            UpdateV1::Unknown {
                revision: 4,
                reason_id: 17,
            },
        )
        .unwrap();
    let stale = session.evidence();
    assert_eq!(stale.kind(), StateKindV1::Stale);
    assert_unknown_head(stale.observation_head(), STREAM.value(), 4, 17);
    assert_eq!(
        stale
            .certificates()
            .map(|certificate| certificate.observation().revision())
            .collect::<Vec<_>>(),
        [2]
    );

    let rejecting_owner = OwnerV1::from_compiled(finite_program([[0x80; 3], [0xFF; 3]]));
    let mut rejecting_session = rejecting_owner.instantiate(STREAM.value()).unwrap();
    rejecting_owner
        .update(
            &mut rejecting_session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &white_only,
            },
        )
        .unwrap();
    assert_eq!(rejecting_session.evidence().kind(), StateKindV1::Failed);
    rejecting_owner
        .update(
            &mut rejecting_session,
            UpdateV1::Unknown {
                revision: 2,
                reason_id: 23,
            },
        )
        .unwrap();
    let waiting = rejecting_session.evidence();
    assert_eq!(waiting.kind(), StateKindV1::Waiting);
    assert_unknown_head(waiting.observation_head(), STREAM.value(), 2, 23);
    assert_eq!(waiting.certificates().len(), 0);
}

#[test]
fn failed_without_previous_removes_every_output_in_canonical_exact_order() {
    let owner = OwnerV1::from_compiled(finite_program_with_outputs(
        [[0x80; 3], [0xFF; 3]],
        vec![
            OutputBinding::new(SECOND_OUTPUT, PAINT),
            OutputBinding::new(OUTPUT, PAINT),
        ],
    ));
    let mut session = owner.instantiate(STREAM.value()).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [ScenarioV1::new(1, &white)];
    let failed = owner
        .update(
            &mut session,
            UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    assert_eq!(failed.evidence().kind(), StateKindV1::Failed);
    assert_eq!(failed.evidence().certificates().len(), 1);

    let mut operations = failed.operations();
    assert_eq!(operations.len(), 2);
    assert_eq!(operations.size_hint(), (2, Some(2)));
    let Some(OperationV1::Remove(first)) = operations.next() else {
        panic!("Failed without previous evidence must remove the first output");
    };
    assert_eq!(first.output_slot(), OutputSlotIdV1::new(OUTPUT.value()));
    assert_eq!(operations.len(), 1);
    assert_eq!(operations.size_hint(), (1, Some(1)));

    let Some(OperationV1::Remove(second)) = operations.next() else {
        panic!("Failed without previous evidence must remove the second output");
    };
    assert_eq!(
        second.output_slot(),
        OutputSlotIdV1::new(SECOND_OUTPUT.value())
    );
    assert_eq!(operations.len(), 0);
    assert_eq!(operations.size_hint(), (0, Some(0)));
    assert!(operations.next().is_none());
    assert!(operations.next().is_none());
}

#[test]
fn ready_sets_and_stale_removes_every_output_in_the_same_canonical_order() {
    let owner = OwnerV1::from_compiled(finite_program_with_outputs(
        [[0x80; 3], [0; 3]],
        vec![
            OutputBinding::new(SECOND_OUTPUT, PAINT),
            OutputBinding::new(OUTPUT, PAINT),
        ],
    ));
    let mut session = owner.instantiate(STREAM.value()).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [ScenarioV1::new(1, &white)];

    {
        let ready = owner
            .update(
                &mut session,
                UpdateV1::Observed {
                    revision: 1,
                    scenarios: &scenarios,
                },
            )
            .unwrap();
        let mut operations = ready.operations();
        assert_eq!(operations.len(), 2);
        for expected in [OUTPUT, SECOND_OUTPUT] {
            let Some(OperationV1::Set(set)) = operations.next() else {
                panic!("Ready must set every compiled output");
            };
            assert_eq!(set.output_slot().value(), expected.value());
            assert_eq!(set.certificate().observation().revision(), 1);
        }
        assert!(operations.next().is_none());
    }

    let stale = owner
        .update(
            &mut session,
            UpdateV1::Unknown {
                revision: 2,
                reason_id: 7,
            },
        )
        .unwrap();
    let mut operations = stale.operations();
    assert_eq!(operations.len(), 2);
    for expected in [OUTPUT, SECOND_OUTPUT] {
        let Some(OperationV1::Remove(remove)) = operations.next() else {
            panic!("Stale must remove every output that lacks current evidence");
        };
        assert_eq!(remove.output_slot().value(), expected.value());
    }
    assert!(operations.next().is_none());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModeledPayload {
    ReadyOnWhite,
    ReadyOnBlack,
    Conflict,
    Unknown(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RejectedInput {
    EmptyScenarios,
    DuplicateScenario,
    MissingSurfaceValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionAction {
    Admit(ModeledPayload),
    Replay,
    OutOfOrder,
    ConflictingReplay,
    Reject(RejectedInput),
}

impl ModeledPayload {
    const fn selected_source(self) -> Option<[u8; 3]> {
        // The fixture below declares exactly these two candidates; its WCAG
        // constraint selects black on white and mid-gray on black.
        match self {
            Self::ReadyOnWhite => Some([0; 3]),
            Self::ReadyOnBlack => Some([0x80; 3]),
            Self::Conflict | Self::Unknown(_) => None,
        }
    }

    const fn conflicting(self) -> Self {
        // Every edge changes the payload, so replaying it at the same revision
        // must exercise RevisionConflict rather than exact idempotence.
        match self {
            Self::ReadyOnWhite => Self::ReadyOnBlack,
            Self::ReadyOnBlack => Self::Conflict,
            Self::Conflict => Self::Unknown(0x00C0_FFEE),
            Self::Unknown(_) => Self::ReadyOnWhite,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModeledVerified {
    revision: u64,
    source: [u8; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModeledLifecycle {
    Waiting,
    Ready(ModeledVerified),
    Stale(ModeledVerified),
    Failed {
        cause_revision: u64,
        previous: Option<ModeledVerified>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModeledSession {
    head: Option<(u64, ModeledPayload)>,
    lifecycle: ModeledLifecycle,
}

impl ModeledSession {
    const fn new() -> Self {
        Self {
            head: None,
            lifecycle: ModeledLifecycle::Waiting,
        }
    }

    const fn next_revision(self) -> u64 {
        match self.head {
            Some((revision, _)) => revision + 1,
            None => 1,
        }
    }

    const fn last_verified(self) -> Option<ModeledVerified> {
        match self.lifecycle {
            ModeledLifecycle::Waiting => None,
            ModeledLifecycle::Ready(verified) | ModeledLifecycle::Stale(verified) => Some(verified),
            ModeledLifecycle::Failed { previous, .. } => previous,
        }
    }

    fn admit(&mut self, revision: u64, payload: ModeledPayload) {
        let previous = self.last_verified();
        self.head = Some((revision, payload));
        self.lifecycle = match payload {
            ModeledPayload::ReadyOnWhite | ModeledPayload::ReadyOnBlack => {
                ModeledLifecycle::Ready(ModeledVerified {
                    revision,
                    source: payload
                        .selected_source()
                        .expect("a modeled Ready payload has one selected source"),
                })
            }
            ModeledPayload::Conflict => ModeledLifecycle::Failed {
                cause_revision: revision,
                previous,
            },
            ModeledPayload::Unknown(_) => match previous {
                Some(verified) => ModeledLifecycle::Stale(verified),
                None => ModeledLifecycle::Waiting,
            },
        };
    }
}

fn apply_modeled_payload<'owner, 'session>(
    owner: &'owner OwnerV1,
    session: &'session mut SessionV1,
    revision: u64,
    payload: ModeledPayload,
) -> Result<ProjectionV1<'owner, 'session>, UpdateErrorV1> {
    let white = [Srgb8::new([0xFF; 3])];
    let black = [Srgb8::new([0; 3])];
    match payload {
        ModeledPayload::ReadyOnWhite => {
            let scenarios = [ScenarioV1::new(1, &white)];
            owner.update(
                session,
                UpdateV1::Observed {
                    revision,
                    scenarios: &scenarios,
                },
            )
        }
        ModeledPayload::ReadyOnBlack => {
            let scenarios = [ScenarioV1::new(1, &black)];
            owner.update(
                session,
                UpdateV1::Observed {
                    revision,
                    scenarios: &scenarios,
                },
            )
        }
        ModeledPayload::Conflict => {
            let scenarios = [ScenarioV1::new(1, &white), ScenarioV1::new(2, &black)];
            owner.update(
                session,
                UpdateV1::Observed {
                    revision,
                    scenarios: &scenarios,
                },
            )
        }
        ModeledPayload::Unknown(reason_id) => owner.update(
            session,
            UpdateV1::Unknown {
                revision,
                reason_id,
            },
        ),
    }
}

fn assert_verified_matches_model(
    certificate: crate::program::VerifiedCertificateV1<'_>,
    expected: ModeledVerified,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(certificate.observation().revision(), expected.revision);
    let outputs = certificate.outputs().collect::<Vec<_>>();
    prop_assert_eq!(outputs.len(), 2);
    for (output, expected_slot) in outputs.into_iter().zip([OUTPUT, SECOND_OUTPUT]) {
        prop_assert_eq!(output.output_slot().value(), expected_slot.value());
        prop_assert_eq!(output.source(), Srgb8::new(expected.source));
        prop_assert_eq!(output.opacity(), 1.0);
    }
    Ok(())
}

fn assert_projection_matches_model(
    projection: ProjectionV1<'_, '_>,
    model: ModeledSession,
) -> Result<(), TestCaseError> {
    let evidence = projection.evidence();
    let expected_kind = match model.lifecycle {
        ModeledLifecycle::Waiting => StateKindV1::Waiting,
        ModeledLifecycle::Ready(_) => StateKindV1::Ready,
        ModeledLifecycle::Stale(_) => StateKindV1::Stale,
        ModeledLifecycle::Failed { .. } => StateKindV1::Failed,
    };
    prop_assert_eq!(evidence.kind(), expected_kind);

    match (evidence.observation_head(), model.head) {
        (ObservationHeadV1::Empty, None) => {}
        (
            ObservationHeadV1::Unknown {
                stream,
                revision,
                reason_id,
            },
            Some((expected_revision, ModeledPayload::Unknown(expected_reason))),
        ) => {
            prop_assert_eq!(stream.value(), STREAM.value());
            prop_assert_eq!(revision, expected_revision);
            prop_assert_eq!(reason_id, expected_reason);
        }
        (
            ObservationHeadV1::Observed { stream, revision },
            Some((expected_revision, expected_payload)),
        ) if !matches!(expected_payload, ModeledPayload::Unknown(_)) => {
            prop_assert_eq!(stream.value(), STREAM.value());
            prop_assert_eq!(revision, expected_revision);
        }
        (actual, expected) => {
            prop_assert!(
                false,
                "raw head drifted: actual={actual:?}, expected={expected:?}"
            );
        }
    }

    let certificates = evidence.certificates().collect::<Vec<_>>();
    let verified_count = certificates
        .iter()
        .filter(|certificate| matches!(certificate, CertificateV1::Verified(_)))
        .count();
    prop_assert!(
        verified_count <= 1,
        "Session retained more than one Verified witness"
    );
    match model.lifecycle {
        ModeledLifecycle::Waiting => {
            prop_assert_eq!(evidence.cause_certificate_index(), None);
            prop_assert_eq!(certificates.len(), 0);
        }
        ModeledLifecycle::Ready(expected) | ModeledLifecycle::Stale(expected) => {
            prop_assert_eq!(evidence.cause_certificate_index(), None);
            prop_assert_eq!(certificates.len(), 1);
            let CertificateV1::Verified(certificate) = certificates[0] else {
                return Err(TestCaseError::fail(
                    "Ready/Stale must retain exactly one Verified witness",
                ));
            };
            assert_verified_matches_model(certificate, expected)?;
        }
        ModeledLifecycle::Failed {
            cause_revision,
            previous,
        } => {
            prop_assert_eq!(evidence.cause_certificate_index(), Some(0));
            prop_assert_eq!(certificates.len(), usize::from(previous.is_some()) + 1);
            let CertificateV1::Conflict(cause) = certificates[0] else {
                return Err(TestCaseError::fail(
                    "Failed cause must be the first Conflict certificate",
                ));
            };
            prop_assert_eq!(cause.observation().revision(), cause_revision);
            if let Some(expected) = previous {
                let CertificateV1::Verified(certificate) = certificates[1] else {
                    return Err(TestCaseError::fail(
                        "Failed history must retain only its last Verified witness",
                    ));
                };
                assert_verified_matches_model(certificate, expected)?;
            }
        }
    }

    let operations = projection.operations().collect::<Vec<_>>();
    match (model.head, model.lifecycle) {
        (None, ModeledLifecycle::Waiting) => prop_assert_eq!(operations.len(), 0),
        (_, ModeledLifecycle::Ready(expected)) => {
            prop_assert_eq!(operations.len(), 2);
            for (operation, expected_slot) in operations.into_iter().zip([OUTPUT, SECOND_OUTPUT]) {
                let OperationV1::Set(set) = operation else {
                    return Err(TestCaseError::fail("Ready must emit Set for every output"));
                };
                prop_assert_eq!(set.output_slot().value(), expected_slot.value());
                prop_assert_eq!(set.source(), Srgb8::new(expected.source));
                prop_assert_eq!(set.opacity(), 1.0);
                prop_assert_eq!(
                    set.certificate().observation().revision(),
                    expected.revision
                );
            }
        }
        (Some(_), _) => {
            prop_assert_eq!(operations.len(), 2);
            for (operation, expected_slot) in operations.into_iter().zip([OUTPUT, SECOND_OUTPUT]) {
                let OperationV1::Remove(remove) = operation else {
                    return Err(TestCaseError::fail(
                        "a non-Ready admitted head must not authorize Set",
                    ));
                };
                prop_assert_eq!(remove.output_slot().value(), expected_slot.value());
            }
        }
        (None, _) => prop_assert!(false, "only Waiting may have an empty raw head"),
    }
    Ok(())
}

fn exercise_modeled_action(
    owner: &OwnerV1,
    session: &mut SessionV1,
    model: &mut ModeledSession,
    action: SessionAction,
) -> Result<(), TestCaseError> {
    let assessments_before = CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get);
    let evaluator_must_run = match action {
        SessionAction::Admit(payload) => {
            let revision = model.next_revision();
            let projection = apply_modeled_payload(owner, session, revision, payload)
                .map_err(|error| TestCaseError::fail(format!("fresh update failed: {error:?}")))?;
            model.admit(revision, payload);
            assert_projection_matches_model(projection, *model)?;
            !matches!(payload, ModeledPayload::Unknown(_))
        }
        SessionAction::Replay => {
            if let Some((revision, payload)) = model.head {
                let projection =
                    apply_modeled_payload(owner, session, revision, payload).map_err(|error| {
                        TestCaseError::fail(format!("exact replay failed: {error:?}"))
                    })?;
                assert_projection_matches_model(projection, *model)?;
            }
            false
        }
        SessionAction::OutOfOrder => {
            if let Some((current, _)) = model.head {
                let incoming = current
                    .checked_sub(1)
                    .expect("an admitted revision starts at one");
                let error =
                    apply_modeled_payload(owner, session, incoming, ModeledPayload::ReadyOnWhite)
                        .err()
                        .ok_or_else(|| TestCaseError::fail("out-of-order update was admitted"))?;
                prop_assert!(
                    matches!(
                        error,
                        UpdateErrorV1::RevisionOutOfOrder {
                            current: actual_current,
                            incoming: actual_incoming,
                        } if actual_current == current && actual_incoming == incoming
                    ),
                    "out-of-order error payload drifted: {error:?}"
                );
            }
            false
        }
        SessionAction::ConflictingReplay => {
            if let Some((revision, payload)) = model.head {
                let conflicting = payload.conflicting();
                prop_assert_ne!(conflicting, payload);
                let error = apply_modeled_payload(owner, session, revision, conflicting)
                    .err()
                    .ok_or_else(|| TestCaseError::fail("conflicting replay was admitted"))?;
                prop_assert!(
                    matches!(
                        error,
                        UpdateErrorV1::RevisionConflict {
                            revision: actual_revision,
                        } if actual_revision == revision
                    ),
                    "revision-conflict error payload drifted: {error:?}"
                );
            }
            false
        }
        SessionAction::Reject(rejected) => {
            let revision = model.next_revision();
            let error = match rejected {
                RejectedInput::EmptyScenarios => {
                    let scenarios = [];
                    owner.update(
                        session,
                        UpdateV1::Observed {
                            revision,
                            scenarios: &scenarios,
                        },
                    )
                }
                RejectedInput::DuplicateScenario => {
                    let white = [Srgb8::new([0xFF; 3])];
                    let scenarios = [ScenarioV1::new(7, &white), ScenarioV1::new(7, &white)];
                    owner.update(
                        session,
                        UpdateV1::Observed {
                            revision,
                            scenarios: &scenarios,
                        },
                    )
                }
                RejectedInput::MissingSurfaceValue => {
                    let empty = [];
                    let scenarios = [ScenarioV1::new(9, &empty)];
                    owner.update(
                        session,
                        UpdateV1::Observed {
                            revision,
                            scenarios: &scenarios,
                        },
                    )
                }
            }
            .err()
            .ok_or_else(|| TestCaseError::fail(format!("{rejected:?} input was admitted")))?;
            match rejected {
                RejectedInput::EmptyScenarios => {
                    prop_assert!(matches!(error, UpdateErrorV1::EmptyScenarioSet));
                }
                RejectedInput::DuplicateScenario => prop_assert!(
                    matches!(error, UpdateErrorV1::DuplicateScenarioId { scenario } if scenario.value() == 7),
                    "duplicate-scenario error payload drifted: {error:?}"
                ),
                RejectedInput::MissingSurfaceValue => prop_assert!(
                    matches!(
                        error,
                        UpdateErrorV1::ScenarioValueCountMismatch {
                            scenario,
                            expected: 1,
                            actual: 0,
                        } if scenario.value() == 9
                    ),
                    "value-count error payload drifted: {error:?}"
                ),
            }
            false
        }
    };

    let assessments_after = CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get);
    if evaluator_must_run {
        prop_assert!(
            assessments_after > assessments_before,
            "a fresh physical observation bypassed every evaluator"
        );
    } else {
        prop_assert_eq!(
            assessments_after,
            assessments_before,
            "replay, Unknown and rejected inputs must not dispatch evaluators",
        );
    }
    assert_projection_matches_model(
        owner
            .project(session)
            .map_err(|error| TestCaseError::fail(format!("matching owner rejected: {error:?}")))?,
        *model,
    )
}

fn exercise_modeled_sequence(
    actions: impl IntoIterator<Item = SessionAction>,
) -> Result<(), TestCaseError> {
    CORE_PROGRAM_ASSESSMENT_CALLS.with(|calls| calls.set(0));
    let owner = OwnerV1::from_compiled(finite_program_with_outputs(
        [[0x80; 3], [0; 3]],
        vec![
            OutputBinding::new(SECOND_OUTPUT, PAINT),
            OutputBinding::new(OUTPUT, PAINT),
        ],
    ));
    let mut session = owner
        .instantiate(STREAM.value())
        .map_err(|error| TestCaseError::fail(format!("fixture stream was rejected: {error:?}")))?;
    let mut model = ModeledSession::new();
    let projection = owner
        .project(&session)
        .map_err(|error| TestCaseError::fail(format!("fixture epoch mismatch: {error:?}")))?;
    assert_projection_matches_model(projection, model)?;
    for action in actions {
        exercise_modeled_action(&owner, &mut session, &mut model, action)?;
    }
    Ok(())
}

fn session_action_strategy() -> impl Strategy<Value = SessionAction> {
    let admit = prop_oneof![
        Just(SessionAction::Admit(ModeledPayload::ReadyOnWhite)),
        Just(SessionAction::Admit(ModeledPayload::ReadyOnBlack)),
        Just(SessionAction::Admit(ModeledPayload::Conflict)),
        any::<u32>().prop_map(|reason| SessionAction::Admit(ModeledPayload::Unknown(reason))),
    ];
    let reject = prop_oneof![
        Just(SessionAction::Reject(RejectedInput::EmptyScenarios)),
        Just(SessionAction::Reject(RejectedInput::DuplicateScenario)),
        Just(SessionAction::Reject(RejectedInput::MissingSurfaceValue)),
    ];
    // Balance behavioral classes; nested strategies still cover every payload
    // and rejected-input variant without over-weighting Admit.
    prop_oneof![
        1 => admit,
        1 => Just(SessionAction::Replay),
        1 => Just(SessionAction::OutOfOrder),
        1 => Just(SessionAction::ConflictingReplay),
        1 => reject,
    ]
}

#[test]
fn hostile_session_corpus_covers_every_lifecycle_recovery_and_rejection_class() {
    // Coverage is separate from generated input so shrinking cannot hide the
    // real initial-state paths behind a state-preparing prefix.
    let actions = [
        SessionAction::Admit(ModeledPayload::Conflict),
        SessionAction::Admit(ModeledPayload::Unknown(0xA11C_E001)),
        SessionAction::Admit(ModeledPayload::ReadyOnWhite),
        SessionAction::Replay,
        SessionAction::Admit(ModeledPayload::Unknown(0xA11C_E002)),
        SessionAction::Admit(ModeledPayload::Conflict),
        SessionAction::Admit(ModeledPayload::ReadyOnBlack),
        SessionAction::OutOfOrder,
        SessionAction::ConflictingReplay,
        SessionAction::Reject(RejectedInput::EmptyScenarios),
        SessionAction::Reject(RejectedInput::DuplicateScenario),
        SessionAction::Reject(RejectedInput::MissingSurfaceValue),
    ];
    exercise_modeled_sequence(actions).expect("hostile Session lifecycle corpus drifted");
}

#[test]
fn arbitrary_finite_update_sequences_match_lifecycle_evidence_and_emission_model() {
    let config = Config {
        cases: 256,
        failure_persistence: None,
        ..Config::default()
    };
    let mut runner =
        TestRunner::new_with_rng(config, TestRng::deterministic_rng(RngAlgorithm::ChaCha));

    runner
        .run(
            &prop::collection::vec(session_action_strategy(), 0..64),
            exercise_modeled_sequence,
        )
        .expect("deterministic Program Session state-machine property failed");
}
