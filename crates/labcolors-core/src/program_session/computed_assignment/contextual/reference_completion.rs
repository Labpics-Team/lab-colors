//! Private incoming path: checked full-point preference comparison, then one
//! strict RGB8 realization. Implicit reference facts do not imply a comparator.
//!
//! A declared field's complete support covers its nominal quantized root model.
//! Independence admission proves that every objective and hard/report factor
//! depends on at most one varying input. Universal componentwise comparison
//! and feasibility therefore reduce to that input's complete marginal support.
//! Source-distance profiles are measured once per source. Global case ordinals
//! identify observation rows, never synchronized positions across fields.

use super::*;
use crate::appearance::{CertifiedAppearanceWorkspace, ContinuousSrgbV1, PointPaintValueV1};
use crate::certified_numeric::{
    CERTIFIED_NUMERIC_MODEL_V1, CertifiedNumericModelV1, CertifiedRgbV1, CertifiedScalarEvidenceV1,
    CertifiedScalarV1, NumericUncertaintyV1, sort_descending_profile, source_distance_squared,
};
use core::ops::Range;
use std::collections::BTreeMap;

mod authored_optimum;

/// Only certified componentwise improvement or proved preference equality
/// can admit a proposal. A shared rounding cell has no comparison authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionAdmissionV1 {
    PreferenceImproved,
    SourceImprovedWithinEquivalentPreference,
    PartiallyAccepted { accepted: usize, preserved: usize },
    AnchorPreserved(CompletionRejectionV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionRejectionV1 {
    PreferenceWorsened,
    PreferenceIncomparable,
    PreferenceUnresolved,
    ImplicitReferenceUnresolved,
    SourceImprovementUnresolved,
    Numeric(NumericUncertaintyV1),
}

pub(crate) struct ComponentAdmissionEvidenceV1 {
    pub(crate) component: usize,
    pub(crate) admission: CompletionAdmissionV1,
    pub(crate) preference_drift_upper: Option<f64>,
}

#[derive(Debug)]
pub(crate) enum ReferenceCompletionErrorV1<E> {
    OwnerMismatch,
    MissingFiniteObservation,
    UnsupportedDeclaredField,
    MissingReference,
    RequiresComputedTargets,
    InvalidUse,
    InvalidTuple,
    ResourceExhausted,
    Numeric(NumericUncertaintyV1),
    Binding(BindingError),
    Evaluation(ProgramSessionEvaluationError<E>),
}

impl<E> From<NumericUncertaintyV1> for ReferenceCompletionErrorV1<E> {
    fn from(value: NumericUncertaintyV1) -> Self {
        Self::Numeric(value)
    }
}
impl<E> From<BindingError> for ReferenceCompletionErrorV1<E> {
    fn from(value: BindingError) -> Self {
        Self::Binding(value)
    }
}
impl<E> ReferenceCompletionErrorV1<E> {
    pub(crate) fn binding_error(&self) -> Option<&BindingError> {
        match self {
            Self::Binding(error) => Some(error),
            _ => None,
        }
    }
}
impl<E> From<ProgramSessionEvaluationError<E>> for ReferenceCompletionErrorV1<E> {
    fn from(value: ProgramSessionEvaluationError<E>) -> Self {
        Self::Evaluation(value)
    }
}

type Error<E> = ReferenceCompletionErrorV1<ProgramEvaluatorError<E>>;

/// Search may screen interval comparisons cheaply. A screening enclosure is
/// never materialized as scalar evidence; ambiguous decisions are refined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PointMeasurementPrecisionV1 {
    Screened,
    Refined,
}

#[derive(Clone, Copy)]
pub(super) enum PointMeasurementV1 {
    Profile(PointMeasurementPrecisionV1),
    RegionWitness,
}

impl PointMeasurementPrecisionV1 {
    fn oklab(
        self,
        cache: &mut crate::certified_numeric::CertifiedOklabCacheV1,
        transforms: &mut crate::certified_numeric::PointTransformCache,
        rgb: CertifiedRgbV1,
    ) -> Result<[CertifiedScalarV1; 3], NumericUncertaintyV1> {
        match self {
            Self::Screened => cache.evaluate_screened(rgb, transforms),
            Self::Refined => cache.evaluate(rgb, transforms),
        }
    }
}

#[derive(Clone, Copy)]
struct ReferenceFactsV1 {
    visible_lab: [CertifiedScalarV1; 3],
    backdrop_lab: [CertifiedScalarV1; 3],
    contrast: CertifiedScalarV1,
}

struct UseV1 {
    slot: Option<CompiledOccurrenceSlotV1>,
    occurrence: Option<OccurrenceId>,
    target: usize,
    factor: usize,
    cases: Range<usize>,
    tone_bits: Vec<Option<u64>>,
    point_anchor: Option<[CertifiedScalarV1; 3]>,
    preferred: Vec<[CertifiedScalarV1; 3]>,
    local_preferred: Vec<[CertifiedScalarV1; 3]>,
    reference: Option<ReferenceFactsV1>,
    point_lab_cache: crate::certified_numeric::CertifiedOklabCacheV1,
}

struct PreferenceMeasurementV1 {
    target_id: TargetId,
    occurrence: Option<OccurrenceId>,
    case: usize,
    visible_lab: [CertifiedScalarV1; 3],
    backdrop_lab: Option<[CertifiedScalarV1; 3]>,
    preferred: Option<[CertifiedScalarV1; 3]>,
    distance: Option<CertifiedScalarV1>,
    reference: Option<ReferenceFactsV1>,
}

struct MeasurementsV1 {
    // Each use owns its complete support. Independent fields can have different
    // cardinalities, so a rectangular case/target matrix has no physical meaning.
    preferences: Vec<Vec<PreferenceMeasurementV1>>,
    source: Vec<CertifiedScalarV1>,
}

/// Every declared use/case retains its identity. No sort or worst-case
/// reduction can turn a redistribution of preference losses into equality.
pub(crate) struct CompletionPointEvidenceV1 {
    pub(crate) preferences: Box<[CompletionPreferenceEvidenceV1]>,
    pub(crate) source_profile: Box<[[u128; 2]]>,
}

pub(crate) struct CompletionPreferenceEvidenceV1 {
    pub(crate) target: TargetId,
    pub(crate) occurrence: Option<OccurrenceId>,
    pub(crate) case: usize,
    pub(crate) visible_lab: [[u128; 2]; 3],
    pub(crate) backdrop_lab: Option<[[u128; 2]; 3]>,
    pub(crate) preferred: Option<[[u128; 2]; 3]>,
    pub(crate) distance: Option<CompletionScalarEvidenceV1>,
    pub(crate) reference: Option<CompletionReferenceFactsV1>,
}

pub(crate) struct CompletionReferenceFactsV1 {
    pub(crate) visible_lab: [[u128; 2]; 3],
    pub(crate) backdrop_lab: [[u128; 2]; 3],
    pub(crate) contrast: CompletionScalarEvidenceV1,
}

/// Cell evidence is diagnostic only. Order always uses enclosing real bounds.
pub(crate) struct CompletionScalarEvidenceV1 {
    pub(crate) bounds: [u128; 2],
    pub(crate) cell: Result<CertifiedScalarEvidenceV1, NumericUncertaintyV1>,
}

pub(crate) struct CompletionUseToneEvidenceV1 {
    pub(crate) occurrence: Option<OccurrenceId>,
    /// Global observation ordinals corresponding to `per_case_bits` in order.
    pub(crate) cases: Range<usize>,
    pub(crate) per_case_bits: Box<[Option<u64>]>,
}

