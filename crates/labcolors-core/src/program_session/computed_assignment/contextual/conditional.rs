//! Condition remaining factors on one candidate source, without equating uses.
//! A unique zero minimum supports early optimality. A separately searched
//! first objective supports only improvement of the fully assessed composition.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ModeV1 {
    PreferredMinimum,
    LocalPrefix,
}

pub(super) struct CandidateV1<'a, 'context> {
    pub(super) context: &'a ContextV1<'context>,
    pub(super) values: &'a mut [Srgb8],
    pub(super) scores: &'a mut [f64],
    pub(super) evidence: &'a mut SearchEvidenceV1,
    pub(super) mode: ModeV1,
}

fn select_task<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    task: &ComputedComponentV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    if task
        .targets
        .iter()
        .any(|&index| selection.targets[index].declaration.intent.is_some())
    {
        select_component_inner(selection, task, runtime, epoch, scenarios, arena, false)
    } else {
        super::super::select_component(selection, task, runtime, epoch, scenarios)
    }
}

pub(super) fn attempt<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
    candidate: CandidateV1<'_, '_>,
) -> Result<Option<SearchEvidenceV1>, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let context = candidate.context;
    let count = context.targets.len();
    if count < 2 {
        return Ok(None);
    }
    let first_case = *context
        .local_cases
        .order
        .first()
        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
    let preferred = context.preferred_rgb[first_case * count];
    let target = &context.targets[0];
    let delta = f64::from_bits(target.declaration.max_delta_bits);
    let unique_minimum = context.opaque[0]
        && (0..scenarios.len().get()).all(|case| context.preferred_rgb[case * count] == preferred)
        && context.domains[0].contains(preferred)
        && squared_distance(lab(preferred), target.anchor_lab) <= delta * delta
        && target
            .effective_intent()
            .is_none_or(|intent| intent.identity.contains(preferred));
    // Each opaque occurrence is admitted as its own source's appearance.
    // Therefore distance zero has exactly one source, in every physical case.
    let preferred_probe = candidate.mode == ModeV1::PreferredMinimum;
    if preferred_probe && !unique_minimum
        || candidate
            .evidence
            .visited
            .checked_add(if preferred_probe { 2 } else { 1 })
            .is_none_or(|visited| visited > selection.budget)
    {
        return Ok(None);
    }
    // The first source is only a proposal. Its local RGB tie never outranks
    // the remaining visible objectives in the full comparison below.
    let prefix = 1;
    let mut extra = SearchEvidenceV1 {
        completion: CompletionV1::Complete,
        visited: 0,
        regions: 0,
        points: 0,
        endpoint_evaluations: 0,
        examined: 0,
        open_regions: 0,
    };
    let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
    let residual =
        components::conditioned(selection, component, prefix, epoch).map_err(allocation)?;
    let mut values = buffer(candidate.values.len(), Srgb8::new([0; 3])).map_err(allocation)?;
    values.copy_from_slice(candidate.values);
    if preferred_probe {
        values[0] = preferred;
    }
    apply_targets::<E>(context.targets, runtime, &values)?;
    if !preferred_probe {
        if let Some(leading) =
            components::leading(selection, component, epoch).map_err(allocation)?
        {
            let result = select_task(selection, &leading, runtime, epoch, scenarios, arena)?;
            components::merge_evidence(&mut extra, result.evidence).map_err(allocation)?;
            let Some(local) = result.values else {
                // A failed bounded proposal is not a new whole-program result.
                // Its work remains counted, and the incumbent stays intact.
                extra.completion = CompletionV1::BudgetExhausted;
                apply_targets::<E>(context.targets, runtime, candidate.values)?;
                return Ok(Some(extra));
            };
            if local.len() != 1 {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            }
            values[0] = local[0];
            apply_targets::<E>(context.targets, runtime, &values)?;
        }
    }
    // A direct preferred proposal has not passed its fixed conditions yet.
    // The local selector checks those same factors; when its objective still
    // depends on the suffix, we keep the already verified incumbent's source.
    if preferred_probe && !residual.fixed_constraints.is_empty() {
        let failed = scan_program_candidate_scoped(
            runtime,
            epoch,
            scenarios,
            0,
            ProgramEvaluationPhaseV1::Hard,
            ProgramCandidateCollectionV1::none(),
            ProgramCandidateScanV1::Constraints {
                indices: &residual.fixed_constraints,
                physical: Some(&component.physical),
            },
        )?;
        candidate.evidence.visited += 1;
        candidate.evidence.points += 1;
        candidate.evidence.examined += 1;
        if failed {
            apply_targets::<E>(context.targets, runtime, candidate.values)?;
            return Ok(None);
        }
    }
    for task in &residual.components {
        // No nested probe: at most one prefix and n-1 residual searches run
        // in addition to the original fallback and its accounted rechecks.
        let result = select_task(selection, task, runtime, epoch, scenarios, arena)?;
        components::merge_evidence(&mut extra, result.evidence).map_err(allocation)?;
        if let Some(local) = result.values {
            if local.len() != task.targets.len() {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            }
            for (&target, value) in task.targets.iter().zip(local) {
                let index = component
                    .targets
                    .binary_search(&target)
                    .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                values[index] = value;
            }
        } else {
            // This is a conditional attempt, not a proof about the unrestricted
            // problem. Keep the known feasible tuple and let full search run.
            extra.completion = CompletionV1::BudgetExhausted;
            break;
        }
    }
    let mut scores = buffer(candidate.scores.len(), 0.0).map_err(allocation)?;
    let failures = assess(
        context,
        runtime,
        epoch,
        scenarios,
        &values,
        &mut scores,
        arena,
    )?;
    candidate.evidence.visited += 1;
    candidate.evidence.points += 1;
    candidate.evidence.examined += 1;
    if failures == 0
        && (!preferred_probe || scores[..prefix].iter().all(|score| *score == 0.0))
        && compare_tuples(
            context,
            &scores,
            &values,
            candidate.scores,
            candidate.values,
        ) != Ordering::Greater
    {
        candidate.values.copy_from_slice(&values);
        candidate.scores.copy_from_slice(&scores);
    } else {
        extra.completion = CompletionV1::BudgetExhausted;
    }
    // A local nonzero minimum can have RGB ties that the remaining visible
    // objectives must decide. Transparent sources can also share a visible
    // result. Neither local optimality nor a selected incumbent proves the
    // global optimum; only the whole comparison may retain this proposal.
    if !preferred_probe {
        extra.completion = CompletionV1::BudgetExhausted;
    }
    apply_targets::<E>(context.targets, runtime, candidate.values)?;
    Ok(Some(extra))
}
