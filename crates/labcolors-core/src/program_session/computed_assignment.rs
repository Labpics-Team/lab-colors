//! Частный вычисляемый selection существующего Program. Политика не знает ID.
//! Полный обход ограничен числом посещённых узлов; физика остаётся у Program.

use super::*;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProgramOwnerIdentityV1 {
    Legacy(ProgramContentIdentityV9),
    Computed(ComputedIdentityV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ComputedIdentityV1(pub(super) [u8; 32]);

pub(super) enum CandidateOriginV1 {
    Fixed,
    DeclaredFinite(usize),
    #[cfg(test)]
    Computed {
        identity: ComputedIdentityV1,
        values: Box<[Srgb8]>,
    },
}

// Закрытая проба не добавляет payload и Drop обычному evidence Program.
#[cfg(not(test))]
const _: () = {
    assert!(std::mem::size_of::<CandidateOriginV1>() <= std::mem::size_of::<Option<usize>>());
    assert!(!std::mem::needs_drop::<CandidateOriginV1>());
};

impl CandidateOriginV1 {
    pub(super) const fn declared_state_index(&self) -> Option<usize> {
        match self {
            Self::DeclaredFinite(index) => Some(*index),
            Self::Fixed => None,
            #[cfg(test)]
            Self::Computed { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentDomainV1 {
    Cube { lower: [u8; 3], upper: [u8; 3] },
    Gray { lower: u8, upper: u8 },
}

impl AssignmentDomainV1 {
    fn cardinality(self) -> Option<u64> {
        match self {
            Self::Cube { lower, upper } => (0..3).try_fold(1_u64, |count, i| {
                count.checked_mul(u64::from(upper[i].checked_sub(lower[i])?) + 1)
            }),
            Self::Gray { lower, upper } => Some(u64::from(upper.checked_sub(lower)?) + 1),
        }
    }

    fn value(self, ordinal: u64) -> Srgb8 {
        match self {
            Self::Gray { lower, .. } => Srgb8::new([lower + ordinal as u8; 3]),
            Self::Cube { lower, upper } => {
                let blue = u64::from(upper[2]) - u64::from(lower[2]) + 1;
                let green = u64::from(upper[1]) - u64::from(lower[1]) + 1;
                Srgb8::new([
                    lower[0] + (ordinal / (blue * green)) as u8,
                    lower[1] + ((ordinal / blue) % green) as u8,
                    lower[2] + (ordinal % blue) as u8,
                ])
            }
        }
    }

    fn contains(self, rgb: Srgb8) -> bool {
        let [r, g, b] = rgb.bytes();
        match self {
            Self::Gray { lower, upper } => r == g && g == b && lower <= r && r <= upper,
            Self::Cube { lower, upper } => rgb
                .bytes()
                .iter()
                .enumerate()
                .all(|(i, value)| lower[i] <= *value && *value <= upper[i]),
        }
    }

    pub(super) fn identity_bytes(self) -> [u8; 7] {
        match self {
            Self::Cube { lower, upper } => [
                0, lower[0], lower[1], lower[2], upper[0], upper[1], upper[2],
            ],
            Self::Gray { lower, upper } => [1, lower, lower, lower, upper, upper, upper],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedTargetV1 {
    domain: AssignmentDomainV1,
    anchor: Srgb8,
    max_delta_bits: u64,
    preference: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputedAdmissionErrorV1 {
    InvalidDomain,
    InvalidTolerance,
    MixedSelection,
    EmptySelection,
    InvalidPreferenceOrder,
    CardinalityOverflow,
}

impl ComputedTargetV1 {
    // Фабрика недоступна старым wire/SDK и private-fixture ABI.
    #[cfg(test)]
    fn new(
        domain: AssignmentDomainV1,
        anchor: Srgb8,
        max_delta: f64,
        preference: u64,
    ) -> Result<Self, ComputedAdmissionErrorV1> {
        domain
            .cardinality()
            .ok_or(ComputedAdmissionErrorV1::InvalidDomain)?;
        if !max_delta.is_finite() || max_delta < 0.0 || !(max_delta * max_delta).is_finite() {
            return Err(ComputedAdmissionErrorV1::InvalidTolerance);
        }
        Ok(Self {
            domain,
            anchor,
            max_delta_bits: if max_delta == 0.0 {
                0
            } else {
                max_delta.to_bits()
            },
            preference,
        })
    }

    pub(super) const fn anchor(&self) -> Srgb8 {
        self.anchor
    }

    pub(super) fn identity_bytes(&self) -> [u8; 26] {
        let mut bytes = [0; 26];
        bytes[..7].copy_from_slice(&self.domain.identity_bytes());
        bytes[7..10].copy_from_slice(&self.anchor.bytes());
        bytes[10..18].copy_from_slice(&self.max_delta_bits.to_le_bytes());
        bytes[18..].copy_from_slice(&self.preference.to_le_bytes());
        bytes
    }
}

struct CompiledComputedTargetV1 {
    binding: CompiledPaintInputSlotV1,
    declaration: ComputedTargetV1,
    anchor_lab: [f64; 3],
    count: u64,
    suffix_count: u64,
}

pub(super) struct ComputedSelectionV1 {
    targets: Box<[CompiledComputedTargetV1]>,
    budget: u64,
    tuple_count: u64,
}

pub(super) fn compile_selection(
    graph: &CompiledAppearanceGraph,
    targets: &[Target],
    finite: Option<&MaterialisedSelectionV1>,
    budget: u64,
) -> Result<ComputedSelectionV1, ProgramCompileError> {
    let invalid = ProgramCompileError::InvalidComputedAssignment;
    if finite.is_some()
        || targets
            .iter()
            .any(|target| matches!(target.intent, TargetIntentV1::Finite(_)))
    {
        return Err(invalid(ComputedAdmissionErrorV1::MixedSelection));
    }
    let mut compiled = Vec::new();
    compiled
        .try_reserve_exact(targets.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for target in targets {
        let TargetIntentV1::Computed(declaration) = &target.intent else {
            continue;
        };
        let count = declaration
            .domain
            .cardinality()
            .ok_or(invalid(ComputedAdmissionErrorV1::InvalidDomain))?;
        let delta = f64::from_bits(declaration.max_delta_bits);
        if !delta.is_finite() || delta < 0.0 || !(delta * delta).is_finite() {
            return Err(invalid(ComputedAdmissionErrorV1::InvalidTolerance));
        }
        compiled.push(CompiledComputedTargetV1 {
            binding: graph
                .bind_paint_input(target_paint_input_id(target.id))
                .ok_or(ProgramCompileError::InternalInvariant)?,
            declaration: declaration.clone(),
            anchor_lab: lab(declaration.anchor)
                .map_err(|_| ProgramCompileError::InternalInvariant)?,
            count,
            suffix_count: 1,
        });
    }
    if compiled.is_empty() {
        return Err(invalid(ComputedAdmissionErrorV1::EmptySelection));
    }
    compiled.sort_unstable_by_key(|target| target.declaration.preference);
    if compiled
        .iter()
        .enumerate()
        .any(|(i, target)| target.declaration.preference != i as u64)
    {
        return Err(invalid(ComputedAdmissionErrorV1::InvalidPreferenceOrder));
    }
    let mut tuple_count = 1_u64;
    for target in compiled.iter_mut().rev() {
        target.suffix_count = tuple_count;
        tuple_count = tuple_count
            .checked_mul(target.count)
            .ok_or(invalid(ComputedAdmissionErrorV1::CardinalityOverflow))?;
    }
    Ok(ComputedSelectionV1 {
        targets: compiled.into_boxed_slice(),
        budget,
        tuple_count,
    })
}

fn lab(rgb: Srgb8) -> Result<[f64; 3], ()> {
    // Сохраняет порядок операций независимой исследовательской пробы: hex →
    // OKLCH → Lab. Здесь не заявляется универсальная вещественная минимальность.
    let [l, c, h] = crate::oklch_from_hex(&rgb.to_hex()).map_err(|_| ())?;
    let (sin, cos) = h.to_radians().sin_cos();
    Ok([l, c * cos, c * sin])
}

fn squared_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionV1 {
    AuthoredExact,
    Complete,
    BudgetExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SearchEvidenceV1 {
    completion: CompletionV1,
    visited: u64,
    examined: u64,
    covered: u64,
    tuple_count: u64,
}

struct SearchResultV1 {
    evidence: SearchEvidenceV1,
    values: Option<Vec<Srgb8>>,
}

#[cfg(test)]
thread_local! {
    static CORRUPT_FINAL_SOURCE: std::cell::Cell<Option<Srgb8>> = const { std::cell::Cell::new(None) };
}

fn buffer<T: Clone>(len: usize, value: T) -> Result<Vec<T>, ()> {
    let mut result = Vec::new();
    result.try_reserve_exact(len).map_err(|_| ())?;
    result.resize(len, value);
    Ok(result)
}

fn select<Evaluation>(
    selection: &ComputedSelectionV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<Evaluation>,
    scenario_set: NonEmptyScenarioSetV1<'_>,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let count = selection.targets.len();
    let allocate = |_| ProgramSessionEvaluationError::ResourceExhausted;
    let mut values = buffer(count, Srgb8::new([0; 3])).map_err(allocate)?;
    let mut best_values = buffer(count, Srgb8::new([0; 3])).map_err(allocate)?;
    let mut scores = buffer(count, 0.0).map_err(allocate)?;
    let mut best_scores = buffer(count, 0.0).map_err(allocate)?;
    let mut next = buffer(count, 0_u64).map_err(allocate)?;
    let mut evidence = SearchEvidenceV1 {
        completion: CompletionV1::BudgetExhausted,
        visited: 0,
        examined: 0,
        covered: 0,
        tuple_count: selection.tuple_count,
    };
    if selection.budget == 0 {
        return Ok(SearchResultV1 {
            evidence,
            values: None,
        });
    }
    values
        .iter_mut()
        .zip(selection.targets.iter())
        .for_each(|(value, target)| *value = target.declaration.anchor);
    evidence.visited += 1;
    if selection
        .targets
        .iter()
        .zip(&values)
        .all(|(target, &value)| target.declaration.domain.contains(value))
    {
        evidence.examined += 1;
        apply::<Evaluation>(selection, runtime, &values)?;
        if !scan_program_candidate(
            runtime,
            epoch,
            scenario_set,
            0,
            ProgramEvaluationPhaseV1::Hard,
            ProgramCandidateCollectionV1::none(),
            None,
        )? {
            evidence.completion = CompletionV1::AuthoredExact;
            return Ok(SearchResultV1 {
                evidence,
                values: Some(values),
            });
        }
    }
    let mut has_best = false;
    let mut depth = 0;
    loop {
        if next[depth] == selection.targets[depth].count {
            next[depth] = 0;
            if depth == 0 {
                break;
            }
            depth -= 1;
            continue;
        }
        if evidence.visited == selection.budget {
            return Ok(SearchResultV1 {
                evidence,
                values: None,
            });
        }
        evidence.visited += 1;
        let target = &selection.targets[depth];
        let value = target.declaration.domain.value(next[depth]);
        next[depth] += 1;
        values[depth] = value;
        scores[depth] = squared_distance(
            lab(value).map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?,
            target.anchor_lab,
        );
        let delta = f64::from_bits(target.declaration.max_delta_bits);
        let worse_prefix = has_best
            && compare_scores(&scores[..=depth], &best_scores[..=depth]) == Ordering::Greater;
        if scores[depth] > delta * delta || worse_prefix {
            evidence.covered += target.suffix_count;
            continue;
        }
        if depth + 1 < count {
            depth += 1;
            continue;
        }
        evidence.covered += 1;
        evidence.examined += 1;
        apply::<Evaluation>(selection, runtime, &values)?;
        if !scan_program_candidate(
            runtime,
            epoch,
            scenario_set,
            0,
            ProgramEvaluationPhaseV1::Hard,
            ProgramCandidateCollectionV1::none(),
            None,
        )? && (!has_best
            || compare_scores(&scores, &best_scores).then_with(|| values.cmp(&best_values))
                == Ordering::Less)
        {
            best_values.copy_from_slice(&values);
            best_scores.copy_from_slice(&scores);
            has_best = true;
        }
    }
    if evidence.covered != evidence.tuple_count {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    evidence.completion = CompletionV1::Complete;
    Ok(SearchResultV1 {
        evidence,
        values: has_best.then_some(best_values),
    })
}

fn compare_scores(left: &[f64], right: &[f64]) -> Ordering {
    left.iter()
        .zip(right)
        .map(|(a, b)| a.total_cmp(b))
        .find(|order| *order != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

fn apply<Evaluation>(
    selection: &ComputedSelectionV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    values: &[Srgb8],
) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    if values.len() != selection.targets.len() {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    for (target, &value) in selection.targets.iter().zip(values) {
        runtime
            .bindings
            .overwrite_paint_input_at(target.binding, EncodedPointPaintValueV1::opaque(value))
            .map_err(map_program_execution_binding_error)?;
    }
    Ok(())
}

struct ComputedStoppedV1 {
    identity: ComputedIdentityV1,
    observation: RevisionBoundObservationV1,
    search: SearchEvidenceV1,
}

impl session_private::EvidenceSealed for ComputedStoppedV1 {}
impl SessionEvidenceV1 for ComputedStoppedV1 {
    fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }
}

struct ComputedVerifiedV1<Evaluation: ProgramConstraintEvaluatorSetV1> {
    observation: RevisionBoundObservationV1,
    origin: CandidateOriginV1,
    search: SearchEvidenceV1,
    arena: ProgramEvaluationArenaLeaseV1<Evaluation>,
}

impl<Evaluation: ProgramConstraintEvaluatorSetV1> session_private::EvidenceSealed
    for ComputedVerifiedV1<Evaluation>
{
}
impl<Evaluation: ProgramConstraintEvaluatorSetV1> SessionEvidenceV1
    for ComputedVerifiedV1<Evaluation>
{
    fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }
}

#[cfg(test)]
struct ComputedSessionPlanV1<Evaluation: ProgramConstraintEvaluatorSetV1>(
    ProgramSessionPlan<Evaluation>,
);
#[cfg(test)]
impl<Evaluation: ProgramConstraintEvaluatorSetV1> session_private::PlanSealed
    for ComputedSessionPlanV1<Evaluation>
{
}
#[cfg(test)]
impl<Evaluation: ProgramConstraintEvaluatorSetV1> SessionPlanV1
    for ComputedSessionPlanV1<Evaluation>
{
    type OwnerLease = ProgramOwnerLeaseV1<Evaluation>;
    type Verified = ComputedVerifiedV1<Evaluation>;
    type Violation = ComputedStoppedV1;
    type Incomplete = ComputedStoppedV1;
    type Error = ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>;
    fn try_acquire_owner(&self) -> Option<Self::OwnerLease> {
        self.0.try_acquire_owner()
    }
    fn observation_schema<'a>(
        &'a self,
        owner: &'a Self::OwnerLease,
    ) -> &'a CanonicalObservationSchemaV1 {
        self.0.observation_schema(owner)
    }
    fn retire_verified(&mut self, evidence: Self::Verified) {
        let slot = evidence.observation.arena_slot();
        self.0
            .evaluation_arenas
            .restore(ProgramEvaluationArenaReturnV1 {
                slot,
                storage: evidence.arena.storage,
            });
    }
    fn evaluate(
        &mut self,
        owner: &Self::OwnerLease,
        observation: RevisionBoundObservationV1,
        _: Option<&Self::Verified>,
        _: SessionObservationBindingPermitV1,
    ) -> Result<SessionDecision<Self::Verified, Self::Violation, Self::Incomplete>, Self::Error>
    {
        let epoch = &owner.0;
        let (
            ProgramOwnerIdentityV1::Computed(identity),
            CompiledTargetSelectionV1::Computed(selection),
        ) = (epoch.content_identity, &epoch.target_selection)
        else {
            return Err(ProgramSessionEvaluationError::InternalInvariant);
        };
        let plan = &mut self.0;
        let mut arena = plan
            .evaluation_arenas
            .guard(observation.arena_slot())
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        let mut runtime = ProgramEvaluationRuntimeV1 {
            bindings: &mut plan.bindings,
            workspace: &mut plan.workspace,
            presentation_cache: &mut plan.presentation_cache,
            family_artifacts: &plan.family_artifacts,
        };
        let scenarios = NonEmptyScenarioSetV1::from_admitted(&observation)
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        let counts = prepare_program_evaluation_arena(epoch, scenarios, arena.storage_mut())?;
        let selected = select(selection, &mut runtime, epoch, scenarios)?;
        match selected.values {
            Some(values) => {
                apply::<Evaluation>(selection, &mut runtime, &values)?;
                #[cfg(test)]
                if let Some(corrupted) = CORRUPT_FINAL_SOURCE.with(|value| value.take()) {
                    runtime
                        .bindings
                        .overwrite_paint_input_at(
                            selection.targets[0].binding,
                            EncodedPointPaintValueV1::opaque(corrupted),
                        )
                        .map_err(map_program_execution_binding_error)?;
                }
                let origin = CandidateOriginV1::Computed {
                    identity,
                    values: values.into_boxed_slice(),
                };
                let result = collect_program_candidate_into(
                    &mut runtime,
                    epoch,
                    scenarios,
                    origin,
                    0,
                    arena.storage_mut(),
                    counts,
                    None,
                )?;
                let ProgramEvaluationOutcomeV1::Verified { origin } = result else {
                    return Err(ProgramSessionEvaluationError::InternalInvariant);
                };
                let CandidateOriginV1::Computed {
                    identity: selected_identity,
                    values,
                } = &origin
                else {
                    return Err(ProgramSessionEvaluationError::InternalInvariant);
                };
                // Допустимость другого RGB не доказывает обещанное preference.
                // После полного physical recheck проверяем тот же tuple в тех
                // же compiled slots, которые потребляла реальная композиция.
                if *selected_identity != identity
                    || selection
                        .targets
                        .iter()
                        .zip(values.iter())
                        .any(|(target, &value)| {
                            runtime.bindings.paint_input_at(target.binding)
                                != Some(EncodedPointPaintValueV1::opaque(value))
                        })
                {
                    return Err(ProgramSessionEvaluationError::InternalInvariant);
                }
                Ok(SessionDecision::Verified(ComputedVerifiedV1 {
                    observation,
                    origin,
                    search: selected.evidence,
                    arena: arena.into_lease(),
                }))
            }
            None => {
                let evidence = ComputedStoppedV1 {
                    identity,
                    observation,
                    search: selected.evidence,
                };
                if selected.evidence.completion == CompletionV1::Complete {
                    Ok(SessionDecision::Violation(evidence))
                } else {
                    Ok(SessionDecision::Incomplete(evidence))
                }
            }
        }
    }
}

#[cfg(test)]
struct ComputedProgramV1<Evaluation: ProgramConstraintEvaluatorSetV1> {
    owner: Shared<ProgramEpochV1<Evaluation>>,
}

#[cfg(test)]
impl<Evaluation: ProgramConstraintEvaluatorSetV1> ComputedProgramV1<Evaluation> {
    fn compile(mut program: Program<Evaluation>, budget: u64) -> Result<Self, ProgramCompileError> {
        program.computed_budget = Some(budget);
        prepare_program(program).map(|epoch| Self {
            owner: Shared::new(epoch),
        })
    }

    fn instantiate(
        &self,
        stream: ObservationStreamId,
    ) -> Result<Session<ComputedSessionPlanV1<Evaluation>>, ProgramSessionInstantiateError> {
        let plan = prepare_session_plan(&self.owner, FamilyArtifactBundleV2::empty())
            .map_err(|failure| failure.cause)?;
        Ok(Session::new(stream, ComputedSessionPlanV1(plan)))
    }
}

#[cfg(test)]
mod tests;
