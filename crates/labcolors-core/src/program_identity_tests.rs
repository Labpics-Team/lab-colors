use crate::Srgb8;
use crate::appearance::{
    EncodedPointPaintValueV1, OccurrenceId, OpacityInputId, PaintId, SurfaceId, SurfaceInputPortId,
};
use crate::composition::AdmittedOpacityV1;
use crate::family::{
    FamilyDeclarationV2, FamilyDefinitionDigestV2, FamilyId, canonical_family_image_digest_v2,
    semantic_family_release_id_v2,
};
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextId, AppearanceContextSchemaReleaseId,
    BackgroundLuminanceRatio, ColorSignal, IEC_SRGB_D65_XYZ_FRAME_V1, SurroundProfileId,
};
use crate::observation::ObservationGroupId;
use crate::program_session::{
    CompositionProfile, ConstraintId, ConstraintInvocation, ConstraintSet,
    CoreProgramConstraintInvocationV1, CoreProgramEvaluatorsV1, CoreProgramV1,
    DeclaredJointSelectionV1, FinitePaintDomainV1, JointCandidateStateV1, ObservationGroup,
    Occurrence, OpacityInput, OutputBinding, OutputSlotId, Paint, PointPresentationRootV1,
    PointPresentationTargetV1, PresentationRootId, Program, ProgramContentIdentityV8, Source,
    SourceId, Surface, Target, TargetCandidateChoiceV1, TargetCandidateId, TargetCandidateV1,
    TargetId,
};
use crate::relation::DirectedRelationV1;
use crate::selection_release::{
    MaterialisedSelectionV1, SelectionCandidateKeyV1, SelectionReleaseV1,
    admit_selection_release_v1, materialise_joint_selection_v1,
};
use crate::wcag22::Wcag22CriterionV1;

fn signal(value: [u8; 3]) -> ColorSignal {
    ColorSignal::from_srgb8(Srgb8::new(value))
}

fn opaque_value(signal: ColorSignal) -> EncodedPointPaintValueV1 {
    EncodedPointPaintValueV1::opaque(signal.srgb8())
}

fn paint_value(signal: ColorSignal, opacity: f64) -> EncodedPointPaintValueV1 {
    EncodedPointPaintValueV1::from_admitted(
        signal.srgb8(),
        AdmittedOpacityV1::new(opacity).unwrap(),
    )
}

fn finite_domain(candidates: Vec<TargetCandidateV1>) -> FinitePaintDomainV1 {
    FinitePaintDomainV1::try_new(candidates).unwrap()
}

fn context(surround: SurroundProfileId) -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        IEC_SRGB_D65_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(64.0).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        surround,
    )
}

fn declared_family(id: FamilyId, members: &[[u8; 3]]) -> FamilyDeclarationV2 {
    let mut members = members.iter().copied().map(signal).collect::<Vec<_>>();
    members.sort_unstable_by_key(|member| member.srgb8().bytes());
    members.dedup_by_key(|member| member.srgb8().bytes());
    let image = canonical_family_image_digest_v2(
        crate::lcs_occurrence::OutputProfileId::Iec61966Srgb8D65V1,
        &members,
    )
    .unwrap();
    let count = u64::try_from(members.len()).unwrap();
    let definition = FamilyDefinitionDigestV2::from_fixture_bytes_v2(b"identity-test-family");
    FamilyDeclarationV2::new(id, semantic_family_release_id_v2(definition, image, count))
}

fn family_identity_program(
    declarations: Vec<(FamilyId, Vec<[u8; 3]>)>,
    bindings: [FamilyId; 2],
) -> CoreProgramV1 {
    let sources = [SourceId::new(1), SourceId::new(2)];
    let targets = [TargetId::new(3), TargetId::new(4)];
    let paints = [PaintId::new(5), PaintId::new(6)];
    let port = SurfaceInputPortId::new(7);
    let surface = SurfaceId::new(8);
    let occurrences = [OccurrenceId::new(9), OccurrenceId::new(10)];
    let source_signals = [signal([0x20, 0x40, 0x60]), signal([0x80, 0x60, 0x40])];

    Program::new(
        vec![
            Source::new(sources[0], source_signals[0]),
            Source::new(sources[1], source_signals[1]),
        ],
        vec![
            Target::fixed(targets[0], sources[0]),
            Target::fixed(targets[1], sources[1]),
        ],
        ObservationGroup::new(ObservationGroupId::new(11), vec![port]),
        vec![],
        vec![
            Paint::Solid {
                id: paints[0],
                target: targets[0],
            },
            Paint::Solid {
                id: paints[1],
                target: targets[1],
            },
        ],
        vec![Surface::Input {
            id: surface,
            input: port,
        }],
        vec![
            Occurrence::new(
                occurrences[0],
                paints[0],
                surface,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(SurroundProfileId::AverageV1),
            ),
            Occurrence::new(
                occurrences[1],
                paints[1],
                surface,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(SurroundProfileId::AverageV1),
            ),
        ],
        ConstraintSet::new(
            vec![
                ConstraintInvocation::intrinsic_family_membership_hard(
                    ConstraintId::new(12),
                    targets[0],
                    bindings[0],
                ),
                ConstraintInvocation::intrinsic_family_membership_hard(
                    ConstraintId::new(13),
                    targets[1],
                    bindings[1],
                ),
                ConstraintInvocation::visible_unary_hard(
                    ConstraintId::new(14),
                    occurrences[0],
                    CoreProgramConstraintInvocationV1::ExactSrgb8(source_signals[0].srgb8()),
                ),
                ConstraintInvocation::visible_unary_hard(
                    ConstraintId::new(15),
                    occurrences[1],
                    CoreProgramConstraintInvocationV1::ExactSrgb8(source_signals[1].srgb8()),
                ),
            ],
            vec![],
        ),
        vec![
            OutputBinding::new(OutputSlotId::new(16), paints[0]),
            OutputBinding::new(OutputSlotId::new(17), paints[1]),
        ],
        CoreProgramEvaluatorsV1,
    )
    .with_families(
        declarations
            .into_iter()
            .map(|(id, members)| declared_family(id, &members))
            .collect(),
    )
}

#[test]
fn family_ids_and_declaration_order_are_alpha_invariant_in_v8() {
    let first = FamilyId::new(100);
    let second = FamilyId::new(200);
    let renamed_first = FamilyId::new(900);
    let renamed_second = FamilyId::new(700);
    let first_members = vec![[0x20, 0x40, 0x60], [0x80, 0x60, 0x40], [0x10, 0x30, 0x50]];
    let second_members = vec![[0x20, 0x40, 0x60], [0x80, 0x60, 0x40], [0x90, 0x70, 0x50]];

    let canonical = family_identity_program(
        vec![
            (first, first_members.clone()),
            (second, second_members.clone()),
        ],
        [first, second],
    )
    .compile()
    .unwrap();
    let renamed = family_identity_program(
        vec![
            (renamed_second, second_members),
            (renamed_first, first_members),
        ],
        [renamed_first, renamed_second],
    )
    .compile()
    .unwrap();

    assert_eq!(canonical.content_identity(), renamed.content_identity());
}

