use super::tests::{compile_assignment, program, update};
use super::*;
use crate::color_intent::IdentityOptionsV1;

fn color(id: u32, occurrence: u32, gray: u8, preference: u64) -> Target {
    let anchor = Srgb8::new([gray; 3]);
    let mut value = ComputedTargetV1::color(
        anchor,
        2.0,
        ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(anchor, false, IdentityOptionsV1::default())
                .expect("gray fixture anchor should define a valid source identity"),
            appearance: Some(OccurrenceId::new(occurrence)),
            tone: None,
        },
    );
    value.preference = preference;
    value.reference_appearance = true;
    Target::new(TargetId::new(id), TargetIntentV1::Computed(value))
}

fn hierarchy() -> Program<Wcag22Srgb8V1> {
    let mut p = program(
        color(10, 5, 68, 0),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.reference_inputs = Some(vec![(SurfaceInputPortId::new(2), Srgb8::new([255; 3]))]);
    p.targets.push(color(20, 15, 102, 1));
    p.paints.push(Paint::Solid {
        id: PaintId::new(13),
        target: TargetId::new(20),
    });
    p.occurrences.push(Occurrence::new(
        OccurrenceId::new(15),
        PaintId::new(13),
        SurfaceId::new(4),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    ));
    p.outputs
        .push(OutputBinding::new(OutputSlotId::new(17), PaintId::new(13)));
    p.constraints
        .hard
        .push(ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(16),
            OccurrenceId::new(15),
            Wcag22CriterionV1::Sc143TextDefault,
        ));
    p.constraints
        .hard
        .push(ConstraintInvocation::preserve_tonal_hierarchy(
            ConstraintId::new(30),
            vec![OccurrenceId::new(5), OccurrenceId::new(15)],
        ));
    p
}

#[test]
fn hierarchy_then_unary_regions_allocate_the_complete_result_once() {
    let mut p = singleton(Srgb8::new([16; 3]), 255, 1.0);
    for index in 1..8 {
        let offset = index as u32 * 10;
        p.targets.push(color(
            10 + offset,
            5 + offset,
            16 + index * 16,
            index as u64,
        ));
        p.paints.push(Paint::Solid {
            id: PaintId::new(3 + offset),
            target: TargetId::new(10 + offset),
        });
        p.occurrences.push(Occurrence::new(
            OccurrenceId::new(5 + offset),
            PaintId::new(3 + offset),
            SurfaceId::new(4),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            p.occurrences[0].context,
        ));
    }
    p.constraints
        .hard
        .push(ConstraintInvocation::preserve_tonal_hierarchy(
            ConstraintId::new(30),
            p.occurrences
                .iter()
                .map(|occurrence| occurrence.id)
                .collect(),
        ));
    p.constraints
        .hard
        .push(ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(31),
            OccurrenceId::new(5),
            Wcag22CriterionV1::Sc143TextDefault,
        ));
    for (id, criterion) in [
        (32, Wcag22CriterionV1::Sc143TextLargeScale),
        (33, Wcag22CriterionV1::Sc143TextDefault),
    ] {
        p.constraints
            .hard
            .push(ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(id),
                OccurrenceId::new(15),
                criterion,
            ));
    }
    let owner = compile_assignment(p, EXECUTION_NODE_LIMIT_V1)
        .expect("ordered hierarchy followed by a unary constraint should compile");
    let (result, allocations) = crate::test_support::measured_allocator_events(|| {
        region_constraints(&owner.owner_generation, None)
    });
    let regions = result.expect("finite region constraints should allocate");
    assert_eq!(regions.len(), 10);
    assert_eq!(allocations.alloc, 1);
    assert_eq!(allocations.realloc, 0);
    let weaker_slot = owner
        .owner_generation
        .graph
        .bind_occurrence(OccurrenceId::new(15))
        .expect("compiled hierarchy occurrence");
    for region in regions {
        if let RegionConstraintV1::TonalHierarchy {
            weaker,
            weaker_contrast,
            ..
        } = region
        {
            assert_eq!(
                weaker_contrast,
                (weaker == weaker_slot).then_some(Wcag22CriterionV1::Sc143TextDefault)
            );
        }
    }
}

#[test]
fn gray_reference_hierarchy_adapts_and_recovers_authored_composition() {
    let owner = compile_assignment(hierarchy(), EXECUTION_NODE_LIMIT_V1)
        .expect("authored gray reference hierarchy should compile");
    let CompiledTargetSelectionV1::Computed(selection) = &owner.owner_generation.target_selection
    else {
        panic!()
    };
    assert_eq!(selection.components.len(), 1);
    assert_eq!(selection.total_budget, EXECUTION_NODE_LIMIT_V1);
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("compiled reference fixture should instantiate observation stream 8");
    for (revision, backdrop) in [(1, 255), (2, 0), (3, 255)] {
        session
            .prepare_update(update(revision, backdrop))
            .expect("well-formed fixture observation should be admitted")
            .commit();
        let verified = session
            .state()
            .current_verified()
            .expect("useful hierarchy solution")
            .computed_for_test();
        let values = verified
            .arena
            .storage
            .outputs
            .iter()
            .map(|output| output.paint.source().bytes()[0])
            .collect::<Vec<_>>();
        if backdrop == 255 {
            assert_eq!(values, [68, 102]);
        } else {
            let contrast = crate::color_intent::reference_transport::contrast_ratio;
            assert!(contrast(Srgb8::new([values[0]; 3]), Srgb8::new([0; 3])) > 9.0);
            assert_eq!(verified.optimality(), ComputedOptimalityV1::Exact);
            assert!(
                verified.search.visited < 4096,
                "gray hierarchy should prune: {:?}",
                verified.search
            );
        }
        let evidence = verified
            .arena
            .storage
            .relation_members
            .iter()
            .find_map(|member| match member {
                ProgramRelationMemberEvidenceV1::TonalHierarchy {
                    measurement: CoreRelationMeasurementV1::TonalHierarchy(value),
                    ..
                } => Some(*value),
                _ => None,
            })
            .expect("verified hierarchy fixture should include tonal-hierarchy evidence");
        assert!(evidence.satisfied());
        let contrast = crate::color_intent::reference_transport::contrast_ratio;
        assert_eq!(
            evidence.required_ratio(),
            contrast(Srgb8::new([68; 3]), Srgb8::new([255; 3]))
                / contrast(Srgb8::new([102; 3]), Srgb8::new([255; 3]))
        );
    }
}

fn compile_reference_composition(
    mut p: Program<Wcag22Srgb8V1>,
    reverse: bool,
) -> CompiledProgram<Wcag22Srgb8V1> {
    if reverse {
        p.targets.reverse();
        p.paints.reverse();
        p.surfaces.reverse();
        p.occurrences.reverse();
        p.constraints.hard.reverse();
    }
    p.assignment = Some(ComputedSelectionPolicyV1::ReferenceComposition(
        p.targets.iter().map(|target| target.id).collect(),
    ));
    p.computed_budget = Some(EXECUTION_NODE_LIMIT_V1);
    p.compile().expect("reference composition compiles")
}

#[test]
fn reference_peers_prune_and_restore_the_authored_hierarchy() {
    for reverse in [false, true] {
        let owner = compile_reference_composition(hierarchy(), reverse);
        let CompiledTargetSelectionV1::Computed(selection) =
            &owner.owner_generation.target_selection
        else {
            panic!("computed reference composition")
        };
        assert_eq!(selection.components.len(), 1);
        assert_eq!(
            selection.components[0].mode,
            ProgramSelectionModeV1::JointReferencePeersV1
        );
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        for (revision, background, expected) in [(1, 0, [177, 134]), (2, 255, [68, 102])] {
            session
                .prepare_update(update(revision, background))
                .expect("observation")
                .commit();
            let frame = session
                .state()
                .current_verified()
                .expect("readable hierarchy")
                .computed_for_test();
            let sources: Vec<_> = frame
                .arena
                .storage
                .outputs
                .iter()
                .map(|output| output.paint.source())
                .collect();
            assert_eq!(sources, expected.map(|gray| Srgb8::new([gray; 3])));
            // This program has one physical observation. The search may
            // assess two endpoints per product region, exactly once.
            assert!(frame.search.endpoint_evaluations <= 2 * frame.search.regions);
            assert_eq!(
                frame.optimality(),
                if background == 255 {
                    ComputedOptimalityV1::Exact
                } else {
                    ComputedOptimalityV1::Unfinished
                },
                "{:?}",
                frame.search
            );
            assert_eq!(
                frame.search.completion,
                if background == 255 {
                    CompletionV1::AuthoredExact
                } else {
                    CompletionV1::Complete
                }
            );
            assert!(frame.search.visited < 4096, "{:?}", frame.search);
        }
    }
}

