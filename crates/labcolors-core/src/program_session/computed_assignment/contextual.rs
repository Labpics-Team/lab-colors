//! Поиск контекстных применений: физический граф, характер, предпочтение.
//! Тональные начальные приближения ускоряют поиск, но не сужают его область.

use super::*;
use crate::color_intent::color_at_lightness;

pub(super) fn execution_profile_identity(budget: u64) -> [u8; 32] {
    const DOMAIN: &[u8] = b"labcolors.assignment-execution.v4.visible-oklab-max-cases.lex-rgb.context-anchor.two-coordinate-tonal-seeds256.quarter-budget.full-domain-dfs.endpoints.full-recheck\0";
    let mut bytes = [0; DOMAIN.len() + 8];
    bytes[..DOMAIN.len()].copy_from_slice(DOMAIN);
    bytes[DOMAIN.len()..].copy_from_slice(&budget.to_le_bytes());
    *crate::sha256::digest(&bytes).as_bytes()
}

struct ContextV1<'a> {
    selection: &'a ComputedSelectionV1,
    domains: Vec<AssignmentDomainV1>,
    preferred: Vec<[f64; 3]>,
    preferred_rgb: Vec<Srgb8>,
    opaque: bool,
}

impl<'a> ContextV1<'a> {
    fn new<E: ProgramConstraintEvaluatorSetV1>(
        selection: &'a ComputedSelectionV1,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
    ) -> Result<Self, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let count = selection.targets.len();
        let cells = count
            .checked_mul(scenarios.len().get())
            .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
        let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
        let mut preferred = buffer(cells, [0.0; 3]).map_err(allocation)?;
        let mut preferred_rgb = buffer(cells, Srgb8::new([0; 3])).map_err(allocation)?;
        let mut opaque = buffer(count, true).map_err(allocation)?;
        let schema = epoch.observation_group.schema.as_slice();
        let mut paints = epoch
            .graph
            .prepare_paints_for_surface_cases(
                runtime.bindings,
                schema.iter().copied(),
                runtime.workspace,
            )
            .map_err(map_program_execution_binding_error)?;
        for case in 0..scenarios.len().get() {
            let inputs = scenarios
                .physical_values(case)
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
            let evaluation = paints.evaluate_surface_inputs_canonical(|i| inputs[i].srgb8());
            for (index, target) in selection.targets.iter().enumerate() {
                let mut rgb = target.declaration.anchor;
                if let Some(intent) = &target.declaration.intent {
                    if let (Some(curve), Some(input)) = (&intent.tone, target.tone_input) {
                        let source = if intent.identity.neutral() {
                            Srgb8::new([128; 3])
                        } else {
                            rgb
                        };
                        rgb = curve.preferred_color(source, inputs[input].srgb8());
                    }
                }
                let cell = case * count + index;
                preferred_rgb[cell] = rgb;
                preferred[cell] = lab(rgb);
                if let Some(slot) = target.appearance {
                    let point = evaluation
                        .occurrence_at(slot)
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    opaque[index] &= point.certificate().subject_opacity().value() == 1.0;
                }
            }
        }
        let mut domains = Vec::new();
        domains
            .try_reserve_exact(count)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        for (index, target) in selection.targets.iter().enumerate() {
            domains.push(
                if opaque[index]
                    && target
                        .declaration
                        .intent
                        .as_ref()
                        .is_some_and(|intent| intent.identity.neutral())
                {
                    // У opaque собственного применения source и visible совпадают.
                    AssignmentDomainV1::Gray {
                        lower: 0,
                        upper: 255,
                    }
                } else {
                    target.declaration.domain
                },
            );
        }
        Ok(Self {
            selection,
            domains,
            preferred,
            preferred_rgb,
            opaque: opaque.into_iter().all(|v| v),
        })
    }
}

