use super::*;
use crate::color_intent::IdentityOptionsV1;
use crate::color_intent::reference_transport::{ReferenceTransportV1, contrast_ratio, polarity};
use crate::observation::{ObservationPayloadInput, ScenarioId, ScenarioInput, SurfaceInputBinding};
use crate::program_session::computed_assignment::tests::{compile_assignment, program, update};
use std::cell::Cell;

thread_local! {
    static EAGER: Cell<bool> = const { Cell::new(false) };
    static POINT_CASES: Cell<u64> = const { Cell::new(0) };
    static BOUND_CASES: Cell<u64> = const { Cell::new(0) };
    static TONAL_CONVERSIONS: Cell<u64> = const { Cell::new(0) };
    static IDENTITY_CHECKS: Cell<u64> = const { Cell::new(0) };
    static VISIBLE_LAB_CONVERSIONS: Cell<u64> = const { Cell::new(0) };
}

pub(super) fn visible_lab_conversion() {
    VISIBLE_LAB_CONVERSIONS.set(VISIBLE_LAB_CONVERSIONS.get() + 1);
}

pub(super) fn identity_check() {
    IDENTITY_CHECKS.set(IDENTITY_CHECKS.get() + 1);
}

pub(super) fn tonal_conversion() {
    TONAL_CONVERSIONS.set(TONAL_CONVERSIONS.get() + 1);
}

pub(super) fn eager() -> bool {
    EAGER.get()
}
pub(super) fn point_case() {
    POINT_CASES.set(POINT_CASES.get() + 1);
}
pub(super) fn bound_case() {
    BOUND_CASES.set(BOUND_CASES.get() + 1);
}

fn with_eager<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            EAGER.set(self.0);
        }
    }
    let _restore = Restore(EAGER.replace(enabled));
    run()
}

fn gray(value: u8) -> Srgb8 {
    Srgb8::new([value; 3])
}

fn color(id: u32, occurrence: u32, anchor: u8, lower: u8, upper: u8, reference: bool) -> Target {
    let mut target = ComputedTargetV1::color(
        gray(anchor),
        2.0,
        ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(gray(anchor), false, IdentityOptionsV1::default())
                .expect("gray source identity"),
            appearance: Some(OccurrenceId::new(occurrence)),
            tone: None,
        },
    );
    target.domain = AssignmentDomainV1::Gray { lower, upper };
    target.preference = u64::from(id);
    target.reference_appearance = reference;
    Target::new(TargetId::new(id), TargetIntentV1::Computed(target))
}

fn observe(backdrops: &[u8]) -> crate::observation::ObservationUpdateInput {
    observe_rgb(
        &backdrops
            .iter()
            .map(|&value| gray(value))
            .collect::<Vec<_>>(),
    )
}

fn observe_rgb(backdrops: &[Srgb8]) -> crate::observation::ObservationUpdateInput {
    let mut observation = update(1, backdrops[0].bytes()[0]);
    let ObservationPayloadInput::Scenarios(cases) = &mut observation.payload else {
        unreachable!()
    };
    cases.scenarios = backdrops
        .iter()
        .enumerate()
        .map(|(index, &backdrop)| ScenarioInput {
            id: ScenarioId::new(100 + index as u32),
            bindings: vec![SurfaceInputBinding::new(
                SurfaceInputPortId::new(2),
                ColorSignal::from_srgb8(backdrop),
            )],
        })
        .collect();
    observation
}

fn red_green_support() -> Vec<Srgb8> {
    let mut support = Vec::new();
    for step in 0..=255_u8 {
        support.push(Srgb8::new([255 - step, step, 0]));
        if step != 255 {
            support.push(Srgb8::new([255 - step, step + 1, 0]));
        }
    }
    assert_eq!(support.len(), 511);
    support
}

