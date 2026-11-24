use super::*;
use crate::color_intent::IdentityOptionsV1;
use crate::observation::{ObservationPayloadInput, ScenarioId, ScenarioInput, SurfaceInputBinding};
use crate::program_session::computed_assignment::tests::{
    compile_assignment, program, target, update,
};
use crate::session::SessionState;
use crate::session_tests::CommitSessionUpdateForTest as _;

// Standalone fixtures retain the original explicit-proposal surface. All
// production preparation uses incoming arguments; this adapter is test-only.
struct StandalonePreparedV1<'program, 'session, E: ProgramConstraintEvaluatorSetV1> {
    epoch: &'program ProgramEpochV1<E>,
    observation: RevisionBoundObservationV1,
    scratch: ReferenceCompletionScratchV1<E>,
    bindings: AdmittedAppearanceBindings,
    workspace: AppearanceWorkspace,
    cache: ProgramPresentationCacheV1,
    family_artifacts: &'session BoundFamilyArtifactBundleV2,
    arena: ProgramEvaluationArenaV1<E>,
}

impl<E: ProgramConstraintEvaluatorSetV1> CompiledProgram<E> {
    fn prepare_reference_completion<'program, 'session>(
        &'program self,
        session: &'session Session<ProgramSessionPlan<E>>,
    ) -> Result<StandalonePreparedV1<'program, 'session, E>, Error<E>> {
        if !self.owns_session(session) {
            return Err(Error::<E>::OwnerMismatch);
        }
        let crate::observation::ObservationHeadViewV1::Observed(observation) = session.raw_head()
        else {
            return Err(Error::<E>::MissingFiniteObservation);
        };
        let epoch = &self.owner_generation;
        let mut scratch = ReferenceCompletionScratchV1::empty(&self.owner_generation);
        prepare(epoch, observation, &mut scratch)?;
        Ok(StandalonePreparedV1 {
            epoch,
            observation: observation.clone(),
            scratch,
            bindings: epoch.binding_template.try_clone_v1()?,
            workspace: epoch.graph.new_workspace()?,
            cache: ProgramPresentationCacheV1::try_new(&epoch.point_presentations)
                .map_err(|_| Error::<E>::ResourceExhausted)?,
            family_artifacts: &session.plan().family_artifacts,
            arena: ProgramEvaluationArenaV1::empty(),
        })
    }
}

impl<'program, E: ProgramConstraintEvaluatorSetV1> StandalonePreparedV1<'program, '_, E> {
    fn sources(&self) -> impl ExactSizeIterator<Item = (TargetId, Srgb8)> + '_ {
        let CompiledTargetSelectionV1::Computed(selection) = &self.epoch.target_selection else {
            panic!()
        };
        selection
            .targets
            .iter()
            .map(|target| (target.id, target.declaration.anchor))
    }

    fn realize(
        &mut self,
        anchor: &[ContinuousSrgbV1],
        completion: &[ContinuousSrgbV1],
        budget: u64,
    ) -> Result<ReferenceCompletionResultV1<'program, E>, Error<E>> {
        let mut prepared = prepare(self.epoch, &self.observation, &mut self.scratch)?;
        let mut runtime = ProgramEvaluationRuntimeV1 {
            bindings: &mut self.bindings,
            workspace: &mut self.workspace,
            presentation_cache: &mut self.cache,
            family_artifacts: self.family_artifacts,
        };
        prepared.realize(&mut runtime, &mut self.arena, anchor, completion, budget)
    }
}

struct IncomingRequestV1 {
    previous: RevisionBoundObservationV1,
    expected_revision: u64,
    expected_backdrop: Srgb8,
}

struct IncomingReceiptV1 {
    observation: RevisionBoundObservationV1,
    emitted: Vec<Srgb8>,
    old_tone: Option<u64>,
    new_tone: Option<u64>,
    buffers: [usize; 5],
}