fn intent_scores<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    scores: &mut [f64],
) -> Result<usize, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let selection = context.selection;
    scores.fill(0.0);
    let count = selection.targets.len();
    let schema = epoch.observation_group.schema.as_slice();
    let mut violations = 0;
    let mut paints = epoch
        .graph
        .prepare_paints_for_surface_cases(
            runtime.bindings,
            schema.iter().copied(),
            runtime.workspace,
        )
        .map_err(map_program_execution_binding_error)?;
    for case in 0..scenarios.len().get() {
        let inputs = scenarios
            .physical_values(case)
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        let evaluation = paints.evaluate_surface_inputs_canonical(|i| inputs[i].srgb8());
        for (index, target) in selection.targets.iter().enumerate() {
            let rgb = if let Some(slot) = target.appearance {
                Srgb8::new(
                    evaluation
                        .occurrence_at(slot)
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?
                        .visible(),
                )
            } else {
                values[index]
            };
            let point_lab = lab(rgb);
            let delta = f64::from_bits(target.declaration.max_delta_bits);
            let identity_failed = target
                .declaration
                .intent
                .as_ref()
                .is_some_and(|intent| !intent.identity.contains(rgb));
            if identity_failed || squared_distance(point_lab, target.anchor_lab) > delta * delta {
                violations += 1;
            }
            scores[index] = scores[index].max(squared_distance(
                point_lab,
                context.preferred[case * count + index],
            ));
        }
    }
    Ok(violations)
}

fn assess<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    scores: &mut [f64],
    arena: &mut ProgramEvaluationArenaV1<E>,
) -> Result<usize, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let selection = context.selection;
    apply::<E>(selection, runtime, values)?;
    let intent_violations = intent_scores(context, runtime, epoch, scenarios, values, scores)?;
    arena.clear();
    scan_program_candidate(
        runtime,
        epoch,
        scenarios,
        0,
        ProgramEvaluationPhaseV1::Hard,
        ProgramCandidateCollectionV1 {
            evidence: ProgramConstraintEvidenceCaptureV1::Report {
                cells: &mut arena.cells,
                relation_members: &mut arena.relation_members,
            },
            outputs: None,
            point_causal: None,
        },
        None,
    )?;
    Ok(intent_violations
        + context
            .domains
            .iter()
            .zip(values)
            .filter(|(domain, value)| !domain.contains(**value))
            .count()
        + arena
            .cells
            .iter()
            .filter(|cell| cell.result().is_violation())
            .count())
}

/// Ближайший source по каждому encoded каналу — только начальное приближение.
/// Проверка всей композиции и всех требований всегда следует отдельно.
fn inverse_hint(desired: Srgb8, backdrop: [u8; 3], alpha: f64) -> Srgb8 {
    if alpha == 1.0 {
        return desired;
    }
    if alpha == 0.0 {
        return Srgb8::new([0; 3]);
    }
    Srgb8::new(std::array::from_fn(|index| {
        let goal = desired.bytes()[index];
        let evaluate =
            |source| crate::composition::source_over_channel_srgb8(source, alpha, backdrop[index]);
        let (mut lo, mut hi) = (0_u8, 255_u8);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if evaluate(mid) < goal {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo > 0 && evaluate(lo - 1).abs_diff(goal) <= evaluate(lo).abs_diff(goal) {
            lo - 1
        } else {
            lo
        }
    }))
}

fn physical_context<E: ProgramConstraintEvaluatorSetV1>(
    target: &CompiledComputedTargetV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    case: usize,
) -> Result<([u8; 3], f64), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let Some(slot) = target.appearance else {
        return Ok(([0; 3], 1.0));
    };
    let inputs = scenarios
        .physical_values(case)
        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
    let mut paints = epoch
        .graph
        .prepare_paints_for_surface_cases(
            runtime.bindings,
            epoch.observation_group.schema.as_slice().iter().copied(),
            runtime.workspace,
        )
        .map_err(map_program_execution_binding_error)?;
    let evaluation = paints.evaluate_surface_inputs_canonical(|i| inputs[i].srgb8());
    let point = evaluation
        .occurrence_at(slot)
        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
    Ok((
        point.certificate().backdrop_rgb(),
        point.certificate().subject_opacity().value(),
    ))
}

pub(super) fn recheck_intents<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let context = ContextV1::new(selection, runtime, epoch, scenarios)?;
    let mut scores =
        buffer(values.len(), 0.0).map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    if context
        .domains
        .iter()
        .zip(values)
        .any(|(domain, value)| !domain.contains(*value))
        || intent_scores(&context, runtime, epoch, scenarios, values, &mut scores)? != 0
    {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    Ok(())
}

struct BoundsV1 {
    lower: Vec<[u8; 3]>,
    constraints: Vec<RegionConstraintV1>,
    constraint_lower: Vec<([u8; 3], [u8; 3])>,
    scores: Vec<f64>,
    tuple_min: Vec<Srgb8>,
    evaluations: u64,
}