struct RealizationV1 {
    values: Option<Vec<Srgb8>>,
    search: SearchEvidenceV1,
    numerical_order_unresolved: bool,
    color_evaluations: u64,
}

/// The borrow pins the exact compiled allocation; observation retains the exact
/// admitted revision/context. Private fields prevent fabricating stage bindings.
pub(crate) struct ReferenceCompletionResultV1<'program, E: ProgramConstraintEvaluatorSetV1> {
    epoch: &'program ProgramEpochV1<E>,
    observation: RevisionBoundObservationV1,
    model: CertifiedNumericModelV1,
    use_tones: Box<[CompletionUseToneEvidenceV1]>,
    anchor: Box<[ContinuousSrgbV1]>,
    completion: Box<[ContinuousSrgbV1]>,
    ideal: Box<[ContinuousSrgbV1]>,
    admission: CompletionAdmissionV1,
    anchor_evidence: CompletionPointEvidenceV1,
    completion_evidence: Option<CompletionPointEvidenceV1>,
    component_admissions: Box<[ComponentAdmissionEvidenceV1]>,
    emitted: Option<Box<[Srgb8]>>,
    search: SearchEvidenceV1,
    numerical_order_unresolved: bool,
    realization_color_evaluations: u64,
}

impl<E: ProgramConstraintEvaluatorSetV1> ReferenceCompletionResultV1<'_, E> {
    pub(crate) fn admission(&self) -> CompletionAdmissionV1 {
        self.admission
    }
    pub(crate) fn ideal(&self) -> &[ContinuousSrgbV1] {
        &self.ideal
    }
    pub(crate) fn emitted(&self) -> Option<&[Srgb8]> {
        self.emitted.as_deref()
    }
    pub(crate) fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }
    pub(crate) fn model(&self) -> CertifiedNumericModelV1 {
        self.model
    }
    pub(crate) fn point_evidence(
        &self,
    ) -> (
        &CompletionPointEvidenceV1,
        Option<&CompletionPointEvidenceV1>,
    ) {
        (&self.anchor_evidence, self.completion_evidence.as_ref())
    }
    pub(crate) fn component_admissions(&self) -> &[ComponentAdmissionEvidenceV1] {
        &self.component_admissions
    }
    pub(crate) fn binds_owner(&self, owner: &CompiledProgram<E>) -> bool {
        core::ptr::eq(self.epoch, owner.owner_generation.as_ref())
    }
    pub(crate) fn proposals(&self) -> (&[ContinuousSrgbV1], &[ContinuousSrgbV1]) {
        (&self.anchor, &self.completion)
    }
    pub(crate) fn use_tones(&self) -> &[CompletionUseToneEvidenceV1] {
        &self.use_tones
    }
    pub(crate) fn rgb8_search_exhausted(&self) -> bool {
        self.search.completion == CompletionV1::Complete
    }
    pub(crate) fn visited(&self) -> u64 {
        self.search.visited
    }
    pub(crate) fn numerical_order_unresolved(&self) -> bool {
        self.numerical_order_unresolved
    }
    pub(crate) fn realization_color_evaluations(&self) -> u64 {
        self.realization_color_evaluations
    }
    pub(crate) fn point_filter_evaluations(&self) -> u64 {
        2
    }
    pub(crate) fn continuous_optimality(&self) -> ComputedOptimalityV1 {
        ComputedOptimalityV1::Unfinished
    }
}

/// Lazy per-Session storage. The ordinary selector never initializes it.
/// It retains no observation backing: rolled-back updates must release their
/// slot in the three-slot Session observation pool.
pub(crate) struct ReferenceCompletionScratchV1<E: ProgramConstraintEvaluatorSetV1> {
    owner_generation: Weak<ProgramEpochV1<E>>,
    storage: Option<ReferenceCompletionStorageV1>,
}

impl<E: ProgramConstraintEvaluatorSetV1> ReferenceCompletionScratchV1<E> {
    pub(in crate::program_session) fn empty(epoch: &Shared<ProgramEpochV1<E>>) -> Self {
        Self {
            owner_generation: Shared::downgrade(epoch),
            storage: None,
        }
    }
}

pub(in crate::program_session) struct CompiledPointAnchorsV1 {
    source_labs: Arc<[[CertifiedScalarV1; 3]]>,
    references: Box<[Option<ReferenceFactsV1>]>,
}

impl CompiledPointAnchorsV1 {
    pub(in crate::program_session) fn new<E: ProgramConstraintEvaluatorSetV1>(
        epoch: &ProgramEpochV1<E>,
    ) -> Result<Self, Error<E>> {
        let CompiledTargetSelectionV1::Computed(selection) = &epoch.target_selection else {
            return Err(Error::<E>::RequiresComputedTargets);
        };
        let mut certified_workspace = epoch.graph.new_certified_workspace()?;
        let bindings = &epoch.binding_template;
        let sources = bindings.paint_inputs_canonical();
        let mut source_labs = reserve::<_, E>(selection.targets.len())?;
        for target in &selection.targets {
            source_labs.push(
                CertifiedRgbV1::from_encoded(target.declaration.anchor.bytes().map(f64::from))?
                    .oklab()?,
            );
        }
        let mut references =
            buffer(selection.targets.len(), None).map_err(|_| Error::<E>::ResourceExhausted)?;
        if selection
            .targets
            .iter()
            .any(|target| target.declaration.reference_appearance)
        {
            let inputs = epoch
                .reference_inputs
                .as_ref()
                .ok_or(Error::<E>::MissingReference)?;
            let mut reference = epoch.graph.prepare_certified_surface_cases(
                None,
                bindings,
                |index| {
                    PointPaintValueV1::from_admitted(
                        CertifiedRgbV1::from_encoded(
                            sources[index].1.source().bytes().map(f64::from),
                        ),
                        sources[index].1.opacity(),
                    )
                },
                &mut certified_workspace,
            )?;
            let view = reference.evaluate_surface_inputs_canonical(|index| {
                CertifiedRgbV1::from_encoded(inputs[index].1.bytes().map(f64::from))
            });
            for (index, target) in selection.targets.iter().enumerate() {
                if !target.declaration.reference_appearance {
                    continue;
                }
                let point = view
                    .occurrence_at(target.appearance.ok_or(Error::<E>::InvalidUse)?)
                    .ok_or(Error::<E>::InvalidUse)?;
                references[index] = Some(ReferenceFactsV1 {
                    visible_lab: point.visible?.oklab()?,
                    backdrop_lab: point.backdrop?.oklab()?,
                    contrast: point.visible?.contrast(point.backdrop?)?,
                });
            }
        }
        Ok(Self {
            source_labs: source_labs.into(),
            references: references.into_boxed_slice(),
        })
    }
}

struct ReferenceCompletionStorageV1 {
    uses: Vec<UseV1>,
    bindings: AdmittedAppearanceBindings,
    certified_workspace: CertifiedAppearanceWorkspace,
    binding_targets: Vec<Option<usize>>,
    source_labs: Arc<[[CertifiedScalarV1; 3]]>,
    point_buffers: super::point_search::BuffersV1,
    point_transforms: crate::certified_numeric::PointTransformCache,
    authored_minimum_rows: Vec<bool>,
}

/// Epoch and observation authorize the operation; scratch is only borrowed
/// during preparation/execution and is never retained by immutable results.
pub(in crate::program_session) struct PreparedObservationV1<
    'program,
    'scratch,
    E: ProgramConstraintEvaluatorSetV1,
    const ALL_TONES: bool,
> {
    epoch: &'program ProgramEpochV1<E>,
    observation: RevisionBoundObservationV1,
    scratch: &'scratch mut ReferenceCompletionStorageV1,
}

