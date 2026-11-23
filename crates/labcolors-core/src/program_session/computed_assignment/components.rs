//! Независимые задачи получают собственный поиск, но публикуются одним кадром.
//! Общий неизменяемый фон не создаёт связь между вычисляемыми цветами.

use super::*;

fn physical_cone<Invocation>(
    selection: &ComputedSelectionV1,
    targets: &[usize],
    constraints: &[CompiledPointConstraint<Invocation>],
    indices: &[usize],
    compiler: &mut AppearanceConeCompilerV1<'_>,
) -> Result<CompiledAppearanceConeV1, BindingError> {
    for &target in targets {
        if let Some(appearance) = selection.targets[target]
            .declaration
            .intent
            .as_ref()
            .and_then(|intent| intent.appearance)
        {
            compiler.occurrence(appearance)?;
        }
    }
    for &index in indices {
        match &constraints[index].body {
            CompiledProgramConstraintBodyV1::ComputedVisibleRelation { occurrences, .. } => {
                for endpoint in occurrences {
                    compiler.occurrence(endpoint.occurrence)?;
                }
            }
            CompiledProgramConstraintBodyV1::VisibleUnary { occurrence, .. } => {
                compiler.occurrence(*occurrence)?;
            }
            CompiledProgramConstraintBodyV1::VisibleRelation {
                reference,
                candidates,
                ..
            } => {
                compiler.occurrence(reference.occurrence)?;
                for candidate in candidates {
                    compiler.occurrence(candidate.occurrence)?;
                }
            }
            CompiledProgramConstraintBodyV1::PointPresentation { terminal, .. } => {
                compiler.occurrence(*terminal)?;
            }
            // Intrinsic checks read admitted target bindings directly.
            CompiledProgramConstraintBodyV1::IntrinsicUnary { .. }
            | CompiledProgramConstraintBodyV1::IntrinsicRelation { .. } => {}
        }
    }
    compiler.finish()
}

struct VisibleDependenciesV1 {
    opacity: Vec<f64>,
    seen: Vec<bool>,
    queue: Vec<usize>,
    surface_inputs: Vec<Option<usize>>,
    input_seen: Vec<bool>,
    inputs: Vec<usize>,
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
        let input_count = graph.surface_input_ports().len();
        let mut surface_inputs = Vec::new();
        surface_inputs
            .try_reserve_exact(program.surfaces.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for surface in &program.surfaces {
            surface_inputs.push(match surface {
                Surface::Input { input, .. } => Some(
                    graph
                        .surface_input_ports()
                        .position(|candidate| candidate == *input)
                        .ok_or(ProgramCompileError::InternalInvariant)?,
                ),
                Surface::FromOccurrence { .. } => None,
            });
        }
        let mut inputs = Vec::new();
        inputs
            .try_reserve_exact(input_count)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        Ok(Self {
            opacity,
            seen: false_slots(program.occurrences.len())?,
            queue,
            surface_inputs,
            input_seen: false_slots(input_count)?,
            inputs,
        })
    }

    fn clear_inputs(&mut self) {
        for input in self.inputs.drain(..) {
            self.input_seen[input] = false;
        }
    }

    fn input(&mut self, input: usize) {
        if !self.input_seen[input] {
            self.input_seen[input] = true;
            self.inputs.push(input);
        }
    }

    fn declared_inputs(&mut self, scratch: &ProgramDependencyScratchV1) {
        for node in &scratch.queue {
            if let IndexedDependencyNodeV1::Surface(surface) = node {
                if let Some(input) = self.surface_inputs[*surface] {
                    self.input(input);
                }
            }
        }
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
                } else if let Some(input) = self.surface_inputs[surface] {
                    self.input(input);
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
        let surface = index.occurrence_surfaces[root];
        if let Some(parent) = index.surface_occurrences[surface] {
            self.visit(index, parent, targets);
        } else if let Some(input) = self.surface_inputs[surface] {
            self.input(input);
        }
        Ok(())
    }