impl BoundsV1 {
    fn may_pass<E: ProgramConstraintEvaluatorSetV1>(
        &mut self,
        context: &ContextV1,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        node: RegionNodeV1,
        values: &[Srgb8],
    ) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let selection = context.selection;
        let count = selection.targets.len();
        self.scores.fill(0.0);
        for (index, domain) in context.domains.iter().enumerate() {
            let lo = if index < node.depth {
                values[index].bytes()
            } else if index == node.depth {
                node.domain.bounds().0
            } else {
                domain.bounds().0
            };
            self.tuple_min[index] = Srgb8::new(lo);
        }
        for case in 0..scenarios.len().get() {
            let inputs = scenarios
                .physical_values(case)
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
            for high in [false, true] {
                for (index, target) in selection.targets.iter().enumerate() {
                    let rgb = if index < node.depth {
                        values[index]
                    } else {
                        let (lo, hi) = if index == node.depth {
                            node.domain
                        } else {
                            context.domains[index]
                        }
                        .bounds();
                        Srgb8::new(if high { hi } else { lo })
                    };
                    runtime
                        .bindings
                        .overwrite_paint_input_at(
                            target.binding,
                            EncodedPointPaintValueV1::opaque(rgb),
                        )
                        .map_err(map_program_execution_binding_error)?;
                }
                let mut paints = epoch
                    .graph
                    .prepare_paints_for_surface_cases(
                        runtime.bindings,
                        epoch.observation_group.schema.as_slice().iter().copied(),
                        runtime.workspace,
                    )
                    .map_err(map_program_execution_binding_error)?;
                let evaluation = paints.evaluate_surface_inputs_canonical(|i| inputs[i].srgb8());
                self.evaluations += 1;
                for (index, constraint) in self.constraints.iter().enumerate() {
                    let point = evaluation
                        .occurrence_at(constraint.slot())
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    if !high {
                        self.constraint_lower[index] =
                            (point.visible(), point.certificate().backdrop_rgb());
                    } else {
                        let (visible, backdrop) = self.constraint_lower[index];
                        if !constraint
                            .may_pass(
                                (visible, point.visible()),
                                (backdrop, point.certificate().backdrop_rgb()),
                            )
                            .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?
                        {
                            return Ok(false);
                        }
                    }
                }
                for (index, target) in selection.targets.iter().enumerate() {
                    let rgb = if let Some(slot) = target.appearance {
                        evaluation
                            .occurrence_at(slot)
                            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?
                            .visible()
                    } else if index < node.depth {
                        values[index].bytes()
                    } else {
                        let (lo, hi) = if index == node.depth {
                            node.domain
                        } else {
                            context.domains[index]
                        }
                        .bounds();
                        if high { hi } else { lo }
                    };
                    if !high {
                        self.lower[index] = rgb;
                        continue;
                    }
                    let lower = self.lower[index];
                    if target
                        .declaration
                        .intent
                        .as_ref()
                        .is_some_and(|intent| !intent.identity.may_intersect_rgb_box(lower, rgb))
                    {
                        return Ok(false);
                    }
                    let domain = AssignmentDomainV1::Cube { lower, upper: rgb };
                    let delta = f64::from_bits(target.declaration.max_delta_bits);
                    if domain.lower_score(target.anchor_lab) > delta * delta {
                        return Ok(false);
                    }
                    self.scores[index] = self.scores[index]
                        .max(domain.lower_score(context.preferred[case * count + index]));
                }
            }
        }
        Ok(true)
    }
}