// A selection fills tones only for the active component. Only the full form
// can measure or realize a complete tuple for the incoming revision.
pub(in crate::program_session) type PreparedPointSelectionV1<'p, 's, E> =
    PreparedObservationV1<'p, 's, E, false>;
type PreparedReferenceCompletionV1<'p, 's, E> = PreparedObservationV1<'p, 's, E, true>;

/// The caller passes the admitted incoming observation, before Session commit.
/// This function never reads a Session head or derives a reference from a frame.
pub(in crate::program_session) fn prepare<
    'program,
    'scratch,
    E: ProgramConstraintEvaluatorSetV1,
>(
    epoch: &'program ProgramEpochV1<E>,
    observation: &RevisionBoundObservationV1,
    scratch: &'scratch mut ReferenceCompletionScratchV1<E>,
) -> Result<PreparedReferenceCompletionV1<'program, 'scratch, E>, Error<E>> {
    let prepared = prepare_selection(epoch, observation, scratch)?;
    prepared.scratch.refresh_tones(epoch, observation)?;
    Ok(PreparedObservationV1 {
        epoch: prepared.epoch,
        observation: prepared.observation,
        scratch: prepared.scratch,
    })
}

pub(in crate::program_session) fn prepare_selection<
    'program,
    'scratch,
    E: ProgramConstraintEvaluatorSetV1,
>(
    epoch: &'program ProgramEpochV1<E>,
    observation: &RevisionBoundObservationV1,
    scratch: &'scratch mut ReferenceCompletionScratchV1<E>,
) -> Result<PreparedPointSelectionV1<'program, 'scratch, E>, Error<E>> {
    if !core::ptr::eq(scratch.owner_generation.as_ptr(), epoch)
        || !observation.shares_schema_backing_with(&epoch.observation_group.schema)
    {
        return Err(Error::<E>::OwnerMismatch);
    }
    if observation.physical_case_count() == 0 {
        return Err(Error::<E>::MissingFiniteObservation);
    }
    if scratch.storage.is_none() {
        scratch.storage = Some(ReferenceCompletionStorageV1::new(epoch)?);
    }
    let storage = scratch
        .storage
        .as_mut()
        .ok_or(Error::<E>::ResourceExhausted)?;
    Ok(PreparedObservationV1 {
        epoch,
        observation: observation.clone(),
        scratch: storage,
    })
}

/// Private operation for a prospective Session update. The epoch lease and
/// observation come from admission; existing byte scratch/arena belong to the
/// same plan. No committed head is consulted, and no result borrows the plan.
pub(in crate::program_session) fn realize_incoming<'program, E: ProgramConstraintEvaluatorSetV1>(
    plan: &mut ProgramSessionPlan<E>,
    epoch: &'program ProgramEpochV1<E>,
    observation: &RevisionBoundObservationV1,
    anchor: &[ContinuousSrgbV1],
    completion: &[ContinuousSrgbV1],
    budget: u64,
) -> Result<ReferenceCompletionResultV1<'program, E>, Error<E>> {
    if !core::ptr::eq(plan.owner_generation.as_ptr(), epoch) {
        return Err(Error::<E>::OwnerMismatch);
    }
    let mut prepared = prepare(epoch, observation, &mut plan.reference_completion)?;
    let mut arena = plan
        .evaluation_arenas
        .guard(observation.arena_slot())
        .ok_or(Error::<E>::Evaluation(
            ProgramSessionEvaluationError::InternalInvariant,
        ))?;
    let mut runtime = ProgramEvaluationRuntimeV1 {
        bindings: &mut plan.bindings,
        workspace: &mut plan.workspace,
        presentation_cache: &mut plan.presentation_cache,
        family_artifacts: &plan.family_artifacts,
    };
    prepared.realize(
        &mut runtime,
        arena.storage_mut(),
        anchor,
        completion,
        budget,
    )
}

impl ReferenceCompletionStorageV1 {
    fn new<E: ProgramConstraintEvaluatorSetV1>(
        epoch: &ProgramEpochV1<E>,
    ) -> Result<Self, Error<E>> {
        let CompiledTargetSelectionV1::Computed(selection) = &epoch.target_selection else {
            return Err(Error::<E>::RequiresComputedTargets);
        };
        let bindings = epoch.binding_template.try_clone_v1()?;
        let certified_workspace = epoch.graph.new_certified_workspace()?;
        let mut binding_targets = reserve::<_, E>(bindings.paint_inputs_canonical().len())?;
        binding_targets.resize(bindings.paint_inputs_canonical().len(), None);
        let anchors = if let Some(anchors) = &selection.point_anchors {
            anchors.clone()
        } else {
            Arc::new(CompiledPointAnchorsV1::new(epoch)?)
        };
        let source_labs = anchors.source_labs.clone();
        let references = &anchors.references;
        let mut uses = reserve::<_, E>(selection.targets.len())?;
        for (index, target) in selection.targets.iter().enumerate() {
            let point_anchor = match target.preferred_intent() {
                CompiledPreferredIntentV1::Point {
                    anchor: PreferredPointAnchorV1::AuthoredSource,
                    ..
                } => Some(source_labs[index]),
                CompiledPreferredIntentV1::Point {
                    anchor: PreferredPointAnchorV1::ReferenceVisible,
                    ..
                } => Some(references[index].ok_or(Error::<E>::InvalidUse)?.visible_lab),
                CompiledPreferredIntentV1::ImplicitReference => None,
            };
            let factor = selection
                .objective_factor(index)
                .ok_or(Error::<E>::InvalidUse)?;
            let binding = bindings
                .paint_input_index(target.binding)
                .ok_or(Error::<E>::InvalidUse)?;
            binding_targets[binding] = Some(index);
            uses.push(UseV1 {
                slot: target.appearance,
                occurrence: target
                    .declaration
                    .intent
                    .as_ref()
                    .and_then(|intent| intent.appearance),
                target: index,
                factor,
                cases: 0..0,
                tone_bits: Vec::new(),
                point_anchor,
                preferred: Vec::new(),
                local_preferred: Vec::new(),
                reference: references[index],
                point_lab_cache: crate::certified_numeric::CertifiedOklabCacheV1::default(),
            });
        }
        Ok(Self {
            uses,
            bindings,
            certified_workspace,
            binding_targets,
            source_labs,
            point_buffers: super::point_search::BuffersV1::default(),
            point_transforms: crate::certified_numeric::PointTransformCache::new()
                .map_err(|_| Error::<E>::ResourceExhausted)?,
            authored_minimum_rows: Vec::new(),
        })
    }