#[test]
fn family_content_is_bound_into_v8_identity() {
    let first = FamilyId::new(100);
    let second = FamilyId::new(200);
    let baseline = family_identity_program(
        vec![
            (
                first,
                vec![[0x20, 0x40, 0x60], [0x80, 0x60, 0x40], [0x10, 0x30, 0x50]],
            ),
            (
                second,
                vec![[0x20, 0x40, 0x60], [0x80, 0x60, 0x40], [0x90, 0x70, 0x50]],
            ),
        ],
        [first, second],
    )
    .compile()
    .unwrap();
    let changed = family_identity_program(
        vec![
            (
                first,
                vec![[0x20, 0x40, 0x60], [0x80, 0x60, 0x40], [0x10, 0x30, 0x50]],
            ),
            (
                second,
                vec![[0x20, 0x40, 0x60], [0x80, 0x60, 0x40], [0x91, 0x70, 0x50]],
            ),
        ],
        [first, second],
    )
    .compile()
    .unwrap();

    assert_ne!(baseline.content_identity(), changed.content_identity());
}

#[test]
fn constraint_family_binding_topology_is_bound_into_v8_identity() {
    let first = FamilyId::new(100);
    let second = FamilyId::new(200);
    let declarations = vec![
        (
            first,
            vec![[0x20, 0x40, 0x60], [0x80, 0x60, 0x40], [0x10, 0x30, 0x50]],
        ),
        (
            second,
            vec![[0x20, 0x40, 0x60], [0x80, 0x60, 0x40], [0x90, 0x70, 0x50]],
        ),
    ];
    let direct = family_identity_program(declarations.clone(), [first, second])
        .compile()
        .unwrap();
    let crossed = family_identity_program(declarations, [second, first])
        .compile()
        .unwrap();

    assert_ne!(direct.content_identity(), crossed.content_identity());
}

#[test]
fn shared_and_duplicated_equal_families_have_distinct_v8_identity() {
    let first = FamilyId::new(100);
    let second = FamilyId::new(200);
    let members = vec![[0x20, 0x40, 0x60], [0x80, 0x60, 0x40]];
    let shared = family_identity_program(vec![(first, members.clone())], [first, first])
        .compile()
        .unwrap();
    let duplicated = family_identity_program(
        vec![(first, members.clone()), (second, members)],
        [first, second],
    )
    .compile()
    .unwrap();

    assert_ne!(shared.content_identity(), duplicated.content_identity());
}

#[derive(Clone, Copy)]
struct FixedIds {
    sources: [SourceId; 2],
    targets: [TargetId; 2],
    paints: [PaintId; 2],
    ports: [SurfaceInputPortId; 2],
    surfaces: [SurfaceId; 2],
    occurrences: [OccurrenceId; 2],
    constraints: [ConstraintId; 2],
    outputs: [OutputSlotId; 2],
    group: ObservationGroupId,
}

#[derive(Clone, Copy)]
enum FixedMutation {
    None,
    SourceSignal,
    TargetSource,
}

fn fixed_program(
    ids: FixedIds,
    reverse_declarations: bool,
    second_signal: Srgb8,
    mutation: FixedMutation,
) -> CoreProgramV1 {
    let mut sources = vec![
        Source::new(ids.sources[0], signal([0x10, 0x20, 0x30])),
        Source::new(
            ids.sources[1],
            ColorSignal::from_srgb8(if matches!(mutation, FixedMutation::SourceSignal) {
                Srgb8::new([0x41, 0x50, 0x60])
            } else {
                second_signal
            }),
        ),
    ];
    let mut targets = vec![
        Target::fixed(ids.targets[0], ids.sources[0]),
        Target::fixed(
            ids.targets[1],
            if matches!(mutation, FixedMutation::TargetSource) {
                ids.sources[0]
            } else {
                ids.sources[1]
            },
        ),
    ];
    let mut paints = vec![
        Paint::Solid {
            id: ids.paints[0],
            target: ids.targets[0],
        },
        Paint::Solid {
            id: ids.paints[1],
            target: ids.targets[1],
        },
    ];
    let mut surfaces = vec![
        Surface::Input {
            id: ids.surfaces[0],
            input: ids.ports[0],
        },
        Surface::Input {
            id: ids.surfaces[1],
            input: ids.ports[1],
        },
    ];
    let mut occurrences = vec![
        Occurrence::new(
            ids.occurrences[0],
            ids.paints[0],
            ids.surfaces[0],
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(SurroundProfileId::AverageV1),
        ),
        Occurrence::new(
            ids.occurrences[1],
            ids.paints[1],
            ids.surfaces[1],
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(SurroundProfileId::DimV1),
        ),
    ];
    let mut hard = vec![
        ConstraintInvocation::visible_unary_hard(
            ids.constraints[0],
            ids.occurrences[0],
            CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0x10, 0x20, 0x30])),
        ),
        ConstraintInvocation::visible_unary_hard(
            ids.constraints[1],
            ids.occurrences[1],
            CoreProgramConstraintInvocationV1::ExactSrgb8(second_signal),
        ),
    ];
    let mut outputs = vec![
        OutputBinding::new(ids.outputs[0], ids.paints[0]),
        OutputBinding::new(ids.outputs[1], ids.paints[1]),
    ];
    let mut ports = ids.ports.to_vec();

    if reverse_declarations {
        sources.reverse();
        targets.reverse();
        paints.reverse();
        surfaces.reverse();
        occurrences.reverse();
        hard.reverse();
        outputs.reverse();
        ports.reverse();
    }

    Program::new(
        sources,
        targets,
        ObservationGroup::new(ids.group, ports),
        vec![],
        paints,
        surfaces,
        occurrences,
        ConstraintSet::new(hard, vec![]),
        outputs,
        CoreProgramEvaluatorsV1,
    )
}

#[test]
fn fixed_graph_identity_ignores_opaque_names_and_unordered_declaration_order() {
    let canonical = FixedIds {
        sources: [SourceId::new(10), SourceId::new(20)],
        targets: [TargetId::new(30), TargetId::new(40)],
        paints: [PaintId::new(50), PaintId::new(60)],
        ports: [SurfaceInputPortId::new(70), SurfaceInputPortId::new(80)],
        surfaces: [SurfaceId::new(90), SurfaceId::new(100)],
        occurrences: [OccurrenceId::new(110), OccurrenceId::new(120)],
        constraints: [ConstraintId::new(130), ConstraintId::new(140)],
        outputs: [OutputSlotId::new(150), OutputSlotId::new(160)],
        group: ObservationGroupId::new(170),
    };
    let renamed = FixedIds {
        sources: [SourceId::new(902), SourceId::new(101)],
        targets: [TargetId::new(804), TargetId::new(203)],
        paints: [PaintId::new(706), PaintId::new(305)],
        ports: [SurfaceInputPortId::new(608), SurfaceInputPortId::new(407)],
        surfaces: [SurfaceId::new(510), SurfaceId::new(409)],
        occurrences: [OccurrenceId::new(312), OccurrenceId::new(211)],
        constraints: [ConstraintId::new(114), ConstraintId::new(913)],
        outputs: [OutputSlotId::new(816), OutputSlotId::new(715)],
        group: ObservationGroupId::new(617),
    };

    let canonical = fixed_program(
        canonical,
        false,
        Srgb8::new([0x40, 0x50, 0x60]),
        FixedMutation::None,
    )
    .compile()
    .unwrap();
    let renamed = fixed_program(
        renamed,
        true,
        Srgb8::new([0x40, 0x50, 0x60]),
        FixedMutation::None,
    )
    .compile()
    .unwrap();

    assert_eq!(canonical.content_identity(), renamed.content_identity());
}

