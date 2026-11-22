//! Независимые задачи получают собственный поиск, но публикуются одним кадром.
//! Общий неизменяемый фон не создаёт связь между вычисляемыми цветами.

use super::*;

struct VisibleDependenciesV1 {
    opacity: Vec<f64>,
    seen: Vec<bool>,
    queue: Vec<usize>,
}

impl VisibleDependenciesV1 {
    fn new<E: ProgramConstraintEvaluatorSetV1>(
        program: &Program<E>,
        graph: &CompiledAppearanceGraph,
        bindings: &AdmittedAppearanceBindings,
    ) -> Result<Self, ProgramCompileError> {
        // Alpha belongs to the existing physical graph. Evaluating its admitted
        // bindings also handles nested opacity and underflow by the same law.
        let mut workspace = graph.new_workspace().map_err(map_binding_compile_error)?;
        let evaluation = graph
            .evaluate_admitted_into(bindings, &mut workspace)
            .map_err(map_binding_compile_error)?;
        let mut opacity = Vec::new();
        opacity
            .try_reserve_exact(program.occurrences.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for occurrence in &program.occurrences {
            opacity.push(
                evaluation
                    .paint(occurrence.subject)
                    .ok_or(ProgramCompileError::InternalInvariant)?
                    .opacity()
                    .value(),
            );
        }
        let mut queue = Vec::new();
        queue
            .try_reserve_exact(program.occurrences.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        Ok(Self {
            opacity,
            seen: false_slots(program.occurrences.len())?,
            queue,
        })
    }

    fn visit(&mut self, index: &IndexedProgramDependenciesV1, root: usize, targets: &mut [bool]) {
        for occurrence in self.queue.drain(..) {
            self.seen[occurrence] = false;
        }
        self.seen[root] = true;
        self.queue.push(root);
        let mut cursor = 0;
        while let Some(&occurrence) = self.queue.get(cursor) {
            cursor += 1;
            let alpha = self.opacity[occurrence];
            if alpha != 0.0 {
                let mut paint = index.occurrence_paints[occurrence];
                loop {
                    match index.paint_dependencies[paint] {
                        IndexedPaintDependencyV1::Target(target) => {
                            targets[target] = true;
                            break;
                        }
                        IndexedPaintDependencyV1::Paint(source) => paint = source,
                    }
                }
            }
            if alpha != 1.0 {
                let surface = index.occurrence_surfaces[occurrence];
                if let Some(parent) = index.surface_occurrences[surface] {
                    if !self.seen[parent] {
                        self.seen[parent] = true;
                        self.queue.push(parent);
                    }
                }
            }
        }
    }

    fn visible(
        &mut self,
        index: &IndexedProgramDependenciesV1,
        occurrence: OccurrenceId,
        targets: &mut [bool],
    ) -> Result<(), ProgramCompileError> {
        let root = index
            .occurrence(occurrence)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        self.visit(index, root, targets);
        Ok(())
    }

    fn backdrop(
        &mut self,
        index: &IndexedProgramDependenciesV1,
        occurrence: OccurrenceId,
        targets: &mut [bool],
    ) -> Result<(), ProgramCompileError> {
        let root = index
            .occurrence(occurrence)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        if let Some(parent) = index.surface_occurrences[index.occurrence_surfaces[root]] {
            self.visit(index, parent, targets);
        }
        Ok(())
    }
}

fn root(parents: &mut [usize], mut target: usize) -> usize {
    while parents[target] != target {
        parents[target] = parents[parents[target]];
        target = parents[target];
    }
    target
}

fn join(parents: &mut [usize], members: &[usize]) {
    let Some((&first, others)) = members.split_first() else {
        return;
    };
    for &target in others {
        let left = root(parents, first);
        let right = root(parents, target);
        parents[right.max(left)] = left.min(right);
    }
}

pub(in crate::program_session) fn compile_components<E: ProgramConstraintEvaluatorSetV1>(
    selection: &mut ComputedSelectionV1,
    program: &Program<E>,
    graph: &CompiledAppearanceGraph,
    bindings: &AdmittedAppearanceBindings,
    constraints: &[CompiledPointConstraint<ProgramConstraintInvocationOf<E>>],
    index: &IndexedProgramDependenciesV1,
    scratch: &mut ProgramDependencyScratchV1,
) -> Result<(), ProgramCompileError> {
    let count = selection.targets.len();
    let mut parents = buffer(count, 0).map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (ordinal, parent) in parents.iter_mut().enumerate() {
        *parent = ordinal;
    }
    let mut selected =
        buffer(program.targets.len(), None).map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (ordinal, target) in selection.targets.iter().enumerate() {
        let index = program
            .targets
            .binary_search_by_key(&target.id, |value| value.id)
            .map_err(|_| ProgramCompileError::InternalInvariant)?;
        selected[index] = Some(ordinal);
    }
    let mut marked = false_slots(program.targets.len())?;
    let mut visible = VisibleDependenciesV1::new(program, graph, bindings)?;
    let mut members = Vec::new();
    members
        .try_reserve_exact(count)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    let collect = |marked: &[bool], members: &mut Vec<usize>| {
        members.clear();
        members.extend(
            marked
                .iter()
                .zip(&selected)
                .filter_map(|(&marked, &selected)| marked.then_some(selected).flatten()),
        );
    };
    let mut constraint_members = Vec::new();
    constraint_members
        .try_reserve_exact(constraints.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (ordinal, constraint) in constraints.iter().enumerate() {
        if !constraint.mode.rejects_candidate() {
            continue;
        }
        marked.fill(false);
        match &constraint.body {
            CompiledProgramConstraintBodyV1::VisibleUnary {
                occurrence,
                invocation,
                ..
            } => {
                match program.evaluator.constraint_content(*invocation) {
                    ProgramConstraintContentV1::ExactSrgb8 { .. } => {
                        visible.visible(index, *occurrence, &mut marked)?;
                    }
                    ProgramConstraintContentV1::Wcag22Srgb8 { .. }
                    | ProgramConstraintContentV1::VisibleSeparation { .. } => {
                        visible.visible(index, *occurrence, &mut marked)?;
                        visible.backdrop(index, *occurrence, &mut marked)?;
                    }
                    // An unrecognised evaluator keeps the complete declared
                    // physical dependency cone. It is never dropped or guessed.
                    _ => mark_constraint_coverage(
                        program,
                        index,
                        scratch,
                        constraint,
                        &mut marked,
                        None,
                    )?,
                }
            }
            // Intrinsic relations, families and causal presentations preserve
            // their existing full dependency contract.
            _ => mark_constraint_coverage(program, index, scratch, constraint, &mut marked, None)?,
        }
        collect(&marked, &mut members);
        join(&mut parents, &members);
        constraint_members.push((ordinal, members.first().copied()));
    }
    // A visible identity/recourse can couple sources even without a separate
    // contrast constraint. Its own variable remains in the same task.
    for (ordinal, target) in selection.targets.iter().enumerate() {
        let Some(appearance) = target
            .declaration
            .intent
            .as_ref()
            .and_then(|intent| intent.appearance)
        else {
            continue;
        };
        marked.fill(false);
        visible.visible(index, appearance, &mut marked)?;
        collect(&marked, &mut members);
        if !members.contains(&ordinal) {
            members.push(ordinal);
        }
        join(&mut parents, &members);
    }
    let mut component_of =
        buffer(count, usize::MAX).map_err(|_| ProgramCompileError::ResourceExhausted)?;
    let mut grouped_targets: Vec<Vec<usize>> = Vec::new();
    grouped_targets
        .try_reserve_exact(count)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for target in 0..count {
        let representative = root(&mut parents, target);
        if component_of[representative] == usize::MAX {
            component_of[representative] = grouped_targets.len();
            grouped_targets.push(Vec::new());
        }
        let component = component_of[representative];
        component_of[target] = component;
        grouped_targets[component]
            .try_reserve(1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        grouped_targets[component].push(target);
    }
    let mut grouped_constraints = buffer(grouped_targets.len(), Vec::new())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    let mut fixed = Vec::new();
    fixed
        .try_reserve_exact(constraint_members.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (constraint, member) in constraint_members {
        if let Some(target) = member {
            let group = &mut grouped_constraints[component_of[target]];
            group
                .try_reserve(1)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            group.push(constraint);
        } else {
            fixed.push(constraint);
        }
    }
    let mut components = Vec::new();
    components
        .try_reserve_exact(grouped_targets.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (targets, constraints) in grouped_targets.into_iter().zip(grouped_constraints) {
        components.push(ComputedComponentV1 {
            targets: targets.into_boxed_slice(),
            constraints: constraints.into_boxed_slice(),
        });
    }
    let constant_check = u64::from(!fixed.is_empty() || components.is_empty());
    selection.total_budget = selection
        .budget
        .checked_mul(
            u64::try_from(components.len()).map_err(|_| ProgramCompileError::ResourceExhausted)?,
        )
        .and_then(|budget| budget.checked_add(constant_check))
        .ok_or(ProgramCompileError::ResourceExhausted)?;
    selection.components = components.into_boxed_slice();
    selection.fixed_constraints = fixed.into_boxed_slice();
    Ok(())
}

fn merge_evidence(left: &mut SearchEvidenceV1, right: SearchEvidenceV1) -> Result<(), ()> {
    for (left, right) in [
        (&mut left.visited, right.visited),
        (&mut left.regions, right.regions),
        (&mut left.points, right.points),
        (&mut left.endpoint_evaluations, right.endpoint_evaluations),
        (&mut left.examined, right.examined),
        (&mut left.open_regions, right.open_regions),
    ] {
        *left = left.checked_add(right).ok_or(())?;
    }
    left.completion = match (left.completion, right.completion) {
        (CompletionV1::BudgetExhausted, _) | (_, CompletionV1::BudgetExhausted) => {
            CompletionV1::BudgetExhausted
        }
        (CompletionV1::AuthoredExact, CompletionV1::AuthoredExact) => CompletionV1::AuthoredExact,
        _ => CompletionV1::Complete,
    };
    Ok(())
}

pub(super) fn select<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let mut evidence = SearchEvidenceV1 {
        completion: CompletionV1::AuthoredExact,
        visited: 0,
        regions: 0,
        points: 0,
        endpoint_evaluations: 0,
        examined: 0,
        open_regions: 0,
    };
    let mut values = buffer(selection.targets.len(), Srgb8::new([0; 3]))
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    for (value, target) in values.iter_mut().zip(selection.targets.iter()) {
        *value = target.declaration.anchor;
    }
    apply::<E>(selection, runtime, &values)?;
    if !selection.fixed_constraints.is_empty() || selection.components.is_empty() {
        // No adjustable source can repair these conditions. Check once before
        // search instead of spending each component's budget on the same refusal.
        let failed = scan_program_candidate_scoped(
            runtime,
            epoch,
            scenarios,
            0,
            ProgramEvaluationPhaseV1::Hard,
            ProgramCandidateCollectionV1::none(),
            None,
            Some(&selection.fixed_constraints),
        )?;
        evidence.visited = 1;
        evidence.points = 1;
        evidence.examined = 1;
        if failed {
            evidence.completion = CompletionV1::Complete;
            return Ok(SearchResultV1 {
                evidence,
                values: None,
            });
        }
    }
    for component in &selection.components {
        let selected = if component
            .targets
            .iter()
            .any(|&index| selection.targets[index].declaration.intent.is_some())
        {
            contextual::select_component(selection, component, runtime, epoch, scenarios, arena)?
        } else {
            select_component(selection, component, runtime, epoch, scenarios)?
        };
        merge_evidence(&mut evidence, selected.evidence)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        let Some(local) = selected.values else {
            if selected.evidence.completion == CompletionV1::Complete {
                // One impossible factor makes the full product impossible.
                evidence.completion = CompletionV1::Complete;
                evidence.open_regions = 0;
            }
            return Ok(SearchResultV1 {
                evidence,
                values: None,
            });
        };
        if local.len() != component.targets.len() {
            return Err(ProgramSessionEvaluationError::InternalInvariant);
        }
        for (&index, value) in component.targets.iter().zip(local) {
            values[index] = value;
        }
    }
    Ok(SearchResultV1 {
        evidence,
        values: Some(values),
    })
}