std::thread_local! {
    static INCOMING_REQUEST: core::cell::RefCell<Option<IncomingRequestV1>> = const { core::cell::RefCell::new(None) };
    static INCOMING_RECEIPTS: core::cell::RefCell<Vec<IncomingReceiptV1>> = const { core::cell::RefCell::new(Vec::new()) };
    static PREFERENCE_ROW_VISITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

pub(super) fn preference_row_visit() {
    PREFERENCE_ROW_VISITS.with(|value| value.set(value.get() + 1));
}

fn fixture_error(error: impl core::fmt::Debug) -> String {
    format!("{error:?}")
}

fn fixture_session(
    program: Program<Wcag22Srgb8V1>,
    budget: u64,
) -> Result<
    (
        CompiledProgram<Wcag22Srgb8V1>,
        Session<ProgramSessionPlan<Wcag22Srgb8V1>>,
    ),
    String,
> {
    let owner = compile_assignment(program, budget).map_err(fixture_error)?;
    let session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(fixture_error)?;
    Ok((owner, session))
}

fn retained_buffers<E: ProgramConstraintEvaluatorSetV1>(
    plan: &ProgramSessionPlan<E>,
) -> [usize; 5] {
    let scratch = plan
        .reference_completion
        .storage
        .as_ref()
        .expect("reference completion has allocated reusable storage");
    [
        scratch.source_labs.as_ptr() as usize,
        scratch.binding_targets.as_ptr() as usize,
        scratch.uses.as_ptr() as usize,
        scratch.uses[0].preferred.as_ptr() as usize,
        plan.bindings.paint_inputs_canonical().as_ptr() as usize,
    ]
}

/// Called only by the actual SessionPlan incoming evaluation, before ordinary
/// selection. It neither changes the selected outcome nor publishes evidence.
pub(in crate::program_session) fn incoming_test_hook<E: ProgramConstraintEvaluatorSetV1>(
    plan: &mut ProgramSessionPlan<E>,
    epoch: &ProgramEpochV1<E>,
    incoming: &RevisionBoundObservationV1,
) {
    let Some(request) = INCOMING_REQUEST.with(|value| value.borrow_mut().take()) else {
        return;
    };
    assert_eq!(incoming.revision().value(), request.expected_revision);
    assert_ne!(incoming.revision(), request.previous.revision());
    assert_eq!(
        incoming
            .physical_values(0)
            .expect("incoming observation carries the first physical case")[0]
            .srgb8(),
        request.expected_backdrop
    );
    let anchor = [point([80., 89., 109.])];
    let old = {
        // Retain a previous-observation result while reusing the incoming
        // transaction's free arena; the committed Ready arena stays leased.
        let mut arena = plan
            .evaluation_arenas
            .guard(incoming.arena_slot())
            .expect("incoming update leases its own evaluation arena");
        let mut runtime = ProgramEvaluationRuntimeV1 {
            bindings: &mut plan.bindings,
            workspace: &mut plan.workspace,
            presentation_cache: &mut plan.presentation_cache,
            family_artifacts: &plan.family_artifacts,
        };
        let mut prepared = prepare(epoch, &request.previous, &mut plan.reference_completion)
            .unwrap_or_else(|_| panic!("previous observation preparation"));
        assert_eq!(
            prepared.sources().next(),
            Some((TargetId::new(10), Srgb8::new([49, 85, 190])))
        );
        prepared
            .realize(&mut runtime, arena.storage_mut(), &anchor, &anchor, 1)
            .unwrap_or_else(|_| panic!("previous observation operation"))
    };
    let old_observation = old.observation().clone();
    let old_tone = old.use_tones()[0].per_case_bits[0];
    let old_measurement = old.point_evidence().0.preferences[0].visible_lab;
    let buffers = retained_buffers(plan);
    let new = realize_incoming(plan, epoch, incoming, &anchor, &anchor, 1)
        .unwrap_or_else(|_| panic!("incoming observation operation"));
    assert_eq!(retained_buffers(plan), buffers);
    // Both borrows of the mutable plan have ended while the first result is
    // still read: the type itself prevents a result/scratch lifetime coupling.
    assert!(old.observation().is_same_binding_as(&old_observation));
    assert_eq!(old.use_tones()[0].per_case_bits[0], old_tone);
    assert_eq!(
        old.point_evidence().0.preferences[0].visible_lab,
        old_measurement
    );
    assert_eq!(
        old.emitted()
            .expect("the previous observation still has its emitted tuple"),
        [Srgb8::new([80, 89, 109])]
    );
    assert!(new.observation().is_same_binding_as(incoming));
    assert_eq!(new.use_tones()[0].occurrence, Some(OccurrenceId::new(5)));
    assert_eq!(
        new.observation().revision().value(),
        request.expected_revision
    );
    INCOMING_RECEIPTS.with(|values| {
        values.borrow_mut().push(IncomingReceiptV1 {
            observation: new.observation().clone(),
            emitted: new
                .emitted()
                .expect("the incoming observation emitted a fresh tuple")
                .to_vec(),
            old_tone,
            new_tone: new.use_tones()[0].per_case_bits[0],
            buffers,
        })
    });
}

fn point(value: [f64; 3]) -> ContinuousSrgbV1 {
    ContinuousSrgbV1::new(value).expect("fixture specifies finite encoded sRGB channels")
}

fn source_target() -> Target {
    let mut value = ComputedTargetV1::declared(
        AssignmentDomainV1::Cube {
            lower: [0; 3],
            upper: [255; 3],
        },
        Srgb8::new([49, 85, 190]),
        2.0,
    );
    value.intent = Some(ColorUseIntentV1 {
        identity: ColorIdentityDomainV1::new(value.anchor, false, IdentityOptionsV1::default())
            .expect("the fixture uses the declared source identity"),
        appearance: Some(OccurrenceId::new(5)),
        tone: None,
    });
    value.reference_appearance = true;
    Target::new(TargetId::new(10), TargetIntentV1::Computed(value))
}

fn single() -> Program<Wcag22Srgb8V1> {
    let mut value = program(
        source_target(),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    value.reference_inputs = Some(vec![(SurfaceInputPortId::new(2), Srgb8::new([255; 3]))]);
    value
}

#[test]
fn explicit_filter_and_strict_realization_are_one_revision_bound_program_operation()
-> Result<(), String> {
    let mut p = single();
    let TargetIntentV1::Computed(value) = &mut p.targets[0].intent else {
        panic!()
    };
    value
        .intent
        .as_mut()
        .ok_or("missing declared color intent")?
        .tone = Some(ToneCurveV1::new(2, vec![[0.0, 0.6], [1.0, 0.6]]).map_err(fixture_error)?);
    let (owner, mut session) = fixture_session(p, 8)?;
    session.commit(update(3, 255)).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([80., 89., 109.])];
    let completion = [point([
        43.36346865614138,
        78.15569710108728,
        182.58830650509347,
    ])];
    let result = prepared
        .realize(&anchor, &completion, 1)
        .map_err(fixture_error)?;
    assert_eq!(
        result.admission(),
        CompletionAdmissionV1::PreferenceImproved
    );
    assert_eq!(result.ideal(), completion);
    assert_eq!(
        result.emitted(),
        Some([Srgb8::new([43, 78, 183])].as_slice())
    );
    assert_eq!(
        result.continuous_optimality(),
        ComputedOptimalityV1::Unfinished
    );
    assert!(!result.rgb8_search_exhausted());
    assert_eq!(result.visited(), 1);
    assert!(result.binds_owner(&owner));
    assert_eq!(result.observation().revision().value(), 3);
    assert_eq!(result.model(), CERTIFIED_NUMERIC_MODEL_V1);
    assert!(
        result.component_admissions()[0]
            .preference_drift_upper
            .ok_or("improved component has no preference drift bound")?
            < 0.0
    );
    assert!(!result.numerical_order_unresolved());
    let (a, b) = result.point_evidence();
    let b = b.ok_or("missing completion evidence to compare with the anchor")?;
    let row = &a.preferences[0];
    assert_eq!(row.target, TargetId::new(10));
    assert_eq!(row.occurrence, Some(OccurrenceId::new(5)));
    assert_eq!(row.case, 0);
    assert!(row.backdrop_lab.is_some());
    let reference = row
        .reference
        .as_ref()
        .ok_or("missing reference for the declared occurrence")?;
    let preferred = row.preferred.ok_or("missing preferred appearance")?;
    assert_eq!(
        preferred[0],
        CertifiedScalarV1::exact(0.6)
            .map_err(fixture_error)?
            .bounds()
    );
    assert_eq!(preferred[1..], reference.visible_lab[1..]);
    assert_eq!(
        reference.backdrop_lab,
        row.backdrop_lab
            .ok_or("missing observed backdrop measurement")?
    );
    assert!(reference.contrast.cell.is_ok());
    assert_ne!(row.visible_lab, b.preferences[0].visible_lab);
    assert_ne!(
        row.distance
            .as_ref()
            .ok_or("point has no certified preference distance")?
            .bounds,
        b.preferences[0]
            .distance
            .as_ref()
            .ok_or("point has no certified preference distance")?
            .bounds
    );
    assert_eq!(result.component_admissions()[0].component, 0);
    assert_eq!(a.source_profile.len(), 1);
    assert_eq!(result.proposals(), (&anchor[..], &completion[..]));
    assert_eq!(
        result.use_tones()[0].per_case_bits[0],
        Some(0.6f64.to_bits())
    );
    Ok(())
}

#[test]
fn changed_implicit_reference_preserves_anchor_before_rgb8_realization() -> Result<(), String> {
    let (owner, mut session) = fixture_session(single(), 4)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([80., 89., 109.])];
    let result = prepared
        .realize(&anchor, &[point([49., 85., 190.])], 1)
        .map_err(fixture_error)?;
    assert_eq!(
        result.admission(),
        CompletionAdmissionV1::AnchorPreserved(CompletionRejectionV1::ImplicitReferenceUnresolved)
    );
    assert_eq!(result.ideal(), anchor);
    assert_eq!(
        result.emitted(),
        Some([Srgb8::new([80, 89, 109])].as_slice())
    );
    Ok(())
}

const SHARED_INK_PANELS: [([u8; 3], f64); 2] = [([234, 221, 234], 0.7), ([211, 233, 221], 0.65)];

fn shared_ink() -> Program<Wcag22Srgb8V1> {
    let mut p = single();
    if let TargetIntentV1::Computed(value) = &mut p.targets[0].intent {
        value
            .intent
            .as_mut()
            .expect("shared ink is declared with an appearance intent")
            .appearance = Some(OccurrenceId::new(21));
    }
    p.occurrences.clear();
    p.constraints = ConstraintSet::new(vec![], vec![]);
    let context = single().occurrences[0].context;
    for (index, (rgb, opacity)) in SHARED_INK_PANELS.into_iter().enumerate() {
        let id = 20 + index as u32 * 10;
        p.sources.push(Source::new(
            SourceId::new(id),
            ColorSignal::from_srgb8(Srgb8::new(rgb)),
        ));
        p.targets
            .push(Target::fixed(TargetId::new(id), SourceId::new(id)));
        p.paints.push(Paint::Solid {
            id: PaintId::new(id),
            target: TargetId::new(id),
        });
        p.opacities
            .push(OpacityInput::new(OpacityInputId::new(id), opacity));
        p.paints.push(Paint::Opacity {
            id: PaintId::new(id + 1),
            source: PaintId::new(id),
            opacity: OpacityInputId::new(id),
        });
        p.occurrences.push(Occurrence::new(
            OccurrenceId::new(id),
            PaintId::new(id + 1),
            SurfaceId::new(4),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context,
        ));
        p.surfaces.push(Surface::FromOccurrence {
            id: SurfaceId::new(id),
            occurrence: OccurrenceId::new(id),
        });
        p.occurrences.push(Occurrence::new(
            OccurrenceId::new(id + 1),
            PaintId::new(3),
            SurfaceId::new(id),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context,
        ));
        p.constraints
            .hard
            .push(ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(id + 1),
                OccurrenceId::new(id + 1),
                Wcag22CriterionV1::Sc143TextDefault,
            ));
    }
    p
}