#[test]
fn presentation_topology_ignores_root_names_and_declaration_order() {
    let ids = FixedIds {
        sources: [SourceId::new(10), SourceId::new(20)],
        targets: [TargetId::new(30), TargetId::new(40)],
        paints: [PaintId::new(50), PaintId::new(60)],
        ports: [SurfaceInputPortId::new(70), SurfaceInputPortId::new(80)],
        surfaces: [SurfaceId::new(90), SurfaceId::new(100)],
        occurrences: [OccurrenceId::new(110), OccurrenceId::new(120)],
        constraints: [ConstraintId::new(130), ConstraintId::new(140)],
        outputs: [OutputSlotId::new(150), OutputSlotId::new(160)],
        group: ObservationGroupId::new(170),
    };
    let canonical = fixed_program(
        ids,
        false,
        Srgb8::new([0x40, 0x50, 0x60]),
        FixedMutation::None,
    )
    .with_point_presentations(
        vec![
            PointPresentationRootV1::new(PresentationRootId::new(1), ids.occurrences[0]),
            PointPresentationRootV1::new(PresentationRootId::new(2), ids.occurrences[1]),
        ],
        vec![
            PointPresentationTargetV1::new(PresentationRootId::new(1), ids.occurrences[0]),
            PointPresentationTargetV1::new(PresentationRootId::new(2), ids.occurrences[1]),
        ],
    )
    .compile()
    .unwrap();
    let renamed = fixed_program(
        ids,
        true,
        Srgb8::new([0x40, 0x50, 0x60]),
        FixedMutation::None,
    )
    .with_point_presentations(
        vec![
            PointPresentationRootV1::new(PresentationRootId::new(900), ids.occurrences[1]),
            PointPresentationRootV1::new(PresentationRootId::new(700), ids.occurrences[0]),
        ],
        vec![
            PointPresentationTargetV1::new(PresentationRootId::new(900), ids.occurrences[1]),
            PointPresentationTargetV1::new(PresentationRootId::new(700), ids.occurrences[0]),
        ],
    )
    .compile()
    .unwrap();
    assert_eq!(canonical.content_identity(), renamed.content_identity());
}

#[test]
fn presentation_root_terminal_relation_changes_v8_identity() {
    let ids = canonical_full_ids();
    let root = PresentationRootId::new(1);
    let first = full_program(ids, false, FullMutation::None)
        .with_point_presentations(
            vec![PointPresentationRootV1::new(root, ids.occurrences[1])],
            vec![PointPresentationTargetV1::new(root, ids.occurrences[0])],
        )
        .compile()
        .unwrap();
    let second = full_program(ids, false, FullMutation::None)
        .with_point_presentations(
            vec![PointPresentationRootV1::new(root, ids.occurrences[2])],
            vec![PointPresentationTargetV1::new(root, ids.occurrences[0])],
        )
        .compile()
        .unwrap();
    assert_ne!(first.content_identity(), second.content_identity());
}

#[test]
fn presentation_target_relation_and_multiplicity_change_v8_identity() {
    let ids = canonical_full_ids();
    let root = PresentationRootId::new(1);
    let nested_target = full_program(ids, false, FullMutation::None)
        .with_point_presentations(
            vec![PointPresentationRootV1::new(root, ids.occurrences[1])],
            vec![PointPresentationTargetV1::new(root, ids.occurrences[0])],
        )
        .compile()
        .unwrap();
    let root_target = full_program(ids, false, FullMutation::None)
        .with_point_presentations(
            vec![PointPresentationRootV1::new(root, ids.occurrences[1])],
            vec![PointPresentationTargetV1::new(root, ids.occurrences[1])],
        )
        .compile()
        .unwrap();
    let duplicated_topology = full_program(ids, false, FullMutation::None)
        .with_point_presentations(
            vec![
                PointPresentationRootV1::new(root, ids.occurrences[1]),
                PointPresentationRootV1::new(PresentationRootId::new(2), ids.occurrences[1]),
            ],
            vec![
                PointPresentationTargetV1::new(root, ids.occurrences[0]),
                PointPresentationTargetV1::new(PresentationRootId::new(2), ids.occurrences[0]),
            ],
        )
        .compile()
        .unwrap();

    assert_ne!(
        nested_target.content_identity(),
        root_target.content_identity()
    );
    assert_ne!(
        nested_target.content_identity(),
        duplicated_topology.content_identity()
    );
}

#[test]
fn canonical_v8_digest_is_cross_platform_golden() {
    let ids = FixedIds {
        sources: [SourceId::new(10), SourceId::new(20)],
        targets: [TargetId::new(30), TargetId::new(40)],
        paints: [PaintId::new(50), PaintId::new(60)],
        ports: [SurfaceInputPortId::new(70), SurfaceInputPortId::new(80)],
        surfaces: [SurfaceId::new(90), SurfaceId::new(100)],
        occurrences: [OccurrenceId::new(110), OccurrenceId::new(120)],
        constraints: [ConstraintId::new(130), ConstraintId::new(140)],
        outputs: [OutputSlotId::new(150), OutputSlotId::new(160)],
        group: ObservationGroupId::new(170),
    };
    let compiled = fixed_program(
        ids,
        false,
        Srgb8::new([0x40, 0x50, 0x60]),
        FixedMutation::None,
    )
    .compile()
    .unwrap();

    let identity: ProgramContentIdentityV8 = compiled.content_identity();
    assert_eq!(
        identity.as_bytes(),
        &[
            94, 214, 174, 159, 139, 60, 138, 90, 242, 92, 158, 76, 153, 97, 208, 124, 207, 174, 28,
            202, 158, 215, 146, 193, 71, 101, 104, 48, 179, 241, 170, 156,
        ]
    );
}

