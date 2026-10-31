//! Вычисляемый selection существующего Program. Политика не знает смысл ID.
//! Области сокращаются до полного выбора; физика остаётся у Program.

use super::*;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ComputedIdentityV1(pub(super) [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssignmentDomainV1 {
    Cube { lower: [u8; 3], upper: [u8; 3] },
    Gray { lower: u8, upper: u8 },
}

impl AssignmentDomainV1 {
    fn bounds(self) -> ([u8; 3], [u8; 3]) {
        match self {
            Self::Cube { lower, upper } => (lower, upper),
            Self::Gray { lower, upper } => ([lower; 3], [upper; 3]),
        }
    }

    fn lower_score(self, anchor: [f64; 3]) -> f64 {
        let (lower, upper) = self.bounds();
        let (lower, upper) = crate::spaces::oklab::srgb8_box_oklab_bounds(lower, upper);
        let nearest = std::array::from_fn(|i| {
            if anchor[i] < lower[i] {
                lower[i]
            } else if anchor[i] > upper[i] {
                upper[i]
            } else {
                anchor[i]
            }
        });
        squared_distance(nearest, anchor)
    }

    fn split(self) -> (Self, Self) {
        match self {
            Self::Gray { lower, upper } => {
                let middle = lower + (upper - lower) / 2;
                (
                    Self::Gray {
                        lower,
                        upper: middle,
                    },
                    Self::Gray {
                        lower: middle + 1,
                        upper,
                    },
                )
            }
            Self::Cube { lower, upper } => {
                let mut axis = 0;
                for i in 1..3 {
                    if upper[i] - lower[i] > upper[axis] - lower[axis] {
                        axis = i;
                    }
                }
                let middle = lower[axis] + (upper[axis] - lower[axis]) / 2;
                let mut left_upper = upper;
                let mut right_lower = lower;
                left_upper[axis] = middle;
                right_lower[axis] = middle + 1;
                (
                    Self::Cube {
                        lower,
                        upper: left_upper,
                    },
                    Self::Cube {
                        lower: right_lower,
                        upper,
                    },
                )
            }
        }
    }

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
pub(crate) struct ComputedTargetV1 {
    domain: AssignmentDomainV1,
    anchor: Srgb8,
    max_delta_bits: u64,
    preference: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComputedAdmissionErrorV1 {
    InvalidDomain,
    InvalidTolerance,
    MixedSelection,
    EmptySelection,
    InvalidPreferenceOrder,
    CardinalityOverflow,
}

// Предел экспериментального синхронного профиля; не клиентская политика.
pub(crate) const EXECUTION_NODE_LIMIT_V1: u64 = 65_536;

pub(super) fn execution_profile_identity(budget: u64) -> [u8; 32] {
    const DOMAIN: &[u8] =
        b"labcolors.assignment-execution.v2.direct-oklab-f64.lex-rgb.region-dfs.leaf64.region-plus-point-budget.full-recheck\0";
    let mut bytes = [0; DOMAIN.len() + 8];
    bytes[..DOMAIN.len()].copy_from_slice(DOMAIN);
    bytes[DOMAIN.len()..].copy_from_slice(&budget.to_le_bytes());
    *crate::sha256::digest(&bytes).as_bytes()
}

impl ComputedTargetV1 {
    pub(crate) fn declared(domain: AssignmentDomainV1, anchor: Srgb8, max_delta: f64) -> Self {
        Self {
            domain,
            anchor,
            max_delta_bits: if max_delta == 0.0 {
                0
            } else {
                max_delta.to_bits()
            },
            preference: u64::MAX,
        }
    }

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
    execution_profile: [u8; 32],
    tuple_count: u64,
}

pub(super) fn assign_preference(
    targets: &mut [Target],
    order: Option<&[TargetId]>,
) -> Result<(), ProgramCompileError> {
    let invalid = |error| ProgramCompileError::InvalidComputedAssignment(error);
    let computed = targets
        .iter()
        .filter(|target| matches!(target.intent, TargetIntentV1::Computed(_)))
        .count();
    let Some(order) = order else {
        return if computed == 0 {
            Ok(())
        } else {
            Err(invalid(ComputedAdmissionErrorV1::InvalidPreferenceOrder))
        };
    };
    if computed == 0 {
        return Err(invalid(ComputedAdmissionErrorV1::EmptySelection));
    }
    if targets
        .iter()
        .any(|target| matches!(target.intent, TargetIntentV1::Finite(_)))
    {
        return Err(invalid(ComputedAdmissionErrorV1::MixedSelection));
    }
    if order.len() != computed {
        return Err(invalid(ComputedAdmissionErrorV1::InvalidPreferenceOrder));
    }
    for (ordinal, id) in order.iter().enumerate() {
        if order[..ordinal].contains(id) {
            return Err(invalid(ComputedAdmissionErrorV1::InvalidPreferenceOrder));
        }
        let Some(Target {
            intent: TargetIntentV1::Computed(value),
            ..
        }) = targets.iter_mut().find(|target| target.id == *id)
        else {
            return Err(invalid(ComputedAdmissionErrorV1::InvalidPreferenceOrder));
        };
        value.preference = ordinal as u64;
    }
    Ok(())
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
            anchor_lab: lab(declaration.anchor),
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
        execution_profile: execution_profile_identity(budget),
        tuple_count,
    })
}

fn lab(rgb: Srgb8) -> [f64; 3] {
    crate::spaces::oklab::srgb_linear_to_oklab(crate::spaces::srgb::srgb_linear_from_srgb8(rgb))
}

fn squared_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let [l, a, b] = std::array::from_fn::<_, 3, _>(|i| a[i] - b[i]);
    (l * l + a * a) + b * b
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
    regions: u64,
    points: u64,
    endpoint_evaluations: u64,
    examined: u64,
    covered: u64,
    tuple_count: u64,
}

struct SearchResultV1 {
    evidence: SearchEvidenceV1,
    values: Option<Vec<Srgb8>>,
}

#[derive(Clone, Copy)]
struct RegionNodeV1 {
    depth: usize,
    domain: AssignmentDomainV1,
    lower_score: f64,
    ordinal: Option<u64>,
}

fn region_node(
    depth: usize,
    domain: AssignmentDomainV1,
    selection: &ComputedSelectionV1,
    evidence: &mut SearchEvidenceV1,
) -> Option<RegionNodeV1> {
    if evidence.visited == selection.budget {
        return None;
    }
    evidence.visited += 1;
    evidence.regions += 1;
    Some(RegionNodeV1 {
        depth,
        domain,
        lower_score: domain.lower_score(selection.targets[depth].anchor_lab),
        ordinal: None,
    })
}

#[derive(Clone, Copy)]
enum RegionConstraintV1 {
    Wcag {
        slot: CompiledOccurrenceSlotV1,
        criterion: Wcag22CriterionV1,
    },
    Exact {
        slot: CompiledOccurrenceSlotV1,
        expected: Srgb8,
    },
}

impl RegionConstraintV1 {
    fn slot(self) -> CompiledOccurrenceSlotV1 {
        match self {
            Self::Wcag { slot, .. } | Self::Exact { slot, .. } => slot,
        }
    }
}

fn region_constraints<Evaluation: ProgramConstraintEvaluatorSetV1>(
    epoch: &ProgramEpochV1<Evaluation>,
) -> Result<Vec<RegionConstraintV1>, ()> {
    let mut result = Vec::new();
    if !epoch.graph.supports_rgb_box_endpoints() {
        return Ok(result);
    }
    result
        .try_reserve_exact(epoch.constraints.len())
        .map_err(|_| ())?;
    for constraint in &epoch.constraints {
        if !ProgramEvaluationPhaseV1::Hard.includes(constraint.mode) {
            continue;
        }
        let CompiledProgramConstraintBodyV1::VisibleUnary {
            slot, invocation, ..
        } = constraint.body
        else {
            continue;
        };
        match epoch.evaluator.constraint_content(invocation) {
            ProgramConstraintContentV1::Wcag22Srgb8 { criterion, .. } => {
                result.push(RegionConstraintV1::Wcag { slot, criterion })
            }
            ProgramConstraintContentV1::ExactSrgb8 { expected, .. } => {
                result.push(RegionConstraintV1::Exact { slot, expected })
            }
            _ => {}
        }
    }
    Ok(result)
}

struct RegionPruningV1 {
    constraints: Vec<RegionConstraintV1>,
    lower: Vec<([u8; 3], [u8; 3])>,
    endpoint_evaluations: u64,
}

impl RegionPruningV1 {
    fn may_pass<Evaluation: ProgramConstraintEvaluatorSetV1>(
        &mut self,
        selection: &ComputedSelectionV1,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<Evaluation>,
        scenario_set: NonEmptyScenarioSetV1<'_>,
        node: RegionNodeV1,
        values: &[Srgb8],
    ) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<Evaluation>>> {
        if self.constraints.is_empty() {
            return Ok(true);
        }
        let schema = epoch.observation_group.schema.as_slice();
        for case_index in 0..scenario_set.len().get() {
            let observed = scenario_set
                .physical_values(case_index)
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
            if observed.len() != schema.len() {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            }
            runtime
                .bindings
                .overwrite_surface_inputs_canonical(schema.iter().copied(), |i| observed[i].srgb8())
                .map_err(map_program_execution_binding_error)?;
            for high in [false, true] {
                for (i, target) in selection.targets.iter().enumerate() {
                    let value = if i < node.depth {
                        values[i]
                    } else {
                        let (lo, hi) = if i == node.depth {
                            node.domain
                        } else {
                            target.declaration.domain
                        }
                        .bounds();
                        Srgb8::new(if high { hi } else { lo })
                    };
                    runtime
                        .bindings
                        .overwrite_paint_input_at(
                            target.binding,
                            EncodedPointPaintValueV1::opaque(value),
                        )
                        .map_err(map_program_execution_binding_error)?;
                }
                let evaluation = epoch
                    .graph
                    .evaluate_admitted_into(runtime.bindings, runtime.workspace)
                    .map_err(map_program_execution_binding_error)?;
                self.endpoint_evaluations += 1;
                for (index, constraint) in self.constraints.iter().enumerate() {
                    let point = evaluation
                        .occurrence_at(constraint.slot())
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    if !high {
                        self.lower[index] = (point.visible(), point.certificate().backdrop_rgb());
                        continue;
                    }
                    let (visible, backdrop) = self.lower[index];
                    let possible = match constraint {
                        RegionConstraintV1::Wcag { criterion, .. } => {
                            crate::wcag22::rgb_boxes_may_pass(
                                (visible, point.visible()),
                                (backdrop, point.certificate().backdrop_rgb()),
                                *criterion,
                            )
                            .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?
                        }
                        RegionConstraintV1::Exact { expected, .. } => {
                            expected.bytes().iter().enumerate().all(|(i, value)| {
                                visible[i] <= *value && *value <= point.visible()[i]
                            })
                        }
                    };
                    if !possible {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }
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
    let constraints = region_constraints(epoch).map_err(allocate)?;
    let lower = buffer(constraints.len(), ([0; 3], [0; 3])).map_err(allocate)?;
    let mut pruning = RegionPruningV1 {
        constraints,
        lower,
        endpoint_evaluations: 0,
    };
    let mut stack = Vec::new();
    // log2 tuple cardinality <=64. Округление глубины трёх RGB-осей добавляет
    // менее трёх шагов на target; ещё один нужен для continuation leaf.
    let stack_limit = count
        .checked_mul(4)
        .and_then(|value| value.checked_add(65))
        .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
    stack
        .try_reserve_exact(stack_limit)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    let mut evidence = SearchEvidenceV1 {
        completion: CompletionV1::BudgetExhausted,
        visited: 0,
        regions: 0,
        points: 0,
        endpoint_evaluations: 0,
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
    evidence.points += 1;
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
    let Some(root) = region_node(
        0,
        selection.targets[0].declaration.domain,
        selection,
        &mut evidence,
    ) else {
        return Ok(SearchResultV1 {
            evidence,
            values: None,
        });
    };
    stack.push(root);
    while let Some(mut node) = stack.pop() {
        let depth = node.depth;
        let target = &selection.targets[depth];
        let volume = node
            .domain
            .cardinality()
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        if node.ordinal.is_none() {
            scores[depth] = node.lower_score;
            let delta = f64::from_bits(target.declaration.max_delta_bits);
            let worse = node.lower_score > delta * delta
                || has_best
                    && compare_scores(&scores[..=depth], &best_scores[..=depth])
                        == Ordering::Greater;
            let possible = !worse
                && pruning.may_pass(selection, runtime, epoch, scenario_set, node, &values)?;
            evidence.endpoint_evaluations = pruning.endpoint_evaluations;
            if !possible {
                evidence.covered += volume * target.suffix_count;
                continue;
            }
            if volume > 64 {
                let (left, right) = node.domain.split();
                let Some(left) = region_node(depth, left, selection, &mut evidence) else {
                    return Ok(SearchResultV1 {
                        evidence,
                        values: None,
                    });
                };
                let Some(right) = region_node(depth, right, selection, &mut evidence) else {
                    return Ok(SearchResultV1 {
                        evidence,
                        values: None,
                    });
                };
                let (first, second) = if left.lower_score <= right.lower_score {
                    (left, right)
                } else {
                    (right, left)
                };
                stack.push(second);
                stack.push(first);
                continue;
            }
            node.ordinal = Some(0);
            stack.push(node);
            continue;
        }
        if evidence.visited == selection.budget {
            return Ok(SearchResultV1 {
                evidence,
                values: None,
            });
        }
        evidence.visited += 1;
        evidence.points += 1;
        let ordinal = node
            .ordinal
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        let value = node.domain.value(ordinal);
        if ordinal + 1 < volume {
            stack.push(RegionNodeV1 {
                ordinal: Some(ordinal + 1),
                ..node
            });
        }
        values[depth] = value;
        scores[depth] = squared_distance(lab(value), target.anchor_lab);
        let delta = f64::from_bits(target.declaration.max_delta_bits);
        let worse_prefix = has_best
            && compare_scores(&scores[..=depth], &best_scores[..=depth]) == Ordering::Greater;
        if scores[depth] > delta * delta || worse_prefix {
            evidence.covered += target.suffix_count;
            continue;
        }
        if depth + 1 < count {
            let Some(next) = region_node(
                depth + 1,
                selection.targets[depth + 1].declaration.domain,
                selection,
                &mut evidence,
            ) else {
                return Ok(SearchResultV1 {
                    evidence,
                    values: None,
                });
            };
            stack.push(next);
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

pub(crate) struct ComputedStoppedV1 {
    pub(super) identity: ComputedIdentityV1,
    pub(super) observation: RevisionBoundObservationV1,
    search: SearchEvidenceV1,
    execution_profile: [u8; 32],
}

impl session_private::EvidenceSealed for ComputedStoppedV1 {}
impl SessionEvidenceV1 for ComputedStoppedV1 {
    fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }
}

pub(crate) struct ComputedVerifiedV1<Evaluation: ProgramConstraintEvaluatorSetV1> {
    pub(super) identity: ComputedIdentityV1,
    pub(super) observation: RevisionBoundObservationV1,
    values: Box<[Srgb8]>,
    search: SearchEvidenceV1,
    execution_profile: [u8; 32],
    pub(super) arena: ProgramEvaluationArenaLeaseV1<Evaluation>,
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

pub(super) fn evaluate<Evaluation>(
    plan: &mut ProgramSessionPlan<Evaluation>,
    epoch: &ProgramEpochV1<Evaluation>,
    observation: RevisionBoundObservationV1,
) -> ProgramSessionEvaluationResult<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    let (
        ProgramOwnerIdentityV1::Computed(identity),
        CompiledTargetSelectionV1::Computed(selection),
    ) = (epoch.content_identity, &epoch.target_selection)
    else {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    };
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
            } = origin
            else {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            };
            // Допустимость другого RGB не доказывает обещанное preference.
            // После полного physical recheck проверяем тот же tuple в тех
            // же compiled slots, которые потребляла реальная композиция.
            if selected_identity != identity
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
            Ok(SessionDecision::Verified(
                ProgramVerifiedOutcomeV1::Computed(ComputedVerifiedV1 {
                    identity,
                    execution_profile: selection.execution_profile,
                    observation,
                    values,
                    search: selected.evidence,
                    arena: arena.into_lease(),
                }),
            ))
        }
        None => {
            let evidence = ComputedStoppedV1 {
                identity,
                execution_profile: selection.execution_profile,
                observation,
                search: selected.evidence,
            };
            if selected.evidence.completion == CompletionV1::Complete {
                Ok(SessionDecision::Violation(
                    ProgramViolationOutcomeV1::Computed(evidence),
                ))
            } else {
                Ok(SessionDecision::Incomplete(evidence))
            }
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod qualification_tests;