    fn refresh_tones<E: ProgramConstraintEvaluatorSetV1>(
        &mut self,
        epoch: &ProgramEpochV1<E>,
        observation: &RevisionBoundObservationV1,
    ) -> Result<(), Error<E>> {
        let CompiledTargetSelectionV1::Computed(selection) = &epoch.target_selection else {
            return Err(Error::<E>::RequiresComputedTargets);
        };
        if let Some(field) = observation.declared_field() {
            let mut inputs = reserve::<_, E>(field.fields().len())?;
            for input in field.fields() {
                inputs.push(
                    u32::try_from(input.varying_input_index())
                        .map_err(|_| Error::<E>::UnsupportedDeclaredField)?,
                );
            }
            // Marginal supports cover the product exactly only when every
            // component/factor reads at most one varying input. Check the same
            // compiler proof used at public field admission, including fixed
            // and report-only factors, before interpreting a full proposal.
            if !selection.accepts_independent_fields(&inputs) {
                return Err(Error::<E>::UnsupportedDeclaredField);
            }
        }
        for usage in &mut self.uses {
            usage.cases = observation.declared_field().map_or_else(
                || 0..observation.physical_case_count(),
                |field| {
                    field
                        .fields()
                        .find(|field| {
                            selection.factors[usage.factor]
                                .inputs
                                .contains(&field.varying_input_index())
                        })
                        .map_or(0..1, |field| {
                            field.case_offset()..field.case_offset() + field.support_count()
                        })
                },
            );
            usage.preferred.clear();
            usage.tone_bits.clear();
            usage
                .preferred
                .try_reserve(usage.cases.len())
                .map_err(|_| Error::<E>::ResourceExhausted)?;
            usage
                .tone_bits
                .try_reserve(usage.cases.len())
                .map_err(|_| Error::<E>::ResourceExhausted)?;
            for case in usage.cases.clone() {
                let inputs = observation
                    .physical_values(case)
                    .ok_or(Error::<E>::MissingFiniteObservation)?;
                let tone = match selection.targets[usage.target].preferred_intent() {
                    CompiledPreferredIntentV1::Point {
                        tone: Some((curve, input)),
                        ..
                    } => Some(curve.preferred_lightness(inputs[input].srgb8())),
                    _ => None,
                };
                if let Some(mut preferred) = usage.point_anchor {
                    if let Some(tone) = tone {
                        preferred[0] = CertifiedScalarV1::exact(tone)?;
                    }
                    usage.preferred.push(preferred);
                }
                usage.tone_bits.push(tone.map(f64::to_bits));
            }
        }
        Ok(())
    }
}

fn reserve<T, E: ProgramConstraintEvaluatorSetV1>(count: usize) -> Result<Vec<T>, Error<E>> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| Error::<E>::ResourceExhausted)?;
    Ok(result)
}

fn copy_slice<T: Copy, E: ProgramConstraintEvaluatorSetV1>(
    values: &[T],
) -> Result<Vec<T>, Error<E>> {
    let mut result = reserve::<_, E>(values.len())?;
    result.extend_from_slice(values);
    Ok(result)
}

fn sort_profile<E: ProgramConstraintEvaluatorSetV1>(
    values: &mut [CertifiedScalarV1],
) -> Result<(), Error<E>> {
    let mut scratch = reserve::<_, E>(values.len())?;
    scratch.extend_from_slice(values);
    sort_descending_profile(values, &mut scratch)?;
    Ok(())
}

impl<E: ProgramConstraintEvaluatorSetV1> PreparedPointSelectionV1<'_, '_, E> {
    fn has_point_terms(&self, component: &ComputedComponentV1) -> bool {
        component
            .targets
            .iter()
            .any(|&target| self.scratch.uses[target].point_anchor.is_some())
    }

    pub(super) fn refresh_point_component(
        &mut self,
        component: &ComputedComponentV1,
        cases: &[usize],
    ) -> Result<(), Error<E>> {
        let CompiledTargetSelectionV1::Computed(selection) = &self.epoch.target_selection else {
            return Err(Error::<E>::RequiresComputedTargets);
        };
        for &target in &component.targets {
            let usage = &mut self.scratch.uses[target];
            usage.local_preferred.clear();
            let Some(anchor) = usage.point_anchor else {
                continue;
            };
            usage
                .local_preferred
                .try_reserve(cases.len())
                .map_err(|_| Error::<E>::ResourceExhausted)?;
            for &case in cases {
                #[cfg(test)]
                super::point_search::tests::refreshed_point_case(selection.targets[target].id);
                let mut point = anchor;
                if let CompiledPreferredIntentV1::Point {
                    tone: Some((curve, input)),
                    ..
                } = selection.targets[target].preferred_intent()
                {
                    let inputs = self
                        .observation
                        .physical_values(case)
                        .ok_or(Error::<E>::MissingFiniteObservation)?;
                    point[0] =
                        CertifiedScalarV1::exact(curve.preferred_lightness(inputs[input].srgb8()))?;
                }
                usage.local_preferred.push(point);
            }
        }
        Ok(())
    }

    pub(super) fn take_point_buffers(&mut self) -> super::point_search::BuffersV1 {
        std::mem::take(&mut self.scratch.point_buffers)
    }

    pub(super) fn restore_point_buffers(&mut self, buffers: super::point_search::BuffersV1) {
        self.scratch.point_buffers = buffers;
    }

    pub(super) fn same_point_expression(
        &self,
        component: &ComputedComponentV1,
        local: usize,
        a: &[Srgb8],
        b: &[Srgb8],
        region: Option<RegionNodeV1>,
    ) -> bool {
        self.same_preference_expression(component, local, |index| {
            region.is_none_or(|node| index < node.depth) && a[index] == b[index]
        })
    }

    pub(super) fn same_preference_expression(
        &self,
        component: &ComputedComponentV1,
        local: usize,
        fixed_equal: impl Fn(usize) -> bool,
    ) -> bool {
        let CompiledTargetSelectionV1::Computed(selection) = &self.epoch.target_selection else {
            return false;
        };
        let usage = &self.scratch.uses[component.targets[local]];
        selection.factors[usage.factor]
            .targets
            .iter()
            .all(|target| {
                component
                    .targets
                    .binary_search(target)
                    .is_ok_and(&fixed_equal)
            })
    }