#[test]
fn source_signal_and_target_source_edge_are_independently_content_bound() {
    let ids = FixedIds {
        sources: [SourceId::new(10), SourceId::new(20)],
        targets: [TargetId::new(30), TargetId::new(40)],
        paints: [PaintId::new(50), PaintId::new(60)],
        ports: [SurfaceInputPortId::new(70), SurfaceInputPortId::new(80)],
        surfaces: [SurfaceId::new(90), SurfaceId::new(100)],
        occurrences: [OccurrenceId::new(110), OccurrenceId::new(120)],
        constraints: [ConstraintId::new(130), ConstraintId::new(140)],
        outputs: [OutputSlotId::new(150), OutputSlotId::new(160)],
        group: ObservationGroupId::new(170),
    };
    let baseline = fixed_program(
        ids,
        false,
        Srgb8::new([0x40, 0x50, 0x60]),
        FixedMutation::None,
    )
    .compile()
    .unwrap()
    .content_identity();
    let changed_signal = fixed_program(
        ids,
        false,
        Srgb8::new([0x40, 0x50, 0x60]),
        FixedMutation::SourceSignal,
    )
    .compile()
    .unwrap()
    .content_identity();
    let changed_target_source = fixed_program(
        ids,
        false,
        Srgb8::new([0x40, 0x50, 0x60]),
        FixedMutation::TargetSource,
    )
    .compile()
    .unwrap()
    .content_identity();

    assert_ne!(changed_signal, baseline);
    assert_ne!(changed_target_source, baseline);
}

#[derive(Clone, Copy)]
struct FullIds {
    sources: [SourceId; 2],
    targets: [TargetId; 2],
    candidates: [[TargetCandidateId; 2]; 2],
    opacities: [OpacityInputId; 2],
    paints: [PaintId; 4],
    port: SurfaceInputPortId,
    surfaces: [SurfaceId; 2],
    occurrences: [OccurrenceId; 3],
    constraints: [ConstraintId; 3],
    outputs: [OutputSlotId; 2],
    group: ObservationGroupId,
}

#[derive(Debug, Clone, Copy)]
enum FullMutation {
    None,
    CompleteSchemaGolden,
    CandidateSignal,
    OpacityValue,
    OpacityPositiveZero,
    OpacityNegativeZero,
    PaintTarget,
    OpacitySource,
    OpacityInput,
    OccurrenceSubject,
    Context,
    ConstraintTarget,
    ConstraintMode,
    ConstraintFamily,
    ConstraintInvocation,
    ConstraintMultiplicity,
    OutputBinding,
    OutputMultiplicity,
}

fn full_program(ids: FullIds, reverse_unordered: bool, mutation: FullMutation) -> CoreProgramV1 {
    let mut candidate_signals = [
        [signal([0x10, 0x20, 0x30]), signal([0x30, 0x20, 0x10])],
        [signal([0x20, 0x60, 0x40]), signal([0x60, 0x40, 0x20])],
    ];
    if matches!(mutation, FullMutation::CandidateSignal) {
        candidate_signals[1][1] = signal([0x61, 0x40, 0x20]);
    }
    let mut sources = vec![
        Source::new(ids.sources[0], signal([0x08, 0x10, 0x18])),
        Source::new(ids.sources[1], signal([0x18, 0x10, 0x08])),
    ];
    let mut targets = (0..2)
        .map(|target| {
            let mut candidates = (0..2)
                .map(|candidate| {
                    TargetCandidateV1::new(
                        ids.candidates[target][candidate],
                        opaque_value(candidate_signals[target][candidate]),
                    )
                })
                .collect::<Vec<_>>();
            if reverse_unordered {
                candidates.reverse();
            }
            Target::finite(ids.targets[target], finite_domain(candidates))
        })
        .collect::<Vec<_>>();
    let mut paints = vec![
        Paint::Solid {
            id: ids.paints[0],
            target: if matches!(mutation, FullMutation::PaintTarget) {
                ids.targets[1]
            } else {
                ids.targets[0]
            },
        },
        Paint::Opacity {
            id: ids.paints[1],
            source: if matches!(mutation, FullMutation::OpacitySource) {
                ids.paints[2]
            } else {
                ids.paints[0]
            },
            opacity: if matches!(mutation, FullMutation::OpacityInput) {
                ids.opacities[1]
            } else {
                ids.opacities[0]
            },
        },
        Paint::Solid {
            id: ids.paints[2],
            target: ids.targets[1],
        },
        Paint::Solid {
            id: ids.paints[3],
            target: ids.targets[0],
        },
    ];
    let mut surfaces = vec![
        Surface::Input {
            id: ids.surfaces[0],
            input: ids.port,
        },
        Surface::FromOccurrence {
            id: ids.surfaces[1],
            occurrence: ids.occurrences[0],
        },
    ];
    let mut occurrences = vec![
        Occurrence::new(
            ids.occurrences[0],
            if matches!(mutation, FullMutation::OccurrenceSubject) {
                ids.paints[2]
            } else {
                ids.paints[1]
            },
            ids.surfaces[0],
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(SurroundProfileId::AverageV1),
        ),
        Occurrence::new(
            ids.occurrences[1],
            ids.paints[2],
            ids.surfaces[1],
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(if matches!(mutation, FullMutation::Context) {
                SurroundProfileId::DarkV1
            } else {
                SurroundProfileId::DimV1
            }),
        ),
        Occurrence::new(
            ids.occurrences[2],
            ids.paints[3],
            ids.surfaces[1],
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(SurroundProfileId::DarkV1),
        ),
    ];
    let mut hard = vec![ConstraintInvocation::visible_unary_hard(
        ids.constraints[0],
        if matches!(mutation, FullMutation::ConstraintTarget) {
            ids.occurrences[1]
        } else {
            ids.occurrences[0]
        },
        CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0x11, 0x22, 0x33])),
    )];
    let second_invocation = if matches!(mutation, FullMutation::ConstraintFamily) {
        CoreProgramConstraintInvocationV1::Wcag22Srgb8(Wcag22CriterionV1::Sc143TextDefault)
    } else {
        CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new(
            if matches!(mutation, FullMutation::ConstraintInvocation) {
                [0x44, 0x55, 0x67]
            } else {
                [0x44, 0x55, 0x66]
            },
        ))
    };
    let mut report_only = Vec::new();
    if matches!(mutation, FullMutation::ConstraintMode) {
        report_only.push(ConstraintInvocation::visible_unary_report_only(
            ids.constraints[1],
            ids.occurrences[1],
            second_invocation,
        ));
    } else {
        hard.push(ConstraintInvocation::visible_unary_hard(
            ids.constraints[1],
            ids.occurrences[1],
            second_invocation,
        ));
    }
    if matches!(mutation, FullMutation::CompleteSchemaGolden) {
        report_only.push(ConstraintInvocation::visible_unary_report_only(
            ConstraintId::new(1_001),
            ids.occurrences[2],
            CoreProgramConstraintInvocationV1::Wcag22Srgb8(
                Wcag22CriterionV1::Sc1411GraphicalObject,
            ),
        ));
        report_only.push(ConstraintInvocation::visible_unary_report_only(
            ConstraintId::new(1_010),
            ids.occurrences[2],
            CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0x21, 0x32, 0x43])),
        ));
        hard.push(ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(1_011),
            ids.occurrences[2],
            CoreProgramConstraintInvocationV1::Wcag22Srgb8(
                Wcag22CriterionV1::Sc1411UiComponentOrState,
            ),
        ));
    }
    hard.push(ConstraintInvocation::visible_unary_hard(
        ids.constraints[2],
        ids.occurrences[2],
        CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0x21, 0x32, 0x43])),
    ));
    if matches!(mutation, FullMutation::ConstraintMultiplicity) {
        hard.push(ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(1_000),
            ids.occurrences[1],
            CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0x44, 0x55, 0x66])),
        ));
    }
    if matches!(mutation, FullMutation::CompleteSchemaGolden) {
        let fixed_target = TargetId::new(1_003);
        let fixed_occurrence = OccurrenceId::new(1_005);
        targets.push(Target::fixed(fixed_target, ids.sources[0]));
        paints.push(Paint::Solid {
            id: PaintId::new(1_004),
            target: fixed_target,
        });
        occurrences.push(Occurrence::new(
            fixed_occurrence,
            PaintId::new(1_004),
            ids.surfaces[0],
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(SurroundProfileId::AverageV1),
        ));
        hard.push(ConstraintInvocation::exact_intrinsic_unary_hard(
            ConstraintId::new(1_006),
            ids.targets[0],
            candidate_signals[0][0].srgb8(),
        ));
        hard.push(ConstraintInvocation::intrinsic_family_membership_hard(
            ConstraintId::new(1_012),
            ids.targets[0],
            FamilyId::new(1_013),
        ));
        hard.push(ConstraintInvocation::exact_intrinsic_relation_hard(
            ConstraintId::new(1_007),
            DirectedRelationV1::try_new(fixed_target, vec![ids.targets[0]]).unwrap(),
        ));
        hard.push(ConstraintInvocation::exact_visible_relation_hard(
            ConstraintId::new(1_008),
            DirectedRelationV1::try_new(fixed_occurrence, vec![ids.occurrences[0]]).unwrap(),
        ));
        hard.push(ConstraintInvocation::declared_srgb8_clean_set_hard(
            ConstraintId::new(1_009),
            PointPresentationTargetV1::new(PresentationRootId::new(1_002), ids.occurrences[0]),
        ));
    }
    let mut outputs = vec![
        OutputBinding::new(
            ids.outputs[0],
            if matches!(mutation, FullMutation::OutputBinding) {
                ids.paints[1]
            } else {
                ids.paints[2]
            },
        ),
        OutputBinding::new(ids.outputs[1], ids.paints[3]),
    ];
    if matches!(mutation, FullMutation::OutputMultiplicity) {
        outputs.push(OutputBinding::new(OutputSlotId::new(1_000), ids.paints[0]));
    }
    let mut opacities = vec![
        OpacityInput::new(
            ids.opacities[0],
            match mutation {
                FullMutation::OpacityValue => 0.5,
                FullMutation::OpacityPositiveZero => 0.0,
                FullMutation::OpacityNegativeZero => -0.0,
                _ => 0.625,
            },
        ),
        OpacityInput::new(ids.opacities[1], 0.25),
    ];
    if reverse_unordered {
        sources.reverse();
        targets.reverse();
        opacities.reverse();
        paints.reverse();
        surfaces.reverse();
        occurrences.reverse();
        hard.reverse();
        report_only.reverse();
        outputs.reverse();
    }

    let mut states = Vec::new();
    for first in 0..2 {
        for second in 0..2 {
            let mut choices = vec![
                TargetCandidateChoiceV1::new(ids.targets[0], ids.candidates[0][first]),
                TargetCandidateChoiceV1::new(ids.targets[1], ids.candidates[1][second]),
            ];
            if reverse_unordered {
                choices.reverse();
            }
            states.push(JointCandidateStateV1::new(choices));
        }
    }

    let program = Program::new(
        sources,
        targets,
        ObservationGroup::new(ids.group, vec![ids.port]),
        opacities,
        paints,
        surfaces,
        occurrences,
        ConstraintSet::new(hard, report_only),
        outputs,
        CoreProgramEvaluatorsV1,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(states));

    if matches!(mutation, FullMutation::CompleteSchemaGolden) {
        program
            .with_families(vec![declared_family(
                FamilyId::new(1_013),
                &[[0x10, 0x20, 0x30], [0x30, 0x20, 0x10]],
            )])
            .with_point_presentations(
                vec![PointPresentationRootV1::new(
                    PresentationRootId::new(1_002),
                    ids.occurrences[1],
                )],
                vec![PointPresentationTargetV1::new(
                    PresentationRootId::new(1_002),
                    ids.occurrences[0],
                )],
            )
    } else {
        program
    }
}