#[test]
fn shared_source_two_materials_keeps_implicit_facts_without_scalar_admission() -> Result<(), String>
{
    let (owner, mut session) = fixture_session(shared_ink(), 4)?;
    let mut input = update(7, 255);
    let ObservationPayloadInput::Scenarios(ref mut set) = input.payload else {
        panic!()
    };
    set.scenarios = [[217, 229, 250], [248, 243, 236]]
        .into_iter()
        .enumerate()
        .map(|(i, rgb)| ScenarioInput {
            id: ScenarioId::new(i as u32),
            bindings: vec![SurfaceInputBinding::new(
                SurfaceInputPortId::new(2),
                ColorSignal::from_srgb8(Srgb8::new(rgb)),
            )],
        })
        .collect();
    session.commit(input).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([80., 89., 109.])];
    let accepted = [point([
        45.55648583981751,
        80.83558199614441,
        185.48517133147033,
    ])];
    // This tuple was accepted by the historical scalar-cell convention. Its
    // archived raw evidence remains valid for that former convention only.
    let result = prepared
        .realize(&anchor, &accepted, 1)
        .map_err(fixture_error)?;
    assert_eq!(
        result.admission(),
        CompletionAdmissionV1::AnchorPreserved(CompletionRejectionV1::ImplicitReferenceUnresolved)
    );
    assert_eq!(result.point_evidence().0.preferences.len(), 2);
    for row in &result.point_evidence().0.preferences {
        assert!(row.preferred.is_none());
        assert!(row.distance.is_none());
        assert!(row.reference.is_some());
    }
    assert_eq!(
        result.emitted(),
        Some([Srgb8::new([80, 89, 109])].as_slice())
    );
    let original = [point([
        45.55648583981752,
        80.83558199614438,
        185.4851713314703,
    ])];
    let rejected = prepared
        .realize(&anchor, &original, 0)
        .map_err(fixture_error)?;
    assert_eq!(
        rejected.admission(),
        CompletionAdmissionV1::AnchorPreserved(CompletionRejectionV1::ImplicitReferenceUnresolved)
    );
    assert_eq!(rejected.ideal(), anchor);
    assert!(
        rejected.component_admissions()[0]
            .preference_drift_upper
            .is_none()
    );
    assert!(rejected.emitted().is_none());
    Ok(())
}

#[test]
fn both_adjustable_parent_and_child_are_jointly_realized_with_nested_opacity() -> Result<(), String>
{
    let mut p = program(
        target(10, 244, 242, 244, 0),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.constraints = ConstraintSet::new(vec![], vec![]);
    p.reference_inputs = Some(vec![(SurfaceInputPortId::new(2), Srgb8::new([255; 3]))]);
    p.opacities
        .push(OpacityInput::new(OpacityInputId::new(1), 0.85));
    p.opacities
        .push(OpacityInput::new(OpacityInputId::new(2), 0.5));
    p.paints.push(Paint::Opacity {
        id: PaintId::new(11),
        source: PaintId::new(3),
        opacity: OpacityInputId::new(1),
    });
    p.occurrences[0].subject = PaintId::new(11);
    p.targets.push(target(20, 1, 1, 3, 1));
    p.paints.push(Paint::Solid {
        id: PaintId::new(20),
        target: TargetId::new(20),
    });
    p.paints.push(Paint::Opacity {
        id: PaintId::new(21),
        source: PaintId::new(20),
        opacity: OpacityInputId::new(2),
    });
    p.surfaces.push(Surface::FromOccurrence {
        id: SurfaceId::new(20),
        occurrence: OccurrenceId::new(5),
    });
    p.occurrences.push(Occurrence::new(
        OccurrenceId::new(25),
        PaintId::new(21),
        SurfaceId::new(20),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    ));
    p.outputs
        .push(OutputBinding::new(OutputSlotId::new(20), PaintId::new(21)));
    for (index, occurrence) in [(0, 5), (1, 25)] {
        let TargetIntentV1::Computed(value) = &mut p.targets[index].intent else {
            panic!()
        };
        value.intent = Some(ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(value.anchor, false, IdentityOptionsV1::default())
                .map_err(fixture_error)?,
            appearance: Some(OccurrenceId::new(occurrence)),
            tone: None,
        });
        value.reference_appearance = true;
    }
    let (owner, mut session) = fixture_session(p, 20)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([242.25; 3]), point([2.75; 3])];
    let result = prepared
        .realize(&anchor, &anchor, 20)
        .map_err(fixture_error)?;
    assert_eq!(
        result.emitted(),
        Some([Srgb8::new([242; 3]), Srgb8::new([3; 3])].as_slice())
    );
    assert!(result.rgb8_search_exhausted());
    assert_eq!(result.visited(), 9); // the complete 3Р“вЂ”3 tuple domain, without a duplicate hint
    assert_eq!(
        result.continuous_optimality(),
        ComputedOptimalityV1::Unfinished
    );
    Ok(())
}

#[test]
fn hard_failure_of_rounded_tuple_is_not_emitted_or_called_conflict() -> Result<(), String> {
    let (owner, mut session) = fixture_session(single(), 1)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([240.; 3])];
    let result = prepared
        .realize(&anchor, &anchor, 1)
        .map_err(fixture_error)?;
    assert!(result.emitted().is_none());
    assert!(!result.rgb8_search_exhausted());
    assert_eq!(result.visited(), 1);
    Ok(())
}

#[test]
fn wrong_program_generation_is_rejected() -> Result<(), String> {
    let owner = compile_assignment(single(), 1).map_err(fixture_error)?;
    let other = compile_assignment(single(), 1).map_err(fixture_error)?;
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(fixture_error)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    assert!(matches!(
        other.prepare_reference_completion(&session),
        Err(ReferenceCompletionErrorV1::OwnerMismatch)
    ));
    Ok(())
}

#[test]
fn immutable_result_outlives_preparation_and_does_not_borrow_the_mutable_session()
-> Result<(), String> {
    let (owner, mut session) = fixture_session(single(), 1)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([80., 89., 109.])];
    let old = prepared
        .realize(&anchor, &anchor, 1)
        .map_err(fixture_error)?;
    drop(prepared);
    session.commit(update(2, 250)).map_err(fixture_error)?;
    assert_eq!(old.observation().revision().value(), 1);
    assert_eq!(old.ideal(), anchor);
    assert_eq!(old.emitted(), Some([Srgb8::new([80, 89, 109])].as_slice()));
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let new = prepared
        .realize(&anchor, &anchor, 1)
        .map_err(fixture_error)?;
    assert_eq!(new.observation().revision().value(), 2);
    assert!(!old.observation().is_same_binding_as(new.observation()));
    assert!(old.binds_owner(&owner) && new.binds_owner(&owner));
    Ok(())
}

#[test]
fn binding_failures_keep_their_typed_cause() {
    let cause: ReferenceCompletionErrorV1<()> = BindingError::IncompatibleEvaluationCone.into();
    assert!(matches!(
        cause.binding_error(),
        Some(BindingError::IncompatibleEvaluationCone)
    ));
}

#[test]
fn declared_tone_is_derived_from_each_actual_context() -> Result<(), String> {
    let mut p = single();
    let TargetIntentV1::Computed(value) = &mut p.targets[0].intent else {
        panic!()
    };
    value
        .intent
        .as_mut()
        .ok_or("missing declared color intent")?
        .tone = Some(ToneCurveV1::new(2, vec![[0.0, 0.8], [1.0, 0.2]]).map_err(fixture_error)?);
    let (owner, mut session) = fixture_session(p, 1)?;
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
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([80., 89., 109.])];
    let result = prepared
        .realize(&anchor, &anchor, 0)
        .map_err(fixture_error)?;
    let tones = &result.use_tones()[0].per_case_bits;
    let curve = ToneCurveV1::new(2, vec![[0.0, 0.8], [1.0, 0.2]]).map_err(fixture_error)?;
    for (case, tone) in tones.iter().enumerate() {
        let backdrop = result
            .observation()
            .physical_values(case)
            .ok_or("reported case has no observed backdrop")?[0]
            .srgb8();
        assert_eq!(*tone, Some(curve.preferred_lightness(backdrop).to_bits()));
    }
    assert_ne!(tones[0], tones[1]);
    Ok(())
}