    /// The same continuous physical interpreter measures point probes and
    /// enclosing source boxes. Cases are the component's unique local support;
    /// hard assessment separately retains every actual observation case.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn measure_point_component(
        &mut self,
        component: &ComputedComponentV1,
        values: &[Srgb8],
        region: Option<RegionNodeV1>,
        cases: &[usize],
        primary: &mut Vec<CertifiedScalarV1>,
        source_profile: &mut Vec<CertifiedScalarV1>,
        measurement: PointMeasurementV1,
    ) -> Result<(), Error<E>> {
        let CompiledTargetSelectionV1::Computed(selection) = &self.epoch.target_selection else {
            return Err(Error::<E>::RequiresComputedTargets);
        };
        let bounds = |local: usize| {
            let local = component.point_representative(local);
            let (lo, hi) = match region {
                Some(node) if local >= node.depth => if local == node.depth {
                    node.domain
                } else {
                    selection.targets[component.targets[local]].search_domain()
                }
                .bounds(),
                _ => (values[local].bytes(), values[local].bytes()),
            };
            (lo.map(f64::from), hi.map(f64::from))
        };
        self.measure_point_domains(
            component,
            bounds,
            cases,
            primary,
            source_profile,
            measurement,
        )
    }

    /// One physical interpreter for Point and explicit Tone terms, irrespective
    /// of the other preferences or constraints in their connected component.
    /// Domains are supplied in compiled-local order. Implicit reference terms
    /// are measured separately; their physical source distance is still shared.
    pub(super) fn measure_point_domains(
        &mut self,
        component: &ComputedComponentV1,
        bounds: impl Fn(usize) -> ([f64; 3], [f64; 3]),
        cases: &[usize],
        primary: &mut Vec<CertifiedScalarV1>,
        source_profile: &mut Vec<CertifiedScalarV1>,
        measurement: PointMeasurementV1,
    ) -> Result<(), Error<E>> {
        primary.clear();
        source_profile.clear();
        primary
            .try_reserve(component.point_variable_count())
            .map_err(|_| Error::<E>::ResourceExhausted)?;
        if let PointMeasurementV1::Profile(precision) = measurement {
            source_profile
                .try_reserve(component.targets.len())
                .map_err(|_| Error::<E>::ResourceExhausted)?;
            for (local, &target) in component.targets.iter().enumerate() {
                if component.point_representative(local) != local {
                    continue;
                }
                let (lo, hi) = bounds(local);
                let lab = precision.oklab(
                    &mut self.scratch.uses[target].point_lab_cache,
                    &mut self.scratch.point_transforms,
                    CertifiedRgbV1::from_encoded_bounds(lo, hi)?,
                )?;
                source_profile.push(source_distance_squared(
                    lab,
                    self.scratch.source_labs[target],
                )?);
            }
        }
        // Transport measures its visible loss on emitted RGB8. In a component
        // with only transport terms this profile contains only source distances
        // (computed above); no continuous visible color enters the comparison.
        if !self.has_point_terms(component) {
            if !cases.is_empty() {
                primary.resize(
                    component.point_variable_count(),
                    CertifiedScalarV1::exact(0.0)?,
                );
            }
            return Ok(());
        }
        let authored = self.scratch.bindings.paint_inputs_canonical();
        let mut paints = self.epoch.graph.prepare_certified_surface_cases(
            Some(&component.physical),
            &self.scratch.bindings,
            |binding| {
                let rgb = self.scratch.binding_targets[binding]
                    .and_then(|target| component.targets.binary_search(&target).ok())
                    .map_or_else(
                        || {
                            CertifiedRgbV1::from_encoded(
                                authored[binding].1.source().bytes().map(f64::from),
                            )
                        },
                        |local| {
                            let (lo, hi) = bounds(local);
                            CertifiedRgbV1::from_encoded_bounds(lo, hi)
                        },
                    );
                PointPaintValueV1::from_admitted(rgb, authored[binding].1.opacity())
            },
            &mut self.scratch.certified_workspace,
        )?;
        for (local_case, &case) in cases.iter().enumerate() {
            let inputs = self
                .observation
                .physical_values(case)
                .ok_or(Error::<E>::MissingFiniteObservation)?;
            let view = paints.evaluate_surface_inputs_canonical(|index| {
                CertifiedRgbV1::from_encoded(inputs[index].srgb8().bytes().map(f64::from))
            });
            for variable in 0..component.point_variable_count() {
                let local = component.point_variable(variable);
                let target = component.targets[local];
                let usage = &mut self.scratch.uses[target];
                if usage.point_anchor.is_none() {
                    if local_case == 0 {
                        primary.push(CertifiedScalarV1::exact(0.0)?);
                    }
                    continue;
                }
                let visible_rgb = if let Some(slot) = usage.slot {
                    view.occurrence_at(slot)
                        .ok_or(Error::<E>::InvalidUse)?
                        .visible?
                } else {
                    let (lo, hi) = bounds(local);
                    CertifiedRgbV1::from_encoded_bounds(lo, hi)?
                };
                let preferred = *usage
                    .local_preferred
                    .get(local_case)
                    .ok_or(Error::<E>::InvalidUse)?;
                let loss = match measurement {
                    PointMeasurementV1::RegionWitness => {
                        visible_rgb.region_loss_witness(preferred)?
                    }
                    PointMeasurementV1::Profile(precision) => {
                        let visible = precision.oklab(
                            &mut usage.point_lab_cache,
                            &mut self.scratch.point_transforms,
                            visible_rgb,
                        )?;
                        source_distance_squared(visible, preferred)?
                    }
                };
                if local_case == 0 {
                    primary.push(loss);
                } else {
                    primary[variable] = primary[variable].max(loss)?;
                }
            }
        }
        Ok(())
    }
}

impl<E: ProgramConstraintEvaluatorSetV1> PreparedReferenceCompletionV1<'_, '_, E> {
    /// The compiled variable order belongs to this prepared operation. Solvers
    /// consume these identities rather than assuming declaration/creation order.
    pub(crate) fn sources(&self) -> impl ExactSizeIterator<Item = (TargetId, Srgb8)> + '_ {
        let selection = match &self.epoch.target_selection {
            CompiledTargetSelectionV1::Computed(selection) => Some(selection),
            _ => None,
        };
        let targets = selection.map_or(&[][..], |selection| selection.targets.as_ref());
        targets
            .iter()
            .map(|target| (target.id, target.declaration.anchor))
    }

    fn measure(&mut self, values: &[ContinuousSrgbV1]) -> Result<MeasurementsV1, Error<E>> {
        if values.len() != self.scratch.source_labs.len() {
            return Err(Error::<E>::InvalidTuple);
        }
        let epoch = self.epoch;
        let CompiledTargetSelectionV1::Computed(selection) = &epoch.target_selection else {
            return Err(Error::<E>::RequiresComputedTargets);
        };
        let mut preferences = reserve::<_, E>(self.scratch.uses.len())?;
        for usage in &self.scratch.uses {
            preferences.push(reserve::<_, E>(usage.cases.len())?);
        }
        let source = self.scratch.bindings.paint_inputs_canonical();
        for component in &selection.components {
            let mut uses_by_cases = BTreeMap::<(usize, usize), Vec<usize>>::new();
            for target in &component.targets {
                for (index, usage) in self.scratch.uses.iter().enumerate() {
                    if usage.target == *target {
                        uses_by_cases
                            .entry((usage.cases.start, usage.cases.end))
                            .or_default()
                            .push(index);
                    }
                }
            }
            if uses_by_cases.is_empty() {
                continue;
            }
            let mut paints = epoch.graph.prepare_certified_surface_cases(
                Some(&component.physical),
                &self.scratch.bindings,
                |index| {
                    PointPaintValueV1::from_admitted(
                        CertifiedRgbV1::from_encoded(
                            self.scratch.binding_targets[index].map_or_else(
                                || source[index].1.source().bytes().map(f64::from),
                                |target| values[target].channels(),
                            ),
                        ),
                        source[index].1.opacity(),
                    )
                },
                &mut self.scratch.certified_workspace,
            )?;
            for ((case_start, case_end), usage_indices) in uses_by_cases {
                for case in case_start..case_end {
                    let inputs = self
                        .observation
                        .physical_values(case)
                        .ok_or(Error::<E>::MissingFiniteObservation)?;
                    let view = usage_indices
                        .iter()
                        .any(|&index| self.scratch.uses[index].slot.is_some())
                        .then(|| {
                            paints.evaluate_surface_inputs_canonical(|index| {
                                CertifiedRgbV1::from_encoded(
                                    inputs[index].srgb8().bytes().map(f64::from),
                                )
                            })
                        });
                    for &usage_index in &usage_indices {
                        let usage = &self.scratch.uses[usage_index];
                        let (visible_lab, backdrop_lab) = if let Some(slot) = usage.slot {
                            let point = view
                                .as_ref()
                                .and_then(|view| view.occurrence_at(slot))
                                .ok_or(Error::<E>::InvalidUse)?;
                            (point.visible?.oklab()?, Some(point.backdrop?.oklab()?))
                        } else {
                            (
                                CertifiedRgbV1::from_encoded(values[usage.target].channels())?
                                    .oklab()?,
                                None,
                            )
                        };
                        let preferred = usage.preferred.get(case - case_start).copied();
                        let distance = preferred
                            .map(|point| source_distance_squared(visible_lab, point))
                            .transpose()?;
                        preferences[usage_index].push(PreferenceMeasurementV1 {
                            target_id: selection.targets[usage.target].id,
                            occurrence: usage.occurrence,
                            case,
                            visible_lab,
                            backdrop_lab,
                            preferred,
                            distance,
                            reference: usage.reference,
                        });
                    }
                }
            }
        }
        let mut source = reserve::<_, E>(values.len())?;
        for (value, anchor) in values.iter().zip(self.scratch.source_labs.iter()) {
            source.push(source_distance_squared(
                CertifiedRgbV1::from_encoded(value.channels())?.oklab()?,
                *anchor,
            )?);
        }
        Ok(MeasurementsV1 {
            preferences,
            source,
        })
    }
}