fn canonical_full_ids() -> FullIds {
    FullIds {
        sources: [SourceId::new(1), SourceId::new(2)],
        targets: [TargetId::new(3), TargetId::new(4)],
        candidates: [
            [TargetCandidateId::new(5), TargetCandidateId::new(6)],
            [TargetCandidateId::new(7), TargetCandidateId::new(8)],
        ],
        opacities: [OpacityInputId::new(9), OpacityInputId::new(10)],
        paints: [
            PaintId::new(11),
            PaintId::new(12),
            PaintId::new(13),
            PaintId::new(14),
        ],
        port: SurfaceInputPortId::new(15),
        surfaces: [SurfaceId::new(16), SurfaceId::new(17)],
        occurrences: [
            OccurrenceId::new(18),
            OccurrenceId::new(19),
            OccurrenceId::new(20),
        ],
        constraints: [
            ConstraintId::new(21),
            ConstraintId::new(22),
            ConstraintId::new(23),
        ],
        outputs: [OutputSlotId::new(24), OutputSlotId::new(25)],
        group: ObservationGroupId::new(26),
    }
}

#[test]
fn complete_program_schema_v8_digest_is_cross_platform_golden() {
    // Вместе с fixed golden этот Program содержит каждый V8 vertex/edge tag,
    // все constraint topology и оба режима. Случайная смена кодировки требует
    // явной смены версии, а не тихого перевыпуска прежнего content address.
    let compiled = full_program(
        canonical_full_ids(),
        false,
        FullMutation::CompleteSchemaGolden,
    )
    .compile()
    .unwrap();

    let schema = crate::program_session::program_identity_graph_schema_for_test(&full_program(
        canonical_full_ids(),
        false,
        FullMutation::CompleteSchemaGolden,
    ))
    .unwrap();
    assert_eq!(schema.0, (1_u8..=22).collect::<Vec<_>>());
    let edge_role_count = crate::program_session::program_identity_edge_role_count_for_test();
    assert_eq!(edge_role_count, 28);
    assert_eq!(schema.1.len(), edge_role_count);
    assert_eq!(
        schema.1,
        (1_u8..=u8::try_from(edge_role_count).unwrap()).collect::<Vec<_>>()
    );

    assert_eq!(
        compiled.content_identity().as_bytes(),
        &[
            146, 121, 82, 10, 253, 244, 184, 221, 77, 198, 219, 37, 215, 66, 5, 155, 57, 56, 28,
            28, 86, 12, 217, 252, 66, 255, 127, 109, 112, 87, 28, 26,
        ]
    );
}