#[test]
fn declared_field_is_not_lifted_from_its_deduplicated_byte_support() -> Result<(), String> {
    let owner = compile_assignment(single(), 1).map_err(fixture_error)?;
    let field = crate::program_wire::ProgramDeclaredFieldV1::from_rgb8(
        &[0; 3],
        0,
        Srgb8::new([254; 3]),
        Srgb8::new([255; 3]),
    )
    .map_err(fixture_error)?;
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(fixture_error)?;
    session
        .prepare_schema_ordered_field(crate::observation::Revision::new(1), &field)
        .map_err(fixture_error)?
        .commit();
    assert!(matches!(
        owner.prepare_reference_completion(&session),
        Err(ReferenceCompletionErrorV1::UnsupportedDeclaredField)
    ));
    Ok(())
}

#[test]
fn private_program_consumer_repairs_full_color_nested_tuple() -> Result<(), String> {
    let ink_id = TargetId::new(10);
    let panel_id = TargetId::new(20);
    let mut p = shared_ink();
    let parent = p
        .targets
        .iter_mut()
        .find(|target| target.id == panel_id)
        .ok_or("nested program has no adjustable panel")?;
    let [(panel_rgb, panel_opacity), fixed_panel] = SHARED_INK_PANELS;
    let source = Srgb8::new(panel_rgb);
    let mut declaration = ComputedTargetV1::color(
        source,
        2.0,
        ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(source, false, IdentityOptionsV1::default())
                .map_err(fixture_error)?,
            appearance: Some(OccurrenceId::new(20)),
            tone: None,
        },
    );
    declaration.reference_appearance = true;
    declaration.preference = 1;
    parent.intent = TargetIntentV1::Computed(declaration);
    let (owner, mut session) = fixture_session(p, 128)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let passes = |ink: [u8; 3], panel: [u8; 3]| {
        [(panel, panel_opacity), fixed_panel]
            .into_iter()
            .all(|(paint, opacity)| {
                let backdrop = paint.map(|channel| {
                    crate::composition::source_over_channel_srgb8(channel, opacity, 255)
                });
                matches!(
                    evaluate_wcag22_srgb8(ink, backdrop, Wcag22CriterionV1::Sc143TextDefault)
                        .expect("sRGB8 pair admits WCAG evaluation"),
                    Wcag22AssessmentV1::Evaluated {
                        decision: Wcag22ApplicableDecisionV1::Pass,
                        ..
                    }
                )
            })
    };
    let green = (0..254)
        .find(|&green| {
            passes([49, green, 190], source.bytes())
                && !passes([49, green + 1, 190], source.bytes())
        })
        .ok_or("fixture contains no adjacent green-channel contrast boundary")?;
    let probe = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor: Vec<_> = probe
        .sources()
        .map(|(id, source)| {
            if id == ink_id {
                point([49., f64::from(green) + 0.51, 190.])
            } else {
                point(source.bytes().map(|channel| f64::from(channel) + 0.25))
            }
        })
        .collect();
    let ink_index = probe
        .sources()
        .position(|(id, _)| id == ink_id)
        .ok_or("prepared sources omit the declared ink")?;
    let panel_index = probe
        .sources()
        .position(|(id, _)| id == panel_id)
        .ok_or("prepared sources omit the adjustable panel")?;
    drop(probe);
    let cold_start = std::time::Instant::now();
    let (cold, cold_allocations) = crate::test_support::measured_allocator_events(|| {
        let mut prepared = owner
            .prepare_reference_completion(&session)
            .map_err(fixture_error)?;
        let result = prepared
            .realize(&anchor, &anchor, 128)
            .map_err(fixture_error)?;
        Ok::<_, String>((
            prepared,
            result,
            crate::test_support::current_allocator_bytes(),
        ))
    });
    let (mut prepared, result, cold_bytes) = cold?;
    let cold_elapsed = cold_start.elapsed();
    let emitted = result
        .emitted()
        .expect("the joint neighborhood must repair this whole tuple");
    assert_eq!(emitted.len(), 2);
    assert!(
        passes(emitted[ink_index].bytes(), emitted[panel_index].bytes()),
        "emitted ink must pass on both composed surfaces"
    );
    assert!(
        matches!(owner.owner_generation.target_selection, CompiledTargetSelectionV1::Computed(ref selection) if selection.targets.iter().all(|target| target.declaration.domain == AssignmentDomainV1::Cube { lower: [0;3], upper: [255;3] }))
    );
    assert!(!result.rgb8_search_exhausted());
    assert!(result.visited() <= 128);
    assert_eq!(result.point_filter_evaluations(), 2);
    assert!(result.realization_color_evaluations() < result.visited());
    let start = std::time::Instant::now();
    let (warm, warm_allocations) = crate::test_support::measured_allocator_events(|| {
        let result = prepared
            .realize(&anchor, &anchor, 128)
            .map_err(fixture_error)?;
        Ok::<_, String>((result, crate::test_support::current_allocator_bytes()))
    });
    let (warm, warm_bytes) = warm?;
    assert_eq!(warm.emitted(), result.emitted());
    eprintln!(
        "private Program full-color joint repair: tuple={emitted:?}, visits={}, color evaluations={}, filter evaluations={}, cold={cold_elapsed:?}, warm={:?}, cold allocations={cold_allocations:?}, warm allocations={warm_allocations:?}, cold bytes={cold_bytes:?}, warm bytes={warm_bytes:?}",
        result.visited(),
        result.realization_color_evaluations(),
        result.point_filter_evaluations(),
        start.elapsed()
    );
    Ok(())
}

fn independent_reference_program(additional: usize) -> Program<Wcag22Srgb8V1> {
    let mut p = single();
    let TargetIntentV1::Computed(first) = &mut p.targets[0].intent else {
        panic!()
    };
    first.preference = 0;
    for (index, source) in [Srgb8::new([88, 64, 136]), Srgb8::new([58, 109, 83])]
        .into_iter()
        .take(additional)
        .enumerate()
    {
        let id = 20 + index as u32 * 10;
        let mut declaration = ComputedTargetV1::color(
            source,
            2.0,
            ColorUseIntentV1 {
                identity: ColorIdentityDomainV1::new(source, false, IdentityOptionsV1::default())
                    .expect("the fixture uses the declared source identity"),
                appearance: Some(OccurrenceId::new(id + 5)),
                tone: None,
            },
        );
        declaration.reference_appearance = true;
        declaration.preference = index as u64 + 1;
        p.targets.push(Target::new(
            TargetId::new(id),
            TargetIntentV1::Computed(declaration),
        ));
        p.paints.push(Paint::Solid {
            id: PaintId::new(id + 3),
            target: TargetId::new(id),
        });
        p.occurrences.push(Occurrence::new(
            OccurrenceId::new(id + 5),
            PaintId::new(id + 3),
            SurfaceId::new(4),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            p.occurrences[0].context,
        ));
        p.constraints
            .hard
            .push(ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(id + 6),
                OccurrenceId::new(id + 5),
                Wcag22CriterionV1::Sc143TextDefault,
            ));
        p.outputs.push(OutputBinding::new(
            OutputSlotId::new(id + 7),
            PaintId::new(id + 3),
        ));
    }
    p
}

