//! Authored Point composition in the ordinary Program selector. Proposal order
//! is heuristic; certified symmetric profiles alone retain a feasible tuple.
//! This byte-domain discovery is not a proof about the continuous source domain.

use super::reference_completion::PreparedReferenceCompletionV1;
use super::*;
use crate::certified_numeric::{CertifiedScalarV1, sort_descending_profile};

fn measured<E: ProgramConstraintEvaluatorSetV1>(
    value: Result<(), reference_completion::ReferenceCompletionErrorV1<ProgramEvaluatorError<E>>>,
) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    match value {
        Ok(()) => Ok(true),
        Err(reference_completion::ReferenceCompletionErrorV1::Numeric(_)) => Ok(false),
        Err(reference_completion::ReferenceCompletionErrorV1::ResourceExhausted) => {
            Err(ProgramSessionEvaluationError::ResourceExhausted)
        }
        Err(reference_completion::ReferenceCompletionErrorV1::Binding(error)) => {
            Err(map_program_execution_binding_error(error))
        }
        Err(reference_completion::ReferenceCompletionErrorV1::Evaluation(error)) => Err(error),
        Err(_) => Err(ProgramSessionEvaluationError::InternalInvariant),
    }
}

pub(in crate::program_session) fn execution_profile_identity(budget: u64) -> [u8; 32] {
    const DOMAIN: &[u8] = b"labcolors.composition-point.v2.certified-full-point.local-support.substitution-equivalent-leaf-profile.lex-minimax.source-frontier.component-budget.hard-region.byte-discovery.continuous-unfinished\0";
    let mut bytes = [0; DOMAIN.len() + 8];
    bytes[..DOMAIN.len()].copy_from_slice(DOMAIN);
    bytes[DOMAIN.len()..].copy_from_slice(&budget.to_le_bytes());
    *crate::sha256::digest(&bytes).as_bytes()
}

#[derive(Default)]
pub(super) struct BuffersV1 {
    values: Vec<Srgb8>,
    best: Vec<Srgb8>,
    working: Vec<Srgb8>,
    authored: Vec<Srgb8>,
    scores: Vec<f64>,
    primary: Vec<CertifiedScalarV1>,
    source: Vec<CertifiedScalarV1>,
    best_primary: Vec<CertifiedScalarV1>,
    best_source: Vec<CertifiedScalarV1>,
    left: Vec<CertifiedScalarV1>,
    right: Vec<CertifiedScalarV1>,
    sort: Vec<CertifiedScalarV1>,
}

fn copy<T: Copy>(destination: &mut Vec<T>, source: &[T]) -> Result<(), ()> {
    destination.clear();
    destination.try_reserve(source.len()).map_err(|_| ())?;
    destination.extend_from_slice(source);
    Ok(())
}

fn profile_order(a: &[CertifiedScalarV1], b: &[CertifiedScalarV1]) -> Option<Ordering> {
    for (&a, &b) in a.iter().zip(b) {
        if a.strictly_below(b) {
            return Some(Ordering::Less);
        }
        if b.strictly_below(a) {
            return Some(Ordering::Greater);
        }
        if !a.proves_equal(b) {
            return None;
        }
    }
    Some(Ordering::Equal)
}

fn sorted_order(buffers: &mut BuffersV1) -> Option<Ordering> {
    let zero = CertifiedScalarV1::exact(0.0).ok()?;
    buffers.sort.resize(buffers.left.len(), zero);
    sort_descending_profile(&mut buffers.left, &mut buffers.sort).ok()?;
    sort_descending_profile(&mut buffers.right, &mut buffers.sort).ok()?;
    profile_order(&buffers.left, &buffers.right)
}

fn compare<E: ProgramConstraintEvaluatorSetV1>(
    prepared: &PreparedReferenceCompletionV1<'_, '_, E>,
    component: &ComputedComponentV1,
    buffers: &mut BuffersV1,
    region: Option<RegionNodeV1>,
) -> Option<Ordering> {
    buffers.left.clear();
    buffers.right.clear();
    // A shared immutable expression can be cancelled from both multisets.
    // Matching non-singleton bounds, in contrast, never authorize cancellation.
    for variable in 0..component.point_variable_count() {
        let local = component.point_variable(variable);
        if prepared.same_point_expression(component, local, &buffers.values, &buffers.best, region)
        {
            continue;
        }
        for (a, b) in buffers
            .primary
            .chunks_exact(component.point_variable_count())
            .zip(
                buffers
                    .best_primary
                    .chunks_exact(component.point_variable_count()),
            )
        {
            buffers.left.push(a[variable]);
            buffers.right.push(b[variable]);
        }
    }
    match sorted_order(buffers)? {
        Ordering::Equal => {}
        order => return Some(order),
    }
    buffers.left.clear();
    buffers.right.clear();
    for variable in 0..component.point_variable_count() {
        let local = component.point_variable(variable);
        if region.is_none_or(|node| local < node.depth)
            && buffers.values[local] == buffers.best[local]
        {
            continue;
        }
        buffers.left.push(buffers.source[variable]);
        buffers.right.push(buffers.best_source[variable]);
    }
    sorted_order(buffers)
}