#[test]
fn every_typed_opaque_namespace_and_unordered_list_is_alpha_invariant() {
    let canonical = canonical_full_ids();
    let renamed = FullIds {
        sources: [SourceId::new(2), SourceId::new(1)],
        targets: [TargetId::new(2), TargetId::new(1)],
        candidates: [
            [TargetCandidateId::new(2), TargetCandidateId::new(1)],
            [TargetCandidateId::new(2), TargetCandidateId::new(1)],
        ],
        opacities: [OpacityInputId::new(2), OpacityInputId::new(1)],
        paints: [
            PaintId::new(4),
            PaintId::new(3),
            PaintId::new(2),
            PaintId::new(1),
        ],
        port: SurfaceInputPortId::new(1),
        surfaces: [SurfaceId::new(2), SurfaceId::new(1)],
        occurrences: [
            OccurrenceId::new(3),
            OccurrenceId::new(2),
            OccurrenceId::new(1),
        ],
        constraints: [
            ConstraintId::new(3),
            ConstraintId::new(2),
            ConstraintId::new(1),
        ],
        outputs: [OutputSlotId::new(2), OutputSlotId::new(1)],
        group: ObservationGroupId::new(1),
    };

    let canonical = full_program(canonical, false, FullMutation::None)
        .compile()
        .unwrap();
    let renamed = full_program(renamed, true, FullMutation::None)
        .compile()
        .unwrap();

    assert_eq!(canonical.content_identity(), renamed.content_identity());
}

#[test]
fn independent_program_content_mutations_change_identity() {
    let ids = canonical_full_ids();
    let baseline = full_program(ids, false, FullMutation::None)
        .compile()
        .unwrap()
        .content_identity();

    for mutation in [
        FullMutation::CandidateSignal,
        FullMutation::OpacityValue,
        FullMutation::PaintTarget,
        FullMutation::OpacitySource,
        FullMutation::OpacityInput,
        FullMutation::OccurrenceSubject,
        FullMutation::Context,
        FullMutation::ConstraintTarget,
        FullMutation::ConstraintMode,
        FullMutation::ConstraintFamily,
        FullMutation::ConstraintInvocation,
        FullMutation::ConstraintMultiplicity,
        FullMutation::OutputBinding,
        FullMutation::OutputMultiplicity,
    ] {
        let compiled = full_program(ids, false, mutation)
            .compile()
            .unwrap_or_else(|error| panic!("{mutation:?} must remain valid: {error:?}"));
        assert_ne!(compiled.content_identity(), baseline, "{mutation:?}");
    }
}

#[test]
fn signed_zero_opacity_has_one_physical_content_identity() {
    let ids = canonical_full_ids();

    let positive = full_program(ids, false, FullMutation::OpacityPositiveZero)
        .compile()
        .unwrap();
    let negative = full_program(ids, false, FullMutation::OpacityNegativeZero)
        .compile()
        .unwrap();

    assert_eq!(positive.content_identity(), negative.content_identity());
}

fn nested_surface_program(
    surface_from_second_occurrence: bool,
    third_uses_nested_surface: bool,
) -> CoreProgramV1 {
    let sources = [SourceId::new(1), SourceId::new(2)];
    let targets = [TargetId::new(3), TargetId::new(4)];
    let paints = [PaintId::new(5), PaintId::new(6)];
    let port = SurfaceInputPortId::new(7);
    let surfaces = [SurfaceId::new(8), SurfaceId::new(9)];
    let occurrences = [
        OccurrenceId::new(10),
        OccurrenceId::new(11),
        OccurrenceId::new(12),
    ];

    Program::new(
        vec![
            Source::new(sources[0], signal([0x20, 0x30, 0x40])),
            Source::new(sources[1], signal([0x70, 0x60, 0x50])),
        ],
        vec![
            Target::fixed(targets[0], sources[0]),
            Target::fixed(targets[1], sources[1]),
        ],
        ObservationGroup::new(ObservationGroupId::new(13), vec![port]),
        vec![],
        vec![
            Paint::Solid {
                id: paints[0],
                target: targets[0],
            },
            Paint::Solid {
                id: paints[1],
                target: targets[1],
            },
        ],
        vec![
            Surface::Input {
                id: surfaces[0],
                input: port,
            },
            Surface::FromOccurrence {
                id: surfaces[1],
                occurrence: occurrences[usize::from(surface_from_second_occurrence)],
            },
        ],
        vec![
            Occurrence::new(
                occurrences[0],
                paints[0],
                surfaces[0],
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(SurroundProfileId::AverageV1),
            ),
            Occurrence::new(
                occurrences[1],
                paints[1],
                surfaces[0],
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(SurroundProfileId::DimV1),
            ),
            Occurrence::new(
                occurrences[2],
                paints[0],
                surfaces[usize::from(third_uses_nested_surface)],
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(SurroundProfileId::DarkV1),
            ),
        ],
        ConstraintSet::new(
            occurrences
                .iter()
                .copied()
                .enumerate()
                .map(|(index, occurrence)| {
                    ConstraintInvocation::visible_unary_hard(
                        ConstraintId::new(14 + index as u32),
                        occurrence,
                        CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([
                            0x20 + index as u8,
                            0x30,
                            0x40,
                        ])),
                    )
                })
                .collect(),
            vec![],
        ),
        vec![
            OutputBinding::new(OutputSlotId::new(17), paints[0]),
            OutputBinding::new(OutputSlotId::new(18), paints[1]),
        ],
        CoreProgramEvaluatorsV1,
    )
}

#[test]
fn surface_and_occurrence_relations_are_content_bound() {
    let baseline = nested_surface_program(false, true)
        .compile()
        .unwrap()
        .content_identity();
    let changed_surface_source = nested_surface_program(true, true)
        .compile()
        .unwrap()
        .content_identity();
    let changed_occurrence_backdrop = nested_surface_program(false, false)
        .compile()
        .unwrap()
        .content_identity();

    assert_ne!(changed_surface_source, baseline);
    assert_ne!(changed_occurrence_backdrop, baseline);
}

#[derive(Clone, Copy)]
enum SubjectPaintShape {
    OpacityFromFirst,
    OpacityFromSecond,
    Solid,
}