fn scalar_evidence(value: CertifiedScalarV1) -> CompletionScalarEvidenceV1 {
    CompletionScalarEvidenceV1 {
        bounds: value.bounds(),
        cell: value.evidence(),
    }
}

fn evidence<E: ProgramConstraintEvaluatorSetV1>(
    values: &MeasurementsV1,
) -> Result<CompletionPointEvidenceV1, Error<E>> {
    let count = values
        .preferences
        .iter()
        .try_fold(0_usize, |count, rows| count.checked_add(rows.len()))
        .ok_or(Error::<E>::ResourceExhausted)?;
    let mut preferences = reserve::<_, E>(count)?;
    for row in values.preferences.iter().flatten() {
        preferences.push(CompletionPreferenceEvidenceV1 {
            target: row.target_id,
            occurrence: row.occurrence,
            case: row.case,
            visible_lab: row.visible_lab.map(CertifiedScalarV1::bounds),
            backdrop_lab: row.backdrop_lab.map(|v| v.map(CertifiedScalarV1::bounds)),
            preferred: row.preferred.map(|v| v.map(CertifiedScalarV1::bounds)),
            distance: row.distance.map(scalar_evidence),
            reference: row.reference.map(|v| CompletionReferenceFactsV1 {
                visible_lab: v.visible_lab.map(CertifiedScalarV1::bounds),
                backdrop_lab: v.backdrop_lab.map(CertifiedScalarV1::bounds),
                contrast: scalar_evidence(v.contrast),
            }),
        });
    }
    let mut profile = copy_slice::<_, E>(&values.source)?;
    sort_profile::<E>(&mut profile)?;
    let mut source = reserve::<_, E>(profile.len())?;
    source.extend(profile.iter().map(|value| value.bounds()));
    Ok(CompletionPointEvidenceV1 {
        preferences: preferences.into_boxed_slice(),
        source_profile: source.into_boxed_slice(),
    })
}

fn component_admission(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    uses: &[UseV1],
    anchor_values: &[ContinuousSrgbV1],
    proposal_values: &[ContinuousSrgbV1],
    anchor: &MeasurementsV1,
    proposal: &MeasurementsV1,
) -> Result<(CompletionAdmissionV1, Option<f64>), NumericUncertaintyV1> {
    let preserved = |reason| CompletionAdmissionV1::AnchorPreserved(reason);
    let changed = component
        .targets
        .iter()
        .any(|&index| anchor_values[index] != proposal_values[index]);
    if changed
        && component
            .targets
            .iter()
            .any(|&index| uses[index].point_anchor.is_none())
    {
        return Ok((
            preserved(CompletionRejectionV1::ImplicitReferenceUnresolved),
            None,
        ));
    }
    let mut improved = false;
    let mut worsened = false;
    let mut unresolved = false;
    let mut equal = true;
    let mut drift = f64::NEG_INFINITY;
    for &target in &component.targets {
        let dependencies = &selection.factors[uses[target].factor].targets;
        if dependencies
            .iter()
            .all(|&index| anchor_values[index] == proposal_values[index])
        {
            // Same immutable program, input observation and source dependencies:
            // this is the same real expression, not merely equal enclosures.
            drift = drift.max(0.0);
            continue;
        }
        // Both measurements use the same prepared use/support binding. No
        // ordinal from another field can stand in for one of this use's cases.
        for (a, b) in anchor.preferences[target]
            .iter()
            .zip(&proposal.preferences[target])
        {
            #[cfg(test)]
            tests::preference_row_visit();
            let (Some(a), Some(b)) = (a.distance, b.distance) else {
                return Ok((
                    preserved(CompletionRejectionV1::ImplicitReferenceUnresolved),
                    None,
                ));
            };
            if a.proves_equal(b) {
                drift = drift.max(0.0);
                continue;
            }
            equal = false;
            let upper = b.upper_difference(a)?;
            drift = drift.max(upper);
            if upper <= 0.0 {
                improved |= b.strictly_below(a);
            } else if a.strictly_below(b) {
                worsened = true;
            } else {
                unresolved = true;
            }
        }
    }
    let drift = Some(if drift == f64::NEG_INFINITY {
        0.0
    } else {
        drift
    });
    if worsened {
        return Ok((
            preserved(if improved {
                CompletionRejectionV1::PreferenceIncomparable
            } else {
                CompletionRejectionV1::PreferenceWorsened
            }),
            drift,
        ));
    }
    if unresolved {
        return Ok((
            preserved(CompletionRejectionV1::PreferenceUnresolved),
            drift,
        ));
    }
    if improved {
        return Ok((CompletionAdmissionV1::PreferenceImproved, drift));
    }
    if !equal {
        return Ok((
            preserved(CompletionRejectionV1::PreferenceUnresolved),
            drift,
        ));
    }
    // Source completion uses a checked partial improvement. An unchanged
    // source proves an exact tie without comparing overlapping intervals.
    let mut source_improved = false;
    for &index in &component.targets {
        if anchor_values[index] == proposal_values[index] {
            continue;
        }
        let (a, b) = (anchor.source[index], proposal.source[index]);
        if a.proves_equal(b) {
            continue;
        }
        if b.upper_difference(a)? > 0.0 {
            return Ok((
                preserved(CompletionRejectionV1::SourceImprovementUnresolved),
                drift,
            ));
        }
        source_improved |= b.strictly_below(a);
    }
    Ok((
        if source_improved {
            CompletionAdmissionV1::SourceImprovedWithinEquivalentPreference
        } else {
            preserved(CompletionRejectionV1::SourceImprovementUnresolved)
        },
        drift,
    ))
}