    fn constraint<E: ProgramConstraintEvaluatorSetV1>(
        &mut self,
        program: &Program<E>,
        index: &IndexedProgramDependenciesV1,
        scratch: &mut ProgramDependencyScratchV1,
        constraint: &CompiledPointConstraint<ProgramConstraintInvocationOf<E>>,
        targets: &mut [bool],
    ) -> Result<(), ProgramCompileError> {
        match &constraint.body {
            CompiledProgramConstraintBodyV1::ComputedVisibleRelation { occurrences, .. } => {
                for endpoint in occurrences {
                    self.visible(index, endpoint.occurrence, targets)?;
                    self.backdrop(index, endpoint.occurrence, targets)?;
                }
            }
            CompiledProgramConstraintBodyV1::VisibleUnary {
                occurrence,
                invocation,
                ..
            } => {
                match program.evaluator.constraint_content(*invocation) {
                    ProgramConstraintContentV1::ExactSrgb8 { .. }
                    | ProgramConstraintContentV1::NominalSrgb8Visible { .. } => {
                        self.visible(index, *occurrence, targets)?;
                    }
                    ProgramConstraintContentV1::Wcag22Srgb8 { .. }
                    | ProgramConstraintContentV1::VisibleSeparation { .. } => {
                        self.visible(index, *occurrence, targets)?;
                        self.backdrop(index, *occurrence, targets)?;
                    }
                    // An unrecognised evaluator keeps the complete declared
                    // physical dependency cone. It is never dropped or guessed.
                    _ => {
                        mark_constraint_coverage(
                            program, index, scratch, constraint, targets, None,
                        )?;
                        self.declared_inputs(scratch);
                    }
                }
            }
            CompiledProgramConstraintBodyV1::VisibleRelation {
                reference,
                candidates,
                ..
            } => {
                // Relation evaluators receive final RGB endpoints and immutable
                // family artifacts. The backdrop remains only in evidence.
                self.visible(index, reference.occurrence, targets)?;
                for candidate in candidates {
                    self.visible(index, candidate.occurrence, targets)?;
                }
            }
            CompiledProgramConstraintBodyV1::PointPresentation { .. } => {
                mark_constraint_coverage(program, index, scratch, constraint, targets, None)?;
                self.declared_inputs(scratch);
            }
            CompiledProgramConstraintBodyV1::IntrinsicUnary { .. }
            | CompiledProgramConstraintBodyV1::IntrinsicRelation { .. } => {
                mark_constraint_coverage(program, index, scratch, constraint, targets, None)?;
            }
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

fn copy_factor(
    targets: &[usize],
    inputs: &[usize],
    kind: ComputedFactorKindV1,
) -> Result<ComputedFactorV1, ProgramCompileError> {
    let copy = |source: &[usize]| -> Result<Box<[usize]>, ProgramCompileError> {
        let mut result = Vec::new();
        result
            .try_reserve_exact(source.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        result.extend_from_slice(source);
        Ok(result.into_boxed_slice())
    };
    Ok(ComputedFactorV1 {
        targets: copy(targets)?,
        inputs: copy(inputs)?,
        kind,
    })
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
    for target in &mut selection.targets {
        if let Some(appearance) = target
            .declaration
            .intent
            .as_ref()
            .and_then(|intent| intent.appearance)
        {
            let occurrence = index
                .occurrence(appearance)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            target.opaque = visible.opacity[occurrence] == 1.0;
        }
    }
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
    let mut factors = Vec::new();
    factors
        .try_reserve_exact(
            constraints
                .len()
                .checked_add(count)
                .ok_or(ProgramCompileError::ResourceExhausted)?,
        )
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (ordinal, constraint) in constraints.iter().enumerate() {
        if !constraint.mode.rejects_candidate() {
            continue;
        }
        marked.fill(false);
        visible.clear_inputs();
        visible.constraint(program, index, scratch, constraint, &mut marked)?;
        collect(&marked, &mut members);
        factors.push(copy_factor(
            &members,
            &visible.inputs,
            ComputedFactorKindV1::Constraint(ordinal),
        )?);
        join(&mut parents, &members);
        constraint_members.push((ordinal, members.first().copied()));
    }
    // A visible identity/recourse can couple sources even without a separate
    // contrast constraint. Its own variable remains in the same task.
    for (ordinal, target) in selection.targets.iter().enumerate() {
        let appearance = target
            .declaration
            .intent
            .as_ref()
            .and_then(|intent| intent.appearance);
        marked.fill(false);
        visible.clear_inputs();
        if let Some(appearance) = appearance {
            visible.visible(index, appearance, &mut marked)?;
            if target.declaration.reference_appearance {
                visible.backdrop(index, appearance, &mut marked)?;
            }
        }
        if let Some(input) = target.tone_input {
            visible.input(input);
        }
        collect(&marked, &mut members);
        if !members.contains(&ordinal) {
            members.push(ordinal);
        }
        factors.push(copy_factor(
            &members,
            &visible.inputs,
            ComputedFactorKindV1::Objective(ordinal),
        )?);
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
    let mut required_inputs = false_slots(visible.input_seen.len())?;
    let mut physical = AppearanceConeCompilerV1::new(graph).map_err(map_binding_compile_error)?;
    for (targets, constraint_indices) in grouped_targets.into_iter().zip(grouped_constraints) {
        visible.clear_inputs();
        for &target in &targets {
            let target = &selection.targets[target];
            if let Some(appearance) = target
                .declaration
                .intent
                .as_ref()
                .and_then(|v| v.appearance)
            {
                visible.visible(index, appearance, &mut marked)?;
                if target.declaration.reference_appearance {
                    visible.backdrop(index, appearance, &mut marked)?;
                }
            }
            if let Some(input) = target.tone_input {
                visible.input(input);
            }
        }
        for &constraint in &constraint_indices {
            visible.constraint(
                program,
                index,
                scratch,
                &constraints[constraint],
                &mut marked,
            )?;
        }
        for &input in &visible.inputs {
            required_inputs[input] = true;
        }
        // Rank relevant ports by authored target/context traversal, never by
        // their numeric schema names. Extra declared ancestry is filtered out
        // by the precise cone above, including behind opaque barriers.
        visible.clear_inputs();
        for &target in &targets {
            let target = &selection.targets[target];
            if let Some(appearance) = target
                .declaration
                .intent
                .as_ref()
                .and_then(|v| v.appearance)
            {
                scratch.scan(index, [appearance])?;
                visible.declared_inputs(scratch);
            }
            if let Some(input) = target.tone_input {
                visible.input(input);
            }
        }
        for &constraint in &constraint_indices {
            visible.constraint(
                program,
                index,
                scratch,
                &constraints[constraint],
                &mut marked,
            )?;
        }
        let mut inputs = Vec::new();
        inputs
            .try_reserve_exact(visible.inputs.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for &input in &visible.inputs {
            if required_inputs[input] {
                inputs.push(input);
                required_inputs[input] = false;
            }
        }
        components.push(ComputedComponentV1 {
            physical: physical_cone(
                selection,
                &targets,
                constraints,
                &constraint_indices,
                &mut physical,
            )
            .map_err(map_binding_compile_error)?,
            targets: targets.into_boxed_slice(),
            constraints: constraint_indices.into_boxed_slice(),
            inputs: inputs.into_boxed_slice(),
        });
    }
    let constant_check = u64::from(!fixed.is_empty() || components.is_empty());
    // Reference point/capacity jobs share one budget. A legacy contextual task
    // may run one local prefix search and at most n-1
    // residual searches. Its original budget still covers the full fallback
    // and whole checks. Singleton tasks need no conditional search.
    let search_units = components
        .iter()
        .try_fold(0_u64, |total, component| {
            let contextual = component
                .targets
                .iter()
                .any(|&target| selection.targets[target].declaration.intent.is_some());
            let reference = component
                .targets
                .iter()
                .any(|&target| selection.targets[target].declaration.reference_appearance);
            let units = if !reference && contextual && component.targets.len() > 1 {
                component.targets.len().checked_add(1)?
            } else {
                1
            };
            total.checked_add(u64::try_from(units).ok()?)
        })
        .ok_or(ProgramCompileError::ResourceExhausted)?;
    selection.total_budget = selection
        .budget
        .checked_mul(search_units)
        .and_then(|budget| budget.checked_add(constant_check))
        .ok_or(ProgramCompileError::ResourceExhausted)?;
    selection.components = components.into_boxed_slice();
    selection.factors = factors.into_boxed_slice();
    selection.fixed_constraints = fixed.into_boxed_slice();
    Ok(())
}

pub(super) struct ConditionalComponentsV1 {
    pub(super) components: Vec<ComputedComponentV1>,
    pub(super) fixed_constraints: Vec<usize>,
}

/// Fixing a candidate prefix changes factor connectivity, not any law.
pub(super) fn conditioned<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    prefix: usize,
    epoch: &ProgramEpochV1<E>,
) -> Result<ConditionalComponentsV1, ()> {
    let input_count = epoch.observation_group.schema.as_slice().len();
    let count = selection.targets.len();
    let mut active = buffer(count, false)?;
    for &target in &component.targets[prefix..] {
        active[target] = true;
    }
    let mut parents = buffer(count, 0)?;
    for (index, parent) in parents.iter_mut().enumerate() {
        *parent = index;
    }
    let mut members = Vec::new();
    members
        .try_reserve_exact(component.targets.len())
        .map_err(|_| ())?;
    for factor in &selection.factors {
        members.clear();
        members.extend(
            factor
                .targets
                .iter()
                .copied()
                .filter(|&target| active[target]),
        );
        join(&mut parents, &members);
    }
    let mut component_of = buffer(count, usize::MAX)?;
    let mut grouped_targets: Vec<Vec<usize>> = Vec::new();
    grouped_targets
        .try_reserve_exact(component.targets.len() - prefix)
        .map_err(|_| ())?;
    for &target in &component.targets[prefix..] {
        let representative = root(&mut parents, target);
        if component_of[representative] == usize::MAX {
            component_of[representative] = grouped_targets.len();
            grouped_targets.push(Vec::new());
        }
        let group = component_of[representative];
        component_of[target] = group;
        grouped_targets[group].try_reserve(1).map_err(|_| ())?;
        grouped_targets[group].push(target);
    }
    let mut grouped_constraints = buffer(grouped_targets.len(), Vec::new())?;
    let mut grouped_factors = buffer(grouped_targets.len(), Vec::new())?;
    let mut fixed_constraints = Vec::new();
    fixed_constraints
        .try_reserve_exact(component.constraints.len())
        .map_err(|_| ())?;
    for (index, factor) in selection.factors.iter().enumerate() {
        if factor
            .constraint()
            .is_some_and(|constraint| component.constraints.binary_search(&constraint).is_err())
        {
            continue;
        }
        if let Some(&target) = factor.targets.iter().find(|&&target| active[target]) {
            let group = component_of[target];
            grouped_factors[group].try_reserve(1).map_err(|_| ())?;
            grouped_factors[group].push(index);
            if let Some(constraint) = factor.constraint() {
                grouped_constraints[group].try_reserve(1).map_err(|_| ())?;
                grouped_constraints[group].push(constraint);
            }
        } else if let Some(constraint) = factor.constraint() {
            fixed_constraints.push(constraint);
        }
    }
    let mut seen_inputs = buffer(input_count, false)?;
    let mut physical = AppearanceConeCompilerV1::new(&epoch.graph).map_err(|_| ())?;
    let mut components = Vec::new();
    components
        .try_reserve_exact(grouped_targets.len())
        .map_err(|_| ())?;
    for ((targets, constraints), factors) in grouped_targets
        .into_iter()
        .zip(grouped_constraints)
        .zip(grouped_factors)
    {
        let mut inputs = Vec::new();
        let capacity = factors.iter().fold(0_usize, |total, &index| {
            total
                .saturating_add(selection.factors[index].inputs.len())
                .min(input_count)
        });
        inputs.try_reserve_exact(capacity).map_err(|_| ())?;
        // Target contexts precede conditions, retaining authored local order.
        for &index in factors
            .iter()
            .filter(|&&index| selection.factors[index].constraint().is_none())
            .chain(
                factors
                    .iter()
                    .filter(|&&index| selection.factors[index].constraint().is_some()),
            )
        {
            for &input in &selection.factors[index].inputs {
                if !seen_inputs[input] {
                    seen_inputs[input] = true;
                    inputs.push(input);
                }
            }
        }
        for &input in &inputs {
            seen_inputs[input] = false;
        }
        components.push(ComputedComponentV1 {
            physical: physical_cone(
                selection,
                &targets,
                &epoch.constraints,
                &constraints,
                &mut physical,
            )
            .map_err(|_| ())?,
            targets: targets.into_boxed_slice(),
            constraints: constraints.into_boxed_slice(),
            inputs: inputs.into_boxed_slice(),
        });
    }
    Ok(ConditionalComponentsV1 {
        components,
        fixed_constraints,
    })
}

/// The first objective may be solved locally only when its physical context
/// does not depend on the remaining variables. Hard factors follow the same
/// dependency boundary, irrespective of which occurrence declared them.
pub(super) fn leading<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    epoch: &ProgramEpochV1<E>,
) -> Result<Option<ComputedComponentV1>, ()> {
    let input_count = epoch.observation_group.schema.as_slice().len();
    let &first = component.targets.first().ok_or(())?;
    let objective = selection
        .factors
        .iter()
        .find(|factor| factor.kind == ComputedFactorKindV1::Objective(first))
        .ok_or(())?;
    if objective.targets.iter().any(|&target| target != first) {
        return Ok(None);
    }
    let mut inputs = Vec::new();
    inputs.try_reserve_exact(input_count).map_err(|_| ())?;
    let mut seen = buffer(input_count, false)?;
    let mut collect_inputs = |factor: &ComputedFactorV1| {
        for &input in &factor.inputs {
            if !seen[input] {
                seen[input] = true;
                inputs.push(input);
            }
        }
    };
    collect_inputs(objective);
    let mut constraints = Vec::new();
    constraints
        .try_reserve_exact(component.constraints.len())
        .map_err(|_| ())?;
    for factor in &selection.factors {
        if factor.targets.as_ref() != [first] {
            continue;
        }
        if let Some(constraint) = factor.constraint() {
            if component.constraints.binary_search(&constraint).is_ok() {
                constraints.push(constraint);
                collect_inputs(factor);
            }
        }
    }
    Ok(Some(ComputedComponentV1 {
        physical: physical_cone(
            selection,
            &[first],
            &epoch.constraints,
            &constraints,
            &mut AppearanceConeCompilerV1::new(&epoch.graph).map_err(|_| ())?,
        )
        .map_err(|_| ())?,
        targets: buffer(1, first)?.into_boxed_slice(),
        constraints: constraints.into_boxed_slice(),
        inputs: inputs.into_boxed_slice(),
    }))
}

pub(super) fn merge_evidence(
    left: &mut SearchEvidenceV1,
    right: SearchEvidenceV1,
) -> Result<(), ()> {
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
    let mut reference_selections = Vec::new();
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
            ProgramCandidateScanV1::Constraints {
                indices: &selection.fixed_constraints,
                physical: None,
            },
        )?;
        evidence.visited = 1;
        evidence.points = 1;
        evidence.examined = 1;
        if failed {
            evidence.completion = CompletionV1::Complete;
            return Ok(SearchResultV1 {
                reference_selections: None,
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
                reference_selections: None,
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
        if let Some(reports) = selected.reference_selections {
            reference_selections
                .try_reserve(reports.len())
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
            reference_selections.extend(reports.iter().cloned());
        }
    }
    Ok(SearchResultV1 {
        reference_selections: (!reference_selections.is_empty())
            .then(|| reference_selections.into()),
        evidence,
        values: Some(values),
    })
}