#[test]
fn independent_point_admission_visits_each_case_target_once() -> Result<(), String> {
    for additional in 0..=2 {
        let mut p = independent_reference_program(additional);
        p.reference_inputs = None;
        for target in &mut p.targets {
            let TargetIntentV1::Computed(declaration) = &mut target.intent else {
                panic!()
            };
            declaration.reference_appearance = false;
        }
        let owner = compile_assignment(p, 1).map_err(fixture_error)?;
        let CompiledTargetSelectionV1::Computed(selection) =
            &owner.owner_generation.target_selection
        else {
            panic!()
        };
        assert_eq!(selection.components.len(), additional + 1);
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
                ColorSignal::from_srgb8(Srgb8::new([250; 3])),
            )],
        });
        session.commit(input).map_err(fixture_error)?;
        let mut prepared = owner
            .prepare_reference_completion(&session)
            .map_err(fixture_error)?;
        let proposal: Vec<_> = prepared
            .sources()
            .map(|(_, source)| ContinuousSrgbV1::from_srgb8(source))
            .collect();
        let anchor: Vec<_> = proposal
            .iter()
            .map(|source| {
                let mut channels = source.channels();
                channels[0] += 1.0;
                point(channels)
            })
            .collect();
        PREFERENCE_ROW_VISITS.with(|value| value.set(0));
        let result = prepared
            .realize(&anchor, &proposal, 0)
            .map_err(fixture_error)?;
        assert_eq!(
            result.admission(),
            CompletionAdmissionV1::PreferenceImproved
        );
        assert_eq!(result.ideal(), proposal);
        assert_eq!(result.observation().physical_case_count(), 2);
        assert_eq!(
            PREFERENCE_ROW_VISITS.with(core::cell::Cell::get),
            2 * proposal.len(),
            "independent components must not rescan other targets' preference rows"
        );
    }
    Ok(())
}

#[test]
fn independent_full_rgb_components_do_not_starve_first_text_repair() -> Result<(), String> {
    let mut singleton_output = None;
    for additional in [0, 2] {
        let owner = compile_assignment(independent_reference_program(additional), 128)
            .map_err(fixture_error)?;
        let CompiledTargetSelectionV1::Computed(selection) =
            &owner.owner_generation.target_selection
        else {
            panic!()
        };
        assert_eq!(selection.components.len(), additional + 1);
        assert!(
            selection
                .components
                .iter()
                .all(|component| component.targets.len() == 1)
        );
        assert!(
            selection
                .components
                .iter()
                .all(|component| component.inputs.len() == 1)
        );
        assert!(selection.fixed_constraints.is_empty());
        assert_eq!(
            selection.targets[0].declaration.anchor,
            Srgb8::new([49, 85, 190])
        );
        assert!(
            selection.targets[0]
                .declaration
                .intent
                .as_ref()
                .ok_or("fixture has no declared source identity")?
                .identity
                .contains(Srgb8::new([49, 122, 190]))
        );
        assert!(matches!(
            evaluate_wcag22_srgb8(
                [49, 123, 190],
                [255; 3],
                Wcag22CriterionV1::Sc143TextDefault
            )
            .expect("sRGB8 pair admits WCAG evaluation"),
            Wcag22AssessmentV1::Evaluated {
                decision: Wcag22ApplicableDecisionV1::Fail,
                ..
            }
        ));
        assert!(matches!(
            evaluate_wcag22_srgb8(
                [49, 122, 190],
                [255; 3],
                Wcag22CriterionV1::Sc143TextDefault
            )
            .expect("sRGB8 pair admits WCAG evaluation"),
            Wcag22AssessmentV1::Evaluated {
                decision: Wcag22ApplicableDecisionV1::Pass,
                ..
            }
        ));
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .map_err(fixture_error)?;
        session.commit(update(1, 255)).map_err(fixture_error)?;
        let mut prepared = owner
            .prepare_reference_completion(&session)
            .map_err(fixture_error)?;
        let anchor: Vec<_> = prepared
            .sources()
            .enumerate()
            .map(|(index, (_, source))| {
                if index == 0 {
                    point([49., 122.51, 190.])
                } else {
                    ContinuousSrgbV1::from_srgb8(source)
                }
            })
            .collect();
        let result = prepared
            .realize(&anchor, &anchor, 2)
            .map_err(fixture_error)?;
        let emitted = result
            .emitted()
            .expect("two local visits repair the first text even with independent full RGB cubes");
        assert_eq!(emitted[0], Srgb8::new([49, 122, 190]));
        assert_eq!(result.visited(), 2 * (additional as u64 + 1));
        assert!(!result.rgb8_search_exhausted());
        assert_eq!(
            result.continuous_optimality(),
            ComputedOptimalityV1::Unfinished
        );
        if let Some(previous) = singleton_output {
            assert_eq!(emitted[0], previous);
        } else {
            singleton_output = Some(emitted[0]);
        }
        if additional == 2 {
            // Old whole-Program traversal spent 27Р“вЂ”27 suffix combinations on
            // the failing first color; even its full 384-visit budget missed
            // the next first-color proposal. Every domain remains the full cube.
            assert!(
                selection
                    .targets
                    .iter()
                    .all(|target| target.declaration.domain
                        == AssignmentDomainV1::Cube {
                            lower: [0; 3],
                            upper: [255; 3]
                        })
            );
            let generous = prepared
                .realize(&anchor, &anchor, 384)
                .map_err(fixture_error)?;
            assert!(generous.emitted().is_some());
            assert_eq!(generous.visited(), 384);
        }
    }
    Ok(())
}

#[test]
fn hard_related_paints_keep_their_joint_component() -> Result<(), String> {
    let mut p = independent_reference_program(2);
    p.constraints
        .hard
        .push(ConstraintInvocation::preserve_tonal_hierarchy(
            ConstraintId::new(90),
            vec![OccurrenceId::new(25), OccurrenceId::new(5)],
        ));
    let owner = compile_assignment(p, 2).map_err(fixture_error)?;
    let CompiledTargetSelectionV1::Computed(selection) = &owner.owner_generation.target_selection
    else {
        panic!()
    };
    let mut sizes: Vec<_> = selection
        .components
        .iter()
        .map(|component| component.targets.len())
        .collect();
    sizes.sort_unstable();
    assert_eq!(sizes, [1, 2]);
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(fixture_error)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let sources: Vec<_> = prepared.sources().map(|(_, source)| source).collect();
    let anchor: Vec<_> = sources
        .iter()
        .copied()
        .map(ContinuousSrgbV1::from_srgb8)
        .collect();
    let result = prepared
        .realize(&anchor, &anchor, 2)
        .map_err(fixture_error)?;
    assert_eq!(result.emitted(), Some(sources.as_slice()));
    assert_eq!(result.visited(), 4);
    assert!(!result.rgb8_search_exhausted());
    Ok(())
}

#[test]
fn failed_fixed_constraint_prevents_emitting_independent_valid_colors() -> Result<(), String> {
    let mut p = independent_reference_program(2);
    p.sources.push(Source::new(
        SourceId::new(90),
        ColorSignal::from_srgb8(Srgb8::new([255; 3])),
    ));
    p.targets
        .push(Target::fixed(TargetId::new(90), SourceId::new(90)));
    p.paints.push(Paint::Solid {
        id: PaintId::new(93),
        target: TargetId::new(90),
    });
    p.occurrences.push(Occurrence::new(
        OccurrenceId::new(95),
        PaintId::new(93),
        SurfaceId::new(4),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    ));
    p.constraints
        .hard
        .push(ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(96),
            OccurrenceId::new(95),
            Wcag22CriterionV1::Sc143TextDefault,
        ));
    let owner = compile_assignment(p, 2).map_err(fixture_error)?;
    let CompiledTargetSelectionV1::Computed(selection) = &owner.owner_generation.target_selection
    else {
        panic!()
    };
    assert_eq!(selection.fixed_constraints.len(), 1);
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(fixture_error)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor: Vec<_> = prepared
        .sources()
        .map(|(_, source)| ContinuousSrgbV1::from_srgb8(source))
        .collect();
    let result = prepared
        .realize(&anchor, &anchor, 2)
        .map_err(fixture_error)?;
    assert!(result.emitted().is_none());
    assert_eq!(result.visited(), 1);
    Ok(())
}