pub(super) fn select_contextual<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let count = selection.targets.len();
    let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
    let mut values = buffer(count, Srgb8::new([0; 3])).map_err(allocation)?;
    for (value, target) in values.iter_mut().zip(selection.targets.iter()) {
        *value = target.declaration.anchor;
    }
    let mut evidence = SearchEvidenceV1 {
        completion: CompletionV1::BudgetExhausted,
        visited: 0,
        regions: 0,
        points: 0,
        endpoint_evaluations: 0,
        examined: 0,
        open_regions: 0,
    };
    if selection.budget == 0 {
        return Ok(SearchResultV1 {
            evidence,
            values: None,
        });
    }
    apply::<E>(selection, runtime, &values)?;
    let context = ContextV1::new(selection, runtime, epoch, scenarios)?;
    let mut scores = buffer(count, 0.0).map_err(allocation)?;
    let mut best_scores = buffer(count, 0.0).map_err(allocation)?;
    let mut best_values = buffer(count, Srgb8::new([0; 3])).map_err(allocation)?;
    let mut current_scores = buffer(count, 0.0).map_err(allocation)?;
    let mut current_values = buffer(count, Srgb8::new([0; 3])).map_err(allocation)?;
    let mut has_best = false;

    // Сначала вся согласованная опора. Для opaque она единственна при нулевой
    // дистанции; при alpha продолжаем поиск, поскольку source может быть не один.
    let mut current_failures = assess(
        &context,
        runtime,
        epoch,
        scenarios,
        &values,
        &mut scores,
        arena,
    )?;
    evidence.visited += 1;
    evidence.points += 1;
    evidence.examined += 1;
    current_scores.copy_from_slice(&scores);
    current_values.copy_from_slice(&values);
    if current_failures == 0 {
        best_scores.copy_from_slice(&scores);
        best_values.copy_from_slice(&values);
        has_best = true;
        if context.opaque && scores.iter().all(|score| *score == 0.0) {
            evidence.completion = CompletionV1::AuthoredExact;
            return Ok(SearchResultV1 {
                evidence,
                values: Some(values),
            });
        }
    }

    // Контекстные опоры строятся внутри Core. Изменение parent учитывается
    // физическим графом перед приготовлением source следующего применения.
    if evidence.visited < selection.budget {
        for (index, target) in selection.targets.iter().enumerate() {
            if target.declaration.intent.is_none() {
                continue;
            }
            apply::<E>(selection, runtime, &values)?;
            let (backdrop, alpha) = physical_context(target, runtime, epoch, scenarios, 0)?;
            values[index] = inverse_hint(context.preferred_rgb[index], backdrop, alpha);
        }
        let failures = assess(
            &context,
            runtime,
            epoch,
            scenarios,
            &values,
            &mut scores,
            arena,
        )?;
        evidence.visited += 1;
        evidence.points += 1;
        evidence.examined += 1;
        if failures < current_failures
            || failures == current_failures
                && compare_scores(&scores, &current_scores)
                    .then_with(|| values.cmp(&current_values))
                    == Ordering::Less
        {
            current_failures = failures;
            current_scores.copy_from_slice(&scores);
            current_values.copy_from_slice(&values);
        }
        if failures == 0
            && (!has_best
                || compare_scores(&scores, &best_scores).then_with(|| values.cmp(&best_values))
                    == Ordering::Less)
        {
            has_best = true;
            best_scores.copy_from_slice(&scores);
            best_values.copy_from_slice(&values);
        }
        if failures == 0 && context.opaque && scores.iter().all(|score| *score == 0.0) {
            evidence.completion = CompletionV1::AuthoredExact;
            return Ok(SearchResultV1 {
                evidence,
                values: Some(values),
            });
        }
    }

    // Два прохода ремонта полного tuple. Число нарушений служит только порядку
    // начальных приближений; готовый результат требует ноль нарушений каждого закона.
    let seed_limit = (selection.budget / 4).max(evidence.visited);
    for _pass in 0..2 {
        let mut changed = false;
        for (index, target) in selection.targets.iter().enumerate() {
            let Some(intent) = &target.declaration.intent else {
                continue;
            };
            for case in 0..scenarios.len().get() {
                if evidence.visited >= seed_limit {
                    break;
                }
                values.copy_from_slice(&current_values);
                apply::<E>(selection, runtime, &values)?;
                let (backdrop, alpha) = physical_context(target, runtime, epoch, scenarios, case)?;
                let source = if intent.identity.neutral() {
                    Srgb8::new([128; 3])
                } else {
                    target.declaration.anchor
                };
                let mut previous = None;
                for step in 0..=255_u32 {
                    if evidence.visited >= seed_limit {
                        break;
                    }
                    let desired = color_at_lightness(source, f64::from(step) / 255.0);
                    let candidate = inverse_hint(desired, backdrop, alpha);
                    if previous == Some(candidate) {
                        continue;
                    }
                    previous = Some(candidate);
                    values.copy_from_slice(&current_values);
                    values[index] = candidate;
                    let failures = assess(
                        &context,
                        runtime,
                        epoch,
                        scenarios,
                        &values,
                        &mut scores,
                        arena,
                    )?;
                    evidence.visited += 1;
                    evidence.points += 1;
                    evidence.examined += 1;
                    if failures < current_failures
                        || failures == current_failures
                            && compare_scores(&scores, &current_scores)
                                .then_with(|| values.cmp(&current_values))
                                == Ordering::Less
                    {
                        current_failures = failures;
                        current_scores.copy_from_slice(&scores);
                        current_values.copy_from_slice(&values);
                        changed = true;
                    }
                    if failures == 0
                        && (!has_best
                            || compare_scores(&scores, &best_scores)
                                .then_with(|| values.cmp(&best_values))
                                == Ordering::Less)
                    {
                        has_best = true;
                        best_scores.copy_from_slice(&scores);
                        best_values.copy_from_slice(&values);
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Полный RGB8/gray domain остаётся доступен после приближений.
    let constraints = region_constraints(epoch).map_err(allocation)?;
    let mut bounds = BoundsV1 {
        constraint_lower: buffer(constraints.len(), ([0; 3], [0; 3])).map_err(allocation)?,
        constraints,
        lower: buffer(count, [0; 3]).map_err(allocation)?,
        scores: buffer(count, 0.0).map_err(allocation)?,
        tuple_min: buffer(count, Srgb8::new([0; 3])).map_err(allocation)?,
        evaluations: 0,
    };
    let stack_limit = context
        .domains
        .iter()
        .try_fold(1_usize, |size, domain| {
            size.checked_add(domain.split_depth() + 1)
        })
        .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
    let mut stack = Vec::new();
    stack
        .try_reserve_exact(stack_limit)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    let Some(root) = region_node(0, context.domains[0], selection, &mut evidence) else {
        return Ok(SearchResultV1 {
            evidence,
            values: has_best.then_some(best_values),
        });
    };
    values.copy_from_slice(&current_values);
    stack.push(root);
    while let Some(mut node) = stack.pop() {
        let depth = node.depth;
        let volume = node
            .domain
            .cardinality()
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        if node.ordinal.is_none() {
            let possible = bounds.may_pass(&context, runtime, epoch, scenarios, node, &values)?;
            evidence.endpoint_evaluations = bounds.evaluations;
            let worse = has_best
                && match compare_scores(&bounds.scores, &best_scores) {
                    Ordering::Greater => true,
                    Ordering::Equal => bounds.tuple_min >= best_values,
                    Ordering::Less => false,
                };
            if !possible || worse {
                evidence
                    .close_region()
                    .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                continue;
            }
            if volume > 64 {
                let (left, right) = node.domain.split();
                let Some(left) = region_node(depth, left, selection, &mut evidence) else {
                    return Ok(SearchResultV1 {
                        evidence,
                        values: has_best.then_some(best_values),
                    });
                };
                let Some(right) = region_node(depth, right, selection, &mut evidence) else {
                    return Ok(SearchResultV1 {
                        evidence,
                        values: has_best.then_some(best_values),
                    });
                };
                let (first, second) = if left.domain.lower_score(context.preferred[depth])
                    <= right.domain.lower_score(context.preferred[depth])
                {
                    (left, right)
                } else {
                    (right, left)
                };
                evidence
                    .close_region()
                    .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                stack.push(second);
                stack.push(first);
            } else {
                node.ordinal = Some(0);
                stack.push(node);
            }
            continue;
        }
        if evidence.visited == selection.budget {
            return Ok(SearchResultV1 {
                evidence,
                values: has_best.then_some(best_values),
            });
        }
        evidence.visited += 1;
        evidence.points += 1;
        let ordinal = node
            .ordinal
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        if ordinal + 1 < volume {
            stack.push(RegionNodeV1 {
                ordinal: Some(ordinal + 1),
                ..node
            });
        } else {
            evidence
                .close_region()
                .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
        }
        values[depth] = node.domain.value(ordinal);
        if depth + 1 < count {
            let Some(next) = region_node(
                depth + 1,
                context.domains[depth + 1],
                selection,
                &mut evidence,
            ) else {
                return Ok(SearchResultV1 {
                    evidence,
                    values: has_best.then_some(best_values),
                });
            };
            stack.push(next);
            continue;
        }
        evidence.examined += 1;
        if assess(
            &context,
            runtime,
            epoch,
            scenarios,
            &values,
            &mut scores,
            arena,
        )? == 0
            && (!has_best
                || compare_scores(&scores, &best_scores).then_with(|| values.cmp(&best_values))
                    == Ordering::Less)
        {
            has_best = true;
            best_scores.copy_from_slice(&scores);
            best_values.copy_from_slice(&values);
        }
    }
    if evidence.open_regions != 0 {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    evidence.completion = CompletionV1::Complete;
    Ok(SearchResultV1 {
        evidence,
        values: has_best.then_some(best_values),
    })
}
