use core::iter::FusedIterator;

use crate::Srgb8;
use crate::appearance::{OccurrenceId, OpacityInputId, PaintId, SurfaceId, SurfaceInputPortId};
use crate::constraints::{
    ExactConstraintIdentityV1, ExactIdentityCapabilityV1, ExactIdentityReleaseV1,
    ProgramVisiblePointBindingV1,
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
use crate::package_bridge::{
    PackageProgramAssessmentV1, PackageProgramCertificateV1, PackageProgramConflictCellV1,
    PackageProgramModeledPointV1, PackageProgramObservationV1, PackageProgramOperationV1,
    PackageProgramOutputSlotIdV1, PackageProgramOwnerV1, PackageProgramPhysicalPointV1,
    PackageProgramScenarioV1, PackageProgramSignalV1, PackageProgramStateKindV1,
    PackageProgramStateViewV1, PackageProgramSurroundV1, PackageProgramUpdateErrorKindV1,
    PackageProgramUpdateV1, PackageProgramVerdictV1, PackageProgramVerifiedCellV1,
};
use crate::program_session::{
    CORE_PROGRAM_ASSESSMENT_CALLS, CompiledCoreProgramV1, CompositionProfile, ConstraintId,
    ConstraintInvocation, ConstraintSet, CoreProgramConstraintInvocationV1,
    CoreProgramEvaluatorsV1, CoreProgramPassEvidenceV1, CoreProgramV1,
    CoreProgramViolationEvidenceV1, DeclaredJointSelectionV1, JointCandidateStateV1,
    ObservationGroup, Occurrence, OpacityInput, OutputBinding, OutputSlotId, Paint, Program,
    ProgramConstraintCellV1, ProgramConstraintResultV1, Source, SourceId, Surface, Target,
    TargetCandidateChoiceV1, TargetCandidateId, TargetCandidateV1, TargetId,
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
const GROUP: ObservationGroupId = ObservationGroupId::new(10);
const STREAM: ObservationStreamId = ObservationStreamId::new(11);

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
        vec![OutputBinding::new(OUTPUT, PAINT)],
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

fn assert_package_observation_matches_core(
    package: PackageProgramObservationV1<'_>,
    core: &crate::observation::RevisionBoundObservationV1,
) {
    assert_eq!(package.stream().value(), core.stream().value());
    assert_eq!(package.revision(), core.revision().value());
    assert_eq!(
        package
            .surface_input_ports()
            .map(|port| port.value())
            .collect::<Vec<_>>(),
        core.schema()
            .iter()
            .map(|port| port.value())
            .collect::<Vec<_>>(),
    );

    let package_cases = package
        .physical_cases()
        .map(|case| {
            let values = case
                .values()
                .map(|value| match value {
                    PackageProgramSignalV1::Iec61966Srgb8D65(value) => value,
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
    assert_eq!(package_cases, core_cases);
}

fn assert_package_binding_matches_core(
    package: PackageProgramAssessmentV1<'_>,
    core: &ProgramVisiblePointBindingV1,
    expected_occurrence: OccurrenceId,
) -> (Srgb8, Srgb8) {
    let core_physical = core.physical();
    let core_occurrence = core_physical.occurrence();
    let core_program_occurrence = core_physical.program_occurrence();
    assert_eq!(core_program_occurrence.occurrence(), expected_occurrence);
    let PackageProgramPhysicalPointV1::EncodedSrgb8SourceOver(package_physical) =
        package.binding().physical();
    assert_eq!(
        package_physical.subject_paint().value(),
        core_program_occurrence.subject().value()
    );
    assert_eq!(
        package_physical.backdrop_surface().value(),
        core_program_occurrence.backdrop_surface().value()
    );
    assert_eq!(
        package_physical.subject(),
        Srgb8::new(core_occurrence.subject_rgb())
    );
    assert_eq!(
        package_physical.opacity().to_bits(),
        core_occurrence.subject_opacity_bits()
    );
    assert_eq!(
        package_physical.backdrop(),
        Srgb8::new(core_occurrence.backdrop_rgb())
    );
    assert_eq!(
        package_physical.visible(),
        Srgb8::new(core_occurrence.output_rgb())
    );

    let PackageProgramModeledPointV1::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1(
        package_modeled,
    ) = package.binding().modeled();
    assert_eq!(
        package_modeled.xyz().map(f64::to_bits),
        core.modeled_lcs()
            .derivation()
            .sample()
            .xyz()
            .map(f64::to_bits)
    );
    let package_context = package_modeled.appearance_context();
    let core_context = core.modeled_lcs().occurrence().context();
    assert_eq!(
        package_context.adapting_luminance_cd_m2().to_bits(),
        core_context.adapting_luminance_cd_m2().to_bits()
    );
    assert_eq!(
        package_context.background_luminance_ratio_yb_yw().to_bits(),
        core_context.background_luminance_ratio().to_bits()
    );
    let core_surround = match core_context.surround_profile() {
        SurroundProfileId::AverageV1 => PackageProgramSurroundV1::Average,
        SurroundProfileId::DimV1 => PackageProgramSurroundV1::Dim,
        SurroundProfileId::DarkV1 => PackageProgramSurroundV1::Dark,
    };
    assert_eq!(package_context.surround(), core_surround);

    (package_physical.visible(), package_physical.backdrop())
}

fn assert_package_assessment_matches_core(
    package: PackageProgramAssessmentV1<'_>,
    core: &ProgramConstraintResultV1<CoreProgramEvaluatorsV1>,
    expected_occurrence: OccurrenceId,
) {
    match (package, core) {
        (
            PackageProgramAssessmentV1::ExactSrgb8(package),
            ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::ExactSrgb8(core)),
        ) => {
            assert_eq!(package.verdict(), PackageProgramVerdictV1::Pass);
            assert_eq!(package.expected(), core.target());
            let (visible, _) = assert_package_binding_matches_core(
                PackageProgramAssessmentV1::ExactSrgb8(package),
                core.binding(),
                expected_occurrence,
            );
            assert_eq!(visible, core.actual());
        }
        (
            PackageProgramAssessmentV1::ExactSrgb8(package),
            ProgramConstraintResultV1::Violation(CoreProgramViolationEvidenceV1::ExactSrgb8(core)),
        ) => {
            assert_eq!(package.verdict(), PackageProgramVerdictV1::Violation);
            assert_eq!(package.expected(), core.target());
            let (visible, _) = assert_package_binding_matches_core(
                PackageProgramAssessmentV1::ExactSrgb8(package),
                core.binding(),
                expected_occurrence,
            );
            assert_eq!(visible, core.actual());
        }
        (
            PackageProgramAssessmentV1::Wcag22Srgb8(package),
            ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::Wcag22Srgb8(core)),
        ) => {
            assert_eq!(package.verdict(), PackageProgramVerdictV1::Pass);
            let measurement = core.measurement().value();
            assert_eq!(package.profile_id(), measurement.profile_id());
            assert_eq!(package.criterion(), measurement.criterion());
            assert_eq!(
                package.foreground_luminance(),
                measurement.measurement().foreground_luminance
            );
            assert_eq!(
                package.background_luminance(),
                measurement.measurement().background_luminance
            );
            assert_eq!(package.numerical_evidence(), measurement.evidence());
            let (visible, backdrop) = assert_package_binding_matches_core(
                PackageProgramAssessmentV1::Wcag22Srgb8(package),
                core.binding(),
                expected_occurrence,
            );
            assert_eq!(visible, Srgb8::new(measurement.measurement().foreground));
            assert_eq!(backdrop, Srgb8::new(measurement.measurement().background));
        }
        (
            PackageProgramAssessmentV1::Wcag22Srgb8(package),
            ProgramConstraintResultV1::Violation(CoreProgramViolationEvidenceV1::Wcag22Srgb8(core)),
        ) => {
            assert_eq!(package.verdict(), PackageProgramVerdictV1::Violation);
            let measurement = core.measurement().value();
            assert_eq!(package.profile_id(), measurement.profile_id());
            assert_eq!(package.criterion(), measurement.criterion());
            assert_eq!(
                package.foreground_luminance(),
                measurement.measurement().foreground_luminance
            );
            assert_eq!(
                package.background_luminance(),
                measurement.measurement().background_luminance
            );
            assert_eq!(package.numerical_evidence(), measurement.evidence());
            let (visible, backdrop) = assert_package_binding_matches_core(
                PackageProgramAssessmentV1::Wcag22Srgb8(package),
                core.binding(),
                expected_occurrence,
            );
            assert_eq!(visible, Srgb8::new(measurement.measurement().foreground));
            assert_eq!(backdrop, Srgb8::new(measurement.measurement().background));
        }
        _ => panic!("package assessment family or verdict drifted from Core"),
    }
}

fn assert_verified_cell_matches_core(
    package: PackageProgramVerifiedCellV1<'_>,
    core: &ProgramConstraintCellV1<CoreProgramEvaluatorsV1>,
    selected_state_index: usize,
) {
    assert_eq!(core.candidate_state_index(), selected_state_index);
    assert_eq!(package.case_index(), core.case_index());
    assert_eq!(package.constraint().value(), core.constraint().value());
    assert_eq!(package.occurrence().value(), core.target().value());
    assert_eq!(
        matches!(
            package.mode(),
            crate::package_bridge::PackageProgramConstraintModeV1::Hard
        ),
        core.is_hard()
    );
    assert_package_assessment_matches_core(package.assessment(), core.result(), core.target());
}

fn assert_conflict_cell_matches_core(
    package: PackageProgramConflictCellV1<'_>,
    core: &ProgramConstraintCellV1<CoreProgramEvaluatorsV1>,
) {
    assert_eq!(package.state_index(), core.candidate_state_index());
    assert_eq!(package.case_index(), core.case_index());
    assert_eq!(package.constraint().value(), core.constraint().value());
    assert_eq!(package.occurrence().value(), core.target().value());
    assert_eq!(
        matches!(
            package.mode(),
            crate::package_bridge::PackageProgramConstraintModeV1::Hard
        ),
        core.is_hard()
    );
    assert_package_assessment_matches_core(package.assessment(), core.result(), core.target());
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

fn consume_package_assessment(
    assessment: PackageProgramAssessmentV1<'_>,
    probe: &mut ProjectionProbe,
) {
    probe.mix(match assessment.verdict() {
        PackageProgramVerdictV1::Pass => 1,
        PackageProgramVerdictV1::Violation => 2,
    });
    match assessment {
        PackageProgramAssessmentV1::ExactSrgb8(evidence) => {
            probe.exact_assessments += 1;
            probe.mix_srgb8(evidence.expected());
        }
        PackageProgramAssessmentV1::Wcag22Srgb8(evidence) => {
            probe.wcag_assessments += 1;
            probe.mix_bytes(evidence.profile_id().key().as_bytes());
            probe.mix_bytes(evidence.criterion().key().as_bytes());
            probe.mix(evidence.foreground_luminance().lower());
            probe.mix(evidence.foreground_luminance().upper());
            probe.mix(evidence.background_luminance().lower());
            probe.mix(evidence.background_luminance().upper());
            probe.mix_bytes(evidence.numerical_evidence().class_key().as_bytes());
        }
    }

    let PackageProgramPhysicalPointV1::EncodedSrgb8SourceOver(physical) =
        assessment.binding().physical();
    probe.mix(u64::from(physical.subject_paint().value()));
    probe.mix(u64::from(physical.backdrop_surface().value()));
    probe.mix_srgb8(physical.subject());
    probe.mix(physical.opacity().to_bits());
    probe.mix_srgb8(physical.backdrop());
    probe.mix_srgb8(physical.visible());

    let PackageProgramModeledPointV1::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1(modeled) =
        assessment.binding().modeled();
    for coordinate in modeled.xyz() {
        probe.mix(coordinate.to_bits());
    }
    let context = modeled.appearance_context();
    probe.mix(context.adapting_luminance_cd_m2().to_bits());
    probe.mix(context.background_luminance_ratio_yb_yw().to_bits());
    probe.mix(match context.surround() {
        PackageProgramSurroundV1::Average => 1,
        PackageProgramSurroundV1::Dim => 2,
        PackageProgramSurroundV1::Dark => 3,
    });
}

fn consume_package_projection(view: PackageProgramStateViewV1<'_>) -> ProjectionProbe {
    let mut probe = ProjectionProbe::new();
    probe.mix(match view.kind() {
        PackageProgramStateKindV1::Waiting => 1,
        PackageProgramStateKindV1::Ready => 2,
        PackageProgramStateKindV1::Failed => 3,
        PackageProgramStateKindV1::Stale => 4,
    });
    probe.mix(view.revision().unwrap_or_default());
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
                let PackageProgramSignalV1::Iec61966Srgb8D65(value) = value;
                probe.mix_srgb8(value);
            });
            consume_exact_fused(case.provenance(), probe, |scenario, probe| {
                probe.provenance += 1;
                probe.mix(u64::from(scenario.value()));
            });
        });
        match certificate {
            PackageProgramCertificateV1::Verified(verified) => {
                probe.mix(
                    verified
                        .selected_state_index()
                        .map_or(0, |index| index as u64 + 1),
                );
                consume_exact_fused(verified.cells(), probe, |cell, probe| {
                    probe.cells += 1;
                    probe.mix(cell.case_index() as u64);
                    probe.mix(u64::from(cell.constraint().value()));
                    probe.mix(u64::from(cell.occurrence().value()));
                    probe.mix(match cell.mode() {
                        crate::package_bridge::PackageProgramConstraintModeV1::Hard => 1,
                        crate::package_bridge::PackageProgramConstraintModeV1::ReportOnly => 2,
                    });
                    consume_package_assessment(cell.assessment(), probe);
                });
                consume_exact_fused(verified.outputs(), probe, |output, probe| {
                    probe.outputs += 1;
                    probe.mix(u64::from(output.output_slot().value()));
                    probe.mix(u64::from(output.paint().value()));
                    probe.mix_srgb8(output.source());
                    probe.mix(output.opacity().to_bits());
                });
            }
            PackageProgramCertificateV1::Conflict(conflict) => {
                probe.mix(conflict.considered_state_count() as u64);
                consume_exact_fused(conflict.cells(), probe, |cell, probe| {
                    probe.cells += 1;
                    probe.mix(cell.state_index() as u64);
                    probe.mix(cell.case_index() as u64);
                    probe.mix(u64::from(cell.constraint().value()));
                    probe.mix(u64::from(cell.occurrence().value()));
                    probe.mix(match cell.mode() {
                        crate::package_bridge::PackageProgramConstraintModeV1::Hard => 1,
                        crate::package_bridge::PackageProgramConstraintModeV1::ReportOnly => 2,
                    });
                    consume_package_assessment(cell.assessment(), probe);
                });
            }
        }
    });
    consume_exact_fused(view.operations(), &mut probe, |operation, probe| {
        probe.operations += 1;
        match operation {
            PackageProgramOperationV1::Set(set) => {
                probe.mix(1);
                probe.mix(u64::from(set.output_slot().value()));
                probe.mix_srgb8(set.source());
                probe.mix(set.opacity().to_bits());
                probe.mix_bytes(set.certificate().content_identity().as_bytes());
                probe.mix(set.certificate().observation().revision());
            }
            PackageProgramOperationV1::Remove(remove) => {
                probe.mix(2);
                probe.mix(u64::from(remove.output_slot().value()));
            }
            PackageProgramOperationV1::Hold(hold) => {
                probe.mix(3);
                probe.mix(u64::from(hold.output_slot().value()));
                probe.mix_bytes(hold.certificate().content_identity().as_bytes());
                probe.mix(hold.certificate().observation().revision());
            }
        }
    });
    std::hint::black_box(probe)
}