#[test]
fn private_program_consumer_repairs_three_independent_full_rgb_components() -> Result<(), String> {
    let (owner, mut session) = fixture_session(independent_reference_program(2), 128)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let probe = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor: Vec<_> = probe
        .sources()
        .map(|(_, source)| {
            if source == Srgb8::new([49, 85, 190]) {
                point([49., 122.51, 190.])
            } else {
                ContinuousSrgbV1::from_srgb8(source)
            }
        })
        .collect();
    drop(probe);
    let start = std::time::Instant::now();
    let (measured, cold_allocations) = crate::test_support::measured_allocator_events(|| {
        let mut prepared = owner
            .prepare_reference_completion(&session)
            .map_err(fixture_error)?;
        let result = prepared
            .realize(&anchor, &anchor, 2)
            .map_err(fixture_error)?;
        Ok::<_, String>((
            prepared,
            result,
            crate::test_support::current_allocator_bytes(),
        ))
    });
    let (mut prepared, cold, cold_bytes) = measured?;
    let cold_elapsed = start.elapsed();
    let start = std::time::Instant::now();
    let (warm, warm_allocations) = crate::test_support::measured_allocator_events(|| {
        let result = prepared
            .realize(&anchor, &anchor, 2)
            .map_err(fixture_error)?;
        Ok::<_, String>((result, crate::test_support::current_allocator_bytes()))
    });
    let (warm, warm_bytes) = warm?;
    let warm_elapsed = start.elapsed();
    let emitted = cold
        .emitted()
        .expect("each independent full-domain component gets its own two visits");
    assert_eq!(
        emitted,
        [
            Srgb8::new([49, 122, 190]),
            Srgb8::new([88, 64, 136]),
            Srgb8::new([58, 109, 83])
        ]
    );
    assert_eq!(warm.emitted(), cold.emitted());
    assert_eq!(cold.visited(), 6);
    assert!(cold.realization_color_evaluations() <= cold.visited());
    assert!(!cold.rgb8_search_exhausted());
    assert_eq!(cold.point_filter_evaluations(), 2);
    eprintln!(
        "private Program independent-component repair: tuple={emitted:?}, visits={}, color evaluations={}, filter evaluations={}, cold={cold_elapsed:?}, warm={warm_elapsed:?}, cold allocations={cold_allocations:?}, warm allocations={warm_allocations:?}, cold bytes={cold_bytes:?}, warm bytes={warm_bytes:?}",
        cold.visited(),
        cold.realization_color_evaluations(),
        cold.point_filter_evaluations()
    );
    Ok(())
}

#[test]
fn incoming_transaction_uses_new_observation_reuses_scratch_and_keeps_old_result()
-> Result<(), String> {
    let mut p = single();
    let TargetIntentV1::Computed(target) = &mut p.targets[0].intent else {
        panic!()
    };
    let curve = ToneCurveV1::new(2, vec![[0.0, 0.8], [1.0, 0.2]]).map_err(fixture_error)?;
    target
        .intent
        .as_mut()
        .ok_or("missing declared color intent")?
        .tone = Some(curve.clone());
    let (_owner, mut session) = fixture_session(p, 1)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    assert!(session.plan().reference_completion.storage.is_none());
    let crate::observation::ObservationHeadViewV1::Observed(previous) = session.raw_head() else {
        panic!()
    };
    let previous = previous.clone();
    INCOMING_RECEIPTS.with(|values| values.borrow_mut().clear());
    let updates = [(2, 250, false), (3, 245, true)];
    for (revision, backdrop, commit) in updates {
        assert_eq!(
            session.raw_head().revision(),
            Some(crate::observation::Revision::new(1))
        );
        INCOMING_REQUEST.with(|value| {
            *value.borrow_mut() = Some(IncomingRequestV1 {
                previous: previous.clone(),
                expected_revision: revision,
                expected_backdrop: Srgb8::new([backdrop; 3]),
            })
        });
        let transaction = session
            .prepare_update(update(revision, backdrop))
            .map_err(fixture_error)?;
        assert!(INCOMING_REQUEST.with(|value| value.borrow().is_none()));
        if commit {
            transaction.commit();
        } else {
            drop(transaction);
        }
    }
    assert_eq!(
        session.raw_head().revision(),
        Some(crate::observation::Revision::new(3))
    );
    let receipts = INCOMING_RECEIPTS.with(|values| core::mem::take(&mut *values.borrow_mut()));
    assert_eq!(receipts.len(), 2);

    assert_eq!(receipts[0].buffers, receipts[1].buffers);
    let tone = |gray| Some(curve.preferred_lightness(Srgb8::new([gray; 3])).to_bits());
    for (receipt, (revision, backdrop, _)) in receipts.iter().zip(updates) {
        assert_eq!(receipt.observation.revision().value(), revision);
        assert_eq!(receipt.old_tone, tone(255));
        assert_eq!(receipt.new_tone, tone(backdrop));
        assert_ne!(receipt.old_tone, receipt.new_tone);
        assert_eq!(receipt.emitted, [Srgb8::new([80, 89, 109])]);
    }
    assert_ne!(receipts[0].new_tone, receipts[1].new_tone);
    assert_eq!(previous.revision().value(), 1);
    Ok(())
}

#[test]
fn incoming_operation_rejects_equal_content_from_a_different_epoch_and_scratch()
-> Result<(), String> {
    let owner = compile_assignment(single(), 1).map_err(fixture_error)?;
    let other = compile_assignment(single(), 1).map_err(fixture_error)?;
    let mut session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(fixture_error)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let crate::observation::ObservationHeadViewV1::Observed(observation) = session.raw_head()
    else {
        panic!()
    };
    let mut scratch = ReferenceCompletionScratchV1::empty(&owner.owner_generation);
    assert!(matches!(
        prepare(&other.owner_generation, observation, &mut scratch),
        Err(ReferenceCompletionErrorV1::OwnerMismatch)
    ));
    assert!(scratch.storage.is_none());
    prepare(&owner.owner_generation, observation, &mut scratch).map_err(fixture_error)?;
    let mut other_session = other
        .instantiate(ObservationStreamId::new(8))
        .map_err(fixture_error)?;
    other_session
        .commit(update(1, 255))
        .map_err(fixture_error)?;
    let crate::observation::ObservationHeadViewV1::Observed(other_observation) =
        other_session.raw_head()
    else {
        panic!()
    };
    assert!(matches!(
        prepare(&other.owner_generation, other_observation, &mut scratch),
        Err(ReferenceCompletionErrorV1::OwnerMismatch)
    ));
    Ok(())
}

#[test]
fn rollback_after_incomplete_releases_the_last_observation_slot() -> Result<(), String> {
    let (_owner, mut session) = fixture_session(single(), 1)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    assert!(matches!(session.state(), SessionState::Ready { .. }));
    let crate::observation::ObservationHeadViewV1::Observed(previous) = session.raw_head() else {
        panic!()
    };
    let previous = previous.clone();
    session.commit(update(2, 0)).map_err(fixture_error)?;
    assert!(matches!(
        session.state(),
        SessionState::Incomplete {
            previous: Some(_),
            ..
        } | SessionState::Failed {
            previous: Some(_),
            ..
        }
    ));
    INCOMING_REQUEST.with(|value| {
        *value.borrow_mut() = Some(IncomingRequestV1 {
            previous,
            expected_revision: 3,
            expected_backdrop: Srgb8::new([250; 3]),
        })
    });
    // Ready revision 1 and current Incomplete revision 2 already retain two
    // backing slots. Revision 3 uses the sole remaining slot and is rolled back.
    drop(
        session
            .prepare_update(update(3, 250))
            .map_err(fixture_error)?,
    );
    assert_eq!(
        session.raw_head().revision(),
        Some(crate::observation::Revision::new(2))
    );
    assert!(INCOMING_REQUEST.with(|value| value.borrow().is_none()));
    // Test-owned snapshots, unlike scratch, legitimately retain their backing.
    // Drop the operation result before asking admission to reuse that slot.
    INCOMING_RECEIPTS.with(|values| values.borrow_mut().clear());
    session.commit(update(4, 255)).map_err(fixture_error)?;
    assert_eq!(
        session.raw_head().revision(),
        Some(crate::observation::Revision::new(4))
    );
    assert!(matches!(session.state(), SessionState::Ready { .. }));
    Ok(())
}