struct SearchV1<'a, 'context, 'program, 'scratch, E: ProgramConstraintEvaluatorSetV1> {
    context: &'a ContextV1<'context>,
    component: &'a ComputedComponentV1,
    prepared: &'a mut PreparedReferenceCompletionV1<'program, 'scratch, E>,
    buffers: &'a mut BuffersV1,
    evidence: SearchEvidenceV1,
    budget: u64,
    has_best: bool,
    best_measured: bool,
    unresolved: bool,
}

impl<E: ProgramConstraintEvaluatorSetV1> SearchV1<'_, '_, '_, '_, E> {
    fn synchronize(&mut self) {
        for local in 0..self.component.targets.len() {
            self.buffers.values[local] =
                self.buffers.values[self.component.point_representative(local)];
        }
    }

    fn assess(
        &mut self,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
    ) -> Result<Option<(usize, bool)>, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>>
    {
        if self.evidence.visited >= self.budget {
            return Ok(None);
        }
        self.evidence.visited += 1;
        self.evidence.points += 1;
        self.evidence.examined += 1;
        self.synchronize();
        let failures = assess_refined(
            self.context,
            runtime,
            epoch,
            scenarios,
            &self.buffers.values,
            &mut self.buffers.scores,
            arena,
        )?;
        if failures != 0 {
            return Ok(Some((failures, false)));
        }
        let measured = measured::<E>(self.prepared.measure_point_component(
            self.component,
            &self.buffers.values,
            None,
            &self.context.local_cases.order,
            &mut self.buffers.primary,
            &mut self.buffers.source,
        ))?;
        let order = if self.has_best && measured && self.best_measured {
            compare(self.prepared, self.component, self.buffers, None)
        } else {
            None
        };
        self.unresolved |= !measured || self.has_best && order.is_none();
        // At a proved P+S tie, canonical tuple order picks a representative;
        // it is not reported as a scientific improvement.
        let retain = !self.has_best
            || order == Some(Ordering::Less)
            || order == Some(Ordering::Equal) && self.buffers.values < self.buffers.best;
        if retain {
            copy(&mut self.buffers.best, &self.buffers.values)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
            copy(&mut self.buffers.best_primary, &self.buffers.primary)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
            copy(&mut self.buffers.best_source, &self.buffers.source)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
            self.has_best = true;
            self.best_measured = measured;
        }
        Ok(Some((0, retain)))
    }

    fn region_worse(
        &mut self,
        node: RegionNodeV1,
    ) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        if !self.has_best || !self.best_measured {
            return Ok(false);
        }
        if !measured::<E>(self.prepared.measure_point_component(
            self.component,
            &self.buffers.values,
            Some(node),
            &self.context.local_cases.order,
            &mut self.buffers.primary,
            &mut self.buffers.source,
        ))? {
            return Ok(false);
        }
        Ok(
            compare(self.prepared, self.component, self.buffers, Some(node))
                == Some(Ordering::Greater),
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::program_session) fn select_component<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
    prepared: &mut PreparedReferenceCompletionV1<'_, '_, E>,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let mut buffers = prepared.take_point_buffers();
    let result = search(
        selection,
        component,
        runtime,
        epoch,
        scenarios,
        arena,
        prepared,
        &mut buffers,
    );
    prepared.restore_point_buffers(buffers);
    result
}