fn paint_shape_program(shape: SubjectPaintShape) -> CoreProgramV1 {
    let sources = [SourceId::new(1), SourceId::new(2)];
    let targets = [TargetId::new(3), TargetId::new(4)];
    let paints = [PaintId::new(5), PaintId::new(6), PaintId::new(7)];
    let opacity = OpacityInputId::new(8);
    let port = SurfaceInputPortId::new(9);
    let surface = SurfaceId::new(10);
    let occurrence = OccurrenceId::new(11);
    let subject = match shape {
        SubjectPaintShape::OpacityFromFirst => Paint::Opacity {
            id: paints[1],
            source: paints[0],
            opacity,
        },
        SubjectPaintShape::OpacityFromSecond => Paint::Opacity {
            id: paints[1],
            source: paints[2],
            opacity,
        },
        SubjectPaintShape::Solid => Paint::Solid {
            id: paints[1],
            target: targets[0],
        },
    };

    Program::new(
        vec![
            Source::new(sources[0], signal([0x20, 0x30, 0x40])),
            Source::new(sources[1], signal([0x70, 0x60, 0x50])),
        ],
        vec![
            Target::fixed(targets[0], sources[0]),
            Target::fixed(targets[1], sources[1]),
        ],
        ObservationGroup::new(ObservationGroupId::new(12), vec![port]),
        vec![OpacityInput::new(opacity, 0.5)],
        vec![
            Paint::Solid {
                id: paints[0],
                target: targets[0],
            },
            subject,
            Paint::Solid {
                id: paints[2],
                target: targets[1],
            },
        ],
        vec![Surface::Input {
            id: surface,
            input: port,
        }],
        vec![Occurrence::new(
            occurrence,
            paints[1],
            surface,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(SurroundProfileId::AverageV1),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(13),
                occurrence,
                CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0x20, 0x30, 0x40])),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OutputSlotId::new(14), paints[1])],
        CoreProgramEvaluatorsV1,
    )
}

#[test]
fn paint_variant_and_dependency_edges_are_content_bound() {
    let baseline = paint_shape_program(SubjectPaintShape::OpacityFromFirst)
        .compile()
        .unwrap()
        .content_identity();
    let changed_source = paint_shape_program(SubjectPaintShape::OpacityFromSecond)
        .compile()
        .unwrap()
        .content_identity();
    let changed_variant = paint_shape_program(SubjectPaintShape::Solid)
        .compile()
        .unwrap()
        .content_identity();

    assert_ne!(changed_source, baseline);
    assert_ne!(changed_variant, baseline);
}

fn source_alias_program(shared: bool) -> CoreProgramV1 {
    let sources = if shared {
        vec![Source::new(SourceId::new(1), signal([0x30, 0x40, 0x50]))]
    } else {
        vec![
            Source::new(SourceId::new(1), signal([0x30, 0x40, 0x50])),
            Source::new(SourceId::new(2), signal([0x30, 0x40, 0x50])),
        ]
    };
    let targets = [TargetId::new(3), TargetId::new(4)];
    let paints = [PaintId::new(5), PaintId::new(6)];
    let port = SurfaceInputPortId::new(7);
    let surface = SurfaceId::new(8);
    let occurrences = [OccurrenceId::new(9), OccurrenceId::new(10)];
    Program::new(
        sources,
        vec![
            Target::fixed(targets[0], SourceId::new(1)),
            Target::fixed(
                targets[1],
                if shared {
                    SourceId::new(1)
                } else {
                    SourceId::new(2)
                },
            ),
        ],
        ObservationGroup::new(ObservationGroupId::new(11), vec![port]),
        vec![],
        vec![
            Paint::Solid {
                id: paints[0],
                target: targets[0],
            },
            Paint::Solid {
                id: paints[1],
                target: targets[1],
            },
        ],
        vec![Surface::Input {
            id: surface,
            input: port,
        }],
        vec![
            Occurrence::new(
                occurrences[0],
                paints[0],
                surface,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(SurroundProfileId::AverageV1),
            ),
            Occurrence::new(
                occurrences[1],
                paints[1],
                surface,
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(SurroundProfileId::AverageV1),
            ),
        ],
        ConstraintSet::new(
            vec![
                ConstraintInvocation::visible_unary_hard(
                    ConstraintId::new(12),
                    occurrences[0],
                    CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0x30, 0x40, 0x50])),
                ),
                ConstraintInvocation::visible_unary_hard(
                    ConstraintId::new(13),
                    occurrences[1],
                    CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0x30, 0x40, 0x50])),
                ),
            ],
            vec![],
        ),
        vec![
            OutputBinding::new(OutputSlotId::new(14), paints[0]),
            OutputBinding::new(OutputSlotId::new(15), paints[1]),
        ],
        CoreProgramEvaluatorsV1,
    )
}

#[test]
fn shared_content_and_equal_duplicated_content_have_distinct_identity() {
    let shared = source_alias_program(true).compile().unwrap();
    let duplicated = source_alias_program(false).compile().unwrap();

    assert_ne!(shared.content_identity(), duplicated.content_identity());
}

fn finite_program_with_opacity(reverse_order: bool, first_opacity: f64) -> CoreProgramV1 {
    let source = SourceId::new(1);
    let target = TargetId::new(2);
    let first = TargetCandidateId::new(3);
    let second = TargetCandidateId::new(4);
    let paint = PaintId::new(5);
    let port = SurfaceInputPortId::new(6);
    let surface = SurfaceId::new(7);
    let occurrence = OccurrenceId::new(8);
    let states = [first, second].map(|candidate| {
        JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(target, candidate)])
    });
    let states = if reverse_order {
        vec![states[1].clone(), states[0].clone()]
    } else {
        states.to_vec()
    };

    Program::new(
        vec![Source::new(source, signal([0; 3]))],
        vec![Target::finite(
            target,
            finite_domain(vec![
                TargetCandidateV1::new(first, paint_value(signal([0; 3]), first_opacity)),
                TargetCandidateV1::new(second, opaque_value(signal([0xFF; 3]))),
            ]),
        )],
        ObservationGroup::new(ObservationGroupId::new(9), vec![port]),
        vec![],
        vec![Paint::Solid { id: paint, target }],
        vec![Surface::Input {
            id: surface,
            input: port,
        }],
        vec![Occurrence::new(
            occurrence,
            paint,
            surface,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(SurroundProfileId::AverageV1),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(10),
                occurrence,
                CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0; 3])),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OutputSlotId::new(11), paint)],
        CoreProgramEvaluatorsV1,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(states))
}

fn finite_program(reverse_order: bool) -> CoreProgramV1 {
    finite_program_with_opacity(reverse_order, 1.0)
}

fn finite_materialised_selection(revision: u64) -> MaterialisedSelectionV1 {
    let target = TargetId::new(2);
    let first = TargetCandidateId::new(3);
    let second = TargetCandidateId::new(4);
    let key = |bytes: &[u8]| SelectionCandidateKeyV1::new(bytes.to_vec().into_boxed_slice());
    let release = SelectionReleaseV1::new(
        revision,
        vec![
            vec![key(b"first")].into_boxed_slice(),
            vec![key(b"second")].into_boxed_slice(),
        ]
        .into_boxed_slice(),
    );
    let admitted = admit_selection_release_v1(release).expect("finite test release must admit");
    materialise_joint_selection_v1(
        &admitted,
        &[
            (
                JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(target, second)]),
                key(b"second"),
            ),
            (
                JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(target, first)]),
                key(b"first"),
            ),
        ],
    )
    .expect("complete finite test bindings must materialise")
}