#[test]
fn reference_peers_preserve_choice_when_a_redundant_material_joins() {
    let gray = |value| Srgb8::new([value; 3]);
    let distance = |a, b| squared_distance(lab(gray(a)), lab(gray(b)));
    let minimum = 0.20_f64;
    let expected = (0..=255_u8)
        .flat_map(|a| (0..=255_u8).map(move |b| [a, b]))
        .filter(|&[a, b]| distance(a, b).sqrt() >= minimum)
        .min_by(|&[a, b], &[c, d]| {
            let profile = |a, b| {
                let losses = [distance(a, 124), distance(b, 132)];
                [losses[0].max(losses[1]), losses[0].min(losses[1])]
            };
            let a = profile(a, b);
            let b = profile(c, d);
            a[0].total_cmp(&b[0]).then_with(|| a[1].total_cmp(&b[1]))
        })
        .expect("finite neutral pair");
    assert_eq!(expected, [99, 159]);
    let separation = |id, a, b, minimum| {
        ConstraintInvocation::pair_separation(
            ConstraintId::new(id),
            DirectedRelationV1::try_new(OccurrenceId::new(a), vec![OccurrenceId::new(b)])
                .expect("distinct uses"),
            VisibleSeparationBoundsV1::new(minimum, 2.0).expect("valid separation"),
        )
    };
    for reverse in [false, true] {
        for material in [None, Some(false), Some(true)] {
            for connected in [false, true] {
                let mut p = hierarchy();
                p.targets = vec![color(10, 5, 124, 0), color(20, 15, 132, 1)];
                p.constraints.hard = vec![separation(30, 5, 15, minimum)];
                if let Some(appearance) = material {
                    let mut extra = if appearance {
                        color(40, 45, 0, 2)
                    } else {
                        super::tests::target(40, 0, 0, 5, 2)
                    };
                    let TargetIntentV1::Computed(value) = &mut extra.intent else {
                        unreachable!()
                    };
                    value.reference_appearance = false;
                    value.domain = AssignmentDomainV1::Gray { lower: 0, upper: 5 };
                    p.targets.push(extra);
                    p.paints.push(Paint::Solid {
                        id: PaintId::new(43),
                        target: TargetId::new(40),
                    });
                    // A distinct physical input also exercises observed-order
                    // projection and report refresh with a partial report set.
                    p.observation_group
                        .surface_input_ports
                        .push(SurfaceInputPortId::new(12));
                    p.reference_inputs
                        .as_mut()
                        .expect("reference")
                        .push((SurfaceInputPortId::new(12), gray(250)));
                    p.surfaces.push(Surface::Input {
                        id: SurfaceId::new(44),
                        input: SurfaceInputPortId::new(12),
                    });
                    p.occurrences.push(Occurrence::new(
                        OccurrenceId::new(45),
                        PaintId::new(43),
                        SurfaceId::new(44),
                        CompositionProfile::EncodedSrgb8SourceOverV1,
                        p.occurrences[0].context,
                    ));
                    p.outputs
                        .push(OutputBinding::new(OutputSlotId::new(47), PaintId::new(43)));
                    if connected {
                        p.constraints.hard.push(separation(50, 5, 45, 0.05));
                    }
                } else if connected {
                    continue;
                }
                let owner = compile_reference_composition(p, reverse);
                let mut session = owner
                    .instantiate(ObservationStreamId::new(8))
                    .expect("session");
                for revision in 1..=2 {
                    let mut observed = gray_cases(revision, &[255, 255]);
                    if material.is_some() {
                        use crate::observation::{ObservationPayloadInput, SurfaceInputBinding};
                        let ObservationPayloadInput::Scenarios(cases) = &mut observed.payload
                        else {
                            unreachable!()
                        };
                        for (case, input) in cases.scenarios.iter_mut().zip([250, 210]) {
                            case.bindings.push(SurfaceInputBinding::new(
                                SurfaceInputPortId::new(12),
                                ColorSignal::from_srgb8(gray(input)),
                            ));
                        }
                        if revision == 2 {
                            cases.scenarios.reverse();
                        }
                    }
                    session
                        .prepare_update(observed)
                        .expect("observation")
                        .commit();
                    let frame = session
                        .state()
                        .current_verified()
                        .expect("feasible composition")
                        .computed_for_test();
                    let sources = &frame.arena.storage.outputs;
                    assert_eq!(
                        [sources[0].paint.source(), sources[1].paint.source()],
                        expected.map(gray),
                        "reverse={reverse}, material={material:?}, connected={connected}"
                    );
                    if material.is_some() {
                        assert_eq!(sources[2].paint.source(), gray(0));
                    }
                    let reports = frame.reference_selections().expect("reference reports");
                    assert_eq!(reports.len(), 2);
                    for report in reports.iter() {
                        let expected = if report.target_id() == 10 {
                            expected[0]
                        } else {
                            assert_eq!(report.target_id(), 20);
                            expected[1]
                        };
                        assert!(
                            report
                                .cases()
                                .iter()
                                .all(|case| case.actual_visible() == gray(expected))
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn explicit_point_tone_priority_survives_a_redundant_reference_constraint() {
    let gray = |value| Srgb8::new([value; 3]);
    let separation = |id, a, b, minimum| {
        ConstraintInvocation::pair_separation(
            ConstraintId::new(id),
            DirectedRelationV1::try_new(OccurrenceId::new(a), vec![OccurrenceId::new(b)])
                .expect("distinct uses"),
            VisibleSeparationBoundsV1::new(minimum, 2.0).expect("valid separation"),
        )
    };
    let expected = (0..=255_u8)
        .filter(|&b| squared_distance(lab(gray(124)), lab(gray(b))).sqrt() >= 0.20)
        .min_by(|&a, &b| {
            squared_distance(lab(gray(a)), lab(gray(132)))
                .total_cmp(&squared_distance(lab(gray(b)), lab(gray(132))))
        })
        .expect("feasible second use at the requested first tone");
    for reverse in [false, true] {
        for connected in [false, true] {
            let mut p = hierarchy();
            p.targets = vec![color(10, 5, 124, 0), color(20, 15, 132, 1)];
            for target in &mut p.targets {
                let TargetIntentV1::Computed(value) = &mut target.intent else {
                    unreachable!()
                };
                value.reference_appearance = false;
            }
            let TargetIntentV1::Computed(first) = &mut p.targets[0].intent else {
                unreachable!()
            };
            let tone = lab(gray(124))[0];
            first.intent.as_mut().expect("color use").tone =
                Some(ToneCurveV1::new(2, vec![[0.0, tone], [1.0, tone]]).expect("constant tone"));
            p.constraints.hard = vec![separation(30, 5, 15, 0.20)];
            let mut extra = color(40, 45, 0, 2);
            let TargetIntentV1::Computed(value) = &mut extra.intent else {
                unreachable!()
            };
            value.domain = AssignmentDomainV1::Gray { lower: 0, upper: 5 };
            p.targets.push(extra);
            p.paints.push(Paint::Solid {
                id: PaintId::new(43),
                target: TargetId::new(40),
            });
            p.occurrences.push(Occurrence::new(
                OccurrenceId::new(45),
                PaintId::new(43),
                SurfaceId::new(4),
                CompositionProfile::EncodedSrgb8SourceOverV1,
                p.occurrences[0].context,
            ));
            p.outputs
                .push(OutputBinding::new(OutputSlotId::new(47), PaintId::new(43)));
            if connected {
                p.constraints.hard.push(separation(50, 5, 45, 0.05));
            }
            let owner = compile_reference_composition(p, reverse);
            let mut session = owner
                .instantiate(ObservationStreamId::new(8))
                .expect("session");
            session
                .prepare_update(update(1, 255))
                .expect("observation")
                .commit();
            let frame = session
                .state()
                .current_verified()
                .expect("feasible composition")
                .computed_for_test();
            let outputs = &frame.arena.storage.outputs;
            assert_eq!(
                [
                    outputs[0].paint.source(),
                    outputs[1].paint.source(),
                    outputs[2].paint.source()
                ],
                [gray(124), gray(expected), gray(0)],
                "reverse={reverse}, connected={connected}"
            );
        }
    }
}

#[test]
fn reference_peers_tone_selects_the_nearest_feasible_surface_across_polarities() {
    // Enumerate the physical neutral feasibility envelope independently of the
    // search. A surface in the middle cannot host both levels of text at the
    // required ratio and minimum contrast, even though each may fit alone.
    let luma = |gray: u8| {
        let s = f64::from(gray) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    let cr = |a: u8, b: u8| (luma(a.max(b)) + 0.05) / (luma(a.min(b)) + 0.05);
    let ratio = cr(23, 255) / cr(96, 255);
    let feasible = |background: u8| {
        [false, true].into_iter().any(|lighter| {
            let mut candidates = (0..=255_u8)
                .filter(|&gray| {
                    (if lighter {
                        gray > background
                    } else {
                        gray < background
                    }) && cr(gray, background) >= 4.5
                })
                .map(|gray| cr(gray, background));
            let Some(first) = candidates.next() else {
                return false;
            };
            let (low, high) = candidates.fold((first, first), |(low, high), contrast| {
                (low.min(contrast), high.max(contrast))
            });
            high / low >= ratio
        })
    };
    let nearest = |tone: f64| {
        (0..=255_u8)
            .filter(|&gray| feasible(gray))
            .min_by(|&a, &b| {
                (luma(a).cbrt() - tone)
                    .abs()
                    .total_cmp(&(luma(b).cbrt() - tone).abs())
            })
            .expect("nonempty feasibility envelope")
    };
    for reverse in [false, true] {
        let mut p = hierarchy();
        p.targets = vec![color(10, 5, 23, 0), color(20, 15, 96, 1)];
        for (index, (id, paint, occurrence, surface, anchor, dark, light, parent)) in [
            (40, 43, 45, 44, 242, 0.23, 0.96, 4),
            (50, 53, 55, 54, 255, 0.31, 1.0, 44),
        ]
        .into_iter()
        .enumerate()
        {
            let mut target = color(id, occurrence, anchor, 2 + index as u64);
            let TargetIntentV1::Computed(value) = &mut target.intent else {
                unreachable!()
            };
            value.intent.as_mut().expect("computed color intent").tone = Some(
                ToneCurveV1::new(2, vec![[0.0, dark], [1.0, light]])
                    .expect("finite authored tone curve"),
            );
            p.targets.push(target);
            p.paints.push(Paint::Solid {
                id: PaintId::new(paint),
                target: TargetId::new(id),
            });
            p.occurrences.push(Occurrence::new(
                OccurrenceId::new(occurrence),
                PaintId::new(paint),
                SurfaceId::new(parent),
                CompositionProfile::EncodedSrgb8SourceOverV1,
                p.occurrences[0].context,
            ));
            p.surfaces.push(Surface::FromOccurrence {
                id: SurfaceId::new(surface),
                occurrence: OccurrenceId::new(occurrence),
            });
            p.outputs.push(OutputBinding::new(
                OutputSlotId::new(id + 7),
                PaintId::new(paint),
            ));
            let members = [60 + 20 * index as u32, 70 + 20 * index as u32];
            for (member, gray) in members.into_iter().zip([23, 96]) {
                let occurrence = OccurrenceId::new(member + 5);
                let paint = PaintId::new(member + 3);
                p.targets
                    .push(color(member, member + 5, gray, u64::from(member)));
                p.paints.push(Paint::Solid {
                    id: paint,
                    target: TargetId::new(member),
                });
                p.occurrences.push(Occurrence::new(
                    occurrence,
                    paint,
                    SurfaceId::new(surface),
                    CompositionProfile::EncodedSrgb8SourceOverV1,
                    p.occurrences[0].context,
                ));
                p.outputs
                    .push(OutputBinding::new(OutputSlotId::new(member + 7), paint));
                p.constraints
                    .hard
                    .push(ConstraintInvocation::visible_unary_hard(
                        ConstraintId::new(member + 6),
                        occurrence,
                        Wcag22CriterionV1::Sc143TextDefault,
                    ));
            }
            p.constraints
                .hard
                .push(ConstraintInvocation::preserve_tonal_hierarchy(
                    ConstraintId::new(100 + index as u32),
                    members.map(|id| OccurrenceId::new(id + 5)).to_vec(),
                ));
        }
        for target in &mut p.targets {
            let TargetIntentV1::Computed(value) = &mut target.intent else {
                unreachable!()
            };
            value.domain = AssignmentDomainV1::Gray {
                lower: 0,
                upper: 255,
            };
        }
        let owner = compile_reference_composition(p, reverse);
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        for (revision, canvas) in [(1, 0), (2, 22), (3, 217), (4, 255), (5, 22)] {
            session
                .prepare_update(update(revision, canvas))
                .expect("observation")
                .commit();
            let frame = session
                .state()
                .current_verified()
                .expect("readable composition")
                .computed_for_test();
            let values = &frame.arena.storage.outputs;
            let surfaces = [
                values[2].paint.source().bytes()[0],
                values[3].paint.source().bytes()[0],
            ];
            let tone = luma(canvas).cbrt();
            let expected = [
                nearest(0.23 + tone * (0.96 - 0.23)),
                nearest(0.31 + tone * (1.0 - 0.31)),
            ];
            assert_eq!(surfaces, expected, "canvas={canvas}, {:?}", frame.search);
        }
    }
}

#[test]
fn reference_peers_nested_materials_match_exhaustive_tuple_profiles() {
    use crate::color_intent::reference_transport::{
        ReferenceTransportV1, contrast_ratio, polarity,
    };
    use crate::constraints::TonalPolarityV1;

    let gray = |value| Srgb8::new([value; 3]);
    let composite = |source: Srgb8, alpha, backdrop: Srgb8| {
        Srgb8::new(
            crate::composition::source_over_srgb8(source.bytes(), alpha, backdrop.bytes())
                .expect("material"),
        )
    };
    // Both gray domains cross the tree's finite-enumeration threshold. The
    // chromatic boxes exercise independent channel bounds and encoded rounding.
    let gray_domains = [
        AssignmentDomainV1::Gray {
            lower: 36,
            upper: 111,
        },
        AssignmentDomainV1::Gray {
            lower: 100,
            upper: 175,
        },
    ];
    let cube_domains = [
        AssignmentDomainV1::Cube {
            lower: [64, 74, 84],
            upper: [68, 78, 88],
        },
        AssignmentDomainV1::Cube {
            lower: [124, 134, 144],
            upper: [128, 138, 148],
        },
    ];
    for (domains, anchors, alphas, backdrops) in [
        (
            gray_domains,
            [gray(68), gray(128)],
            [1.0, 0.5],
            &[0, 20][..],
        ),
        (
            gray_domains,
            [gray(68), gray(128)],
            [0.25, 0.75],
            &[128, 160, 200],
        ),
        (gray_domains, [gray(68), gray(128)], [0.5, 1.0], &[220, 255]),
        (
            gray_domains,
            [gray(68), gray(128)],
            [0.0, 1.0],
            &[0, 128, 255],
        ),
        (
            gray_domains,
            [gray(68), gray(128)],
            [1.0, 0.0],
            &[0, 128, 255],
        ),
        (
            gray_domains,
            [gray(68), gray(128)],
            [0.0, 0.0],
            &[0, 128, 255],
        ),
        (
            gray_domains,
            [gray(68), gray(128)],
            [1.0, 1.0],
            &[0, 128, 255],
        ),
        (
            gray_domains,
            [gray(68), gray(128)],
            [0.001, 0.999],
            &[0, 63, 128, 200, 255],
        ),
        (
            cube_domains,
            [Srgb8::new([66, 76, 86]), Srgb8::new([126, 136, 146])],
            [0.55, 0.35],
            &[0, 127, 240],
        ),
    ] {
        let reference_parent = composite(anchors[1], alphas[1], gray(255));
        let models = [
            ReferenceTransportV1::new(
                composite(anchors[0], alphas[0], reference_parent),
                reference_parent,
            ),
            ReferenceTransportV1::new(reference_parent, gray(255)),
        ];
        // Independent complete finite oracle. It uses point composition and
        // transport only, without region bounds, pruning, seeds or search order.
        let profile = |tuple: [Srgb8; 2]| {
            let mut tiers = [[0.0_f64; 2]; 3];
            for &backdrop in backdrops {
                let parent = composite(tuple[1], alphas[1], gray(backdrop));
                let backgrounds = [parent, gray(backdrop)];
                for target in 0..2 {
                    let visible = composite(tuple[target], alphas[target], backgrounds[target]);
                    let direction = polarity(visible, backgrounds[target]);
                    let (lo, hi) = domains[target].bounds();
                    let ceiling = match direction {
                        TonalPolarityV1::Darker => contrast_ratio(
                            composite(Srgb8::new(lo), alphas[target], backgrounds[target]),
                            backgrounds[target],
                        ),
                        TonalPolarityV1::Lighter => contrast_ratio(
                            composite(Srgb8::new(hi), alphas[target], backgrounds[target]),
                            backgrounds[target],
                        ),
                        TonalPolarityV1::Equal => 1.0,
                    };
                    tiers[0][target] = tiers[0][target]
                        .max(1.0 - (ceiling / models[target].reference_contrast()).min(1.0));
                    tiers[1][target] = tiers[1][target].max(squared_distance(
                        lab(visible),
                        models[target].lab_for(backgrounds[target], direction),
                    ));
                }
            }
            for target in 0..2 {
                tiers[2][target] = squared_distance(lab(tuple[target]), lab(anchors[target]));
            }
            for tier in &mut tiers {
                tier.sort_unstable_by(|a, b| b.total_cmp(a));
            }
            tiers
        };
        let compare = |left: &[[f64; 2]; 3], right: &[[f64; 2]; 3]| {
            left.iter()
                .flatten()
                .zip(right.iter().flatten())
                .map(|(a, b)| a.total_cmp(b))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        };
        let identities = anchors.map(|anchor| {
            ColorIdentityDomainV1::new(anchor, false, IdentityOptionsV1::default())
                .expect("source identity")
        });
        let expected = (0..domains[0].cardinality().expect("finite domain"))
            .flat_map(|left| {
                (0..domains[1].cardinality().expect("finite domain"))
                    .map(move |right| [domains[0].value(left), domains[1].value(right)])
            })
            .filter(|tuple| (0..2).all(|target| identities[target].contains(tuple[target])))
            .map(profile)
            .min_by(compare)
            .expect("nonempty tuples");
        let mut previous = None;
        for reverse in [false, true] {
            let mut p = hierarchy();
            p.constraints = ConstraintSet::new(vec![], vec![]);
            for target in 0..2 {
                let mut value = ComputedTargetV1::color(
                    anchors[target],
                    2.0,
                    ColorUseIntentV1 {
                        identity: identities[target],
                        appearance: Some(OccurrenceId::new(5 + target as u32 * 10)),
                        tone: None,
                    },
                );
                value.domain = domains[target];
                value.reference_appearance = true;
                p.targets[target].intent = TargetIntentV1::Computed(value);
                p.opacities.push(OpacityInput::new(
                    OpacityInputId::new(50 + target as u32),
                    alphas[target],
                ));
                p.paints.push(Paint::Opacity {
                    id: PaintId::new(60 + target as u32),
                    source: PaintId::new(3 + target as u32 * 10),
                    opacity: OpacityInputId::new(50 + target as u32),
                });
                p.occurrences[target].subject = PaintId::new(60 + target as u32);
                p.outputs[target] = OutputBinding::new(
                    OutputSlotId::new(7 + target as u32 * 10),
                    PaintId::new(60 + target as u32),
                );
            }
            p.surfaces.push(Surface::FromOccurrence {
                id: SurfaceId::new(44),
                occurrence: OccurrenceId::new(15),
            });
            p.occurrences[0].against = SurfaceId::new(44);
            let owner = compile_reference_composition(p, reverse);
            let CompiledTargetSelectionV1::Computed(selection) =
                &owner.owner_generation.target_selection
            else {
                panic!("computed reference composition")
            };
            // Removing a physical dependency changes the decomposition, not
            // the objective of any remaining reference use.
            assert!(
                selection.components.iter().all(
                    |component| component.mode == ProgramSelectionModeV1::JointReferencePeersV1
                )
            );
            let mut session = owner
                .instantiate(ObservationStreamId::new(8))
                .expect("session");
            let mut observed = backdrops.to_vec();
            if reverse {
                observed.reverse();
            }
            session
                .prepare_update(gray_cases(1, &observed))
                .expect("cases")
                .commit();
            let frame = session
                .state()
                .current_verified()
                .expect("finite reference result")
                .computed_for_test();
            let tuple = std::array::from_fn(|i| frame.arena.storage.outputs[i].paint.source());
            assert_eq!(
                profile(tuple),
                expected,
                "domains={domains:?}, alpha={alphas:?}, cases={backdrops:?}, tuple={tuple:?}"
            );
            // Exhausting RGB8 proves the finite oracle, while the symmetric
            // policy's continuous minimum remains unproven.
            assert_eq!(
                frame.optimality(),
                ComputedOptimalityV1::Unfinished,
                "{:?}",
                frame.search
            );
            assert_ne!(frame.search.completion, CompletionV1::BudgetExhausted);
            if let Some(previous) = previous {
                assert_eq!(tuple, previous, "declaration and case permutations");
            }
            previous = Some(tuple);
        }
    }
}

#[test]
fn reference_alpha_is_composited_once_and_bounds_max_change_in_visible_space() {
    let mut p = hierarchy();
    p.constraints = ConstraintSet::new(vec![], vec![]);
    p.opacities
        .push(OpacityInput::new(OpacityInputId::new(50), 0.5));
    p.paints.push(Paint::Opacity {
        id: PaintId::new(51),
        source: PaintId::new(3),
        opacity: OpacityInputId::new(50),
    });
    p.occurrences[0].subject = PaintId::new(51);
    p.outputs[0] = OutputBinding::new(OutputSlotId::new(7), PaintId::new(51));
    if let TargetIntentV1::Computed(value) = &mut p.targets[0].intent {
        value.max_delta_bits = 0_f64.to_bits();
    }
    let owner = compile_assignment(p, 4096).expect("half-opacity reference fixture should compile");
    let CompiledTargetSelectionV1::Computed(selection) = &owner.owner_generation.target_selection
    else {
        panic!()
    };
    assert_eq!(selection.targets[0].declaration.anchor, Srgb8::new([68; 3]));
    assert_eq!(
        selection.targets[0].source_anchor_lab,
        lab(Srgb8::new([68; 3]))
    );
    assert_eq!(
        selection.targets[0].effective_anchor(),
        Srgb8::new([162; 3])
    );
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("compiled reference fixture should instantiate observation stream 8");
    session
        .prepare_update(update(1, 255))
        .expect("well-formed fixture observation should be admitted")
        .commit();
    let verified = session
        .state()
        .current_verified()
        .expect("reference alpha must preserve authored source")
        .computed_for_test();
    assert_eq!(
        verified.arena.storage.outputs[0].paint.source(),
        Srgb8::new([68; 3])
    );
    assert_eq!(verified.optimality(), ComputedOptimalityV1::Exact);
}

#[test]
fn gray_source_identity_remains_neutral_while_reference_appearance_is_tinted() {
    use crate::observation::{ObservationPayloadInput, SurfaceInputBinding};
    let make = |force_neutral| {
        let mut p = hierarchy();
        p.constraints = ConstraintSet::new(vec![], vec![]);
        p.reference_inputs
            .as_mut()
            .expect("hierarchy fixture should contain authored reference bindings")[0]
            .1 = Srgb8::new([0, 0, 255]);
        p.opacities
            .push(OpacityInput::new(OpacityInputId::new(50), 0.5));
        p.paints.push(Paint::Opacity {
            id: PaintId::new(51),
            source: PaintId::new(3),
            opacity: OpacityInputId::new(50),
        });
        p.occurrences[0].subject = PaintId::new(51);
        p.outputs[0] = OutputBinding::new(OutputSlotId::new(7), PaintId::new(51));
        if let TargetIntentV1::Computed(value) = &mut p.targets[0].intent {
            value.max_delta_bits = 0_f64.to_bits();
            value
                .intent
                .as_mut()
                .expect("color fixture should carry source-identity intent")
                .identity = ColorIdentityDomainV1::new(
                value.anchor,
                force_neutral,
                IdentityOptionsV1::default(),
            )
            .expect("neutral source fixture should define a valid identity domain");
        }
        compile_assignment(p, 4096)
            .expect("tinted reference fixture with neutral source intent should compile")
    };
    let owner = make(false);
    let forced = make(true);
    assert_ne!(owner.content_identity(), forced.content_identity());
    let CompiledTargetSelectionV1::Computed(selection) = &owner.owner_generation.target_selection
    else {
        panic!()
    };
    assert_eq!(
        selection.targets[0].effective_anchor(),
        Srgb8::new([34, 34, 162])
    );
    assert!(
        selection.targets[0]
            .effective_intent()
            .expect("compiled color fixture should retain source-identity intent")
            .identity
            .neutral()
    );
    assert!(!selection.targets[0].opaque);
    assert_eq!(
        selection.targets[0].search_domain(),
        AssignmentDomainV1::Gray {
            lower: 0,
            upper: 255,
        }
    );
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("compiled reference fixture should instantiate observation stream 8");
    let mut observed = update(1, 255);
    let ObservationPayloadInput::Scenarios(scenarios) = &mut observed.payload else {
        panic!()
    };
    scenarios.scenarios[0].bindings[0] = SurfaceInputBinding::new(
        SurfaceInputPortId::new(2),
        ColorSignal::from_srgb8(Srgb8::new([0, 0, 255])),
    );
    session
        .prepare_update(observed)
        .expect("well-formed fixture observation should be admitted")
        .commit();
    assert_eq!(
        session
            .state()
            .current_verified()
            .expect("unchanged tinted reference should yield a verified authored source")
            .computed_for_test()
            .arena
            .storage
            .outputs[0]
            .paint
            .source(),
        Srgb8::new([68; 3])
    );
}

#[test]
fn orange_source_identity_survives_a_neutral_reference_appearance() {
    use crate::observation::{ObservationPayloadInput, SurfaceInputBinding};
    let anchor = Srgb8::new([255, 128, 0]);
    let identity = ColorIdentityDomainV1::new(anchor, false, IdentityOptionsV1::default())
        .expect("orange fixture anchor should define a valid identity domain");
    let mut value = ComputedTargetV1::color(
        anchor,
        2.0,
        ColorUseIntentV1 {
            identity,
            appearance: Some(OccurrenceId::new(5)),
            tone: None,
        },
    );
    value.reference_appearance = true;
    let mut p = program(
        Target::new(TargetId::new(10), TargetIntentV1::Computed(value)),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.constraints = ConstraintSet::new(vec![], vec![]);
    p.reference_inputs = Some(vec![(
        SurfaceInputPortId::new(2),
        Srgb8::new([0, 127, 255]),
    )]);
    p.opacities
        .push(OpacityInput::new(OpacityInputId::new(50), 0.5));
    p.paints.push(Paint::Opacity {
        id: PaintId::new(51),
        source: PaintId::new(3),
        opacity: OpacityInputId::new(50),
    });
    p.occurrences[0].subject = PaintId::new(51);
    p.outputs[0] = OutputBinding::new(OutputSlotId::new(7), PaintId::new(51));
    let owner = compile_assignment(p, 4096)
        .expect("orange source on a complementary reference backdrop should compile");
    let CompiledTargetSelectionV1::Computed(selection) = &owner.owner_generation.target_selection
    else {
        panic!()
    };
    assert_eq!(
        selection.targets[0].effective_anchor(),
        Srgb8::new([128; 3])
    );
    assert_eq!(
        selection.targets[0]
            .effective_intent()
            .expect("compiled color fixture should retain source-identity intent")
            .identity,
        identity
    );
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("compiled reference fixture should instantiate observation stream 8");
    for (revision, backdrop) in [(1, [0, 127, 255]), (2, [255; 3]), (3, [0, 127, 255])] {
        let mut observed = update(revision, 255);
        let ObservationPayloadInput::Scenarios(scenarios) = &mut observed.payload else {
            panic!()
        };
        scenarios.scenarios[0].bindings[0] = SurfaceInputBinding::new(
            SurfaceInputPortId::new(2),
            ColorSignal::from_srgb8(Srgb8::new(backdrop)),
        );
        session
            .prepare_update(observed)
            .expect("well-formed fixture observation should be admitted")
            .commit();
        let verified = session
            .state()
            .current_verified()
            .expect("orange material fixture should yield a verified color on each tested backdrop")
            .computed_for_test();
        let source = verified.arena.storage.outputs[0].paint.source();
        assert!(
            identity.contains(source),
            "source material must remain orange: {source:?}"
        );
        if backdrop == [0, 127, 255] {
            assert_eq!(source, anchor);
            assert_eq!(verified.optimality(), ComputedOptimalityV1::Exact);
        } else {
            assert!(
                !source.is_achromatic(),
                "neutral appearance is not a neutral source"
            );
        }
    }
}

#[test]
fn invalid_reference_and_order_are_compile_errors() {
    for kind in 0..5 {
        let mut p = hierarchy();
        match kind {
            0 => p.reference_inputs = None,
            1 => p.reference_inputs = Some(vec![]),
            2 => p
                .reference_inputs
                .as_mut()
                .expect("hierarchy fixture should contain authored reference bindings")
                .push((SurfaceInputPortId::new(2), Srgb8::new([255; 3]))),
            3 => {
                p.constraints
                    .hard
                    .last_mut()
                    .expect("hierarchy fixture should end with a tonal-hierarchy constraint")
                    .body = ProgramConstraintBodyV1::ComputedVisibleRelation {
                    occurrences: vec![OccurrenceId::new(15), OccurrenceId::new(5)],
                    invocation: CoreComputedVisibleRelationInvocationV1::PreserveTonalHierarchy,
                }
            }
            _ => {
                p.constraints
                    .hard
                    .last_mut()
                    .expect("hierarchy fixture should end with a tonal-hierarchy constraint")
                    .body = ProgramConstraintBodyV1::ComputedVisibleRelation {
                    occurrences: vec![OccurrenceId::new(5), OccurrenceId::new(5)],
                    invocation: CoreComputedVisibleRelationInvocationV1::PreserveTonalHierarchy,
                }
            }
        }
        assert!(matches!(
            compile_assignment(p, 1024),
            Err(ProgramCompileError::InvalidComputedAssignment(_))
        ));
    }
}

#[test]
fn reference_map_and_relation_order_are_semantic_but_declaration_order_is_not() {
    let owner = compile_assignment(hierarchy(), 1024)
        .expect("authored gray reference hierarchy should compile");
    let mut p = hierarchy();
    p.targets.reverse();
    p.paints.reverse();
    p.occurrences.reverse();
    p.outputs.reverse();
    p.constraints.hard.reverse();
    let permuted =
        compile_assignment(p, 1024).expect("permuted hierarchy declarations should compile");
    assert_eq!(owner.content_identity(), permuted.content_identity());
    let mut p = hierarchy();
    p.reference_inputs
        .as_mut()
        .expect("hierarchy fixture should contain authored reference bindings")[0]
        .1 = Srgb8::new([240; 3]);
    let changed = compile_assignment(p, 1024)
        .expect("hierarchy with an altered authored reference should compile");
    assert_ne!(owner.content_identity(), changed.content_identity());
}

#[test]
fn contextual_reference_identity_has_a_cross_platform_golden() {
    let mut p = hierarchy();
    let TargetIntentV1::Computed(target) = &mut p.targets[0].intent else {
        unreachable!("hierarchy fixture declares computed targets")
    };
    target
        .intent
        .as_mut()
        .expect("reference target carries color intent")
        .tone = Some(
        ToneCurveV1::new(2, vec![[0.0, 0.8], [1.0, 0.2]])
            .expect("finite descending tone curve is valid"),
    );
    let (_, roles) = crate::program_session::program_identity_graph_schema_for_test(&p)
        .expect("reference hierarchy has valid identity graph bindings");
    // The legacy V9 golden covers roles 1..=37. Pin the added tone-input,
    // appearance, ordered-member and member-occurrence bindings here.
    assert_eq!(
        roles
            .into_iter()
            .filter(|role| *role >= 38)
            .collect::<Vec<_>>(),
        [38, 39, 40, 41]
    );
    let owner =
        compile_assignment(p, 1024).expect("reference hierarchy with explicit tone compiles");
    assert_eq!(
        owner.content_identity().as_bytes(),
        &[
            63, 176, 24, 203, 195, 74, 182, 156, 7, 147, 120, 218, 129, 28, 27, 214, 243, 161, 114,
            34, 128, 184, 113, 126, 82, 232, 217, 103, 26, 57, 115, 234,
        ]
    );
}

#[test]
fn opaque_endpoints_keep_the_computed_backdrop_in_the_relation_component() {
    let mut p = hierarchy();
    p.targets.push(color(40, 45, 255, 2));
    p.paints.push(Paint::Solid {
        id: PaintId::new(43),
        target: TargetId::new(40),
    });
    p.occurrences.push(Occurrence::new(
        OccurrenceId::new(45),
        PaintId::new(43),
        SurfaceId::new(4),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    ));
    p.surfaces.push(Surface::FromOccurrence {
        id: SurfaceId::new(44),
        occurrence: OccurrenceId::new(45),
    });
    p.occurrences[0].against = SurfaceId::new(44);
    p.occurrences[1].against = SurfaceId::new(44);
    p.outputs
        .push(OutputBinding::new(OutputSlotId::new(47), PaintId::new(43)));
    let owner = compile_assignment(p, 1024)
        .expect("hierarchy with a computed shared backdrop should compile");
    let CompiledTargetSelectionV1::Computed(selection) = &owner.owner_generation.target_selection
    else {
        panic!()
    };
    assert_eq!(selection.components.len(), 1);
    assert_eq!(selection.components[0].targets.len(), 3);
}

fn singleton(anchor: Srgb8, reference: u8, alpha: f64) -> Program<Wcag22Srgb8V1> {
    let mut target = color(10, 5, anchor.bytes()[0], 0);
    let TargetIntentV1::Computed(value) = &mut target.intent else {
        unreachable!()
    };
    value.anchor = anchor;
    value
        .intent
        .as_mut()
        .expect("color fixture should carry source-identity intent")
        .identity = ColorIdentityDomainV1::new(anchor, false, IdentityOptionsV1::default())
        .expect("singleton anchor should define a valid source identity");
    let mut p = program(target, Wcag22CriterionV1::Sc143TextDefault, Wcag22Srgb8V1);
    p.constraints = ConstraintSet::new(vec![], vec![]);
    p.reference_inputs = Some(vec![(
        SurfaceInputPortId::new(2),
        Srgb8::new([reference; 3]),
    )]);
    if alpha != 1.0 {
        p.opacities
            .push(OpacityInput::new(OpacityInputId::new(50), alpha));
        p.paints.push(Paint::Opacity {
            id: PaintId::new(51),
            source: PaintId::new(3),
            opacity: OpacityInputId::new(50),
        });
        p.occurrences[0].subject = PaintId::new(51);
        p.outputs[0] = OutputBinding::new(OutputSlotId::new(7), PaintId::new(51));
    }
    p
}

#[test]
fn reference_adapts_a_translucent_backdrop_with_an_adaptive_foreground() {
    use crate::color_intent::reference_transport::contrast_ratio;
    use crate::observation::{ObservationPayloadInput, SurfaceInputBinding};

    let mut p = singleton(Srgb8::new([13, 110, 253]), 255, 0.55);
    p.targets.push(color(20, 15, 255, 1));
    p.paints.push(Paint::Solid {
        id: PaintId::new(13),
        target: TargetId::new(20),
    });
    p.surfaces.push(Surface::FromOccurrence {
        id: SurfaceId::new(14),
        occurrence: OccurrenceId::new(5),
    });
    p.occurrences.push(Occurrence::new(
        OccurrenceId::new(15),
        PaintId::new(13),
        SurfaceId::new(14),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    ));
    p.outputs
        .push(OutputBinding::new(OutputSlotId::new(17), PaintId::new(13)));
    for (id, occurrence, criterion) in [
        (6, 5, Wcag22CriterionV1::Sc1411UiComponentOrState),
        (16, 15, Wcag22CriterionV1::Sc143TextDefault),
    ] {
        p.constraints
            .hard
            .push(ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(id),
                OccurrenceId::new(occurrence),
                criterion,
            ));
    }
    let owner =
        compile_assignment(p, EXECUTION_NODE_LIMIT_V1).expect("reference composition compiles");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    let mut incoming = update(1, 255);
    let ObservationPayloadInput::Scenarios(set) = &mut incoming.payload else {
        unreachable!()
    };
    let paper = Srgb8::new([217, 195, 166]);
    set.scenarios[0].bindings[0] =
        SurfaceInputBinding::new(SurfaceInputPortId::new(2), ColorSignal::from_srgb8(paper));
    session
        .prepare_update(incoming)
        .expect("surface accepted")
        .commit();
    let frame = session
        .state()
        .current_verified()
        .expect("verified paints")
        .computed_for_test();
    let fill = frame.arena.storage.outputs[0].paint.source();
    let ink = frame.arena.storage.outputs[1].paint.source();
    let visible = Srgb8::new(
        crate::composition::source_over_srgb8(fill.bytes(), 0.55, paper.bytes())
            .expect("finite material opacity"),
    );
    assert_ne!(fill.bytes(), [13, 110, 253]);
    assert!(contrast_ratio(visible, paper) >= 3.0);
    assert!(contrast_ratio(ink, visible) >= 4.5);
    assert_eq!(frame.optimality(), ComputedOptimalityV1::Unfinished);
}

fn gray_cases(revision: u64, backdrops: &[u8]) -> crate::observation::ObservationUpdateInput {
    use crate::observation::{
        ObservationPayloadInput, ScenarioId, ScenarioInput, SurfaceInputBinding,
    };
    let mut observed = update(revision, backdrops[0]);
    let ObservationPayloadInput::Scenarios(cases) = &mut observed.payload else {
        unreachable!()
    };
    cases.scenarios = backdrops
        .iter()
        .enumerate()
        .map(|(index, &value)| ScenarioInput {
            id: ScenarioId::new(100 + index as u32),
            bindings: vec![SurfaceInputBinding::new(
                SurfaceInputPortId::new(2),
                ColorSignal::from_srgb8(Srgb8::new([value; 3])),
            )],
        })
        .collect();
    observed
}

// Independent finite enumeration of the selection contract. The established
// physical compositor and qualified transport supply points; this oracle has
// no search tree, region pruning, memo lookup or asynchronous capacity jobs.
fn gray_oracle(anchor: u8, reference: u8, alpha: f64, backdrops: &[u8]) -> (u8, f64) {
    use crate::color_intent::reference_transport::{
        ReferenceTransportV1, contrast_ratio, polarity,
    };
    let compose = |source, backdrop| {
        Srgb8::new([crate::composition::source_over_channel_srgb8(source, alpha, backdrop); 3])
    };
    let model = ReferenceTransportV1::new(compose(anchor, reference), Srgb8::new([reference; 3]));
    let signature = |source| {
        backdrops
            .iter()
            .map(|&backdrop| {
                if model.reference_contrast() == 1.0 {
                    crate::constraints::TonalPolarityV1::Equal
                } else {
                    polarity(compose(source, backdrop), Srgb8::new([backdrop; 3]))
                }
            })
            .collect::<Vec<_>>()
    };
    let ratio = |source| {
        backdrops.iter().fold(1.0_f64, |result, &backdrop| {
            result.min(
                contrast_ratio(compose(source, backdrop), Srgb8::new([backdrop; 3]))
                    / model.reference_contrast(),
            )
        })
    };
    let mut best: Option<(u8, [f64; 4], f64)> = None;
    for source in 0..=255 {
        let signs = signature(source);
        let capacity = (0..=255)
            .filter(|&other| signature(other) == signs)
            .fold(0.0_f64, |best, other| best.max(ratio(other)));
        let mut key = [1.0 - capacity, 0.0, 0.0, 0.0];
        for (&backdrop, &direction) in backdrops.iter().zip(&signs) {
            let preferred = model.lab_for(Srgb8::new([backdrop; 3]), direction);
            key[1] = key[1].max(squared_distance(preferred, model.reference_lab()));
            key[2] = key[2].max(f64::from(
                model.reference_contrast() != 1.0 && direction != model.reference_polarity(),
            ));
            key[3] = key[3].max(squared_distance(lab(compose(source, backdrop)), preferred));
        }
        let less = best.is_none_or(|(previous, old, _)| {
            key.iter()
                .zip(old)
                .map(|(a, b)| a.total_cmp(&b))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or_else(|| {
                    (source != anchor)
                        .cmp(&(previous != anchor))
                        .then_with(|| source.cmp(&previous))
                })
                == Ordering::Less
        });
        if less {
            best = Some((source, key, capacity));
        }
    }
    let (source, _, capacity) =
        best.expect("exhaustive nonempty gray domain should contain a selected source");
    (source, capacity)
}

#[test]
fn reference_gray_selection_matches_exhaustive_joint_capacity_oracle() {
    for (anchor, reference, alpha, backdrops) in [
        (16, 255, 1.0, vec![0]),
        (248, 0, 1.0, vec![255]),
        (224, 0, 0.322, vec![79]),
        (100, 255, 1.0, vec![0, 90, 180]),
        (170, 0, 0.5, vec![50, 120, 200]),
        (128, 128, 1.0, vec![0, 128, 255]),
    ] {
        let expected = gray_oracle(anchor, reference, alpha, &backdrops);
        let owner = compile_assignment(singleton(Srgb8::new([anchor; 3]), reference, alpha), 4096)
            .expect("single-color gray oracle fixture should compile");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("compiled reference fixture should instantiate observation stream 8");
        for (revision, cases) in [
            (1, backdrops.clone()),
            (
                2,
                backdrops
                    .iter()
                    .rev()
                    .chain(backdrops.iter())
                    .copied()
                    .collect(),
            ),
        ] {
            session
                .prepare_update(gray_cases(revision, &cases))
                .expect("well-formed fixture observation should be admitted")
                .commit();
            let verified = session
                .state()
                .current_verified()
                .expect("exhaustive gray-domain fixture should yield a verified color")
                .computed_for_test();
            assert_eq!(
                verified.values[0].bytes()[0],
                expected.0,
                "anchor={anchor}, reference={reference}, alpha={alpha}, cases={cases:?}"
            );
            assert_eq!(
                verified.optimality(),
                ComputedOptimalityV1::Exact,
                "{:?}",
                verified.search
            );
            let capacity = verified
                .reference_selections()
                .expect("verified reference-color fixture should expose selection evidence")[0]
                .capacity()
                .expect("implicit reference-strength selection should include capacity bounds");
            assert!(capacity.complete());
            assert_eq!(capacity.ratio_lower(), expected.1);
            assert_eq!(capacity.ratio_upper(), expected.1);
        }
    }
}

#[test]
fn reference_real_neutral_tints_keep_strength_without_a_readability_floor() {
    use crate::color_intent::reference_transport::contrast_ratio;
    for (anchor, reference, backdrop) in [
        ([16, 16, 18], 255, 0),
        ([246, 248, 250], 0, 255),
        ([254, 254, 255], 0, 255),
        ([32, 32, 33], 255, 0),
    ] {
        let owner = compile_assignment(
            singleton(Srgb8::new(anchor), reference, 1.0),
            EXECUTION_NODE_LIMIT_V1,
        )
        .expect("real near-neutral reference fixture should compile");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("compiled reference fixture should instantiate observation stream 8");
        session
            .prepare_update(update(1, backdrop))
            .expect("well-formed fixture observation should be admitted")
            .commit();
        let verified = session
            .state()
            .current_verified()
            .expect("near-neutral reference fixture should yield a verified color")
            .computed_for_test();
        let value = verified.values[0];
        let reference_cr = contrast_ratio(Srgb8::new(anchor), Srgb8::new([reference; 3]));
        let actual_cr = contrast_ratio(value, Srgb8::new([backdrop; 3]));
        assert!(
            actual_cr / reference_cr > 0.95,
            "{anchor:?} -> {value:?}, CR={actual_cr}/{reference_cr}, {:?}",
            verified.search
        );
        assert!(verified.search.visited <= EXECUTION_NODE_LIMIT_V1);
    }
}

#[test]
fn explicit_tone_reports_override_without_implicit_capacity() {
    let mut p = singleton(Srgb8::new([16; 3]), 255, 1.0);
    let TargetIntentV1::Computed(target) = &mut p.targets[0].intent else {
        unreachable!()
    };
    target
        .intent
        .as_mut()
        .expect("color fixture should carry source-identity intent")
        .tone = Some(
        ToneCurveV1::new(2, vec![[0.0, 0.5], [1.0, 0.5]])
            .expect("constant midpoint tone fixture should form a valid curve"),
    );
    let owner = compile_assignment(p, 4096).expect("explicit midpoint tone fixture should compile");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("compiled reference fixture should instantiate observation stream 8");
    session
        .prepare_update(update(1, 0))
        .expect("well-formed fixture observation should be admitted")
        .commit();
    let verified = session
        .state()
        .current_verified()
        .expect("explicit midpoint tone fixture should yield a verified color")
        .computed_for_test();
    assert!((lab(verified.values[0])[0] - 0.5).abs() < 0.003);
    let report = &verified
        .reference_selections()
        .expect("verified reference-color fixture should expose selection evidence")[0];
    assert_eq!(
        report.preference(),
        crate::program_wire::ProgramReferencePreferenceV1::ExplicitTone
    );
    assert_eq!(report.capacity(), None);
    assert_eq!(verified.optimality(), ComputedOptimalityV1::Exact);
}

#[test]
fn explicit_reference_tone_retains_lightness_below_rgb8_resolution() {
    let lightness = 0.001_f64;
    let mut p = singleton(Srgb8::new([0; 3]), 255, 1.0);
    let TargetIntentV1::Computed(target) = &mut p.targets[0].intent else {
        unreachable!("singleton fixture declares a computed target")
    };
    target.max_delta_bits = 0.0_f64.to_bits();
    target
        .intent
        .as_mut()
        .expect("color fixture has intent")
        .tone = Some(
        ToneCurveV1::new(2, vec![[0.0, lightness], [1.0, lightness]])
            .expect("constant finite tone is valid"),
    );
    let owner = compile_assignment(p, 4096).expect("black reference fixture compiles");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("reference fixture instantiates");
    session
        .prepare_update(update(1, 255))
        .expect("well-formed authored observation is admitted")
        .commit();
    let verified = session
        .state()
        .current_verified()
        .expect("zero maxChange preserves authored black")
        .computed_for_test();
    assert_eq!(&*verified.values, &[Srgb8::new([0; 3])]);
    let reports = verified
        .reference_selections()
        .expect("reference evidence exists");
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].capacity(), None);
    assert_eq!(
        reports[0].cases()[0].virtual_distance_squared(),
        lightness * lightness
    );
    assert_eq!(verified.optimality(), ComputedOptimalityV1::Exact);
}

#[test]
fn reference_capacity_cannot_borrow_a_different_more_important_paint() {
    use crate::observation::{ObservationPayloadInput, SurfaceInputBinding};
    let mut p = hierarchy();
    p.targets = vec![color(10, 5, 160, 0), color(20, 15, 149, 1)];
    p.observation_group = ObservationGroup::new(
        ObservationGroupId::new(1),
        vec![SurfaceInputPortId::new(2), SurfaceInputPortId::new(22)],
    );
    p.reference_inputs
        .as_mut()
        .expect("hierarchy fixture should contain authored reference bindings")
        .push((SurfaceInputPortId::new(22), Srgb8::new([255; 3])));
    p.surfaces.push(Surface::Input {
        id: SurfaceId::new(24),
        input: SurfaceInputPortId::new(22),
    });
    p.occurrences[0].against = SurfaceId::new(24);
    // The same B physical paint is also inspected on fixed white. The peer
    // relation limits B under the selected A; it is not a separate use/value.
    p.occurrences.push(Occurrence::new(
        OccurrenceId::new(25),
        PaintId::new(13),
        SurfaceId::new(24),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    ));
    p.constraints = ConstraintSet::new(
        vec![ConstraintInvocation::preserve_tonal_hierarchy(
            ConstraintId::new(30),
            vec![OccurrenceId::new(25), OccurrenceId::new(5)],
        )],
        vec![],
    );
    let owner = compile_assignment(p, EXECUTION_NODE_LIMIT_V1)
        .expect("shared-paint hierarchy fixture should compile");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("compiled reference fixture should instantiate observation stream 8");
    let mut observed = update(1, 90);
    let ObservationPayloadInput::Scenarios(cases) = &mut observed.payload else {
        unreachable!()
    };
    cases.scenarios[0].bindings.push(SurfaceInputBinding::new(
        SurfaceInputPortId::new(22),
        ColorSignal::from_srgb8(Srgb8::new([255; 3])),
    ));
    session
        .prepare_update(observed)
        .expect("well-formed fixture observation should be admitted")
        .commit();
    let verified = session
        .state()
        .current_verified()
        .expect("shared-paint hierarchy fixture should yield a verified assignment")
        .computed_for_test();
    assert_eq!(
        &*verified.values,
        &[Srgb8::new([160; 3]), Srgb8::new([3; 3])],
        "{:?}",
        verified.search
    );
    assert_eq!(
        verified.optimality(),
        ComputedOptimalityV1::Exact,
        "{:?}",
        verified.search
    );
    let capacity = verified
        .reference_selections()
        .expect("verified reference-color fixture should expose selection evidence")[1]
        .capacity()
        .expect("implicit reference-strength selection should include capacity bounds");
    assert!(capacity.complete());
    assert_eq!(capacity.ratio_lower(), 1.0);
}

#[test]
fn reference_budget_keeps_unresolved_capacity_honest() {
    let owner = compile_assignment(singleton(Srgb8::new([100; 3]), 255, 1.0), 1)
        .expect("one-attempt reference fixture should compile");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("compiled reference fixture should instantiate observation stream 8");
    session
        .prepare_update(update(1, 90))
        .expect("well-formed fixture observation should be admitted")
        .commit();
    let verified = session
        .state()
        .current_verified()
        .expect("one-attempt fixture should retain a feasible verified assignment")
        .computed_for_test();
    assert_eq!(verified.optimality(), ComputedOptimalityV1::Unfinished);
    assert_eq!(verified.search.visited, 1);
    let capacity = verified
        .reference_selections()
        .expect("verified reference-color fixture should expose selection evidence")[0]
        .capacity()
        .expect("implicit reference-strength selection should include capacity bounds");
    assert!(!capacity.complete());
    assert!(capacity.ratio_lower() < capacity.ratio_upper());
    assert_eq!(capacity.ratio_upper(), 1.0);
}

#[test]
fn reference_search_spends_last_visit_on_another_open_traversal() {
    // In each fixture the next scheduled region needs two visits, while the
    // other traversal has a complete tuple ready for the one remaining visit.
    for (anchor, reference, alpha, backdrops, budget) in [
        (100, 255, 1.0, vec![0, 90, 180], 47),
        (170, 0, 0.5, vec![50, 120, 200], 46),
    ] {
        let owner =
            compile_assignment(singleton(Srgb8::new([anchor; 3]), reference, alpha), budget)
                .expect("bounded reference declaration should compile");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("compiled reference fixture should instantiate observation stream 8");
        session
            .prepare_update(gray_cases(1, &backdrops))
            .expect("well-formed fixture observation should be admitted")
            .commit();
        let verified = session
            .state()
            .current_verified()
            .expect("bounded reference search should retain a feasible assignment")
            .computed_for_test();
        assert_eq!(verified.search.visited, budget);
        assert_eq!(verified.optimality(), ComputedOptimalityV1::Unfinished);
    }
}

#[test]
fn reference_variable_backdrop_matches_complete_tuple_enumeration() {
    use crate::color_intent::reference_transport::{
        ReferenceTransportV1, contrast_ratio, polarity,
    };
    let gray = |v| Srgb8::new([v; 3]);
    let half = |source, backdrop| (u16::from(source) + u16::from(backdrop)).div_ceil(2) as u8;
    let backdrops = [0_u8, 20];
    let anchors = [68_u8, 128];
    let models = [
        ReferenceTransportV1::new(gray(68), gray(half(128, 255))),
        ReferenceTransportV1::new(gray(half(128, 255)), gray(255)),
    ];
    let tuples = (60..=75)
        .flat_map(|foreground| (128..=143).map(move |background| [foreground, background]))
        .collect::<Vec<_>>();
    let appearance = |tuple: [u8; 2], target: usize, case: usize| {
        let parent = half(tuple[1], backdrops[case]);
        if target == 0 {
            (gray(tuple[0]), gray(parent))
        } else {
            (gray(parent), gray(backdrops[case]))
        }
    };
    let context = |tuple, target| {
        (0..backdrops.len())
            .map(|case| {
                let (visible, backdrop) = appearance(tuple, target, case);
                (backdrop, polarity(visible, backdrop))
            })
            .collect::<Vec<_>>()
    };
    let ratio = |tuple, target: usize| {
        (0..backdrops.len()).fold(1.0_f64, |result, case| {
            let (visible, backdrop) = appearance(tuple, target, case);
            result.min(contrast_ratio(visible, backdrop) / models[target].reference_contrast())
        })
    };
    let key = |tuple: [u8; 2]| {
        std::array::from_fn::<_, 2, _>(|target| {
            let environment = context(tuple, target);
            let capacity = tuples
                .iter()
                .copied()
                .filter(|other| {
                    other[..target] == tuple[..target] && context(*other, target) == environment
                })
                .fold(0.0_f64, |result, other| result.max(ratio(other, target)));
            let mut key = [1.0 - capacity, 0.0, 0.0, 0.0];
            for case in 0..backdrops.len() {
                let (visible, backdrop) = appearance(tuple, target, case);
                let direction = polarity(visible, backdrop);
                let preferred = models[target].lab_for(backdrop, direction);
                key[1] = key[1].max(squared_distance(preferred, models[target].reference_lab()));
                key[2] = key[2].max(f64::from(direction != models[target].reference_polarity()));
                key[3] = key[3].max(squared_distance(lab(visible), preferred));
            }
            key
        })
    };
    let expected = *tuples
        .iter()
        .min_by(|&&left, &&right| {
            key(left)
                .into_iter()
                .flatten()
                .zip(key(right).into_iter().flatten())
                .map(|(a, b)| a.total_cmp(&b))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or_else(|| {
                    left.iter()
                        .zip(anchors)
                        .map(|(v, anchor)| *v != anchor)
                        .cmp(right.iter().zip(anchors).map(|(v, anchor)| *v != anchor))
                        .then_with(|| left.cmp(&right))
                })
        })
        .expect("nonempty enumerated foreground and backdrop domain should have a minimum");
    let expected_key = key(expected);
    let mut p = hierarchy();
    p.constraints = ConstraintSet::new(vec![], vec![]);
    p.targets[1] = color(20, 15, 128, 1);
    for (target, (lower, upper)) in p.targets.iter_mut().zip([(60, 75), (128, 143)]) {
        let TargetIntentV1::Computed(value) = &mut target.intent else {
            unreachable!()
        };
        value.domain = AssignmentDomainV1::Gray { lower, upper };
    }
    p.opacities
        .push(OpacityInput::new(OpacityInputId::new(50), 0.5));
    p.paints.push(Paint::Opacity {
        id: PaintId::new(51),
        source: PaintId::new(13),
        opacity: OpacityInputId::new(50),
    });
    p.occurrences[1].subject = PaintId::new(51);
    p.outputs[1] = OutputBinding::new(OutputSlotId::new(17), PaintId::new(51));
    p.surfaces.push(Surface::FromOccurrence {
        id: SurfaceId::new(44),
        occurrence: OccurrenceId::new(15),
    });
    p.occurrences[0].against = SurfaceId::new(44);
    let owner = compile_assignment(p, EXECUTION_NODE_LIMIT_V1)
        .expect("finite foreground and backdrop composition fixture should compile");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("compiled reference fixture should instantiate observation stream 8");
    session
        .prepare_update(gray_cases(1, &backdrops))
        .expect("well-formed fixture observation should be admitted")
        .commit();
    let verified = session
        .state()
        .current_verified()
        .expect("enumerated foreground and backdrop fixture should yield a verified assignment")
        .computed_for_test();
    assert_eq!(&*verified.values, &expected.map(gray));
    assert_eq!(
        verified.optimality(),
        ComputedOptimalityV1::Exact,
        "{:?}",
        verified.search
    );
    let reports = verified
        .reference_selections()
        .expect("verified reference-color fixture should expose selection evidence");
    assert_eq!(reports.len(), expected_key.len());
    for ((report, key), target_id) in reports.iter().zip(expected_key).zip([10, 20]) {
        assert_eq!(report.target_id(), target_id);
        let capacity = report
            .capacity()
            .expect("implicit reference-strength selection should include capacity bounds");
        assert!(capacity.complete());
        assert!((capacity.ratio_lower() - (1.0 - key[0])).abs() < 2.0 * f64::EPSILON);
        assert_eq!(capacity.ratio_lower(), capacity.ratio_upper());
    }
}

#[test]
fn reference_chain_prunes_unavoidable_strength_loss() {
    for count in [1, 2, 4, 8] {
        let mut p = singleton(Srgb8::new([17; 3]), 255, 1.0);
        for index in 1..count {
            let offset = index as u32 * 10;
            p.targets.push(color(
                10 + offset,
                5 + offset,
                if index % 2 == 0 { 17 } else { 221 },
                index as u64,
            ));
            p.paints.push(Paint::Solid {
                id: PaintId::new(3 + offset),
                target: TargetId::new(10 + offset),
            });
            p.surfaces.push(Surface::FromOccurrence {
                id: SurfaceId::new(4 + offset),
                occurrence: OccurrenceId::new(5 + offset - 10),
            });
            p.occurrences.push(Occurrence::new(
                OccurrenceId::new(5 + offset),
                PaintId::new(3 + offset),
                SurfaceId::new(4 + offset),
                CompositionProfile::EncodedSrgb8SourceOverV1,
                p.occurrences[0].context,
            ));
            p.outputs.push(OutputBinding::new(
                OutputSlotId::new(7 + offset),
                PaintId::new(3 + offset),
            ));
        }
        let owner = compile_assignment(p, EXECUTION_NODE_LIMIT_V1)
            .expect("authored reference chain should compile");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("compiled reference fixture should instantiate observation stream 8");
        session
            .prepare_update(update(1, 136))
            .expect("well-formed fixture observation should be admitted")
            .commit();
        let verified = session
            .state()
            .current_verified()
            .expect("reference chain should yield a verified assignment under the test budget")
            .computed_for_test();
        assert_eq!(
            verified.optimality(),
            ComputedOptimalityV1::Exact,
            "chain n={count}: {:?}",
            verified.search
        );
        assert!(
            verified.search.visited < 4096 * count as u64,
            "chain n={count}: {:?}",
            verified.search
        );
    }
}

#[test]
fn chromatic_reference_materials_preserve_readable_hierarchy() {
    use crate::observation::{ObservationPayloadInput, SurfaceInputBinding};
    for count in [2, 4, 8] {
        for priority in 0..3 {
            let sources = [[37, 99, 235], [217, 119, 6], [124, 58, 237], [13, 148, 136]];
            let mut p = singleton(Srgb8::new(sources[0]), 255, 1.0);
            p.reference_inputs
                .as_mut()
                .expect("chromatic fixture declares its authored backdrop")[0]
                .1 = Srgb8::new([246, 244, 240]);
            p.targets.clear();
            p.paints.clear();
            p.occurrences.clear();
            p.outputs.clear();
            let appearance_context = hierarchy().occurrences[0].context;
            let mut authored_values = Vec::new();
            for index in 0..count + 2 {
                let offset = index as u32 * 10;
                let anchor = Srgb8::new(if index < count {
                    sources[index % sources.len()]
                } else if index == count {
                    [8, 21, 40]
                } else {
                    [20, 37, 59]
                });
                let mut target = color(10 + offset, 5 + offset, 0, index as u64);
                let TargetIntentV1::Computed(value) = &mut target.intent else {
                    unreachable!()
                };
                value.anchor = anchor;
                value
                    .intent
                    .as_mut()
                    .expect("chromatic target carries source identity")
                    .identity =
                    ColorIdentityDomainV1::new(anchor, false, IdentityOptionsV1::default())
                        .expect("fixture colors admit their own source identity");
                value.preference = match priority {
                    0 => index,
                    1 => count + 1 - index,
                    _ if index < count => count + 1 - index,
                    _ => index - count,
                } as u64;
                authored_values.push((value.preference, anchor));
                p.targets.push(target);
                p.paints.push(Paint::Solid {
                    id: PaintId::new(3 + offset),
                    target: TargetId::new(10 + offset),
                });
                let mut paint = PaintId::new(3 + offset);
                if index < count {
                    p.opacities.push(OpacityInput::new(
                        OpacityInputId::new(1 + offset),
                        if index % 2 == 0 { 0.12 } else { 0.10 },
                    ));
                    p.paints.push(Paint::Opacity {
                        id: PaintId::new(8 + offset),
                        source: paint,
                        opacity: OpacityInputId::new(1 + offset),
                    });
                    paint = PaintId::new(8 + offset);
                    p.surfaces.push(Surface::FromOccurrence {
                        id: SurfaceId::new(14 + offset),
                        occurrence: OccurrenceId::new(5 + offset),
                    });
                }
                p.occurrences.push(Occurrence::new(
                    OccurrenceId::new(5 + offset),
                    paint,
                    SurfaceId::new(4 + index.min(count) as u32 * 10),
                    CompositionProfile::EncodedSrgb8SourceOverV1,
                    appearance_context,
                ));
                p.outputs
                    .push(OutputBinding::new(OutputSlotId::new(7 + offset), paint));
                if index >= count {
                    p.constraints
                        .hard
                        .push(ConstraintInvocation::visible_unary_hard(
                            ConstraintId::new(6 + offset),
                            OccurrenceId::new(5 + offset),
                            Wcag22CriterionV1::Sc143TextDefault,
                        ));
                }
            }
            p.constraints
                .hard
                .push(ConstraintInvocation::preserve_tonal_hierarchy(
                    ConstraintId::new(999),
                    vec![
                        OccurrenceId::new(5 + count as u32 * 10),
                        OccurrenceId::new(15 + count as u32 * 10),
                    ],
                ));
            let owner = compile_assignment(p, EXECUTION_NODE_LIMIT_V1)
                .expect("authored chromatic composition compiles");
            let mut session = owner
                .instantiate(ObservationStreamId::new(8))
                .expect("chromatic composition instantiates");
            authored_values.sort_unstable();
            let authored_values = authored_values
                .into_iter()
                .map(|(_, anchor)| anchor)
                .collect::<Vec<_>>();
            for (revision, backdrop) in [
                [246, 244, 240],
                [16, 24, 39],
                [100, 116, 139],
                [63, 36, 63],
                [246, 244, 240],
            ]
            .into_iter()
            .enumerate()
            {
                let mut observation = update(revision as u64 + 1, 0);
                let ObservationPayloadInput::Scenarios(cases) = &mut observation.payload else {
                    unreachable!()
                };
                cases.scenarios[0].bindings[0] = SurfaceInputBinding::new(
                    SurfaceInputPortId::new(2),
                    ColorSignal::from_srgb8(Srgb8::new(backdrop)),
                );
                session
                    .prepare_update(observation)
                    .expect("declared backdrop update is admitted")
                    .commit();
                let verified = session
                    .state()
                    .current_verified()
                    .unwrap_or_else(|| {
                        panic!("count {count}, priority {priority}, backdrop {backdrop:?}")
                    })
                    .computed_for_test();
                assert!(verified.search.visited <= EXECUTION_NODE_LIMIT_V1);
                if backdrop == [246, 244, 240] {
                    assert_eq!(verified.optimality(), ComputedOptimalityV1::Exact);
                    assert_eq!(verified.values.as_ref(), authored_values.as_slice());
                }
            }
        }
    }
}