impl<'program, E: ProgramConstraintEvaluatorSetV1> PreparedReferenceCompletionV1<'program, '_, E> {
    /// The proposals contain every unique computed source, in compiled target
    /// order. Neither proposal authorizes overriding a strict material/domain
    /// constraint. Each compiled component independently preserves its anchor
    /// or admits a checked proposal before the merged tuple is realized.
    /// `budget` limits each compiled independent component; visited evidence
    /// sums their work and the compiler's once-only fixed-constraint check.
    pub(crate) fn realize(
        &mut self,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
        anchor: &[ContinuousSrgbV1],
        completion: &[ContinuousSrgbV1],
        budget: u64,
    ) -> Result<ReferenceCompletionResultV1<'program, E>, Error<E>> {
        if anchor.len() != completion.len() || anchor.is_empty() {
            return Err(Error::<E>::InvalidTuple);
        }
        let anchor_measurement = self.measure(anchor)?;
        let anchor_evidence = evidence::<E>(&anchor_measurement)?;
        let completion_measurement = match self.measure(completion) {
            Ok(value) => Ok(value),
            Err(ReferenceCompletionErrorV1::Numeric(error)) => Err(error),
            Err(error) => return Err(error),
        };
        let completion_evidence = completion_measurement
            .as_ref()
            .ok()
            .map(evidence::<E>)
            .transpose()?;
        let CompiledTargetSelectionV1::Computed(selection) = &self.epoch.target_selection else {
            return Err(Error::<E>::RequiresComputedTargets);
        };
        let mut ideal = copy_slice::<_, E>(anchor)?;
        let mut component_admissions = reserve::<_, E>(selection.components.len())?;
        let mut accepted = 0;
        let mut preference_improved = false;
        for (index, component) in selection.components.iter().enumerate() {
            let compared = match &completion_measurement {
                Ok(measurement) => component_admission(
                    selection,
                    component,
                    &self.scratch.uses,
                    anchor,
                    completion,
                    &anchor_measurement,
                    measurement,
                ),
                Err(error) => Err(*error),
            };
            let (admission, preference_drift_upper) = compared.unwrap_or_else(|error| {
                (
                    CompletionAdmissionV1::AnchorPreserved(CompletionRejectionV1::Numeric(error)),
                    None,
                )
            });
            if matches!(
                admission,
                CompletionAdmissionV1::PreferenceImproved
                    | CompletionAdmissionV1::SourceImprovedWithinEquivalentPreference
            ) {
                for &target in &component.targets {
                    ideal[target] = completion[target];
                }
                accepted += 1;
                preference_improved |= admission == CompletionAdmissionV1::PreferenceImproved;
            }
            component_admissions.push(ComponentAdmissionEvidenceV1 {
                component: index,
                admission,
                preference_drift_upper,
            });
        }
        let preserved = component_admissions.len() - accepted;
        let admission = if accepted == 0 {
            component_admissions.first().map_or(
                CompletionAdmissionV1::AnchorPreserved(
                    CompletionRejectionV1::SourceImprovementUnresolved,
                ),
                |value| value.admission,
            )
        } else if preserved != 0 {
            CompletionAdmissionV1::PartiallyAccepted {
                accepted,
                preserved,
            }
        } else if preference_improved {
            CompletionAdmissionV1::PreferenceImproved
        } else {
            CompletionAdmissionV1::SourceImprovedWithinEquivalentPreference
        };
        let RealizationV1 {
            values: emitted,
            search,
            numerical_order_unresolved,
            color_evaluations: realization_color_evaluations,
        } = self.realize_frozen(runtime, arena, &ideal, budget)?;
        let mut use_tones = reserve::<_, E>(self.scratch.uses.len())?;
        for usage in &self.scratch.uses {
            use_tones.push(CompletionUseToneEvidenceV1 {
                occurrence: usage.occurrence,
                cases: usage.cases.clone(),
                per_case_bits: copy_slice::<_, E>(&usage.tone_bits)?.into_boxed_slice(),
            });
        }
        Ok(ReferenceCompletionResultV1 {
            epoch: self.epoch,
            observation: self.observation.clone(),
            model: CERTIFIED_NUMERIC_MODEL_V1,
            use_tones: use_tones.into_boxed_slice(),
            anchor: copy_slice::<_, E>(anchor)?.into_boxed_slice(),
            completion: copy_slice::<_, E>(completion)?.into_boxed_slice(),
            ideal: ideal.into_boxed_slice(),
            admission,
            anchor_evidence,
            completion_evidence,
            component_admissions: component_admissions.into_boxed_slice(),
            emitted: emitted.map(Vec::into_boxed_slice),
            search,
            numerical_order_unresolved,
            realization_color_evaluations,
        })
    }

    fn realize_frozen(
        &self,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
        ideal: &[ContinuousSrgbV1],
        requested_budget: u64,
    ) -> Result<RealizationV1, Error<E>> {
        let epoch = self.epoch;
        let CompiledTargetSelectionV1::Computed(selection) = &epoch.target_selection else {
            return Err(Error::<E>::RequiresComputedTargets);
        };
        let scenarios = NonEmptyScenarioSetV1::from_admitted(&self.observation)
            .ok_or(Error::<E>::MissingFiniteObservation)?;
        prepare_program_evaluation_arena(epoch, scenarios, arena)?;
        let mut values = reserve::<_, E>(selection.targets.len())?;
        values.extend(
            selection
                .targets
                .iter()
                .map(|target| target.declaration.anchor),
        );
        apply::<E>(selection, runtime, &values)?;
        let mut search = empty_search(CompletionV1::Complete);
        let mut numerical_order_unresolved = false;
        let mut color_evaluations = 0_u64;
        if !selection.fixed_constraints.is_empty() || selection.components.is_empty() {
            // The compiler proved these conditions independent of every
            // adjustable paint. Match the existing once-only constant check.
            let failed = scan_program_candidate_scoped(
                runtime,
                epoch,
                scenarios,
                0,
                ProgramEvaluationPhaseV1::Hard,
                ProgramCandidateCollectionV1::none(),
                ProgramCandidateScanV1::Constraints {
                    indices: &selection.fixed_constraints,
                    physical: None,
                },
            )?;
            search.visited = 1;
            search.points = 1;
            search.examined = 1;
            if failed {
                return Ok(RealizationV1 {
                    values: None,
                    search,
                    numerical_order_unresolved,
                    color_evaluations,
                });
            }
        }
        for component in &selection.components {
            let selected = realize_component(
                selection,
                component,
                runtime,
                epoch,
                scenarios,
                arena,
                FrozenRealizationRequestV1 {
                    sources: ideal,
                    budget: requested_budget,
                },
            )?;
            components::merge_evidence(&mut search, selected.search)
                .map_err(|_| Error::<E>::ResourceExhausted)?;
            numerical_order_unresolved |= selected.numerical_order_unresolved;
            color_evaluations = color_evaluations
                .checked_add(selected.color_evaluations)
                .ok_or(Error::<E>::ResourceExhausted)?;
            let Some(local) = selected.values else {
                // A bounded component without a feasible point is unfinished;
                // independent colors cannot repair or consume its own budget.
                if selected.search.completion == CompletionV1::Complete {
                    search.completion = CompletionV1::Complete;
                }
                return Ok(RealizationV1 {
                    values: None,
                    search,
                    numerical_order_unresolved,
                    color_evaluations,
                });
            };
            apply_targets::<E>(
                ComputedTargetsV1::new(selection, Some(component)),
                runtime,
                &local,
            )?;
            for (&index, value) in component.targets.iter().zip(local) {
                values[index] = value;
            }
        }
        // A scoped pass never authorizes publication. Reapply and verify the
        // merged tuple with all cases, hard relations and material constraints.
        // For declared fields these are complete nominal RGB8 marginal supports.
        // The admission proof above makes this universal check equivalent to
        // checking their Cartesian product. It makes no rasterizer claim and
        // does not extend the point metric to unquantized field interpolation.
        let context = ContextV1::new::<E>(selection, None, scenarios)?;
        let mut scores = buffer(values.len(), 0.0).map_err(|_| Error::<E>::ResourceExhausted)?;
        if assess(
            &context,
            runtime,
            epoch,
            scenarios,
            &values,
            &mut scores,
            arena,
        )? != 0
        {
            return Err(Error::<E>::Evaluation(
                ProgramSessionEvaluationError::InternalInvariant,
            ));
        }
        Ok(RealizationV1 {
            values: Some(values),
            search,
            numerical_order_unresolved,
            color_evaluations,
        })
    }
}

/// A frozen tuple uses the compiled variable order. Its source-distance
/// objective introduces no dependency between independently constrained paints.
struct FrozenRealizationRequestV1<'a> {
    sources: &'a [ContinuousSrgbV1],
    budget: u64,
}

fn empty_search(completion: CompletionV1) -> SearchEvidenceV1 {
    SearchEvidenceV1 {
        completion,
        numerical_order_unresolved: false,
        visited: 0,
        regions: 0,
        points: 0,
        endpoint_evaluations: 0,
        examined: 0,
        open_regions: 0,
    }
}