#[test]
fn red_green_full_support_finishes_with_a_complete_infeasibility_proof() {
    let p = program(
        color(10, 5, 0, 0, 255, false),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    let owner = compile_assignment(p, EXECUTION_NODE_LIMIT_V1).expect("finite declaration");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(observe_rgb(&red_green_support()))
        .expect("complete physical support")
        .commit();
    let crate::session::SessionState::Failed {
        cause: ProgramViolationOutcomeV1::Computed(cause),
        ..
    } = session.state()
    else {
        panic!("full source domain must be proven infeasible");
    };
    assert_eq!(cause.search.completion, CompletionV1::Complete);
    assert_eq!(cause.search.open_regions, 0);
}

#[test]
#[ignore = "same-process release measurement; run explicitly with --release --ignored --nocapture"]
fn measure_red_green_511_case_refinement() {
    let support = red_green_support();
    for panel in [false, true] {
        let mut expected = None;
        for eager in [true, false] {
            let mut times = Vec::new();
            let mut counts = Vec::new();
            for iteration in 0..6 {
                let mut p = program(
                    color(10, 5, 0, 0, 255, false),
                    Wcag22CriterionV1::Sc143TextDefault,
                    Wcag22Srgb8V1,
                );
                if panel {
                    p.sources.push(Source::new(
                        SourceId::new(30),
                        ColorSignal::from_srgb8(gray(255)),
                    ));
                    p.targets
                        .push(Target::fixed(TargetId::new(31), SourceId::new(30)));
                    p.paints.push(Paint::Solid {
                        id: PaintId::new(32),
                        target: TargetId::new(31),
                    });
                    p.paints.push(Paint::Opacity {
                        id: PaintId::new(34),
                        source: PaintId::new(32),
                        opacity: OpacityInputId::new(33),
                    });
                    p.opacities
                        .push(OpacityInput::new(OpacityInputId::new(33), 0.25));
                    p.occurrences.push(Occurrence::new(
                        OccurrenceId::new(35),
                        PaintId::new(34),
                        SurfaceId::new(4),
                        CompositionProfile::EncodedSrgb8SourceOverV1,
                        p.occurrences[0].context,
                    ));
                    p.surfaces.push(Surface::FromOccurrence {
                        id: SurfaceId::new(36),
                        occurrence: OccurrenceId::new(35),
                    });
                    p.occurrences[0].against = SurfaceId::new(36);
                }
                let owner = compile_assignment(p, EXECUTION_NODE_LIMIT_V1)
                    .expect("measurement declaration");
                let mut session = owner
                    .instantiate(ObservationStreamId::new(8))
                    .expect("session");
                let observation = observe_rgb(&support);
                POINT_CASES.set(0);
                BOUND_CASES.set(0);
                let start = std::time::Instant::now();
                with_eager(eager, || {
                    session
                        .prepare_update(observation)
                        .expect("observation")
                        .commit()
                });
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                let outcome = if let Some(verified) = session.state().current_verified() {
                    let result = verified.computed_for_test();
                    Some((result.values.to_vec(), result.optimality()))
                } else {
                    let crate::session::SessionState::Failed {
                        cause: ProgramViolationOutcomeV1::Computed(cause),
                        ..
                    } = session.state()
                    else {
                        panic!("measurement must finish its domain proof");
                    };
                    assert_eq!(cause.search.completion, CompletionV1::Complete);
                    assert_eq!(cause.search.open_regions, 0);
                    None
                };
                if let Some(previous) = &expected {
                    assert_eq!(&outcome, previous);
                } else {
                    expected = Some(outcome);
                }
                if iteration != 0 {
                    times.push(elapsed);
                    counts.push((POINT_CASES.get(), BOUND_CASES.get()));
                }
            }
            times.sort_by(f64::total_cmp);
            assert!(counts.windows(2).all(|pair| pair[0] == pair[1]));
            eprintln!(
                "panel={panel} eager={eager} median_ms={} point_cases={} bound_cases={} outcome={expected:?}",
                times[2], counts[0].0, counts[0].1
            );
        }
    }
}

fn translucent(p: &mut Program<Wcag22Srgb8V1>, occurrence: usize, paint: u32, alpha: f64) {
    p.opacities
        .push(OpacityInput::new(OpacityInputId::new(50), alpha));
    p.paints.push(Paint::Opacity {
        id: PaintId::new(51),
        source: PaintId::new(paint),
        opacity: OpacityInputId::new(50),
    });
    p.occurrences[occurrence].subject = PaintId::new(51);
}

#[test]
fn canonical_working_cases_grow_without_reallocation_or_duplicate_weight() {
    let local = LocalCasesV1 {
        order: vec![4, 1, 3, 0],
        representatives: vec![0, 1, 1, 3, 4],
    };
    let mut cases = CaseRefinementV1::new(&local, 5).expect("working cases");
    assert_eq!(cases.cases, [4, 0]);
    let (_, allocations) = crate::test_support::measured_allocator_events(|| {
        cases.add(3);
        cases.add(2);
        cases.add(3);
        cases.add(1);
    });
    assert_eq!(cases.cases, local.order);
    assert!(cases.complete());
    assert_eq!(allocations.alloc, 0);
    assert_eq!(allocations.realloc, 0);
}

#[test]
fn reordering_local_inputs_preserves_joint_row_witnesses() {
    let mut p = program(
        color(10, 5, 120, 0, 255, false),
        Wcag22CriterionV1::Sc1411UiComponentOrState,
        Wcag22Srgb8V1,
    );
    let ports = [2, 8, 9].map(SurfaceInputPortId::new);
    p.observation_group.surface_input_ports = ports.to_vec();
    for (index, input) in ports.into_iter().skip(1).enumerate() {
        p.surfaces.push(Surface::Input {
            id: SurfaceId::new(20 + index as u32),
            input,
        });
    }
    let owner = compile_assignment(p, 4096).expect("three input ports");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    let mut observation = update(1, 0);
    let ObservationPayloadInput::Scenarios(cases) = &mut observation.payload else {
        unreachable!()
    };
    cases.scenarios = [[0, 255, 0], [255, 0, 0], [0, 255, 1]]
        .into_iter()
        .enumerate()
        .map(|(index, row)| ScenarioInput {
            id: ScenarioId::new(100 + index as u32),
            bindings: ports
                .into_iter()
                .zip(row)
                .map(|(port, value)| {
                    SurfaceInputBinding::new(port, ColorSignal::from_srgb8(gray(value)))
                })
                .collect(),
        })
        .collect();
    session.prepare_update(observation).expect("cases").commit();
    let observed = &session
        .state()
        .current_verified()
        .expect("verified cases")
        .computed_for_test()
        .observation;
    let scenarios = NonEmptyScenarioSetV1::from_admitted(observed).expect("admitted cases");
    assert_eq!(scenarios.len().get(), 3);
    let mut local = local_cases::<Wcag22Srgb8V1>(&[0, 1], scenarios).expect("joint support");
    let row = |case| {
        let values = scenarios.physical_values(case).expect("physical row");
        [values[0].srgb8().bytes()[0], values[1].srgb8().bytes()[0]]
    };
    assert_eq!(
        local.order.iter().copied().map(row).collect::<Vec<_>>(),
        [[0, 255], [255, 0]]
    );
    let witnesses = local.representatives.clone();
    let original_order = local.order.clone();
    for (case, &representative) in witnesses.iter().enumerate() {
        assert_eq!(row(case), row(representative));
        assert!(original_order.contains(&representative));
    }
    local
        .reorder::<Wcag22Srgb8V1>(&[1, 0], scenarios)
        .expect("same inputs in reversed order");
    assert_eq!(local.representatives, witnesses);
    assert_eq!(local.order, [original_order[1], original_order[0]]);
}

fn check_membership_pass(
    context: &ContextV1<'_>,
    epoch: &ProgramEpochV1<Wcag22Srgb8V1>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    cases: &[usize],
    expected_checks: u64,
    expected_failures: usize,
) {
    let mut bindings = epoch.binding_template.try_clone_v1().expect("bindings");
    for (target, &value) in context.targets.iter().zip(values) {
        bindings
            .overwrite_paint_input_at(target.binding, EncodedPointPaintValueV1::opaque(value))
            .expect("candidate source");
    }
    let mut workspace = epoch.graph.new_workspace().expect("physical workspace");
    let mut scores = vec![0.0; values.len()];
    let mut direct_scores = vec![0.0_f64; values.len()];
    let mut direct_extrema = vec![usize::MAX; values.len()];
    IDENTITY_CHECKS.set(0);
    let (_, allocations) = crate::test_support::measured_allocator_events(|| {
        let mut collector = IntentScoreCollectorV1::new(context, values, &mut scores);
        let mut paints = prepare_candidate_paints(
            &epoch.graph,
            None,
            &bindings,
            epoch.observation_group.schema.as_slice(),
            &mut workspace,
        )
        .expect("prepared physical inputs");
        let mut direct_failures = 0;
        let mut direct_witness = None;
        for &case in cases {
            let inputs = scenarios.physical_values(case).expect("case inputs");
            let evaluation = paints.evaluate_surface_inputs_canonical(|i| inputs[i].srgb8());
            for (index, target) in context.targets.iter().enumerate() {
                let visible = Srgb8::new(
                    evaluation
                        .occurrence_at(target.appearance.expect("appearance target"))
                        .expect("physical occurrence")
                        .visible(),
                );
                // The uncached predicate observes the actual physical case,
                // including transparent visible identity and repeated failures.
                let identity = target.effective_intent().expect("identity target").identity;
                if !identity.contains(target.identity_point(values[index], visible)) {
                    direct_failures += usize::from(context.represents_case(case));
                    direct_witness = context
                        .case_refinement
                        .borrow()
                        .first_case(direct_witness.into_iter().chain(std::iter::once(case)));
                }
                let score =
                    squared_distance(lab(visible), context.preferred[case * values.len() + index]);
                let previous = direct_extrema[index];
                if previous == usize::MAX
                    || score > direct_scores[index]
                    || score == direct_scores[index]
                        && context.case_refinement.borrow().ranks[case]
                            < context.case_refinement.borrow().ranks[previous]
                {
                    direct_extrema[index] = case;
                }
                direct_scores[index] = direct_scores[index].max(score);
            }
            assert!(collector.observe(case, &evaluation).is_ok());
        }
        assert_eq!(direct_failures, expected_failures, "discriminating fixture");
        assert_eq!(collector.violations(), direct_failures);
        assert_eq!(collector.has_violation, direct_failures != 0);
        assert_eq!(collector.violation_case, direct_witness);
        assert_eq!(&*collector.scores, &direct_scores);
        assert_eq!(&*collector.extrema, &direct_extrema);
    });
    assert_eq!(IDENTITY_CHECKS.get(), expected_checks);
    assert_eq!(allocations, crate::test_support::AllocatorEvents::default());
}

#[test]
fn identity_membership_reuses_true_and_false_without_skipping_physical_cases() {
    for reference in [false, true] {
        let mut p = program(
            color(10, 5, 128, 0, 255, reference),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        );
        p.constraints = ConstraintSet::new(vec![], vec![]);
        if reference {
            p.reference_inputs = Some(vec![(SurfaceInputPortId::new(2), gray(255))]);
        }
        let TargetIntentV1::Computed(target) = &mut p.targets[0].intent else {
            panic!("computed target");
        };
        target.intent.as_mut().expect("color intent").identity = ColorIdentityDomainV1::new(
            gray(128),
            false,
            IdentityOptionsV1 {
                lightness: Some([0.0, lab(gray(160))[0]]),
                ..IdentityOptionsV1::default()
            },
        )
        .expect("bounded lightness identity");
        translucent(&mut p, 0, 3, 0.5);
        let owner = compile_assignment(p, 4096).expect("identity program");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        session
            .prepare_update(observe(&[0, 128, 255]))
            .expect("cases")
            .commit();
        let observed = &session
            .state()
            .current_verified()
            .expect("feasible program")
            .computed_for_test()
            .observation;
        let scenarios = NonEmptyScenarioSetV1::from_admitted(observed).expect("admitted cases");
        let epoch = &owner.owner_generation;
        let CompiledTargetSelectionV1::Computed(selection) = &epoch.target_selection else {
            panic!("computed selection");
        };
        let component = &selection.components[0];
        let context = ContextV1::new::<Wcag22Srgb8V1>(selection, Some(component), scenarios)
            .expect("fixed target identity context");
        let pass = |values: &[Srgb8], cases: &[usize], work, failures| {
            check_membership_pass(&context, epoch, scenarios, values, cases, work, failures);
        };
        pass(
            &[gray(128)],
            &[0, 0, 1, 1, 2, 2],
            if reference { 1 } else { 3 },
            if reference { 0 } else { 2 },
        );
        // A fresh collector retains both a true source result and a false visible result.
        pass(&[gray(128)], &[2, 2], 0, if reference { 0 } else { 2 });
        pass(
            &[gray(128)],
            &[2, 0, 2],
            if reference { 0 } else { 2 },
            if reference { 0 } else { 2 },
        );
        pass(
            &[gray(200)],
            &[0, 1, 2],
            if reference { 1 } else { 3 },
            if reference { 3 } else { 2 },
        );
        pass(&[gray(128)], &[2], 1, if reference { 0 } else { 1 });
        let fresh = ContextV1::new::<Wcag22Srgb8V1>(selection, Some(component), scenarios)
            .expect("independent context");
        check_membership_pass(
            &fresh,
            epoch,
            scenarios,
            &[gray(128)],
            &[2],
            1,
            if reference { 0 } else { 1 },
        );
    }
}

#[test]
fn identity_membership_never_borrows_another_targets_result_for_the_same_rgb() {
    let mut p = program(
        color(10, 5, 128, 0, 255, false),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.constraints = ConstraintSet::new(vec![], vec![]);
    let mut second = color(20, 15, 200, 0, 255, false);
    let TargetIntentV1::Computed(target) = &mut second.intent else {
        panic!("computed target")
    };
    target.intent.as_mut().expect("color intent").identity = ColorIdentityDomainV1::new(
        gray(200),
        false,
        IdentityOptionsV1 {
            lightness: Some([lab(gray(160))[0], 1.0]),
            ..IdentityOptionsV1::default()
        },
    )
    .expect("different identity");
    p.targets.push(second);
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
    let owner = compile_assignment(p, 4096).expect("separate identities");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(observe(&[255]))
        .expect("case")
        .commit();
    let observed = &session
        .state()
        .current_verified()
        .expect("feasible program")
        .computed_for_test()
        .observation;
    let scenarios = NonEmptyScenarioSetV1::from_admitted(observed).expect("admitted case");
    let epoch = &owner.owner_generation;
    let CompiledTargetSelectionV1::Computed(selection) = &epoch.target_selection else {
        panic!("computed selection");
    };
    let context = ContextV1::new::<Wcag22Srgb8V1>(selection, None, scenarios)
        .expect("complete target context");
    check_membership_pass(&context, epoch, scenarios, &[gray(128); 2], &[0, 0], 2, 2);
    check_membership_pass(&context, epoch, scenarios, &[gray(128); 2], &[0, 0], 0, 2);
}

fn check_visible_lab_pass(
    context: &ContextV1<'_>,
    epoch: &ProgramEpochV1<Wcag22Srgb8V1>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    cases: &[usize],
    expected_conversions: u64,
) -> usize {
    let count = values.len();
    let mut bindings = epoch.binding_template.try_clone_v1().expect("bindings");
    for (target, &value) in context.targets.iter().zip(values) {
        bindings
            .overwrite_paint_input_at(target.binding, EncodedPointPaintValueV1::opaque(value))
            .expect("candidate source");
    }
    let mut workspace = epoch.graph.new_workspace().expect("physical workspace");
    let mut scores = vec![0.0; count];
    let mut direct_scores = vec![0.0_f64; count];
    let mut direct_extrema = vec![usize::MAX; count];
    let mut appearance = vec![AppearancePointV1::default(); scenarios.len().get() * count];
    VISIBLE_LAB_CONVERSIONS.set(0);
    let (failures, allocations) = crate::test_support::measured_allocator_events(|| {
        let mut collector = IntentScoreCollectorV1::new(context, values, &mut scores);
        collector.appearance = Some(&mut appearance);
        let mut paints = prepare_candidate_paints(
            &epoch.graph,
            None,
            &bindings,
            epoch.observation_group.schema.as_slice(),
            &mut workspace,
        )
        .expect("prepared physical inputs");
        let mut direct_failures = 0;
        let mut direct_has_violation = false;
        let mut direct_witness = None;
        for &case in cases {
            let inputs = scenarios.physical_values(case).expect("case inputs");
            let evaluation = paints.evaluate_surface_inputs_canonical(|i| inputs[i].srgb8());
            assert!(collector.observe(case, &evaluation).is_ok());
            for (index, target) in context.targets.iter().enumerate() {
                let (visible, backdrop, alpha) = if let Some(slot) = target.appearance {
                    let occurrence = evaluation.occurrence_at(slot).expect("physical occurrence");
                    (
                        Srgb8::new(occurrence.visible()),
                        Srgb8::new(occurrence.certificate().backdrop_rgb()),
                        occurrence.certificate().subject_opacity().value(),
                    )
                } else {
                    (values[index], values[index], 1.0)
                };
                let direct_lab = lab(visible);
                let point = collector.appearance.as_deref().expect("appearance report")
                    [case * count + index];
                assert_eq!(point.visible, visible);
                assert_eq!(point.backdrop, backdrop);
                assert_eq!(point.alpha.to_bits(), alpha.to_bits());
                assert_eq!(point.lab.map(f64::to_bits), direct_lab.map(f64::to_bits));
                let delta = f64::from_bits(target.declaration.max_delta_bits);
                let identity_failed = target.effective_intent().is_some_and(|intent| {
                    !intent
                        .identity
                        .contains(target.identity_point(values[index], visible))
                });
                if identity_failed
                    || squared_distance(direct_lab, target.anchor_lab) > delta * delta
                {
                    direct_has_violation = true;
                    direct_failures += usize::from(context.represents_case(case));
                    direct_witness = context
                        .case_refinement
                        .borrow()
                        .first_case(direct_witness.into_iter().chain(std::iter::once(case)));
                }
                let score = squared_distance(direct_lab, context.preferred[case * count + index]);
                let previous = direct_extrema[index];
                if context.represents_case(case)
                    && (previous == usize::MAX
                        || score > direct_scores[index]
                        || score == direct_scores[index]
                            && context.case_refinement.borrow().ranks[case]
                                < context.case_refinement.borrow().ranks[previous])
                {
                    direct_extrema[index] = case;
                }
                direct_scores[index] = direct_scores[index].max(score);
            }
        }
        assert_eq!(
            collector.violations(),
            direct_failures.max(usize::from(direct_has_violation))
        );
        assert_eq!(collector.has_violation, direct_has_violation);
        assert_eq!(collector.violation_case, direct_witness);
        for (&actual, &expected) in collector.scores.iter().zip(&direct_scores) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        assert_eq!(&*collector.extrema, &direct_extrema);
        direct_failures
    });
    assert_eq!(VISIBLE_LAB_CONVERSIONS.get(), expected_conversions);
    assert_eq!(allocations, crate::test_support::AllocatorEvents::default());
    failures
}

#[test]
fn visible_lab_reuse_preserves_exact_composites_and_every_case_decision() {
    for reference in [false, true] {
        let mut p = program(
            color(10, 5, 128, 0, 255, reference),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        );
        p.constraints = ConstraintSet::new(vec![], vec![]);
        if reference {
            p.reference_inputs = Some(vec![(SurfaceInputPortId::new(2), gray(255))]);
        }
        let TargetIntentV1::Computed(target) = &mut p.targets[0].intent else {
            panic!("computed target");
        };
        target.max_delta_bits = 0.15_f64.to_bits();
        translucent(&mut p, 0, 3, 0.5);
        let owner = compile_assignment(p, 4096).expect("visible objective program");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        session
            .prepare_update(observe(&[96, 128, 160]))
            .expect("cases")
            .commit();
        let observed = &session
            .state()
            .current_verified()
            .expect("feasible program")
            .computed_for_test()
            .observation;
        let scenarios = NonEmptyScenarioSetV1::from_admitted(observed).expect("admitted cases");
        let epoch = &owner.owner_generation;
        let CompiledTargetSelectionV1::Computed(selection) = &epoch.target_selection else {
            panic!("computed selection");
        };
        let component = &selection.components[0];
        let context = ContextV1::new::<Wcag22Srgb8V1>(selection, Some(component), scenarios)
            .expect("visible objective context");
        let pass = |values: &[Srgb8], cases: &[usize], conversions| {
            check_visible_lab_pass(&context, epoch, scenarios, values, cases, conversions)
        };
        pass(&[gray(128)], &[0, 0, 1, 1, 2, 2], 3);
        pass(&[gray(128)], &[2, 2], 0);
        pass(&[gray(128)], &[2, 0, 2], 2);
        assert_eq!(
            pass(&[gray(0)], &[0, 1, 2], 3),
            3,
            "changed source must fail max_delta"
        );
        pass(&[gray(128)], &[2], 1);
        // Equal red channels must not hide changes in either other channel.
        pass(&[Srgb8::new([128, 120, 140])], &[2], 1);
        pass(&[Srgb8::new([128, 140, 140])], &[2], 1);
        pass(&[Srgb8::new([128, 140, 142])], &[2], 1);
        let fresh = ContextV1::new::<Wcag22Srgb8V1>(selection, Some(component), scenarios)
            .expect("independent context");
        check_visible_lab_pass(&fresh, epoch, scenarios, &[gray(128)], &[2], 1);
    }
}

#[test]
fn visible_lab_hits_keep_opaque_backdrops_fresh_and_support_sources_without_appearance() {
    let mut declared = ComputedTargetV1::declared(
        AssignmentDomainV1::Gray {
            lower: 0,
            upper: 255,
        },
        gray(128),
        2.0,
    );
    declared.preference = 10;
    let mut p = program(
        Target::new(TargetId::new(10), TargetIntentV1::Computed(declared)),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.constraints = ConstraintSet::new(vec![], vec![]);
    p.targets.push(color(20, 15, 200, 0, 255, false));
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
    let owner = compile_assignment(p, 4096).expect("mixed appearance targets");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(observe(&[0, 128, 255]))
        .expect("cases")
        .commit();
    let observed = &session
        .state()
        .current_verified()
        .expect("feasible program")
        .computed_for_test()
        .observation;
    let scenarios = NonEmptyScenarioSetV1::from_admitted(observed).expect("admitted cases");
    let epoch = &owner.owner_generation;
    let CompiledTargetSelectionV1::Computed(selection) = &epoch.target_selection else {
        panic!("computed selection");
    };
    let context = ContextV1::new::<Wcag22Srgb8V1>(selection, None, scenarios)
        .expect("complete target context");
    assert!(
        context
            .targets
            .iter()
            .next()
            .expect("source target")
            .appearance
            .is_none()
    );
    check_visible_lab_pass(
        &context,
        epoch,
        scenarios,
        &[gray(64), gray(192)],
        &[0, 1, 2, 0],
        2,
    );
    check_visible_lab_pass(
        &context,
        epoch,
        scenarios,
        &[gray(64), gray(192)],
        &[2, 1, 0],
        0,
    );
    check_visible_lab_pass(
        &context,
        epoch,
        scenarios,
        &[gray(64), gray(128)],
        &[0, 1, 2],
        1,
    );
}

#[test]
fn transparent_visible_objective_matches_full_source_enumeration() {
    let backdrops = [32, 48, 64, 80, 96, 112, 128];
    let anchor = 80;
    let visible = |source, backdrop| {
        gray(crate::composition::source_over_channel_srgb8(
            source, 0.5, backdrop,
        ))
    };
    let score = |source| {
        backdrops.iter().fold(0.0_f64, |maximum, &backdrop| {
            maximum.max(squared_distance(
                lab(visible(source, backdrop)),
                lab(gray(anchor)),
            ))
        })
    };
    let expected = (64..=95)
        .min_by(|&left, &right| {
            score(left)
                .total_cmp(&score(right))
                .then_with(|| (left != anchor).cmp(&(right != anchor)))
                .then_with(|| left.cmp(&right))
        })
        .expect("nonempty finite source domain");
    for order in [
        backdrops.to_vec(),
        vec![128, 64, 32, 80, 48, 112, 96, 64, 32],
    ] {
        let mut p = program(
            color(10, 5, anchor, 64, 95, false),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        );
        p.constraints = ConstraintSet::new(vec![], vec![]);
        translucent(&mut p, 0, 3, 0.5);
        let owner = compile_assignment(p, 4096).expect("transparent contextual declaration");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        session
            .prepare_update(observe(&order))
            .expect("observation")
            .commit();
        let result = session
            .state()
            .current_verified()
            .expect("full-support result")
            .computed_for_test();
        assert_eq!(&*result.values, &[gray(expected)]);
        assert_eq!(result.optimality(), ComputedOptimalityV1::Exact);
    }
}

#[test]
fn interior_tone_extremum_changes_the_later_lexicographic_target() {
    let curve = ToneCurveV1::new(
        2,
        vec![
            [0.0, lab(gray(80))[0]],
            [lab(gray(128))[0], lab(gray(160))[0]],
            [1.0, lab(gray(80))[0]],
        ],
    )
    .expect("authored nonmonotone contextual tone");
    let visible = |source| {
        gray(crate::composition::source_over_channel_srgb8(
            source, 0.5, 64,
        ))
    };
    let score = |source, cases: &[u8]| {
        cases.iter().fold(0.0_f64, |maximum, &backdrop| {
            maximum.max(squared_distance(
                lab(visible(source)),
                lab(curve.preferred_color(gray(80), gray(backdrop))),
            ))
        })
    };
    let oracle = |cases: &[u8]| {
        (64..=192)
            .min_by(|&left, &right| {
                score(left, cases)
                    .total_cmp(&score(right, cases))
                    .then_with(|| (left != 80).cmp(&(right != 80)))
                    .then_with(|| left.cmp(&right))
            })
            .expect("finite later-target domain")
    };
    let endpoints = [0, 255];
    let full = [0, 128, 255];
    let endpoint_source = oracle(&endpoints);
    let full_source = oracle(&full);
    assert_ne!(
        endpoint_source, full_source,
        "interior context must distinguish the test"
    );
    // The first objective has a larger numeric distance at every tuple. Its
    // endpoint cannot replace the later target's interior extremum in a
    // lexicographic objective, even though that later distance is smaller.
    let first_score = squared_distance(lab(gray(64)), lab(gray(255)));
    assert!((64..=192).all(|source| first_score > score(source, &full)));

    let mut p = program(
        color(10, 5, 64, 64, 64, false),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.constraints = ConstraintSet::new(vec![], vec![]);
    let TargetIntentV1::Computed(first) = &mut p.targets[0].intent else {
        unreachable!()
    };
    first.intent.as_mut().expect("first intent").tone =
        Some(ToneCurveV1::new(2, vec![[0.0, 1.0], [1.0, 1.0]]).expect("constant first preference"));
    p.targets.push(color(20, 15, 80, 64, 192, false));
    let TargetIntentV1::Computed(second) = &mut p.targets[1].intent else {
        unreachable!()
    };
    second.intent.as_mut().expect("later intent").tone = Some(curve);
    p.paints.push(Paint::Solid {
        id: PaintId::new(13),
        target: TargetId::new(20),
    });
    p.surfaces.push(Surface::FromOccurrence {
        id: SurfaceId::new(44),
        occurrence: OccurrenceId::new(5),
    });
    p.occurrences.push(Occurrence::new(
        OccurrenceId::new(15),
        PaintId::new(13),
        SurfaceId::new(44),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    ));
    p.outputs
        .push(OutputBinding::new(OutputSlotId::new(17), PaintId::new(13)));
    translucent(&mut p, 1, 13, 0.5);
    let owner = compile_assignment(p, 4096).expect("coupled lexicographic declarations");
    let CompiledTargetSelectionV1::Computed(selection) = &owner.owner_generation.target_selection
    else {
        unreachable!()
    };
    assert_eq!(
        selection.components.len(),
        1,
        "the target extrema must share one search context"
    );
    for eager in [false, true] {
        for (cases, expected) in [
            (endpoints.to_vec(), endpoint_source),
            (full.to_vec(), full_source),
            (vec![255, 128, 0, 128], full_source),
        ] {
            let mut session = owner
                .instantiate(ObservationStreamId::new(8))
                .expect("session");
            with_eager(eager, || {
                session
                    .prepare_update(observe(&cases))
                    .expect("contextual observations")
                    .commit()
            });
            let result = session
                .state()
                .current_verified()
                .expect("exact contextual result")
                .computed_for_test();
            assert_eq!(&*result.values, &[gray(64), gray(expected)]);
            assert_eq!(result.optimality(), ComputedOptimalityV1::Exact);
        }
    }
}

#[test]
fn interior_hard_case_rejects_sources_that_pass_both_endpoints() {
    let backdrops = [0, 32, 64, 96, 128, 160, 192, 224, 255];
    assert!(contrast_ratio(gray(117), gray(0)) >= 4.5);
    assert!(contrast_ratio(gray(117), gray(255)) >= 4.5);
    assert!((110..=126).all(|source| {
        backdrops
            .iter()
            .any(|&backdrop| contrast_ratio(gray(source), gray(backdrop)) < 4.5)
    }));
    let p = program(
        color(10, 5, 117, 110, 126, false),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    let owner = compile_assignment(p, 4096).expect("contextual declaration");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(observe(&backdrops))
        .expect("observation")
        .commit();
    let crate::session::SessionState::Failed {
        cause: ProgramViolationOutcomeV1::Computed(cause),
        ..
    } = session.state()
    else {
        panic!("interior obstruction must produce a complete infeasibility proof");
    };
    assert_eq!(cause.search.completion, CompletionV1::Complete);
    assert_eq!(cause.search.open_regions, 0);
}

#[test]
fn reference_capacity_and_objective_match_full_joint_enumeration_with_interior_cases() {
    let backdrops = [0, 20, 60, 100, 140, 180, 220];
    let half =
        |source, backdrop| crate::composition::source_over_channel_srgb8(source, 0.5, backdrop);
    let models = [
        ReferenceTransportV1::new(gray(68), gray(half(128, 255))),
        ReferenceTransportV1::new(gray(half(128, 255)), gray(255)),
    ];
    let tuples = (64..=71)
        .flat_map(|foreground| (128..=135).map(move |background| [foreground, background]))
        .collect::<Vec<_>>();
    let appearance = |tuple: [u8; 2], target: usize, case: usize| {
        let parent = half(tuple[1], backdrops[case]);
        if target == 0 {
            (gray(tuple[0]), gray(parent))
        } else {
            (gray(parent), gray(backdrops[case]))
        }
    };
    let signature = |tuple, target| {
        (0..backdrops.len())
            .map(|case| {
                let (visible, backdrop) = appearance(tuple, target, case);
                (backdrop, polarity(visible, backdrop))
            })
            .collect::<Vec<_>>()
    };
    let ratio = |tuple, target: usize| {
        (0..backdrops.len()).fold(1.0_f64, |minimum, case| {
            let (visible, backdrop) = appearance(tuple, target, case);
            minimum.min(contrast_ratio(visible, backdrop) / models[target].reference_contrast())
        })
    };
    let key = |tuple: [u8; 2]| {
        std::array::from_fn::<_, 2, _>(|target| {
            let environment = signature(tuple, target);
            let capacity = tuples
                .iter()
                .copied()
                .filter(|other| {
                    other[..target] == tuple[..target] && signature(*other, target) == environment
                })
                .fold(0.0_f64, |maximum, other| maximum.max(ratio(other, target)));
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
                .map(|(left, right)| left.total_cmp(&right))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or_else(|| {
                    left.iter()
                        .zip([68, 128])
                        .map(|(value, anchor)| *value != anchor)
                        .cmp(
                            right
                                .iter()
                                .zip([68, 128])
                                .map(|(value, anchor)| *value != anchor),
                        )
                        .then_with(|| left.cmp(&right))
                })
        })
        .expect("finite joint optimum");
    let expected_key = key(expected);
    for order in [
        backdrops.to_vec(),
        vec![220, 60, 0, 140, 20, 180, 100, 60, 0],
    ] {
        let mut p = program(
            color(10, 5, 68, 64, 71, true),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        );
        p.reference_inputs = Some(vec![(SurfaceInputPortId::new(2), gray(255))]);
        p.constraints = ConstraintSet::new(vec![], vec![]);
        p.targets.push(color(20, 15, 128, 128, 135, true));
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
        translucent(&mut p, 1, 13, 0.5);
        p.surfaces.push(Surface::FromOccurrence {
            id: SurfaceId::new(44),
            occurrence: OccurrenceId::new(15),
        });
        p.occurrences[0].against = SurfaceId::new(44);
        let owner =
            compile_assignment(p, EXECUTION_NODE_LIMIT_V1).expect("nested reference declaration");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("session");
        TONAL_CONVERSIONS.set(0);
        session
            .prepare_update(observe(&order))
            .expect("observation")
            .commit();
        let result = session
            .state()
            .current_verified()
            .expect("full reference support")
            .computed_for_test();
        assert_eq!(&*result.values, &expected.map(gray));
        assert_eq!(result.optimality(), ComputedOptimalityV1::Exact);
        assert_eq!(
            TONAL_CONVERSIONS.get(),
            0,
            "early complete search does not build terminal proposals"
        );
        let reports = result.reference_selections().expect("reference reports");
        for (target, report) in reports.iter().enumerate() {
            assert_eq!(report.cases().len(), backdrops.len());
            assert_eq!(
                report.cases().iter().fold(0.0_f64, |maximum, case| maximum
                    .max(case.virtual_distance_squared())),
                expected_key[target][3],
            );
            let capacity = report.capacity().expect("implicit reference capacity");
            assert!(capacity.complete());
            assert_eq!(capacity.ratio_lower(), capacity.ratio_upper());
            assert!(
                (capacity.ratio_lower() - (1.0 - expected_key[target][0])).abs()
                    < 2.0 * f64::EPSILON
            );
        }
    }
}

#[test]
fn chromatic_suffix_is_refined_at_the_selected_translucent_panel() {
    let panel_anchor = Srgb8::new([244, 241, 236]);
    let brand_anchor = Srgb8::new([49, 85, 190]);
    let rgb_target = |id, occurrence, anchor| {
        let mut target = ComputedTargetV1::color(
            anchor,
            2.0,
            ColorUseIntentV1 {
                identity: ColorIdentityDomainV1::new(anchor, false, IdentityOptionsV1::default())
                    .expect("source identity"),
                appearance: Some(OccurrenceId::new(occurrence)),
                tone: None,
            },
        );
        target.preference = u64::from(id);
        target.reference_appearance = true;
        Target::new(TargetId::new(id), TargetIntentV1::Computed(target))
    };
    let mut p = program(
        rgb_target(10, 5, panel_anchor),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.reference_inputs = Some(vec![(SurfaceInputPortId::new(2), gray(255))]);
    p.constraints = ConstraintSet::new(
        vec![ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(16),
            OccurrenceId::new(15),
            Wcag22CriterionV1::Sc143TextDefault,
        )],
        vec![],
    );
    translucent(&mut p, 0, 3, 0.85);
    p.targets.push(rgb_target(20, 15, brand_anchor));
    p.paints.push(Paint::Solid {
        id: PaintId::new(13),
        target: TargetId::new(20),
    });
    p.surfaces.push(Surface::FromOccurrence {
        id: SurfaceId::new(44),
        occurrence: OccurrenceId::new(5),
    });
    p.occurrences.push(Occurrence::new(
        OccurrenceId::new(15),
        PaintId::new(13),
        SurfaceId::new(44),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    ));
    p.outputs
        .push(OutputBinding::new(OutputSlotId::new(17), PaintId::new(13)));
    let owner = compile_assignment(p, EXECUTION_NODE_LIMIT_V1).expect("nested chromatic program");
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("session");
    session
        .prepare_update(update(1, 255))
        .expect("reference control")
        .commit();
    let control = session
        .state()
        .current_verified()
        .expect("reference control result")
        .computed_for_test();
    assert_eq!(&*control.values, &[panel_anchor, brand_anchor]);
    assert_eq!(control.optimality(), ComputedOptimalityV1::Exact);
    assert!(
        control
            .reference_selections()
            .expect("control reports")
            .iter()
            .all(|report| report
                .cases()
                .iter()
                .all(|case| case.virtual_distance_squared() == 0.0))
    );
    let support = red_green_support();
    let mut observation = observe_rgb(&support);
    observation.revision = crate::observation::Revision::new(2);
    TONAL_CONVERSIONS.set(0);
    session
        .prepare_update(observation)
        .expect("field support")
        .commit();
    assert!(
        TONAL_CONVERSIONS.get() <= 512,
        "two tonal walks are computed at most once and reused by the terminal pass"
    );
    let result = session
        .state()
        .current_verified()
        .expect("feasible composition")
        .computed_for_test();
    if result.optimality() == ComputedOptimalityV1::Unfinished {
        assert_eq!(
            result.search.visited, EXECUTION_NODE_LIMIT_V1,
            "the terminal proposal reserve must describe actual admitted work"
        );
    }
    let compose = |source: Srgb8, backdrop: Srgb8| {
        Srgb8::new(std::array::from_fn(|channel| {
            crate::composition::source_over_channel_srgb8(
                source.bytes()[channel],
                0.85,
                backdrop.bytes()[channel],
            )
        }))
    };
    let model = ReferenceTransportV1::new(brand_anchor, compose(panel_anchor, gray(255)));
    let backdrops: Vec<_> = support
        .iter()
        .map(|&backdrop| compose(result.values[0], backdrop))
        .collect();
    let score = |source| {
        backdrops.iter().fold(0.0_f64, |maximum, &backdrop| {
            maximum.max(squared_distance(
                lab(source),
                model.lab_for(backdrop, polarity(source, backdrop)),
            ))
        })
    };
    let identity = ColorIdentityDomainV1::new(brand_anchor, false, IdentityOptionsV1::default())
        .expect("brand identity");
    let witness = (0..=255_u32)
        .map(|step| color_at_lightness(brand_anchor, f64::from(step) / 255.0))
        .filter(|&source| identity.contains(source))
        .filter(|&source| {
            backdrops
                .iter()
                .all(|&backdrop| contrast_ratio(source, backdrop) >= 4.5)
        })
        .min_by(|&left, &right| score(left).total_cmp(&score(right)))
        .expect("blue feasible at the exact same selected panel");
    assert!(
        score(result.values[1]) <= score(witness),
        "bounded joint search must polish its feasible chromatic suffix: {:?} vs {witness:?}",
        result.values
    );
    assert_eq!(
        result.reference_selections().expect("reports")[1]
            .capacity()
            .expect("capacity")
            .ratio_lower(),
        1.0
    );
    assert!(
        backdrops
            .iter()
            .all(|&backdrop| polarity(result.values[1], backdrop) == polarity(witness, backdrop))
    );
    assert_eq!(
        result.reference_selections().expect("reports")[1]
            .cases()
            .iter()
            .fold(0.0_f64, |maximum, case| maximum
                .max(case.virtual_distance_squared())),
        score(result.values[1])
    );
    let field = crate::program_wire::ProgramDeclaredFieldV1::from_rgb8(
        &[0; 3],
        0,
        Srgb8::new([255, 0, 0]),
        Srgb8::new([0, 255, 0]),
    )
    .expect("public affine source declaration");
    let mut field_session = owner
        .instantiate(ObservationStreamId::new(8))
        .expect("independent field session");
    field_session
        .prepare_schema_ordered_field(crate::observation::Revision::new(1), &field)
        .expect("whole declared field admission")
        .commit();
    let field_result = field_session
        .state()
        .current_verified()
        .expect("feasible whole field")
        .computed_for_test();
    assert!(field_result.observation.declared_field().is_some());
    assert_eq!(
        field_result.observation.physical_case_count(),
        support.len()
    );
    assert_eq!(field_result.values, result.values);
    assert_eq!(field_result.optimality(), result.optimality());
    assert_eq!(field_result.search.visited, result.search.visited);
}
