//! Private vertical path: certified finite point completion, then one strict
//! whole-tuple RGB8 realization. No continuous optimizer or field certificate.

use super::*;
use crate::appearance::{CertifiedAppearanceWorkspace, ContinuousSrgbV1, PointPaintValueV1};
use crate::certified_numeric::{
    CERTIFIED_NUMERIC_MODEL_V1, CertifiedNumericModelV1, CertifiedRgbV1, CertifiedScalarEvidenceV1,
    CertifiedScalarV1, NumericUncertaintyV1, sort_descending_profile, source_distance_squared,
};
use crate::observation::ObservationHeadViewV1;

/// A point-filter outcome never asserts a mathematical tie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionAdmissionV1 {
    UnresolvedWithinCell,
    AnchorPreserved(CompletionRejectionV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionRejectionV1 {
    MeasurementCellChanged,
    PrimaryCellChanged,
    SourceImprovementUnresolved,
    Numeric(NumericUncertaintyV1),
}

#[derive(Debug)]
pub(crate) enum ReferenceCompletionErrorV1<E> {
    OwnerMismatch,
    MissingFiniteObservation,
    UnsupportedDeclaredField,
    MissingReference,
    RequiresComputedTargets,
    MissingDeclaredReferenceUse,
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

struct UseV1 {
    slot: CompiledOccurrenceSlotV1,
    tones: Vec<Option<CertifiedScalarV1>>,
    objective: Option<usize>,
    reference_contrast: CertifiedScalarV1,
}

struct MeasurementsV1 {
    measured: Vec<CertifiedScalarV1>,
    primary: Vec<CertifiedScalarV1>,
    source: Vec<CertifiedScalarV1>,
}

/// Complete cell evidence for anchor and proposal, in canonical case/use order.
/// Profiles are enclosing order statistics, not a host-f64 sort of estimates.
pub(crate) struct CompletionPointEvidenceV1 {
    pub(crate) measurements: Box<[CompletionScalarEvidenceV1]>,
    pub(crate) primary_profile: Box<[CompletionScalarEvidenceV1]>,
    pub(crate) source_profile: Box<[[u128; 2]]>,
}

/// An enclosure survives even when it cannot establish one rounding cell.
pub(crate) struct CompletionScalarEvidenceV1 {
    pub(crate) bounds: [u128; 2],
    pub(crate) cell: Result<CertifiedScalarEvidenceV1, NumericUncertaintyV1>,
}

pub(crate) struct CompletionUseToneEvidenceV1 {
    pub(crate) occurrence: OccurrenceId,
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
    owner: &'program CompiledProgram<E>,
    observation: RevisionBoundObservationV1,
    model: CertifiedNumericModelV1,
    use_tones: Box<[CompletionUseToneEvidenceV1]>,
    anchor: Box<[ContinuousSrgbV1]>,
    completion: Box<[ContinuousSrgbV1]>,
    ideal: Box<[ContinuousSrgbV1]>,
    admission: CompletionAdmissionV1,
    anchor_evidence: CompletionPointEvidenceV1,
    completion_evidence: Option<CompletionPointEvidenceV1>,
    primary_drift_upper: Option<f64>,
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
    pub(crate) fn primary_drift_upper(&self) -> Option<f64> {
        self.primary_drift_upper
    }
    pub(crate) fn binds_owner(&self, owner: &CompiledProgram<E>) -> bool {
        Shared::ptr_eq(&self.owner.owner_generation, &owner.owner_generation)
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

pub(crate) struct PreparedReferenceCompletionV1<
    'program,
    'session,
    E: ProgramConstraintEvaluatorSetV1,
> {
    owner: &'program CompiledProgram<E>,
    session: &'session Session<ProgramSessionPlan<E>>,
    observation: RevisionBoundObservationV1,
    uses: Vec<UseV1>,
    use_tones: Vec<CompletionUseToneEvidenceV1>,
    objective_count: usize,
    bindings: AdmittedAppearanceBindings,
    certified_workspace: CertifiedAppearanceWorkspace,
    binding_targets: Vec<Option<usize>>,
    source_labs: Vec<[CertifiedScalarV1; 3]>,
}

impl<E: ProgramConstraintEvaluatorSetV1> CompiledProgram<E> {
    /// Derive objectives from the already compiled target intents. Shared
    /// physical occurrences additionally constrain the completion witness; they
    /// never become extra primary weights or a second authoring root.
    pub(crate) fn prepare_reference_completion<'program, 'session>(
        &'program self,
        session: &'session Session<ProgramSessionPlan<E>>,
    ) -> Result<PreparedReferenceCompletionV1<'program, 'session, E>, Error<E>> {
        if !self.owns_session(session) {
            return Err(Error::<E>::OwnerMismatch);
        }
        let ObservationHeadViewV1::Observed(observation) = session.raw_head() else {
            return Err(Error::<E>::MissingFiniteObservation);
        };
        if observation.declared_field().is_some() {
            return Err(Error::<E>::UnsupportedDeclaredField);
        }
        let epoch = &self.owner_generation;
        let CompiledTargetSelectionV1::Computed(selection) = &epoch.target_selection else {
            return Err(Error::<E>::RequiresComputedTargets);
        };
        let inputs = epoch
            .reference_inputs
            .as_ref()
            .ok_or(Error::<E>::MissingReference)?;
        let bindings = epoch.binding_template.try_clone_v1()?;
        let mut certified_workspace = epoch.graph.new_certified_workspace()?;
        let sources = bindings.paint_inputs_canonical();
        let mut reference = epoch.graph.prepare_certified_surface_cases(
            None,
            &bindings,
            |index| {
                PointPaintValueV1::from_admitted(
                    CertifiedRgbV1::from_encoded(sources[index].1.source().bytes().map(f64::from)),
                    sources[index].1.opacity(),
                )
            },
            &mut certified_workspace,
        )?;
        let reference = reference.evaluate_surface_inputs_canonical(|index| {
            CertifiedRgbV1::from_encoded(inputs[index].1.bytes().map(f64::from))
        });
        let mut objective_count = 0;
        let mut uses = reserve::<_, E>(epoch.presentation_occurrence_contexts.len())?;
        let mut use_tones = reserve::<_, E>(epoch.presentation_occurrence_contexts.len())?;
        for point in &epoch.presentation_occurrence_contexts {
            let target = selection.targets.iter().find(|target| {
                target.appearance == Some(point.slot) && target.declaration.reference_appearance
            });
            let objective = target.map(|_| {
                let index = objective_count;
                objective_count += 1;
                index
            });
            let mut tones = reserve::<_, E>(observation.physical_case_count())?;
            let mut tone_bits = reserve::<_, E>(observation.physical_case_count())?;
            for case in 0..observation.physical_case_count() {
                let inputs = observation
                    .physical_values(case)
                    .ok_or(Error::<E>::MissingFiniteObservation)?;
                let tone = target
                    .and_then(|target| {
                        target
                            .declaration
                            .intent
                            .as_ref()?
                            .tone
                            .as_ref()
                            .zip(target.tone_input)
                    })
                    .map(|(curve, input)| curve.preferred_lightness(inputs[input].srgb8()));
                tones.push(tone.map(CertifiedScalarV1::exact).transpose()?);
                tone_bits.push(tone.map(f64::to_bits));
            }
            let value = reference
                .occurrence_at(point.slot)
                .ok_or(Error::<E>::InvalidUse)?;
            uses.push(UseV1 {
                slot: point.slot,
                tones,
                objective,
                reference_contrast: value.visible?.contrast(value.backdrop?)?,
            });
            use_tones.push(CompletionUseToneEvidenceV1 {
                occurrence: point.occurrence,
                per_case_bits: tone_bits.into_boxed_slice(),
            });
        }
        if objective_count == 0 {
            return Err(Error::<E>::MissingDeclaredReferenceUse);
        }
        let mut binding_targets = reserve::<_, E>(sources.len())?;
        for (id, _) in sources {
            binding_targets.push(
                selection
                    .targets
                    .iter()
                    .position(|target| target_paint_input_id(target.id) == *id),
            );
        }
        let mut source_labs = reserve::<_, E>(selection.targets.len())?;
        for target in &selection.targets {
            source_labs.push(
                CertifiedRgbV1::from_encoded(target.declaration.anchor.bytes().map(f64::from))?
                    .oklab()?,
            );
        }
        Ok(PreparedReferenceCompletionV1 {
            owner: self,
            session,
            observation: observation.clone(),
            uses,
            use_tones,
            objective_count,
            bindings,
            certified_workspace,
            binding_targets,
            source_labs,
        })
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

impl<E: ProgramConstraintEvaluatorSetV1> PreparedReferenceCompletionV1<'_, '_, E> {
    /// The compiled variable order belongs to this prepared operation. Solvers
    /// consume these identities rather than assuming declaration/creation order.
    pub(crate) fn sources(&self) -> impl ExactSizeIterator<Item = (TargetId, Srgb8)> + '_ {
        let selection = match &self.owner.owner_generation.target_selection {
            CompiledTargetSelectionV1::Computed(selection) => Some(selection),
            _ => None,
        };
        let targets = selection.map_or(&[][..], |selection| selection.targets.as_ref());
        targets
            .iter()
            .map(|target| (target.id, target.declaration.anchor))
    }

    fn measure(&mut self, values: &[ContinuousSrgbV1]) -> Result<MeasurementsV1, Error<E>> {
        if values.len() != self.source_labs.len() {
            return Err(Error::<E>::InvalidTuple);
        }
        let epoch = &self.owner.owner_generation;
        let source = self.bindings.paint_inputs_canonical();
        let mut paints = epoch.graph.prepare_certified_surface_cases(
            None,
            &self.bindings,
            |index| {
                let channels = self.binding_targets[index].map_or_else(
                    || source[index].1.source().bytes().map(f64::from),
                    |target| values[target].channels(),
                );
                PointPaintValueV1::from_admitted(
                    CertifiedRgbV1::from_encoded(channels),
                    source[index].1.opacity(),
                )
            },
            &mut self.certified_workspace,
        )?;
        let count = self.observation.physical_case_count();
        let mut measured = reserve::<_, E>(
            count
                .checked_mul(self.uses.len())
                .ok_or(Error::<E>::ResourceExhausted)?,
        )?;
        let mut primary = reserve::<_, E>(self.objective_count)?;
        primary.resize(self.objective_count, CertifiedScalarV1::exact(1.0)?);
        for case in 0..count {
            let inputs = self
                .observation
                .physical_values(case)
                .ok_or(Error::<E>::MissingFiniteObservation)?;
            let view = paints.evaluate_surface_inputs_canonical(|index| {
                CertifiedRgbV1::from_encoded(inputs[index].srgb8().bytes().map(f64::from))
            });
            for usage in &self.uses {
                let occurrence = view
                    .occurrence_at(usage.slot)
                    .ok_or(Error::<E>::InvalidUse)?;
                let (m, loss) = if let Some(tone) = usage.tones[case] {
                    let l = occurrence.visible?.oklab()?[0];
                    let offset = CertifiedScalarV1::exact(
                        crate::spaces::srgb::RELATIVE_LUMINANCE_RATIO_OFFSET,
                    )?;
                    (
                        l,
                        l.pow(3)?
                            .add(offset)?
                            .ratio_loss(tone.pow(3)?.add(offset)?)?,
                    )
                } else {
                    let cr = occurrence.visible?.contrast(occurrence.backdrop?)?;
                    (cr, cr.ratio_loss(usage.reference_contrast)?)
                };
                measured.push(m);
                if let Some(index) = usage.objective {
                    primary[index] = primary[index].max(loss)?;
                }
            }
        }
        let mut source_profile = reserve::<_, E>(values.len())?;
        for (point, reference) in values.iter().zip(&self.source_labs) {
            source_profile.push(source_distance_squared(
                CertifiedRgbV1::from_encoded(point.channels())?.oklab()?,
                *reference,
            )?);
        }
        sort_profile::<E>(&mut primary)?;
        sort_profile::<E>(&mut source_profile)?;
        Ok(MeasurementsV1 {
            measured,
            primary,
            source: source_profile,
        })
    }
}

fn evidence<E: ProgramConstraintEvaluatorSetV1>(
    values: &MeasurementsV1,
) -> Result<CompletionPointEvidenceV1, Error<E>> {
    fn project<E: ProgramConstraintEvaluatorSetV1>(
        values: &[CertifiedScalarV1],
    ) -> Result<Box<[CompletionScalarEvidenceV1]>, Error<E>> {
        let mut result = reserve::<_, E>(values.len())?;
        for value in values {
            result.push(CompletionScalarEvidenceV1 {
                bounds: value.bounds(),
                cell: value.evidence(),
            });
        }
        Ok(result.into_boxed_slice())
    }
    let mut source = reserve::<_, E>(values.source.len())?;
    source.extend(values.source.iter().map(|value| value.bounds()));
    Ok(CompletionPointEvidenceV1 {
        measurements: project::<E>(&values.measured)?,
        primary_profile: project::<E>(&values.primary)?,
        source_profile: source.into_boxed_slice(),
    })
}

fn admission(
    anchor: &MeasurementsV1,
    completion: &MeasurementsV1,
) -> Result<(CompletionAdmissionV1, Option<f64>), NumericUncertaintyV1> {
    for (a, b) in anchor.measured.iter().zip(&completion.measured) {
        if a.nearest_even_cell()? != b.nearest_even_cell()? {
            return Ok((
                CompletionAdmissionV1::AnchorPreserved(
                    CompletionRejectionV1::MeasurementCellChanged,
                ),
                None,
            ));
        }
    }
    let mut drift = 0.0_f64;
    for (a, b) in anchor.primary.iter().zip(&completion.primary) {
        if a.nearest_even_cell()? != b.nearest_even_cell()? {
            return Ok((
                CompletionAdmissionV1::AnchorPreserved(CompletionRejectionV1::PrimaryCellChanged),
                None,
            ));
        }
        drift = drift.max(b.upper_difference(*a)?);
    }
    // Strict interval lexicographic improvement; equal binary64 cells are never
    // used to skip an unresolved earlier source-displacement dimension.
    let improves = completion
        .source
        .iter()
        .zip(&anchor.source)
        .find_map(|(b, a)| {
            if b.strictly_below(*a) {
                Some(true)
            } else if a.strictly_below(*b) {
                Some(false)
            } else if a.proves_equal(*b) {
                None
            } else {
                Some(false)
            }
        })
        .unwrap_or(false);
    Ok((
        if improves {
            CompletionAdmissionV1::UnresolvedWithinCell
        } else {
            CompletionAdmissionV1::AnchorPreserved(
                CompletionRejectionV1::SourceImprovementUnresolved,
            )
        },
        Some(drift),
    ))
}

impl<'program, E: ProgramConstraintEvaluatorSetV1> PreparedReferenceCompletionV1<'program, '_, E> {
    /// The proposals contain every unique computed source, in compiled target
    /// order. Neither proposal authorizes overriding a strict material/domain
    /// constraint. Rejected completion preserves anchor bytes before realization.
    pub(crate) fn realize(
        &mut self,
        anchor: &[ContinuousSrgbV1],
        completion: &[ContinuousSrgbV1],
        budget: u64,
    ) -> Result<ReferenceCompletionResultV1<'program, E>, Error<E>> {
        if anchor.len() != completion.len() || anchor.is_empty() {
            return Err(Error::<E>::InvalidTuple);
        }
        let anchor_measurement = self.measure(anchor)?;
        let anchor_evidence = evidence::<E>(&anchor_measurement)?;
        let completion_measurement = self.measure(completion);
        let (admission, primary_drift_upper, completion_evidence) = match completion_measurement {
            Ok(measurement) => match admission(&anchor_measurement, &measurement) {
                Ok((outcome, drift)) => {
                    let projected = evidence::<E>(&measurement)?;
                    (outcome, drift, Some(projected))
                }
                Err(error) => (
                    CompletionAdmissionV1::AnchorPreserved(CompletionRejectionV1::Numeric(error)),
                    None,
                    None,
                ),
            },
            Err(ReferenceCompletionErrorV1::Numeric(error)) => (
                CompletionAdmissionV1::AnchorPreserved(CompletionRejectionV1::Numeric(error)),
                None,
                None,
            ),
            Err(error) => return Err(error),
        };
        let ideal = if admission == CompletionAdmissionV1::UnresolvedWithinCell {
            completion
        } else {
            anchor
        };
        let RealizationV1 {
            values: emitted,
            search,
            numerical_order_unresolved,
            color_evaluations: realization_color_evaluations,
        } = self.realize_frozen(ideal, budget)?;
        let mut use_tones = reserve::<_, E>(self.use_tones.len())?;
        for usage in &self.use_tones {
            use_tones.push(CompletionUseToneEvidenceV1 {
                occurrence: usage.occurrence,
                per_case_bits: copy_slice::<_, E>(&usage.per_case_bits)?.into_boxed_slice(),
            });
        }
        Ok(ReferenceCompletionResultV1 {
            owner: self.owner,
            observation: self.observation.clone(),
            model: CERTIFIED_NUMERIC_MODEL_V1,
            use_tones: use_tones.into_boxed_slice(),
            anchor: copy_slice::<_, E>(anchor)?.into_boxed_slice(),
            completion: copy_slice::<_, E>(completion)?.into_boxed_slice(),
            ideal: copy_slice::<_, E>(ideal)?.into_boxed_slice(),
            admission,
            anchor_evidence,
            completion_evidence,
            primary_drift_upper,
            emitted: emitted.map(Vec::into_boxed_slice),
            search,
            numerical_order_unresolved,
            realization_color_evaluations,
        })
    }

    fn realize_frozen(
        &self,
        ideal: &[ContinuousSrgbV1],
        requested_budget: u64,
    ) -> Result<RealizationV1, Error<E>> {
        let epoch = &self.owner.owner_generation;
        let CompiledTargetSelectionV1::Computed(selection) = &epoch.target_selection else {
            return Err(Error::<E>::RequiresComputedTargets);
        };
        let scenarios = NonEmptyScenarioSetV1::from_admitted(&self.observation)
            .ok_or(Error::<E>::MissingFiniteObservation)?;
        let context = ContextV1::new::<E>(selection, None, scenarios)?;
        let budget = requested_budget.min(selection.total_budget);
        let mut search = SearchEvidenceV1 {
            completion: CompletionV1::BudgetExhausted,
            visited: 0,
            regions: 0,
            points: 0,
            endpoint_evaluations: 0,
            examined: 0,
            open_regions: 0,
        };
        let mut bindings = epoch.binding_template.try_clone_v1()?;
        let mut workspace = epoch.graph.new_workspace()?;
        let mut cache = ProgramPresentationCacheV1::try_new(&epoch.point_presentations)
            .map_err(|_| Error::<E>::ResourceExhausted)?;
        let mut runtime = ProgramEvaluationRuntimeV1 {
            bindings: &mut bindings,
            workspace: &mut workspace,
            presentation_cache: &mut cache,
            family_artifacts: &self.session.plan().family_artifacts,
        };
        let mut arena = ProgramEvaluationArenaV1::empty();
        prepare_program_evaluation_arena(epoch, scenarios, &mut arena)?;
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
        let mut best_profile =
            buffer(ideal.len(), zero).map_err(|_| Error::<E>::ResourceExhausted)?;
        let mut profile = buffer(ideal.len(), zero).map_err(|_| Error::<E>::ResourceExhausted)?;
        let mut profile_scratch =
            buffer(ideal.len(), zero).map_err(|_| Error::<E>::ResourceExhausted)?;
        let mut local_scores = reserve::<_, E>(neighbors.len())?;
        for points in &neighbors {
            local_scores
                .push(buffer(points.len(), None).map_err(|_| Error::<E>::ResourceExhausted)?);
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
                &mut runtime,
                epoch,
                scenarios,
                &values,
                &mut scores,
                &mut arena,
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
        // Materialize only after a fresh, complete strict recheck of the same
        // tuple. This terminal verification is not an extra search proposal.
        if let Some(values) = &best {
            if assess(
                &context,
                &mut runtime,
                epoch,
                scenarios,
                values,
                &mut scores,
                &mut arena,
            )? != 0
            {
                return Err(Error::<E>::Evaluation(
                    ProgramSessionEvaluationError::InternalInvariant,
                ));
            }
        }
        Ok(RealizationV1 {
            values: best,
            search,
            numerical_order_unresolved: unresolved,
            color_evaluations,
        })
    }
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
mod tests;