/// The same component/runtime/arena seam as existing component selection. The
/// graph, target membership, relevant contexts and cone all remain compiler-owned.
fn realize_component<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
    request: FrozenRealizationRequestV1<'_>,
) -> Result<RealizationV1, Error<E>> {
    let context = ContextV1::new::<E>(selection, Some(component), scenarios)?;
    let budget = request.budget.min(selection.budget);
    let mut search = empty_search(CompletionV1::BudgetExhausted);
    let mut local_ideal = reserve::<_, E>(component.targets.len())?;
    for &index in &component.targets {
        local_ideal.push(*request.sources.get(index).ok_or(Error::<E>::InvalidTuple)?);
    }
    let ideal = local_ideal.as_slice();
    let mut scores = buffer(ideal.len(), 0.0).map_err(|_| Error::<E>::ResourceExhausted)?;
    let mut ideal_labs = reserve::<_, E>(ideal.len())?;
    let mut rounded = reserve::<_, E>(ideal.len())?;
    for (value, domain) in ideal.iter().zip(&context.domains) {
        ideal_labs.push(CertifiedRgbV1::from_encoded(value.channels())?.oklab()?);
        let (lower, upper) = domain.bounds();
        let channels = value.channels();
        let hint = Srgb8::new(std::array::from_fn(|i| {
            channels[i]
                .round_ties_even()
                .clamp(f64::from(lower[i]), f64::from(upper[i])) as u8
        }));
        rounded.push(if domain.contains(hint) {
            hint
        } else {
            domain.value(0)
        });
    }
    // Joint repair starts around every frozen source, including parents.
    // Ordering here proposes work only: strict scans and certified scores
    // decide acceptance. The full admitted domains remain the fallback.
    let mut local_domains = reserve::<_, E>(ideal.len())?;
    let mut neighbors = reserve::<_, E>(ideal.len())?;
    for ((value, domain), ideal) in rounded.iter().zip(&context.domains).zip(ideal) {
        let (lo, hi) = domain.bounds();
        let bytes = value.bytes();
        let lower = std::array::from_fn(|i| lo[i].max(bytes[i].saturating_sub(1)));
        let upper = std::array::from_fn(|i| hi[i].min(bytes[i].saturating_add(1)));
        let local = match domain {
            AssignmentDomainV1::Gray { .. } => AssignmentDomainV1::Gray {
                lower: lower[0],
                upper: upper[0],
            },
            AssignmentDomainV1::Cube { .. } => AssignmentDomainV1::Cube { lower, upper },
        };
        let count = local.cardinality().ok_or(Error::<E>::InvalidTuple)?;
        let mut points = reserve::<_, E>(count as usize)?;
        for ordinal in 0..count {
            points.push(local.value(ordinal));
        }
        let goal = ideal.channels();
        points.sort_unstable_by(|left, right| {
            let score = |value: Srgb8| {
                value
                    .bytes()
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (f64::from(v) - goal[i]).powi(2))
                    .sum::<f64>()
            };
            score(*left)
                .total_cmp(&score(*right))
                .then_with(|| left.cmp(right))
        });
        local_domains.push(local);
        neighbors.push(points);
    }
    let mut values = reserve::<_, E>(neighbors.len())?;
    values.extend(neighbors.iter().map(|points| points[0]));
    let mut ordinals = buffer(ideal.len(), 0_u64).map_err(|_| Error::<E>::ResourceExhausted)?;
    let mut best: Option<Vec<Srgb8>> = None;
    let zero = CertifiedScalarV1::exact(0.0)?;
    let mut best_profile = buffer(ideal.len(), zero).map_err(|_| Error::<E>::ResourceExhausted)?;
    let mut profile = buffer(ideal.len(), zero).map_err(|_| Error::<E>::ResourceExhausted)?;
    let mut profile_scratch =
        buffer(ideal.len(), zero).map_err(|_| Error::<E>::ResourceExhausted)?;
    let mut local_scores = reserve::<_, E>(neighbors.len())?;
    for points in &neighbors {
        local_scores.push(buffer(points.len(), None).map_err(|_| Error::<E>::ResourceExhausted)?);
    }
    let mut last_scores = buffer(ideal.len(), None::<(Srgb8, CertifiedScalarV1)>)
        .map_err(|_| Error::<E>::ResourceExhausted)?;
    let mut color_evaluations = 0;
    let mut unresolved = false;
    let mut local = true;
    loop {
        if search.visited >= budget {
            break;
        }
        search.visited += 1;
        search.points += 1;
        search.examined += 1;
        // This is the existing complete-frame scan, including source
        // identity, visible maxChange, all hard constraints and every case.
        if assess(
            &context,
            runtime,
            epoch,
            scenarios,
            &values,
            &mut scores,
            arena,
        )? == 0
        {
            for (index, (value, ideal_lab)) in values.iter().zip(&ideal_labs).enumerate() {
                let cached = if local {
                    local_scores[index][ordinals[index] as usize]
                } else {
                    last_scores[index]
                        .filter(|(rgb, _)| rgb == value)
                        .map(|(_, score)| score)
                };
                let score = if let Some(score) = cached {
                    score
                } else {
                    color_evaluations += 1;
                    let score = source_distance_squared(
                        CertifiedRgbV1::from_encoded(value.bytes().map(f64::from))?.oklab()?,
                        *ideal_lab,
                    )?;
                    if local {
                        local_scores[index][ordinals[index] as usize] = Some(score);
                    } else {
                        last_scores[index] = Some((*value, score));
                    }
                    score
                };
                profile[index] = score;
            }
            sort_descending_profile(&mut profile, &mut profile_scratch)?;
            let order = if best.is_none() {
                Some(Ordering::Less)
            } else {
                compare_profiles(&profile, &best_profile)
            };
            if order == Some(Ordering::Less) {
                if let Some(best) = &mut best {
                    best.copy_from_slice(&values);
                } else {
                    best = Some(copy_slice::<_, E>(&values)?);
                }
                best_profile.copy_from_slice(&profile);
            } else if order.is_none() {
                unresolved = true;
            }
        }
        loop {
            let mut carried = true;
            for i in (0..values.len()).rev() {
                let count = if local {
                    neighbors[i].len() as u64
                } else {
                    context.domains[i]
                        .cardinality()
                        .ok_or(Error::<E>::InvalidTuple)?
                };
                ordinals[i] += 1;
                if ordinals[i] < count {
                    values[i] = if local {
                        neighbors[i][ordinals[i] as usize]
                    } else {
                        context.domains[i].value(ordinals[i])
                    };
                    carried = false;
                    break;
                }
                ordinals[i] = 0;
                values[i] = if local {
                    neighbors[i][0]
                } else {
                    context.domains[i].value(0)
                };
            }
            if carried {
                if local && local_domains != context.domains {
                    local = false;
                    for (value, domain) in values.iter_mut().zip(&context.domains) {
                        *value = domain.value(0);
                    }
                } else {
                    search.completion = CompletionV1::Complete;
                    break;
                }
            }
            if local
                || !local_domains
                    .iter()
                    .zip(&values)
                    .all(|(domain, value)| domain.contains(*value))
            {
                break;
            }
        }
        if search.completion == CompletionV1::Complete {
            break;
        }
    }
    // Lexicographic order of a descending multiset is preserved when a fixed
    // multiset is merged into it. Minimizing each independent source profile
    // therefore preserves the frozen whole-tuple objective; primary policy is
    // not evaluated or changed by this realization step.
    Ok(RealizationV1 {
        values: best,
        search,
        numerical_order_unresolved: unresolved,
        color_evaluations,
    })
}
fn compare_profiles(left: &[CertifiedScalarV1], right: &[CertifiedScalarV1]) -> Option<Ordering> {
    for (left, right) in left.iter().zip(right) {
        if left.strictly_below(*right) {
            return Some(Ordering::Less);
        }
        if right.strictly_below(*left) {
            return Some(Ordering::Greater);
        }
        if !left.proves_equal(*right) {
            return None;
        }
    }
    Some(Ordering::Equal)
}

#[cfg(test)]
pub(in crate::program_session) mod tests;
