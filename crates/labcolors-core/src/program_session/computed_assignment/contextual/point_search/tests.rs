use super::*;
use crate::color_intent::IdentityOptionsV1;
use crate::observation::{ObservationPayloadInput, ScenarioId, ScenarioInput, SurfaceInputBinding};
use crate::program_session::computed_assignment::tests::{program, rgb, target, update};
use crate::session_tests::CommitSessionUpdateForTest as _;

std::thread_local! { static REFRESHES: std::cell::RefCell<Vec<TargetId>> = const { std::cell::RefCell::new(Vec::new()) }; }
std::thread_local! { static SEED_REFRESHES: std::cell::RefCell<Vec<TargetId>> = const { std::cell::RefCell::new(Vec::new()) }; }
std::thread_local! { static FORCE_REFINED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
std::thread_local! { static PROFILE_MEASUREMENTS: std::cell::Cell<[u64;5]> = const { std::cell::Cell::new([0;5]) }; }

pub(super) fn force_refined() -> bool {
    FORCE_REFINED.get()
}

pub(super) fn measured_profile(region: bool, precision: PointMeasurementPrecisionV1) {
    PROFILE_MEASUREMENTS.with(|counts| {
        let mut value = counts.get();
        value[usize::from(region) * 2
            + usize::from(precision == PointMeasurementPrecisionV1::Refined)] += 1;
        counts.set(value);
    });
}

pub(super) fn refined_incumbent() {
    PROFILE_MEASUREMENTS.with(|counts| {
        let mut value = counts.get();
        value[4] += 1;
        counts.set(value);
    });
}

fn take_profile_measurements() -> [u64; 5] {
    PROFILE_MEASUREMENTS.replace([0; 5])
}

fn with_refined<T>(value: bool, operation: impl FnOnce() -> T) -> T {
    struct Guard(bool);
    impl Drop for Guard {
        fn drop(&mut self) {
            FORCE_REFINED.set(self.0);
        }
    }
    let _guard = Guard(FORCE_REFINED.replace(value));
    operation()
}
pub(in crate::program_session::computed_assignment::contextual) fn refreshed_point_case(
    target: TargetId,
) {
    REFRESHES.with(|values| values.borrow_mut().push(target));
}

fn take_refreshes() -> Vec<TargetId> {
    REFRESHES.with(|values| std::mem::take(&mut *values.borrow_mut()))
}

pub(in crate::program_session::computed_assignment::contextual) fn refreshed_point_seed(
    target: TargetId,
) {
    SEED_REFRESHES.with(|values| values.borrow_mut().push(target));
}

fn take_seed_refreshes() -> Vec<TargetId> {
    SEED_REFRESHES.with(|values| std::mem::take(&mut *values.borrow_mut()))
}

fn fixture_error(error: impl core::fmt::Debug) -> String {
    format!("{error:?}")
}

fn compile(
    mut p: Program<Wcag22Srgb8V1>,
    budget: u64,
    reverse: bool,
) -> CompiledProgram<Wcag22Srgb8V1> {
    let mut ids: Vec<_> = p
        .targets
        .iter()
        .filter(|target| matches!(target.intent, TargetIntentV1::Computed(_)))
        .map(|target| target.id)
        .collect();
    if reverse {
        ids.reverse();
    }
    p.assignment = Some(ComputedSelectionPolicyV1::AuthoredComposition {
        legacy_reference_order: ids,
    });
    p.computed_budget = Some(budget);
    p.compile()
        .expect("joint fixture declares every computed target in the composition")
}

fn joint_gray() -> Program<Wcag22Srgb8V1> {
    let mut p = program(
        target(10, 240, 240, 248, 0),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.targets.push(target(20, 118, 108, 118, 1));
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
    p.constraints = ConstraintSet::new(
        vec![ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(16),
            OccurrenceId::new(15),
            Wcag22CriterionV1::Sc143TextDefault,
        )],
        vec![],
    );
    p.outputs
        .push(OutputBinding::new(OutputSlotId::new(17), PaintId::new(13)));
    p
}

#[test]
fn unavailable_subnormal_witness_leaves_the_refined_region_comparable() -> Result<(), String> {
    use crate::certified_numeric::{CertifiedRgbV1, source_distance_squared};

    let tiny = CertifiedRgbV1::from_encoded([f64::from_bits(1); 3]).map_err(fixture_error)?;
    let preferred = CertifiedRgbV1::from_encoded([128.0; 3])
        .and_then(CertifiedRgbV1::oklab)
        .map_err(fixture_error)?;
    let witness = tiny.region_loss_witness(preferred);
    assert!(witness.is_err());
    assert!(
        !measured::<Wcag22Srgb8V1>(witness.map(|_| ()).map_err(Into::into))
            .map_err(fixture_error)?
    );
    let mut transforms =
        crate::certified_numeric::PointTransformCache::new().map_err(fixture_error)?;
    let refined = crate::certified_numeric::CertifiedOklabCacheV1::default()
        .evaluate(tiny, &mut transforms)
        .and_then(|lab| source_distance_squared(lab, preferred));
    assert!(
        measured::<Wcag22Srgb8V1>(refined.map(|_| ()).map_err(Into::into))
            .map_err(fixture_error)?
    );
    Ok(())
}

#[test]
fn public_joint_point_matches_tiny_symmetric_oracle_and_bootstraps_feasibility()
-> Result<(), String> {
    let mut oracle = Vec::new();
    for parent in 240..=248 {
        for child in 108..=118 {
            if matches!(
                evaluate_wcag22_srgb8([child; 3], [parent; 3], Wcag22CriterionV1::Sc143TextDefault)
                    .map_err(fixture_error)?,
                Wcag22AssessmentV1::Evaluated {
                    decision: Wcag22ApplicableDecisionV1::Pass,
                    ..
                }
            ) {
                let mut losses = [
                    squared_distance(lab(Srgb8::new([parent; 3])), lab(Srgb8::new([240; 3]))),
                    squared_distance(lab(Srgb8::new([child; 3])), lab(Srgb8::new([118; 3]))),
                ];
                losses.sort_by(|a, b| b.total_cmp(a));
                oracle.push((losses, [parent, child]));
            }
        }
    }
    oracle.sort_by(|a, b| {
        a.0[0]
            .total_cmp(&b.0[0])
            .then_with(|| a.0[1].total_cmp(&b.0[1]))
    });
    let expected = oracle[0].1;
    assert!(
        expected[0] > 240,
        "symmetric minimax must move the parent too, unlike ordered selection"
    );
    let mut identity = None;
    for reverse in [false, true] {
        let owner = compile(joint_gray(), 1024, reverse);
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .map_err(fixture_error)?;
        session.commit(update(1, 255)).map_err(fixture_error)?;
        let current = session
            .state()
            .current_verified()
            .expect("the feasible update produced a verified assignment")
            .computed_for_test();
        let actual: Vec<_> = current
            .arena
            .storage
            .outputs
            .iter()
            .map(|out| out.paint.source().bytes()[0])
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(current.optimality(), ComputedOptimalityV1::Unfinished);
        assert_eq!(current.selection_components().len(), 1);
        assert_eq!(
            current.selection_components()[0].mode(),
            ProgramSelectionModeV1::JointPointV1
        );
        assert!(current.search.visited <= 1024);
        if let Some(identity) = identity {
            assert_eq!(current.identity, identity);
        }
        identity = Some(current.identity);
    }
    Ok(())
}

#[test]
fn public_point_starts_from_invalid_authored_source_and_reuses_compiled_anchors()
-> Result<(), String> {
    let owner = compile(
        program(
            target(10, 150, 0, 255, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        ),
        1024,
        false,
    );
    let CompiledTargetSelectionV1::Computed(selection) = &owner.owner_generation.target_selection
    else {
        panic!()
    };
    let anchors = selection
        .point_anchors
        .as_ref()
        .expect("compiled point selection retains its source anchors")
        .as_ref() as *const _;
    for _ in 0..2 {
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .map_err(fixture_error)?;
        session.commit(update(1, 255)).map_err(fixture_error)?;
        assert_eq!(rgb(&session), Srgb8::new([118; 3]));
        assert_eq!(
            selection
                .point_anchors
                .as_ref()
                .expect("compiled point selection retains its source anchors")
                .as_ref() as *const _,
            anchors
        );
        session.commit(update(2, 0)).map_err(fixture_error)?;
        assert_eq!(rgb(&session), Srgb8::new([150; 3]));
        session.commit(update(3, 255)).map_err(fixture_error)?;
        assert_eq!(rgb(&session), Srgb8::new([118; 3]));
    }
    Ok(())
}

#[test]
fn public_point_profile_keeps_every_unique_local_case() -> Result<(), String> {
    let mut p = program(
        target(10, 110, 90, 110, 0),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.constraints = ConstraintSet::new(vec![], vec![]);
    let curve = ToneCurveV1::new(2, vec![[0.0, 0.8], [1.0, 0.2]]).map_err(fixture_error)?;
    if let TargetIntentV1::Computed(target) = &mut p.targets[0].intent {
        target.intent = Some(ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(
                Srgb8::new([110; 3]),
                true,
                IdentityOptionsV1::default(),
            )
            .map_err(fixture_error)?,
            appearance: Some(OccurrenceId::new(5)),
            tone: Some(curve.clone()),
        });
    }
    let owner = compile(p, 256, false);
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(fixture_error)?;
    let mut input = update(1, 255);
    let ObservationPayloadInput::Scenarios(set) = &mut input.payload else {
        panic!()
    };
    set.scenarios.push(ScenarioInput {
        id: ScenarioId::new(10),
        bindings: vec![SurfaceInputBinding::new(
            SurfaceInputPortId::new(2),
            ColorSignal::from_srgb8(Srgb8::new([0; 3])),
        )],
    });
    session.commit(input).map_err(fixture_error)?;
    let loss = |gray| {
        let value = lab(Srgb8::new([gray; 3]));
        let source = lab(Srgb8::new([110; 3]));
        let mut rows = [0, 255].map(|backdrop| {
            squared_distance(
                value,
                [
                    curve.preferred_lightness(Srgb8::new([backdrop; 3])),
                    source[1],
                    source[2],
                ],
            )
        });
        rows.sort_by(|a, b| b.total_cmp(a));
        rows
    };
    let expected = (90..=110)
        .min_by(|&a, &b| {
            let a = loss(a);
            let b = loss(b);
            a[0].total_cmp(&b[0]).then_with(|| a[1].total_cmp(&b[1]))
        })
        .expect("the inclusive gray domain contains a minimizer");
    assert_eq!(rgb(&session), Srgb8::new([expected; 3]));
    Ok(())
}

#[test]
fn independent_support_reuses_other_component_and_refreshes_only_unique_local_cases()
-> Result<(), String> {
    let mut p = program(
        target(10, 150, 0, 255, 0),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.targets.push(target(20, 150, 0, 255, 1));
    p.observation_group
        .surface_input_ports
        .push(SurfaceInputPortId::new(12));
    p.surfaces.push(Surface::Input {
        id: SurfaceId::new(14),
        input: SurfaceInputPortId::new(12),
    });
    p.paints.push(Paint::Solid {
        id: PaintId::new(13),
        target: TargetId::new(20),
    });
    p.occurrences.push(Occurrence::new(
        OccurrenceId::new(15),
        PaintId::new(13),
        SurfaceId::new(14),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    ));
    p.constraints
        .hard
        .push(ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(16),
            OccurrenceId::new(15),
            Wcag22CriterionV1::Sc143TextDefault,
        ));
    p.outputs
        .push(OutputBinding::new(OutputSlotId::new(17), PaintId::new(13)));
    let owner = compile(p, 1024, false);
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(fixture_error)?;
    let observed = |revision, first| {
        let mut input = update(revision, first);
        let ObservationPayloadInput::Scenarios(set) = &mut input.payload else {
            panic!()
        };
        set.scenarios = [0, 10]
            .into_iter()
            .enumerate()
            .map(|(index, second)| ScenarioInput {
                id: ScenarioId::new(index as u32),
                bindings: vec![
                    SurfaceInputBinding::new(
                        SurfaceInputPortId::new(2),
                        ColorSignal::from_srgb8(Srgb8::new([first; 3])),
                    ),
                    SurfaceInputBinding::new(
                        SurfaceInputPortId::new(12),
                        ColorSignal::from_srgb8(Srgb8::new([second; 3])),
                    ),
                ],
            })
            .collect();
        input
    };
    take_refreshes();
    take_seed_refreshes();
    session.commit(observed(1, 255)).map_err(fixture_error)?;
    let refreshed = take_refreshes();
    assert_eq!(
        refreshed
            .iter()
            .filter(|&&id| id == TargetId::new(10))
            .count(),
        1
    );
    assert_eq!(
        refreshed
            .iter()
            .filter(|&&id| id == TargetId::new(20))
            .count(),
        2
    );
    assert_eq!(take_seed_refreshes(), refreshed);
    assert_eq!(rgb(&session), Srgb8::new([118; 3]));
    session.commit(observed(2, 250)).map_err(fixture_error)?;
    assert_eq!(take_refreshes(), vec![TargetId::new(10)]);
    assert_eq!(take_seed_refreshes(), vec![TargetId::new(10)]);
    session.commit(observed(3, 250)).map_err(fixture_error)?;
    assert!(take_refreshes().is_empty());
    assert!(take_seed_refreshes().is_empty());
    let current = session
        .state()
        .current_verified()
        .expect("the feasible update produced a verified assignment")
        .computed_for_test();
    assert_eq!(current.selection_components().len(), 2);
    assert!(current.search.visited <= 2048);
    Ok(())
}

#[test]
fn explicit_reference_point_preserves_ordinary_reference_reports() -> Result<(), String> {
    let mut p = program(
        target(10, 100, 90, 110, 0),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    if let TargetIntentV1::Computed(target) = &mut p.targets[0].intent {
        target.reference_appearance = true;
        target.intent = Some(ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(
                Srgb8::new([100; 3]),
                true,
                IdentityOptionsV1::default(),
            )
            .map_err(fixture_error)?,
            appearance: Some(OccurrenceId::new(5)),
            tone: Some(ToneCurveV1::new(2, vec![[0.0, 0.45], [1.0, 0.45]]).map_err(fixture_error)?),
        });
    }
    p.reference_inputs = Some(vec![(SurfaceInputPortId::new(2), Srgb8::new([255; 3]))]);
    let owner = compile(p, 256, false);
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(fixture_error)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let current = session
        .state()
        .current_verified()
        .expect("the feasible update produced a verified assignment")
        .computed_for_test();
    let report = &current
        .reference_selections()
        .expect("the reference selection exposes its transfer evidence")[0];
    assert_eq!(
        report.preference,
        crate::program_wire::ProgramReferencePreferenceV1::ExplicitTone
    );
    assert!(report.capacity.is_none());
    assert_eq!(report.cases.len(), 1);
    assert_eq!(report.cases[0].actual_visible, rgb(&session));
    assert_eq!(report.cases[0].reference_visible, Srgb8::new([100; 3]));
    assert_eq!(report.cases[0].actual_backdrop, Srgb8::new([255; 3]));
    Ok(())
}

#[test]
fn unresolved_profile_never_falls_through_to_source_tie()
-> Result<(), crate::certified_numeric::NumericUncertaintyV1> {
    let a = crate::certified_numeric::CertifiedRgbV1::from_encoded([52.0; 3])?.oklab()?;
    let distance = crate::certified_numeric::source_distance_squared(
        a,
        [CertifiedScalarV1::exact(0.9)?, a[1], a[2]],
    )?;
    assert_ne!(distance.bounds()[0], distance.bounds()[1]);
    assert_eq!(profile_order(&[distance], &[distance]), None);
    let lo = CertifiedScalarV1::exact(1.0)?;
    let hi = CertifiedScalarV1::exact(2.0)?;
    assert_eq!(profile_order(&[hi, lo], &[hi, hi]), Some(Ordering::Less));
    Ok(())
}

fn repeated_children(opacity: f64, duplicate: bool) -> Program<Wcag22Srgb8V1> {
    let mut p = joint_gray();
    if opacity != 1.0 {
        p.opacities
            .push(OpacityInput::new(OpacityInputId::new(1), opacity));
        p.paints.push(Paint::Opacity {
            id: PaintId::new(11),
            source: PaintId::new(3),
            opacity: OpacityInputId::new(1),
        });
        p.occurrences[0].subject = PaintId::new(11);
    }
    for (index, (anchor, appearance)) in [([244, 234, 219], 5), ([0, 103, 244], 15)]
        .into_iter()
        .enumerate()
    {
        p.targets[index].intent = TargetIntentV1::Computed(ComputedTargetV1::color(
            Srgb8::new(anchor),
            1.0,
            ColorUseIntentV1 {
                identity: ColorIdentityDomainV1::new(
                    Srgb8::new(anchor),
                    false,
                    IdentityOptionsV1::default(),
                )
                .expect("the chromatic fixture uses the default source identity"),
                appearance: Some(OccurrenceId::new(appearance)),
                tone: None,
            },
        ));
    }
    if duplicate {
        let mut declaration = match &p.targets[1].intent {
            TargetIntentV1::Computed(target) => target.clone(),
            _ => unreachable!(),
        };
        declaration
            .intent
            .as_mut()
            .expect("the duplicated child has a declared color intent")
            .appearance = Some(OccurrenceId::new(25));
        p.targets.push(Target::new(
            TargetId::new(30),
            TargetIntentV1::Computed(declaration),
        ));
        p.paints.push(Paint::Solid {
            id: PaintId::new(23),
            target: TargetId::new(30),
        });
        p.occurrences.push(Occurrence::new(
            OccurrenceId::new(25),
            PaintId::new(23),
            SurfaceId::new(14),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            p.occurrences[0].context,
        ));
        p.constraints
            .hard
            .push(ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(26),
                OccurrenceId::new(25),
                Wcag22CriterionV1::Sc143TextDefault,
            ));
        p.outputs
            .push(OutputBinding::new(OutputSlotId::new(27), PaintId::new(23)));
    }
    p
}

#[test]
fn repeated_local_requirements_have_one_profile_and_one_search_dimension() -> Result<(), String> {
    // Public regression previously emitted two unequal siblings, including
    // [6,96,236] and [6,97,237]. Copying the latter strictly improved P.
    for opacity in [1.0, 0.5] {
        let owners = [
            compile(repeated_children(opacity, false), 256, false),
            compile(repeated_children(opacity, true), 256, false),
        ];
        let mut sessions = owners
            .iter()
            .map(|owner| owner.instantiate(ObservationStreamId::new(8)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(fixture_error)?;
        let CompiledTargetSelectionV1::Computed(selection) =
            &owners[1].owner_generation.target_selection
        else {
            panic!()
        };
        assert_eq!(selection.components[0].point_variable_count(), 2);
        for (revision, backdrop) in [[244, 234, 219], [255, 255, 255]].into_iter().enumerate() {
            let mut results = Vec::new();
            for session in &mut sessions {
                let mut input = update(revision as u64 + 1, 0);
                let ObservationPayloadInput::Scenarios(set) = &mut input.payload else {
                    panic!()
                };
                set.scenarios[0].bindings[0] = SurfaceInputBinding::new(
                    SurfaceInputPortId::new(2),
                    ColorSignal::from_srgb8(Srgb8::new(backdrop)),
                );
                session.commit(input).map_err(fixture_error)?;
                let current = session
                    .state()
                    .current_verified()
                    .expect("bootstrap must find a hard-feasible tuple")
                    .computed_for_test();
                results.push(
                    current
                        .arena
                        .storage
                        .outputs
                        .iter()
                        .map(|output| output.paint.source())
                        .collect::<Vec<_>>(),
                );
            }
            assert_eq!(
                &results[1][..2],
                &results[0],
                "opacity={opacity}, backdrop={backdrop:?}"
            );
            assert_eq!(results[1][2], results[0][1]);
        }
    }
    Ok(())
}

#[test]
fn different_requirement_or_downstream_effect_prevents_source_quotient() -> Result<(), String> {
    let mut different = repeated_children(1.0, true);
    different.constraints.hard[1] = ConstraintInvocation::visible_unary_hard(
        ConstraintId::new(26),
        OccurrenceId::new(25),
        Wcag22CriterionV1::Sc143TextLargeScale,
    );
    let mut downstream = repeated_children(1.0, true);
    downstream.targets.push(target(40, 30, 0, 255, 0));
    downstream.paints.push(Paint::Solid {
        id: PaintId::new(33),
        target: TargetId::new(40),
    });
    downstream.surfaces.push(Surface::FromOccurrence {
        id: SurfaceId::new(34),
        occurrence: OccurrenceId::new(15),
    });
    downstream.occurrences.push(Occurrence::new(
        OccurrenceId::new(35),
        PaintId::new(33),
        SurfaceId::new(34),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        downstream.occurrences[0].context,
    ));
    downstream
        .constraints
        .hard
        .push(ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(36),
            OccurrenceId::new(35),
            Wcag22CriterionV1::Sc143TextDefault,
        ));
    let mut related = repeated_children(1.0, true);
    related
        .constraints
        .hard
        .push(ConstraintInvocation::exact_intrinsic_distinction_hard(
            ConstraintId::new(37),
            DirectedRelationV1::try_new(TargetId::new(20), vec![TargetId::new(30)])
                .map_err(fixture_error)?,
        ));
    related.assignment = Some(ComputedSelectionPolicyV1::AuthoredComposition {
        legacy_reference_order: vec![TargetId::new(10), TargetId::new(20), TargetId::new(30)],
    });
    // This production relation family requires an immutable reference. Source
    // quotienting must not bypass that earlier admission boundary either.
    assert!(matches!(
        related.compile(),
        Err(ProgramCompileError::SolverDependentIntrinsicRelationReference { .. })
    ));
    let mut transparent_different = repeated_children(1.0, true);
    transparent_different.constraints.hard[1] = ConstraintInvocation::visible_unary_hard(
        ConstraintId::new(26),
        OccurrenceId::new(25),
        Wcag22CriterionV1::Sc143TextLargeScale,
    );
    transparent_different
        .opacities
        .push(OpacityInput::new(OpacityInputId::new(1), 0.0));
    for (occurrence, source, paint) in [(1, 13, 101), (2, 23, 102)] {
        transparent_different.paints.push(Paint::Opacity {
            id: PaintId::new(paint),
            source: PaintId::new(source),
            opacity: OpacityInputId::new(1),
        });
        transparent_different.occurrences[occurrence].subject = PaintId::new(paint);
    }
    for p in [different, downstream, transparent_different] {
        let owner = compile(p, 256, false);
        let CompiledTargetSelectionV1::Computed(selection) =
            &owner.owner_generation.target_selection
        else {
            panic!()
        };
        assert!(
            selection
                .components
                .iter()
                .all(|component| component.point_equivalence.is_none())
        );
    }
    Ok(())
}

#[test]
fn authored_constant_or_exact_appearance_stops_after_one_hard_check() -> Result<(), String> {
    for (opacity, backdrop, exact_stop) in [(0.0, 255, true), (0.5, 100, true), (0.5, 255, false)] {
        let mut p = program(
            target(10, 100, 90, 110, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        );
        p.constraints = ConstraintSet::new(vec![], vec![]);
        p.opacities
            .push(OpacityInput::new(OpacityInputId::new(1), opacity));
        p.paints.push(Paint::Opacity {
            id: PaintId::new(11),
            source: PaintId::new(3),
            opacity: OpacityInputId::new(1),
        });
        p.occurrences[0].subject = PaintId::new(11);
        if let TargetIntentV1::Computed(target) = &mut p.targets[0].intent {
            target.intent = Some(ColorUseIntentV1 {
                identity: ColorIdentityDomainV1::new(
                    Srgb8::new([100; 3]),
                    true,
                    IdentityOptionsV1::default(),
                )
                .map_err(fixture_error)?,
                appearance: Some(OccurrenceId::new(5)),
                tone: None,
            });
        }
        let owner = compile(p, 64, false);
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .map_err(fixture_error)?;
        session.commit(update(1, backdrop)).map_err(fixture_error)?;
        let current = session
            .state()
            .current_verified()
            .expect("the feasible update produced a verified assignment")
            .computed_for_test();
        assert_eq!(
            current.search.visited == 1,
            exact_stop,
            "opacity={opacity}, backdrop={backdrop}"
        );
    }
    Ok(())
}

fn small_alpha_program() -> Program<Wcag22Srgb8V1> {
    let mut p = program(
        // More than one leaf keeps the ambiguous comparison outside the incumbent box.
        target(10, 100, 80, 120, 0),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.constraints = ConstraintSet::new(vec![], vec![]);
    p.opacities
        .push(OpacityInput::new(OpacityInputId::new(1), 1e-15));
    p.paints.push(Paint::Opacity {
        id: PaintId::new(11),
        source: PaintId::new(3),
        opacity: OpacityInputId::new(1),
    });
    p.occurrences[0].subject = PaintId::new(11);
    if let TargetIntentV1::Computed(target) = &mut p.targets[0].intent {
        target.intent = Some(ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(
                Srgb8::new([100; 3]),
                true,
                IdentityOptionsV1::default(),
            )
            .expect("the chromatic fixture uses the default source identity"),
            appearance: Some(OccurrenceId::new(5)),
            tone: None,
        });
    }
    p
}

#[test]
fn screened_search_preserves_refined_tuple_and_every_budget_counter() -> Result<(), String> {
    let mut totals = [0_u64; 5];
    for (p, budget, backdrops) in [
        (joint_gray(), 128, vec![[255; 3]]),
        (small_alpha_program(), 64, vec![[128; 3]]),
        (translucent_child_over_variable_parent(), 1, vec![[192; 3]]),
        (translucent_child_over_variable_parent(), 17, vec![[192; 3]]),
        (
            translucent_child_over_variable_parent(),
            255,
            vec![[192; 3]],
        ),
        (
            translucent_child_over_variable_parent(),
            4096,
            vec![[192; 3]],
        ),
        (
            repeated_children(1.0, true),
            256,
            vec![[255; 3], [244, 234, 219]],
        ),
        (
            repeated_children(0.5, true),
            256,
            vec![[255; 3], [244, 234, 219]],
        ),
    ] {
        let owner = compile(p, budget, false);
        let run = |refined| {
            with_refined(refined, || {
                take_profile_measurements();
                let mut session = owner
                    .instantiate(ObservationStreamId::new(8))
                    .map_err(fixture_error)?;
                let mut history = Vec::new();
                for (revision, backdrop) in backdrops.iter().enumerate() {
                    let mut input = update(revision as u64 + 1, 0);
                    let ObservationPayloadInput::Scenarios(set) = &mut input.payload else {
                        panic!()
                    };
                    set.scenarios[0].bindings[0] = SurfaceInputBinding::new(
                        SurfaceInputPortId::new(2),
                        ColorSignal::from_srgb8(Srgb8::new(*backdrop)),
                    );
                    session.commit(input).map_err(fixture_error)?;
                    let current = session
                        .state()
                        .current_verified()
                        .expect("the feasible update produced a verified assignment")
                        .computed_for_test();
                    assert!(current.search.visited <= budget);
                    history.push((
                        current
                            .arena
                            .storage
                            .outputs
                            .iter()
                            .map(|output| output.paint.source())
                            .collect::<Vec<_>>(),
                        current.search,
                    ));
                }
                Ok::<_, String>((history, take_profile_measurements()))
            })
        };
        let (screened, counts) = run(false)?;
        let (refined, full_counts) = run(true)?;
        assert_eq!(
            screened, refined,
            "screening must preserve both decisions and traversal"
        );
        assert_eq!(full_counts[0] + full_counts[2], 0);
        for (total, count) in totals.iter_mut().zip(counts) {
            *total += count;
        }
    }
    assert!(
        totals[0] > 0 && totals[2] > 0,
        "exercise screening of points and regions"
    );
    assert!(
        totals[1] > 0 && totals[3] > 0 && totals[4] > 0,
        "ambiguous challenger, region, and incumbent all require refinement: {totals:?}"
    );
    eprintln!(
        "screened search measurements [point screen, point full, region screen, region full, incumbent full]={totals:?}"
    );
    Ok(())
}

fn translucent_child_over_variable_parent() -> Program<Wcag22Srgb8V1> {
    let mut p = joint_gray();
    p.targets[0] = target(10, 32, 0, 255, 0);
    p.targets[1] = target(20, 224, 220, 228, 1);
    p.constraints = ConstraintSet::new(vec![], vec![]);
    p.opacities
        .push(OpacityInput::new(OpacityInputId::new(1), 0.875));
    p.paints.push(Paint::Opacity {
        id: PaintId::new(33),
        source: PaintId::new(13),
        opacity: OpacityInputId::new(1),
    });
    p.occurrences[1].subject = PaintId::new(33);
    for (target, occurrence) in p.targets.iter_mut().zip([5, 15]) {
        let TargetIntentV1::Computed(t) = &mut target.intent else {
            unreachable!()
        };
        t.intent = Some(ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(t.anchor, true, IdentityOptionsV1::default())
                .expect("the fixture declares neutral sources"),
            appearance: Some(OccurrenceId::new(occurrence)),
            tone: None,
        });
    }
    p
}