#[test]
fn one_program_retains_typed_exact_and_wcag22_outcomes() {
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
            context(),
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

    let ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::ExactSrgb8(evidence)) =
        exact.result()
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
    assert_eq!(
        evidence.binding().modeled_lcs(),
        exact.modeled_lcs_occurrence(),
    );

    let ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::Wcag22Srgb8(evidence)) =
        wcag.result()
    else {
        panic!("the second cell must retain WCAG22-specific pass evidence");
    };
    assert_eq!(evidence.release(), &wcag22_profile_v1().profile_id);
    assert_eq!(
        evidence.binding().modeled_lcs(),
        wcag.modeled_lcs_occurrence(),
    );
    assert_ne!(
        core::any::type_name_of_val(evidence.identity()),
        core::any::type_name_of_val(exact.result()),
    );
}

#[test]
fn fixed_package_certificate_retains_none_selection_and_nonunit_output_opacity() {
    let owner = PackageProgramOwnerV1::from_compiled(fixed_translucent_program());
    let mut session = owner.instantiate(STREAM.value()).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let scenarios = [PackageProgramScenarioV1::new(1, &white)];
    let state = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &scenarios,
        })
        .unwrap();
    let Some(PackageProgramCertificateV1::Verified(certificate)) = state.certificates().next()
    else {
        panic!("the exact translucent midpoint must be verified");
    };
    assert_eq!(certificate.selected_state_index(), None);
    let PackageProgramAssessmentV1::ExactSrgb8(assessment) =
        certificate.cells().next().unwrap().assessment()
    else {
        panic!("the fixed Program has one Exact certificate cell");
    };
    let PackageProgramPhysicalPointV1::EncodedSrgb8SourceOver(physical) =
        assessment.binding().physical();
    assert_eq!(physical.opacity().to_bits(), 0.5_f64.to_bits());
    assert_eq!(physical.visible(), Srgb8::new([0x80; 3]));
    assert_eq!(
        certificate.outputs().next().unwrap().opacity().to_bits(),
        physical.opacity().to_bits()
    );
    let Some(PackageProgramOperationV1::Set(set)) = state.operations().next() else {
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
            ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::ExactSrgb8(_))
        ));
    }
    for cell in [cells[1].result(), cells[3].result()] {
        assert!(matches!(
            cell,
            ProgramConstraintResultV1::Pass(CoreProgramPassEvidenceV1::Wcag22Srgb8(_))
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
        ProgramConstraintResultV1::Violation(CoreProgramViolationEvidenceV1::ExactSrgb8(_))
    ));
    assert!(matches!(
        cells[1].result(),
        ProgramConstraintResultV1::Violation(CoreProgramViolationEvidenceV1::Wcag22Srgb8(_))
    ));
}