#[test]
fn full_preferred_point_improvement_works_without_reference() -> Result<(), String> {
    let mut p = single();
    p.reference_inputs = None;
    let TargetIntentV1::Computed(target) = &mut p.targets[0].intent else {
        panic!()
    };
    target.reference_appearance = false;
    let (owner, mut session) = fixture_session(p, 1)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([80., 89., 109.])];
    let better = [point([49., 85., 190.])];
    let result = prepared
        .realize(&anchor, &better, 1)
        .map_err(fixture_error)?;
    assert_eq!(
        result.admission(),
        CompletionAdmissionV1::PreferenceImproved
    );
    assert_eq!(result.ideal(), better);
    assert_eq!(
        result.emitted(),
        Some([Srgb8::new([49, 85, 190])].as_slice())
    );
    Ok(())
}

#[test]
fn same_full_distance_rounding_cell_cannot_authorize_worsening() -> Result<(), String> {
    let mut p = single();
    let mut target = ComputedTargetV1::declared(
        AssignmentDomainV1::Gray {
            lower: 0,
            upper: 255,
        },
        Srgb8::new([32; 3]),
        2.0,
    );
    target.intent = Some(ColorUseIntentV1 {
        identity: ColorIdentityDomainV1::new(
            Srgb8::new([32; 3]),
            true,
            IdentityOptionsV1::default(),
        )
        .map_err(fixture_error)?,
        appearance: Some(OccurrenceId::new(5)),
        tone: Some(ToneCurveV1::new(2, vec![[0.0, 0.9], [1.0, 0.9]]).map_err(fixture_error)?),
    });
    target.reference_appearance = true;
    p.targets[0] = Target::new(TargetId::new(10), TargetIntentV1::Computed(target));
    let (owner, mut session) = fixture_session(p, 1)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([52.0; 3])];
    let worse = [point([f64::from_bits(0x4049_ffff_ffff_ffff); 3])];
    let source = CertifiedRgbV1::from_encoded([32.0; 3])
        .map_err(fixture_error)?
        .oklab()
        .map_err(fixture_error)?;
    let preferred = [
        CertifiedScalarV1::exact(0.9).map_err(fixture_error)?,
        source[1],
        source[2],
    ];
    let a = CertifiedRgbV1::from_encoded(anchor[0].channels())
        .map_err(fixture_error)?
        .oklab()
        .map_err(fixture_error)?;
    let b = CertifiedRgbV1::from_encoded(worse[0].channels())
        .map_err(fixture_error)?
        .oklab()
        .map_err(fixture_error)?;
    let da = source_distance_squared(a, preferred).map_err(fixture_error)?;
    let db = source_distance_squared(b, preferred).map_err(fixture_error)?;
    assert_eq!(
        da.nearest_even_cell().map_err(fixture_error)?,
        db.nearest_even_cell().map_err(fixture_error)?
    );
    assert!(da.strictly_below(db));
    assert!(
        source_distance_squared(b, source)
            .map_err(fixture_error)?
            .strictly_below(source_distance_squared(a, source).map_err(fixture_error)?)
    );
    let result = prepared
        .realize(&anchor, &worse, 1)
        .map_err(fixture_error)?;
    assert_eq!(
        result.admission(),
        CompletionAdmissionV1::AnchorPreserved(CompletionRejectionV1::PreferenceWorsened)
    );
    let (a, b) = result.point_evidence();
    assert_eq!(
        a.preferences[0]
            .distance
            .as_ref()
            .ok_or("point has no certified preference distance")?
            .cell
            .map_err(fixture_error)?
            .cell_bits,
        b.ok_or("missing completion evidence to compare with the anchor")?
            .preferences[0]
            .distance
            .as_ref()
            .ok_or("point has no certified preference distance")?
            .cell
            .map_err(fixture_error)?
            .cell_bits
    );
    assert_eq!(
        result.ideal(),
        anchor,
        "a source improvement cannot override a proven full preference worsening"
    );
    Ok(())
}

#[test]
fn independent_point_improves_while_changed_implicit_component_preserves_anchor()
-> Result<(), String> {
    for connected in [false, true] {
        let mut p = independent_reference_program(1);
        let TargetIntentV1::Computed(point_target) = &mut p.targets[1].intent else {
            panic!()
        };
        point_target.reference_appearance = false;
        if connected {
            p.constraints
                .hard
                .push(ConstraintInvocation::preserve_tonal_hierarchy(
                    ConstraintId::new(90),
                    vec![OccurrenceId::new(25), OccurrenceId::new(5)],
                ));
        }
        let (owner, mut session) = fixture_session(p, 2)?;
        session.commit(update(1, 255)).map_err(fixture_error)?;
        let mut prepared = owner
            .prepare_reference_completion(&session)
            .map_err(fixture_error)?;
        let anchor = [point([49., 85., 190.]), point([90., 64., 136.])];
        let proposed = [point([49., 90., 190.]), point([88., 64., 136.])];
        let result = prepared
            .realize(&anchor, &proposed, 2)
            .map_err(fixture_error)?;
        if connected {
            assert_eq!(result.component_admissions().len(), 1);
            assert_eq!(
                result.admission(),
                CompletionAdmissionV1::AnchorPreserved(
                    CompletionRejectionV1::ImplicitReferenceUnresolved
                )
            );
            assert_eq!(result.ideal(), anchor);
        } else {
            assert_eq!(
                result.admission(),
                CompletionAdmissionV1::PartiallyAccepted {
                    accepted: 1,
                    preserved: 1
                }
            );
            assert_eq!(result.ideal(), [anchor[0], proposed[1]]);
            assert_eq!(
                result.emitted(),
                Some([Srgb8::new([49, 85, 190]), Srgb8::new([88, 64, 136])].as_slice())
            );
            assert!(
                result
                    .component_admissions()
                    .iter()
                    .any(|row| row.admission == CompletionAdmissionV1::PreferenceImproved)
            );
            assert!(result.component_admissions().iter().any(|row| row.admission
                == CompletionAdmissionV1::AnchorPreserved(
                    CompletionRejectionV1::ImplicitReferenceUnresolved
                )));
        }
    }
    Ok(())
}

fn invisible_black_source() -> Program<Wcag22Srgb8V1> {
    let mut p = single();
    p.reference_inputs = None;
    p.constraints = ConstraintSet::new(vec![], vec![]);
    let mut target = ComputedTargetV1::declared(
        AssignmentDomainV1::Gray {
            lower: 0,
            upper: 255,
        },
        Srgb8::new([0; 3]),
        2.0,
    );
    target.intent = Some(ColorUseIntentV1 {
        identity: ColorIdentityDomainV1::new(
            Srgb8::new([0; 3]),
            true,
            IdentityOptionsV1::default(),
        )
        .expect("the fixture uses the declared source identity"),
        appearance: Some(OccurrenceId::new(5)),
        tone: None,
    });
    p.targets[0] = Target::new(TargetId::new(10), TargetIntentV1::Computed(target));
    p.opacities
        .push(OpacityInput::new(OpacityInputId::new(33), 0.0));
    p.paints.push(Paint::Opacity {
        id: PaintId::new(33),
        source: PaintId::new(3),
        opacity: OpacityInputId::new(33),
    });
    p.occurrences[0] = Occurrence::new(
        OccurrenceId::new(5),
        PaintId::new(33),
        SurfaceId::new(4),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    );
    p
}