#[allow(clippy::too_many_arguments)]
fn search<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
    prepared: &mut PreparedReferenceCompletionV1<'_, '_, E>,
    buffers: &mut BuffersV1,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let context = ContextV1::point::<E>(selection, component, scenarios)?;
    prepared
        .refresh_point_component(component, &context.local_cases.order)
        .map_err(|error| match error {
            reference_completion::ReferenceCompletionErrorV1::ResourceExhausted => {
                ProgramSessionEvaluationError::ResourceExhausted
            }
            _ => ProgramSessionEvaluationError::InternalInvariant,
        })?;
    let count = context.targets.len();
    let variables = component.point_variable_count();
    buffers.authored.clear();
    buffers
        .authored
        .try_reserve(count)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    buffers.authored.extend(
        context
            .targets
            .iter()
            .map(|target| target.declaration.anchor),
    );
    copy(&mut buffers.values, &buffers.authored)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    copy(&mut buffers.working, &buffers.authored)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    buffers.scores.resize(count, 0.0);
    let rows = variables
        .checked_mul(context.local_cases.order.len())
        .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
    for values in [&mut buffers.left, &mut buffers.right, &mut buffers.sort] {
        values.clear();
        values
            .try_reserve(rows.max(count))
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    }
    let mut state = SearchV1 {
        context: &context,
        component,
        prepared,
        buffers,
        evidence: SearchEvidenceV1 {
            completion: CompletionV1::BudgetExhausted,
            visited: 0,
            regions: 0,
            points: 0,
            endpoint_evaluations: 0,
            examined: 0,
            open_regions: 0,
        },
        budget: selection.budget,
        has_best: false,
        best_measured: false,
        unresolved: false,
    };
    let authored = state.assess(runtime, epoch, scenarios, arena)?;
    let authored_optimal = authored.is_some_and(|(failures, _)| failures == 0)
        && match state
            .prepared
            .authored_point_optimal(component, &context.local_cases.order)
        {
            Ok(proven) => proven,
            Err(error) => {
                measured::<E>(Err(error))?;
                false
            }
        };
    if authored_optimal {
        // Each primary term is zero or constant and the source profile is
        // zero by identity. The public source-domain
        // optimality remains Unfinished for this discovery implementation.
        state.evidence.completion = CompletionV1::AuthoredExact;
    } else {
        let seed_limit = (selection.budget / COORDINATE_BUDGET_DIVISOR).max(state.evidence.visited);
        // Independent contextual starts and physical coordinate repair share
        // the component's existing cap; no conditional subsolver runs here.
        for origin in 0..=context.local_cases.order.len() {
            if state.evidence.visited >= seed_limit {
                break;
            }
            let origin_limit = state.evidence.visited
                + (seed_limit - state.evidence.visited)
                    / (context.local_cases.order.len() + 1 - origin) as u64;
            copy(&mut state.buffers.values, &state.buffers.authored)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
            if origin > 0 {
                let case = context.local_cases.order[origin - 1];
                for (local, target) in context.targets.iter().enumerate() {
                    if component.point_representative(local) != local {
                        continue;
                    }
                    apply_targets::<E>(context.targets, runtime, &state.buffers.values)?;
                    let (backdrop, alpha) = physical_context(
                        target,
                        context.physical,
                        runtime,
                        epoch,
                        scenarios,
                        case,
                    )?;
                    state.buffers.values[local] =
                        inverse_hint(context.preferred_rgb[case * count + local], backdrop, alpha);
                }
            }
            let Some((mut failures, _)) = state.assess(runtime, epoch, scenarios, arena)? else {
                break;
            };
            copy(&mut state.buffers.working, &state.buffers.values)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
            let mut heuristic = state.buffers.scores.iter().copied().fold(0.0_f64, f64::max);
            for pass in 0..2 {
                for position in 0..variables {
                    let variable = if pass == 0 {
                        variables - 1 - position
                    } else {
                        position
                    };
                    let local = component.point_variable(variable);
                    let target = &context.targets[local];
                    let Some(intent) = target.effective_intent() else {
                        continue;
                    };
                    for &case in &context.local_cases.order {
                        apply_targets::<E>(context.targets, runtime, &state.buffers.working)?;
                        let (backdrop, alpha) = physical_context(
                            target,
                            context.physical,
                            runtime,
                            epoch,
                            scenarios,
                            case,
                        )?;
                        let mut previous = None;
                        for step in 0..=255 {
                            if state.evidence.visited >= origin_limit {
                                break;
                            }
                            let value = if target.declaration.reference_appearance {
                                color_at_lightness(
                                    target.declaration.anchor,
                                    f64::from(step) / 255.0,
                                )
                            } else {
                                tonal_hint(
                                    target,
                                    &intent,
                                    f64::from(step) / 255.0,
                                    backdrop,
                                    alpha,
                                )
                            };
                            if previous == Some(value) || !context.domains[local].contains(value) {
                                continue;
                            }
                            previous = Some(value);
                            copy(&mut state.buffers.values, &state.buffers.working)
                                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
                            state.buffers.values[local] = value;
                            let Some((next_failures, retained)) =
                                state.assess(runtime, epoch, scenarios, arena)?
                            else {
                                break;
                            };
                            let score =
                                state.buffers.scores.iter().copied().fold(0.0_f64, f64::max);
                            if retained
                                || next_failures < failures
                                || next_failures != 0
                                    && next_failures == failures
                                    && score < heuristic
                            {
                                copy(&mut state.buffers.working, &state.buffers.values).map_err(
                                    |_| ProgramSessionEvaluationError::ResourceExhausted,
                                )?;
                                failures = next_failures;
                                heuristic = score;
                            }
                        }
                    }
                }
            }
        }
        let constraints = region_constraints(epoch, context.constraint_indices)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        let mut bounds = BoundsV1 {
            constraint_lower: buffer(constraints.len(), RegionPointV1::default())
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?,
            constraints,
            lower: buffer(count, [0; 3])
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?,
            scores: buffer(count, 0.0)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?,
            changes_min: buffer(count, false)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?,
            tuple_min: buffer(count, Srgb8::new([0; 3]))
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?,
            evaluations: 0,
            appearance: None,
        };
        let mut stack = Vec::new();
        let capacity = context
            .domains
            .iter()
            .try_fold(1usize, |n, domain| n.checked_add(domain.split_depth() + 1))
            .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
        stack
            .try_reserve_exact(capacity)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        let seed = context.local_cases.order[0];
        if let Some(node) = region_node(
            0,
            context.domains[0],
            selection.budget,
            context.preferred[seed * count],
            &mut state.evidence,
        ) {
            stack.push(node);
        }
        let mut complete = !stack.is_empty();
        while let Some(mut node) = stack.pop() {
            state.synchronize();
            let depth = node.depth;
            let volume = node
                .domain
                .cardinality()
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
            if node.ordinal.is_none() {
                let may_pass = bounds.may_pass(
                    &context,
                    runtime,
                    epoch,
                    scenarios,
                    node,
                    &state.buffers.values,
                )?;
                state.evidence.endpoint_evaluations = bounds.evaluations;
                if !may_pass || state.region_worse(node)? {
                    state
                        .evidence
                        .close_region()
                        .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                    continue;
                }
                if volume > 16 {
                    let (a, b) = node.domain.split();
                    let preferred = context.preferred[seed * count + depth];
                    let Some(a) =
                        region_node(depth, a, selection.budget, preferred, &mut state.evidence)
                    else {
                        complete = false;
                        break;
                    };
                    let Some(b) =
                        region_node(depth, b, selection.budget, preferred, &mut state.evidence)
                    else {
                        complete = false;
                        break;
                    };
                    state
                        .evidence
                        .close_region()
                        .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                    let (first, second) = if a.lower_score <= b.lower_score {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    stack.push(second);
                    stack.push(first);
                } else {
                    node.ordinal = Some(0);
                    stack.push(node);
                }
                continue;
            }
            if state.evidence.visited >= selection.budget {
                complete = false;
                break;
            }
            let ordinal = node.ordinal.unwrap();
            if ordinal + 1 < volume {
                stack.push(RegionNodeV1 {
                    ordinal: Some(ordinal + 1),
                    ..node
                });
            } else {
                state
                    .evidence
                    .close_region()
                    .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
            }
            state.buffers.values[depth] = node.domain.value(ordinal);
            let next_depth =
                (depth + 1..count).find(|&local| component.point_representative(local) == local);
            if let Some(next_depth) = next_depth {
                state.evidence.visited += 1;
                state.evidence.points += 1;
                if let Some(next) = region_node(
                    next_depth,
                    context.domains[next_depth],
                    selection.budget,
                    context.preferred[seed * count + next_depth],
                    &mut state.evidence,
                ) {
                    stack.push(next);
                } else {
                    complete = false;
                    break;
                }
            } else {
                state.assess(runtime, epoch, scenarios, arena)?;
            }
        }
        if complete {
            state.evidence.completion = CompletionV1::Complete;
        }
    }
    let values = if state.has_best {
        let mut values = Vec::new();
        copy(&mut values, &state.buffers.best)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        Some(values)
    } else {
        None
    };
    let reference_selections = if let Some(values) = &values {
        if context
            .targets
            .iter()
            .any(|target| target.reference_backdrop.is_some())
        {
            Some(reference_search::point_reports(
                &context, runtime, epoch, scenarios, values,
            )?)
        } else {
            None
        }
    } else {
        None
    };
    Ok(SearchResultV1 {
        values,
        evidence: state.evidence,
        reference_selections,
    })
}

#[cfg(test)]
pub(super) mod tests;