fn finite_program_with_permuted_source_colors(swap_source_signals: bool) -> CoreProgramV1 {
    let sources = [SourceId::new(1), SourceId::new(2)];
    let target = TargetId::new(3);
    let first = TargetCandidateId::new(4);
    let second = TargetCandidateId::new(5);
    let paint = PaintId::new(6);
    let port = SurfaceInputPortId::new(7);
    let surface = SurfaceId::new(8);
    let occurrence = OccurrenceId::new(9);

    let mut signals = [signal([0x12, 0x34, 0x56]), signal([0xAB, 0xCD, 0xEF])];
    if swap_source_signals {
        signals.reverse();
    }

    Program::new(
        vec![
            Source::new(sources[0], signals[0]),
            Source::new(sources[1], signals[1]),
        ],
        vec![Target::finite(
            target,
            finite_domain(vec![
                TargetCandidateV1::new(first, opaque_value(signal([0; 3]))),
                TargetCandidateV1::new(second, opaque_value(signal([0xFF; 3]))),
            ]),
        )],
        ObservationGroup::new(ObservationGroupId::new(10), vec![port]),
        vec![],
        vec![Paint::Solid { id: paint, target }],
        vec![Surface::Input {
            id: surface,
            input: port,
        }],
        vec![Occurrence::new(
            occurrence,
            paint,
            surface,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(SurroundProfileId::AverageV1),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(11),
                occurrence,
                CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0; 3])),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OutputSlotId::new(12), paint)],
        CoreProgramEvaluatorsV1,
    )
    .with_joint_selection(DeclaredJointSelectionV1::new(vec![
        JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(target, first)]),
        JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(target, second)]),
    ]))
}

#[test]
fn finite_target_has_no_source_incidence_in_content_identity() {
    // Оба Source намеренно остаются членами Program: перестановка их цветов
    // сохраняет мультимножество вершин, но меняет цвет вершины, которую выделило
    // бы запрещённое finite Target→Source ребро. Равенство тем самым доказывает
    // отсутствие ребра, не заявляя, что прочие декларации исчезают из identity.
    let first = finite_program_with_permuted_source_colors(false)
        .compile()
        .unwrap();
    let second = finite_program_with_permuted_source_colors(true)
        .compile()
        .unwrap();

    assert_eq!(first.content_identity(), second.content_identity());
}

fn fixed_single_target_program() -> CoreProgramV1 {
    let source = SourceId::new(1);
    let target = TargetId::new(2);
    let paint = PaintId::new(5);
    let port = SurfaceInputPortId::new(6);
    let surface = SurfaceId::new(7);
    let occurrence = OccurrenceId::new(8);

    Program::new(
        vec![Source::new(source, signal([0; 3]))],
        vec![Target::fixed(target, source)],
        ObservationGroup::new(ObservationGroupId::new(9), vec![port]),
        vec![],
        vec![Paint::Solid { id: paint, target }],
        vec![Surface::Input {
            id: surface,
            input: port,
        }],
        vec![Occurrence::new(
            occurrence,
            paint,
            surface,
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(SurroundProfileId::AverageV1),
        )],
        ConstraintSet::new(
            vec![ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(10),
                occurrence,
                CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0; 3])),
            )],
            vec![],
        ),
        vec![OutputBinding::new(OutputSlotId::new(11), paint)],
        CoreProgramEvaluatorsV1,
    )
}

#[test]
fn fixed_and_finite_target_domains_have_distinct_identity() {
    let fixed = fixed_single_target_program().compile().unwrap();
    let finite = finite_program(false).compile().unwrap();

    assert_ne!(fixed.content_identity(), finite.content_identity());
}

#[test]
fn content_identity_retains_the_explicit_joint_state_order() {
    let forward = finite_program(false).compile().unwrap();
    let reversed = finite_program(true).compile().unwrap();

    assert_ne!(forward.content_identity(), reversed.content_identity());
}

#[test]
fn content_identity_binds_the_exact_selection_release_even_when_order_is_equal() {
    let first = finite_program(false)
        .replace_materialised_joint_selection_for_test(finite_materialised_selection(7))
        .compile()
        .unwrap();
    let second = finite_program(false)
        .replace_materialised_joint_selection_for_test(finite_materialised_selection(8))
        .compile()
        .unwrap();

    assert_ne!(first.content_identity(), second.content_identity());
}

#[test]
fn finite_candidate_opacity_is_part_of_v8_content_identity() {
    let quarter = finite_program_with_opacity(false, 0.25).compile().unwrap();
    let half = finite_program_with_opacity(false, 0.5).compile().unwrap();

    assert_ne!(quarter.content_identity(), half.content_identity());
}

#[derive(Clone, Copy)]
enum RegularIncidence {
    OneCycle,
    TwoCycles,
}

fn regular_incidence_program(kind: RegularIncidence) -> CoreProgramV1 {
    let source = SourceId::new(1);
    let target = TargetId::new(2);
    let paints = [10, 11, 12, 13].map(PaintId::new);
    let ports = [20, 21, 22, 23].map(SurfaceInputPortId::new);
    let surfaces = [30, 31, 32, 33].map(SurfaceId::new);
    let incidence = match kind {
        RegularIncidence::OneCycle => [
            (0, 0),
            (0, 1),
            (1, 1),
            (1, 2),
            (2, 2),
            (2, 3),
            (3, 3),
            (3, 0),
        ],
        RegularIncidence::TwoCycles => [
            (0, 0),
            (0, 1),
            (1, 0),
            (1, 1),
            (2, 2),
            (2, 3),
            (3, 2),
            (3, 3),
        ],
    };
    let occurrences = incidence
        .iter()
        .enumerate()
        .map(|(index, (paint, surface))| {
            Occurrence::new(
                OccurrenceId::new(40 + index as u32),
                paints[*paint],
                surfaces[*surface],
                CompositionProfile::EncodedSrgb8SourceOverV1,
                context(SurroundProfileId::AverageV1),
            )
        })
        .collect::<Vec<_>>();
    let constraints = occurrences
        .iter()
        .enumerate()
        .map(|(index, occurrence)| {
            ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(60 + index as u32),
                occurrence.id(),
                CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new([0x20; 3])),
            )
        })
        .collect();

    Program::new(
        vec![Source::new(source, signal([0x20; 3]))],
        vec![Target::fixed(target, source)],
        ObservationGroup::new(ObservationGroupId::new(3), ports.to_vec()),
        vec![],
        paints
            .iter()
            .copied()
            .map(|id| Paint::Solid { id, target })
            .collect(),
        surfaces
            .iter()
            .copied()
            .zip(ports)
            .map(|(id, input)| Surface::Input { id, input })
            .collect(),
        occurrences,
        ConstraintSet::new(constraints, vec![]),
        paints
            .iter()
            .copied()
            .enumerate()
            .map(|(index, paint)| OutputBinding::new(OutputSlotId::new(80 + index as u32), paint))
            .collect(),
        CoreProgramEvaluatorsV1,
    )
}

#[test]
fn exact_canon_distinguishes_regular_non_isomorphic_programs() {
    let one_cycle = regular_incidence_program(RegularIncidence::OneCycle)
        .compile()
        .unwrap();
    let two_cycles = regular_incidence_program(RegularIncidence::TwoCycles)
        .compile()
        .unwrap();

    assert_ne!(one_cycle.content_identity(), two_cycles.content_identity());
}
