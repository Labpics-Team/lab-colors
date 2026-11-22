//! Improve a complete feasible tuple without changing its domain or priority.
//! Every accepted move is assessed in its entire connected composition.

use super::*;

fn exchange_work(count: usize) -> u64 {
    let count = count as u64;
    count.saturating_mul(count.saturating_add(2))
}

pub(super) fn reserved_work(context: &ContextV1, budget: u64) -> u64 {
    let count = context.targets.len() as u64;
    if count < 2 {
        return 0;
    }
    // Two suffix passes have n*(n-1) coordinates in total. Each considers
    // the existing 256 tonal samples in every distinct local context.
    let repair = count
        .saturating_mul(count - 1)
        .saturating_mul(256)
        .saturating_mul(context.local_cases.order.len() as u64);
    repair
        .saturating_add(count)
        .saturating_add(exchange_work(context.targets.len()).saturating_mul(2))
        .min(budget / COORDINATE_BUDGET_DIVISOR)
}

pub(super) struct StateV1<'a> {
    pub(super) best_values: &'a mut [Srgb8],
    pub(super) best_scores: &'a mut [f64],
    pub(super) values: &'a mut [Srgb8],
    pub(super) scores: &'a mut [f64],
    pub(super) current_values: &'a mut [Srgb8],
    pub(super) current_scores: &'a mut [f64],
    pub(super) pool: &'a mut [Srgb8],
    pub(super) evidence: &'a mut SearchEvidenceV1,
    pub(super) budget: u64,
}

impl StateV1<'_> {
    fn assess<E: ProgramConstraintEvaluatorSetV1>(
        &mut self,
        context: &ContextV1,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
    ) -> Result<Option<usize>, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        if self.evidence.visited >= self.budget {
            return Ok(None);
        }
        let failures = super::assess(
            context,
            runtime,
            epoch,
            scenarios,
            self.values,
            self.scores,
            arena,
        )?;
        self.evidence.visited += 1;
        self.evidence.points += 1;
        self.evidence.examined += 1;
        Ok(Some(failures))
    }

    fn retain(&mut self, context: &ContextV1, failures: usize) {
        if failures == 0
            && compare_tuples(
                context,
                self.scores,
                self.values,
                self.best_scores,
                self.best_values,
            ) == Ordering::Less
        {
            self.best_scores.copy_from_slice(self.scores);
            self.best_values.copy_from_slice(self.values);
        }
    }

    fn exchange<E: ProgramConstraintEvaluatorSetV1>(
        &mut self,
        context: &ContextV1,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
    ) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let count = context.targets.len();
        let seed = context.local_cases.order[0];
        for index in 0..count {
            self.pool.copy_from_slice(self.best_values);
            let target = &context.targets[index];
            apply_targets::<E>(context.targets, runtime, self.best_values)?;
            let (backdrop, alpha) = physical_context(target, runtime, epoch, scenarios, seed)?;
            let preferred =
                inverse_hint(context.preferred_rgb[seed * count + index], backdrop, alpha);
            for ordinal in 0..count + 2 {
                let candidate = match ordinal {
                    ordinal if ordinal < count => self.pool[ordinal],
                    ordinal if ordinal == count => target.declaration.anchor,
                    _ => preferred,
                };
                if candidate == self.best_values[index]
                    || !context.domains[index].contains(candidate)
                    || self.pool[..ordinal.min(count)].contains(&candidate)
                {
                    continue;
                }
                self.values.copy_from_slice(self.best_values);
                self.values[index] = candidate;
                let Some(failures) = self.assess(context, runtime, epoch, scenarios, arena)? else {
                    return Ok(());
                };
                self.retain(context, failures);
            }
        }
        Ok(())
    }
}

pub(super) fn improve<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
    mut state: StateV1<'_>,
) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let count = context.targets.len();
    let seed = context.local_cases.order[0];
    let full_budget = state.budget;
    // Keep a final valid exchange available after a parent move has changed
    // its children's context. This reuses buffers and the original visit cap.
    state.budget = full_budget.saturating_sub(exchange_work(count));
    state.exchange(context, runtime, epoch, scenarios, arena)?;
    'proposals: for fixed in 0..count {
        let target = &context.targets[fixed];
        if target.declaration.intent.is_none() {
            continue;
        }
        apply_targets::<E>(context.targets, runtime, state.best_values)?;
        let (backdrop, alpha) = physical_context(target, runtime, epoch, scenarios, seed)?;
        let preferred = inverse_hint(context.preferred_rgb[seed * count + fixed], backdrop, alpha);
        if preferred == state.best_values[fixed] || !context.domains[fixed].contains(preferred) {
            continue;
        }
        state.values.copy_from_slice(state.best_values);
        state.values[fixed] = preferred;
        let Some(mut failures) = state.assess(context, runtime, epoch, scenarios, arena)? else {
            break;
        };
        if failures == 0 {
            state.retain(context, failures);
            continue;
        }
        if compare_scores(&state.scores[..=fixed], &state.best_scores[..=fixed])
            == Ordering::Greater
        {
            continue;
        }
        state.current_values.copy_from_slice(state.values);
        state.current_scores.copy_from_slice(state.scores);
        // A more preferred source may need its dependent colors to move before
        // it is feasible. Only the lower-priority suffix is free during repair;
        // no invalid intermediate tuple is ever published or retained as best.
        for pass in 0..2 {
            let mut changed = false;
            for position in fixed + 1..count {
                let index = if pass == 0 {
                    count + fixed - position
                } else {
                    position
                };
                let target = &context.targets[index];
                let Some(intent) = &target.declaration.intent else {
                    continue;
                };
                for &case in &context.local_cases.order {
                    state.values.copy_from_slice(state.current_values);
                    apply_targets::<E>(context.targets, runtime, state.values)?;
                    let (backdrop, alpha) =
                        physical_context(target, runtime, epoch, scenarios, case)?;
                    let source = if intent.identity.neutral() {
                        Srgb8::new([128; 3])
                    } else {
                        target.declaration.anchor
                    };
                    let mut previous = None;
                    for step in 0..=255_u32 {
                        let desired = color_at_lightness(source, f64::from(step) / 255.0);
                        let candidate = inverse_hint(desired, backdrop, alpha);
                        if previous == Some(candidate) {
                            continue;
                        }
                        previous = Some(candidate);
                        state.values.copy_from_slice(state.current_values);
                        state.values[index] = candidate;
                        let Some(next_failures) =
                            state.assess(context, runtime, epoch, scenarios, arena)?
                        else {
                            break 'proposals;
                        };
                        if next_failures < failures
                            || next_failures == failures
                                && compare_tuples(
                                    context,
                                    state.scores,
                                    state.values,
                                    state.current_scores,
                                    state.current_values,
                                ) == Ordering::Less
                        {
                            failures = next_failures;
                            state.current_values.copy_from_slice(state.values);
                            state.current_scores.copy_from_slice(state.scores);
                            changed = true;
                        }
                        state.retain(context, next_failures);
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }
    state.budget = full_budget;
    state.exchange(context, runtime, epoch, scenarios, arena)
}