#[test]
fn package_projection_preserves_every_exposed_ready_and_conflict_field_against_core() {
    let ready_core_owner = finite_program([[0x80; 3], [0; 3]]);
    let ready_identity = ready_core_owner.content_identity();
    let ready_package_owner =
        PackageProgramOwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    let mut ready_core_session = ready_core_owner.instantiate(STREAM).unwrap();
    let mut ready_package_session = ready_package_owner.instantiate(STREAM.value()).unwrap();
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
        PackageProgramScenarioV1::new(1, &ready_white),
        PackageProgramScenarioV1::new(2, &ready_white),
        PackageProgramScenarioV1::new(3, &ready_gray),
    ];
    let package_ready = ready_package_session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &ready_scenarios,
        })
        .unwrap();
    let mut package_certificates = package_ready.certificates();
    assert_eq!(package_certificates.len(), 1);
    let Some(PackageProgramCertificateV1::Verified(package_verified)) = package_certificates.next()
    else {
        panic!("Ready must retain exactly one Verified certificate");
    };
    assert!(package_certificates.next().is_none());
    assert!(package_certificates.next().is_none());
    assert_eq!(
        package_verified.content_identity().as_bytes(),
        ready_identity.as_bytes()
    );
    assert_package_observation_matches_core(
        package_verified.observation(),
        core_verified.report().observation(),
    );
    assert_eq!(
        package_verified.selected_state_index(),
        core_verified.selected_state_index()
    );
    let selected_state_index = core_verified.selected_state_index().unwrap();
    let mut package_cells = package_verified.cells();
    let mut core_cells = core_verified.report().cells().iter();
    assert_eq!(package_cells.len(), core_cells.len());
    while let (Some(package), Some(core)) = (package_cells.next(), core_cells.next()) {
        assert_verified_cell_matches_core(package, core, selected_state_index);
        assert_eq!(package_cells.len(), core_cells.len());
    }
    assert!(package_cells.next().is_none());
    assert!(package_cells.next().is_none());
    assert!(core_cells.next().is_none());

    let mut package_outputs = package_verified.outputs();
    let mut core_outputs = core_verified.outputs().iter();
    assert_eq!(package_outputs.len(), core_outputs.len());
    while let (Some(package), Some(core)) = (package_outputs.next(), core_outputs.next()) {
        assert_eq!(package.output_slot().value(), core.output().value());
        assert_eq!(package.paint().value(), core.paint().id().value());
        assert_eq!(package.source(), core.paint().source());
        assert_eq!(
            package.opacity().to_bits(),
            core.paint().opacity().value().to_bits()
        );
        assert_eq!(package_outputs.len(), core_outputs.len());
    }
    assert!(package_outputs.next().is_none());
    assert!(package_outputs.next().is_none());
    assert!(core_outputs.next().is_none());
    let mut ready_operations = package_ready.operations();
    assert_eq!(ready_operations.len(), core_verified.outputs().len());
    for core_output in core_verified.outputs() {
        let Some(PackageProgramOperationV1::Set(set)) = ready_operations.next() else {
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
            package_verified.content_identity()
        );
        assert_eq!(
            set.certificate().observation().revision(),
            package_verified.observation().revision()
        );
    }
    assert!(ready_operations.next().is_none());
    assert!(ready_operations.next().is_none());

    let conflict_core_owner = finite_program([[0; 3], [0xFF; 3]]);
    let conflict_identity = conflict_core_owner.content_identity();
    let conflict_package_owner =
        PackageProgramOwnerV1::from_compiled(finite_program([[0; 3], [0xFF; 3]]));
    let mut conflict_core_session = conflict_core_owner.instantiate(STREAM).unwrap();
    let mut conflict_package_session = conflict_package_owner.instantiate(STREAM.value()).unwrap();
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
        PackageProgramScenarioV1::new(1, &conflict_white),
        PackageProgramScenarioV1::new(2, &conflict_black),
    ];
    let package_failed = conflict_package_session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &conflict_scenarios,
        })
        .unwrap();
    let mut package_certificates = package_failed.certificates();
    assert_eq!(package_certificates.len(), 1);
    let Some(PackageProgramCertificateV1::Conflict(package_conflict)) = package_certificates.next()
    else {
        panic!("Failed without previous state must retain one Conflict certificate");
    };
    assert!(package_certificates.next().is_none());
    assert!(package_certificates.next().is_none());
    assert_eq!(
        package_conflict.content_identity().as_bytes(),
        conflict_identity.as_bytes()
    );
    assert_package_observation_matches_core(
        package_conflict.observation(),
        core_conflict.report().observation(),
    );
    assert_eq!(
        package_conflict.considered_state_count(),
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
    let mut package_cells = package_conflict.cells();
    let mut core_cells = core_conflict.report().cells().iter();
    assert_eq!(package_cells.len(), core_cells.len());
    while let (Some(package), Some(core)) = (package_cells.next(), core_cells.next()) {
        assert_conflict_cell_matches_core(package, core);
        assert_eq!(package_cells.len(), core_cells.len());
    }
    assert!(package_cells.next().is_none());
    assert!(package_cells.next().is_none());
    assert!(core_cells.next().is_none());
    let mut failed_operations = package_failed.operations();
    assert_eq!(failed_operations.len(), 1);
    assert!(matches!(
        failed_operations.next(),
        Some(PackageProgramOperationV1::Remove(_))
    ));
    assert!(failed_operations.next().is_none());
    assert!(failed_operations.next().is_none());
}