fn authored_material_minimum(
    source: [u8; 3],
    alpha: f64,
    backgrounds: &[[u8; 3]],
    tone: Option<f64>,
) -> Result<bool, String> {
    let mut p = invisible_black_source();
    p.opacities[0] = OpacityInput::new(OpacityInputId::new(33), alpha);
    let anchor = Srgb8::new(source);
    let mut target = ComputedTargetV1::declared(
        AssignmentDomainV1::Cube {
            lower: [0; 3],
            upper: [255; 3],
        },
        anchor,
        2.0,
    );
    target.intent = Some(ColorUseIntentV1 {
        identity: ColorIdentityDomainV1::new(anchor, false, IdentityOptionsV1::default())
            .map_err(fixture_error)?,
        appearance: Some(OccurrenceId::new(5)),
        tone: tone.map(|value| {
            ToneCurveV1::new(2, vec![[0.0, value], [1.0, value]])
                .expect("constant fixture tone is in the unit range")
        }),
    });
    p.targets[0] = Target::new(TargetId::new(10), TargetIntentV1::Computed(target));
    let (owner, mut session) = fixture_session(p, 1)?;
    let mut input = update(1, 0);
    let ObservationPayloadInput::Scenarios(set) = &mut input.payload else {
        panic!()
    };
    set.scenarios = backgrounds
        .iter()
        .enumerate()
        .map(|(index, &background)| ScenarioInput {
            id: ScenarioId::new(index as u32 + 10),
            bindings: vec![SurfaceInputBinding::new(
                SurfaceInputPortId::new(2),
                ColorSignal::from_srgb8(Srgb8::new(background)),
            )],
        })
        .collect();
    session.commit(input).map_err(fixture_error)?;
    let CompiledTargetSelectionV1::Computed(selection) = &owner.owner_generation.target_selection
    else {
        panic!()
    };
    let mut standalone = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let mut prepared = prepare(
        standalone.epoch,
        &standalone.observation,
        &mut standalone.scratch,
    )
    .map_err(fixture_error)?;
    let cases: Vec<_> = (0..backgrounds.len()).collect();
    prepared
        .authored_point_optimal(&selection.components[0], &cases)
        .map_err(fixture_error)
}

#[test]
fn authored_point_minimum_proves_transparent_and_same_background_cases() -> Result<(), String> {
    for source in [[255; 3], [0, 103, 244]] {
        for alpha in [0.0, 0.5, 1.0] {
            assert!(authored_material_minimum(source, alpha, &[source], None)?);
        }
        // Zero opacity makes every primary distance invariant even when it is
        // nonzero, including several cases and a non-authored tone preference.
        assert!(authored_material_minimum(
            source,
            0.0,
            &[[0; 3], [128; 3], [255; 3]],
            Some(0.6)
        )?);
    }
    Ok(())
}

#[test]
fn authored_point_minimum_retains_nonzero_physics_and_every_case() -> Result<(), String> {
    assert!(!authored_material_minimum(
        [128; 3],
        0.5,
        &[[128; 3], [127; 3]],
        None
    )?);
    assert!(!authored_material_minimum(
        [128; 3],
        0.5,
        &[[128; 3]],
        Some(0.1)
    )?);
    // The real visible color differs from its source although both round to
    // the same binary64 encoded cell (and therefore the same emitted byte).
    assert!(!authored_material_minimum(
        [128; 3],
        f64::from_bits(1.0_f64.to_bits() - 1),
        &[[127; 3]],
        None,
    )?);
    // Even the smallest positive material contribution is not a constant
    // expression over the source domain.
    assert!(!authored_material_minimum(
        [128; 3],
        f64::from_bits(1),
        &[[127; 3]],
        None
    )?);
    Ok(())
}

#[test]
fn source_completion_requires_proved_preference_equality() -> Result<(), String> {
    let (owner, mut session) = fixture_session(invisible_black_source(), 1)?;
    session.commit(update(1, 0)).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([52.; 3])];
    let proposed = [point([32.; 3])];
    let result = prepared
        .realize(&anchor, &proposed, 1)
        .map_err(fixture_error)?;
    assert_eq!(
        result.admission(),
        CompletionAdmissionV1::SourceImprovedWithinEquivalentPreference
    );
    assert_eq!(result.ideal(), proposed);
    assert_eq!(result.emitted(), Some([Srgb8::new([32; 3])].as_slice()));
    for evidence in [
        result.point_evidence().0,
        result
            .point_evidence()
            .1
            .ok_or("missing completion evidence to compare with the anchor")?,
    ] {
        assert_eq!(
            evidence.preferences[0]
                .distance
                .as_ref()
                .ok_or("point has no certified preference distance")?
                .bounds,
            [0, 0]
        );
    }
    Ok(())
}

#[test]
fn matching_non_singleton_enclosures_are_unresolved_without_an_equality_witness()
-> Result<(), String> {
    let (owner, mut session) = fixture_session(invisible_black_source(), 1)?;
    session.commit(update(1, 255)).map_err(fixture_error)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([52.; 3])];
    let proposed = [point([32.; 3])];
    let result = prepared
        .realize(&anchor, &proposed, 1)
        .map_err(fixture_error)?;
    let (a, b) = result.point_evidence();
    let a = a.preferences[0]
        .distance
        .as_ref()
        .ok_or("point has no certified preference distance")?;
    let b = b
        .ok_or("missing completion evidence to compare with the anchor")?
        .preferences[0]
        .distance
        .as_ref()
        .ok_or("point has no certified preference distance")?;
    assert_eq!(a.bounds, b.bounds);
    assert_ne!(a.bounds[0], a.bounds[1]);
    assert_eq!(
        result.admission(),
        CompletionAdmissionV1::AnchorPreserved(CompletionRejectionV1::PreferenceUnresolved)
    );
    assert_eq!(result.ideal(), anchor);
    Ok(())
}

#[test]
fn better_worst_case_cannot_hide_worsening_another_declared_case() -> Result<(), String> {
    let mut p = single();
    p.reference_inputs = None;
    p.constraints = ConstraintSet::new(vec![], vec![]);
    let mut target = ComputedTargetV1::declared(
        AssignmentDomainV1::Gray {
            lower: 0,
            upper: 255,
        },
        Srgb8::new([128; 3]),
        2.0,
    );
    target.intent = Some(ColorUseIntentV1 {
        identity: ColorIdentityDomainV1::new(
            Srgb8::new([128; 3]),
            true,
            IdentityOptionsV1::default(),
        )
        .map_err(fixture_error)?,
        appearance: Some(OccurrenceId::new(5)),
        tone: Some(ToneCurveV1::new(2, vec![[0.0, 0.8], [1.0, 0.2]]).map_err(fixture_error)?),
    });
    p.targets[0] = Target::new(TargetId::new(10), TargetIntentV1::Computed(target));
    let (owner, mut session) = fixture_session(p, 1)?;
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
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let anchor = [point([128.; 3])];
    let proposed = [point([110.; 3])];
    let result = prepared
        .realize(&anchor, &proposed, 1)
        .map_err(fixture_error)?;
    assert_eq!(
        result.admission(),
        CompletionAdmissionV1::AnchorPreserved(CompletionRejectionV1::PreferenceIncomparable)
    );
    assert_eq!(result.ideal(), anchor);
    assert_eq!(result.point_evidence().0.preferences.len(), 2);
    let worst = |point: &CompletionPointEvidenceV1| {
        point
            .preferences
            .iter()
            .map(|row| {
                f64::from_bits(
                    row.distance
                        .as_ref()
                        .expect("profile row has a certified distance")
                        .cell
                        .expect("profile distance has a unique rounding cell")
                        .cell_bits,
                )
            })
            .fold(0.0_f64, f64::max)
    };
    assert!(
        worst(
            result
                .point_evidence()
                .1
                .ok_or("missing completion evidence to compare with the anchor")?
        ) < worst(result.point_evidence().0)
    );
    Ok(())
}