#[test]
fn committed_projection_is_zero_alloc_and_repeats_no_composite_transform_or_evaluator_dispatch() {
    crate::composition::reset_source_over_evaluation_count();
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(0));
    CORE_PROGRAM_ASSESSMENT_CALLS.with(|calls| calls.set(0));

    let owner = PackageProgramOwnerV1::from_compiled(finite_program([[0; 3], [0xFF; 3]]));
    let mut session = owner.instantiate(STREAM.value()).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let white_only = [PackageProgramScenarioV1::new(1, &white)];
    session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &white_only,
        })
        .unwrap();
    let Some(PackageProgramCertificateV1::Verified(ready_certificate)) =
        session.state().certificates().next()
    else {
        panic!("black must be selected for the white-only physical support");
    };
    assert_eq!(ready_certificate.selected_state_index(), Some(0));

    let ready_compositions = crate::composition::source_over_evaluation_count();
    let ready_derivations = MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get);
    let ready_assessments = CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get);
    assert!(ready_compositions > 0);
    assert!(ready_derivations > 0);
    assert!(ready_assessments > 0);
    let (ready_probe, ready_allocations) = crate::test_support::measured_allocations(|| {
        consume_package_projection(std::hint::black_box(session.state()))
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

    session
        .update(PackageProgramUpdateV1::Unknown {
            revision: 2,
            reason_id: 7,
        })
        .unwrap();
    let stale_compositions = crate::composition::source_over_evaluation_count();
    let stale_derivations = MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get);
    let stale_assessments = CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get);
    let (stale_probe, stale_allocations) = crate::test_support::measured_allocations(|| {
        consume_package_projection(std::hint::black_box(session.state()))
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
    let opposing_backdrops = [
        PackageProgramScenarioV1::new(1, &white),
        PackageProgramScenarioV1::new(2, &black),
    ];
    session
        .update(PackageProgramUpdateV1::Observed {
            revision: 3,
            scenarios: &opposing_backdrops,
        })
        .unwrap();
    let failed_compositions = crate::composition::source_over_evaluation_count();
    let failed_derivations = MODELED_TRISTIMULUS_DERIVATION_CALLS.with(core::cell::Cell::get);
    let failed_assessments = CORE_PROGRAM_ASSESSMENT_CALLS.with(core::cell::Cell::get);
    let (failed_probe, failed_allocations) = crate::test_support::measured_allocations(|| {
        consume_package_projection(std::hint::black_box(session.state()))
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
    let package_owner = PackageProgramOwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    let mut core_session = core_owner.instantiate(STREAM).unwrap();
    let mut package_session = package_owner.instantiate(STREAM.value()).unwrap();
    let package_values = BACKDROPS.map(|value| [Srgb8::new(value)]);
    let mut first_package_backing = None;
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
        let package_scenarios = permutation
            .map(|index| PackageProgramScenarioV1::new(IDS[index], &package_values[index]));
        let package_state = package_session
            .update(PackageProgramUpdateV1::Observed {
                revision: 1,
                scenarios: &package_scenarios,
            })
            .unwrap();
        let Some(PackageProgramCertificateV1::Verified(package_verified)) =
            package_state.certificates().next()
        else {
            panic!("the canonical observation must keep one Verified certificate");
        };
        assert_eq!(
            package_verified.content_identity().as_bytes(),
            content_identity.as_bytes()
        );
        assert_package_observation_matches_core(
            package_verified.observation(),
            core_verified.report().observation(),
        );

        let projected_cases = package_verified
            .observation()
            .physical_cases()
            .map(|case| {
                let values = case
                    .values()
                    .map(|value| match value {
                        PackageProgramSignalV1::Iec61966Srgb8D65(value) => value,
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

        let backing = PackageProgramCertificateV1::Verified(package_verified)
            .observation_backing_ptr_for_test();
        match first_package_backing {
            None => {
                first_package_backing = Some(backing);
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
fn concrete_package_bridge_projects_total_ready_and_stale_operations() {
    let owner = PackageProgramOwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    assert_eq!(owner.surface_input_port_count(), 1);
    assert_eq!(
        owner.output_slots().collect::<Vec<_>>(),
        [PackageProgramOutputSlotIdV1::new(OUTPUT.value())]
    );

    let mut session = owner.instantiate(11).unwrap();
    let initial = session.state();
    assert_eq!(initial.kind(), PackageProgramStateKindV1::Waiting);
    assert_eq!(initial.revision(), None);
    assert_eq!(initial.certificates().len(), 0);
    assert_eq!(initial.operations().len(), 0);

    let white = [Srgb8::new([0xFF; 3])];
    let gray = [Srgb8::new([0x80; 3])];
    let scenarios = [
        PackageProgramScenarioV1::new(2, &gray),
        PackageProgramScenarioV1::new(1, &white),
    ];
    let ready = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &scenarios,
        })
        .unwrap();
    assert_eq!(ready.kind(), PackageProgramStateKindV1::Ready);
    assert_eq!(ready.revision(), Some(1));
    assert_eq!(ready.cause_certificate_index(), None);
    let certificates = ready.certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    assert!(matches!(
        certificates[0],
        PackageProgramCertificateV1::Verified(_)
    ));
    assert_eq!(certificates[0].observation().revision(), 1);
    let ready_backing = certificates[0].observation_backing_ptr_for_test();
    let mut operations = ready.operations();
    let Some(PackageProgramOperationV1::Set(set)) = operations.next() else {
        panic!("Ready must emit one Set operation");
    };
    assert_eq!(
        set.output_slot(),
        PackageProgramOutputSlotIdV1::new(OUTPUT.value())
    );
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
        PackageProgramCertificateV1::Verified(set.certificate()).observation_backing_ptr_for_test(),
        ready_backing
    );
    assert!(operations.next().is_none());
    drop(operations);
    drop(certificates);

    let reordered = [
        PackageProgramScenarioV1::new(1, &white),
        PackageProgramScenarioV1::new(2, &gray),
    ];
    let replay = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &reordered,
        })
        .unwrap();
    let replay_certificate = replay.certificates().next().unwrap();
    assert_eq!(
        replay_certificate.observation_backing_ptr_for_test(),
        ready_backing,
        "scenario permutation at the same revision must be exact idempotence"
    );

    let changed_same_revision = [PackageProgramScenarioV1::new(1, &white)];
    let error = match session.update(PackageProgramUpdateV1::Observed {
        revision: 1,
        scenarios: &changed_same_revision,
    }) {
        Ok(_) => panic!("changed payload at the same revision must be rejected"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramUpdateErrorKindV1::RevisionConflict
    );
    assert_eq!(session.state().kind(), PackageProgramStateKindV1::Ready);
    assert_eq!(session.state().revision(), Some(1));

    let stale = session
        .update(PackageProgramUpdateV1::Unknown {
            revision: 2,
            reason_id: 7,
        })
        .unwrap();
    assert_eq!(stale.kind(), PackageProgramStateKindV1::Stale);
    assert_eq!(stale.revision(), Some(2));
    let certificates = stale.certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    assert!(matches!(
        certificates[0],
        PackageProgramCertificateV1::Verified(_)
    ));
    assert_eq!(certificates[0].observation().revision(), 1);
    let mut operations = stale.operations();
    let Some(PackageProgramOperationV1::Hold(hold)) = operations.next() else {
        panic!("Stale must emit one Hold operation");
    };
    assert_eq!(
        hold.output_slot(),
        PackageProgramOutputSlotIdV1::new(OUTPUT.value())
    );
    assert_eq!(
        hold.certificate().observation().revision(),
        certificates[0].observation().revision()
    );
    assert_eq!(
        hold.certificate().content_identity(),
        certificates[0].content_identity()
    );
    assert_eq!(
        PackageProgramCertificateV1::Verified(hold.certificate())
            .observation_backing_ptr_for_test(),
        ready_backing
    );
    assert!(operations.next().is_none());
}

#[test]
fn concrete_package_bridge_distinguishes_failed_remove_from_failed_hold() {
    let white = [Srgb8::new([0xFF; 3])];
    let black = [Srgb8::new([0; 3])];
    let white_only = [PackageProgramScenarioV1::new(1, &white)];

    let owner = PackageProgramOwnerV1::from_compiled(finite_program([[0x80; 3], [0xFF; 3]]));
    let mut session = owner.instantiate(11).unwrap();
    let failed = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &white_only,
        })
        .unwrap();
    assert_eq!(failed.kind(), PackageProgramStateKindV1::Failed);
    assert_eq!(failed.cause_certificate_index(), Some(0));
    let certificates = failed.certificates().collect::<Vec<_>>();
    assert_eq!(certificates.len(), 1);
    assert!(matches!(
        certificates[0],
        PackageProgramCertificateV1::Conflict(_)
    ));
    assert_eq!(certificates[0].observation().revision(), 1);
    let mut operations = failed.operations();
    let Some(PackageProgramOperationV1::Remove(remove)) = operations.next() else {
        panic!("Failed without previous evidence must emit one Remove operation");
    };
    assert_eq!(
        remove.output_slot(),
        PackageProgramOutputSlotIdV1::new(OUTPUT.value())
    );
    assert!(operations.next().is_none());

    let owner = PackageProgramOwnerV1::from_compiled(finite_program([[0; 3], [0xFF; 3]]));
    let mut session = owner.instantiate(12).unwrap();
    let previous = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 1,
            scenarios: &white_only,
        })
        .unwrap();
    let previous_backing = previous
        .certificates()
        .next()
        .unwrap()
        .observation_backing_ptr_for_test();
    let both = [
        PackageProgramScenarioV1::new(1, &white),
        PackageProgramScenarioV1::new(2, &black),
    ];
    let failed = session
        .update(PackageProgramUpdateV1::Observed {
            revision: 2,
            scenarios: &both,
        })
        .unwrap();
    assert_eq!(failed.kind(), PackageProgramStateKindV1::Failed);
    assert_eq!(failed.cause_certificate_index(), Some(0));
    let certificates = failed.certificates().collect::<Vec<_>>();
    assert_eq!(
        certificates
            .iter()
            .map(|certificate| match certificate {
                PackageProgramCertificateV1::Verified(value) => {
                    ("verified", value.observation().revision())
                }
                PackageProgramCertificateV1::Conflict(value) => {
                    ("conflict", value.observation().revision())
                }
            })
            .collect::<Vec<_>>(),
        [("conflict", 2), ("verified", 1)]
    );
    let mut operations = failed.operations();
    let Some(PackageProgramOperationV1::Hold(hold)) = operations.next() else {
        panic!("Failed with previous evidence must emit one Hold operation");
    };
    assert_eq!(
        hold.output_slot(),
        PackageProgramOutputSlotIdV1::new(OUTPUT.value())
    );
    assert_eq!(hold.certificate().observation().revision(), 1);
    assert_eq!(
        hold.certificate().content_identity(),
        certificates[1].content_identity()
    );
    assert_eq!(
        PackageProgramCertificateV1::Verified(hold.certificate())
            .observation_backing_ptr_for_test(),
        previous_backing
    );
    assert!(operations.next().is_none());
}

#[test]
fn concrete_package_bridge_rejects_transport_shape_before_core_admission() {
    let owner = PackageProgramOwnerV1::from_compiled(finite_program([[0x80; 3], [0; 3]]));
    let mut session = owner.instantiate(11).unwrap();
    let empty_values = [];
    let malformed = [PackageProgramScenarioV1::new(1, &empty_values)];
    let error = match session.update(PackageProgramUpdateV1::Observed {
        revision: 1,
        scenarios: &malformed,
    }) {
        Ok(_) => panic!("schema-short package input must fail before Core admission"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramUpdateErrorKindV1::InvalidObservation
    );
    assert_eq!(session.state().kind(), PackageProgramStateKindV1::Waiting);
    assert_eq!(session.state().revision(), None);

    let white = [Srgb8::new([0xFF; 3])];
    let valid = [PackageProgramScenarioV1::new(1, &white)];
    session
        .update(PackageProgramUpdateV1::Observed {
            revision: 2,
            scenarios: &valid,
        })
        .unwrap();
    let duplicate = [
        PackageProgramScenarioV1::new(7, &white),
        PackageProgramScenarioV1::new(7, &white),
    ];
    let error = match session.update(PackageProgramUpdateV1::Observed {
        revision: 1,
        scenarios: &duplicate,
    }) {
        Ok(_) => panic!("duplicate scenario IDs must precede revision admission"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        PackageProgramUpdateErrorKindV1::InvalidObservation
    );
    assert_eq!(session.state().kind(), PackageProgramStateKindV1::Ready);
    assert_eq!(session.state().revision(), Some(2));
}
