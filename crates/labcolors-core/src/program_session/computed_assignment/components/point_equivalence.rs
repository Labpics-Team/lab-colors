//! Substitution-equivalent leaf choices in the idempotent Point profile.
//!
//! Matching colours or automorphic vertices are insufficient: a pairwise
//! separation or a downstream use can require different source values. This
//! proof accepts only own unary factors and rejects every other dependency.

use super::*;

fn same_intent(a: &CompiledComputedTargetV1, b: &CompiledComputedTargetV1) -> bool {
    let a_decl = &a.declaration;
    let b_decl = &b.declaration;
    a_decl.domain == b_decl.domain
        && a_decl.anchor == b_decl.anchor
        && a_decl.max_delta_bits == b_decl.max_delta_bits
        && a_decl.reference_appearance == b_decl.reference_appearance
        && a.tone_input == b.tone_input
        && a.effective_anchor == b.effective_anchor
        && a.reference_backdrop == b.reference_backdrop
        && match (&a_decl.intent, &b_decl.intent) {
            (Some(a), Some(b)) => a.identity == b.identity && a.tone == b.tone,
            (None, None) => true,
            _ => false,
        }
}

fn own_occurrence<E: ProgramConstraintEvaluatorSetV1>(
    target: &CompiledComputedTargetV1,
    program: &Program<E>,
    index: &IndexedProgramDependenciesV1,
) -> Option<usize> {
    let occurrence = index.occurrence(target.declaration.intent.as_ref()?.appearance?)?;
    let mut paint = index.occurrence_paints[occurrence];
    loop {
        match index.paint_dependencies[paint] {
            IndexedPaintDependencyV1::Target(source) => {
                return (program.targets[source].id == target.id).then_some(occurrence);
            }
            IndexedPaintDependencyV1::Paint(source) => paint = source,
        }
    }
}

fn same_backdrop(a: Surface, b: Surface) -> bool {
    match (a, b) {
        (Surface::Input { input: a, .. }, Surface::Input { input: b, .. }) => a == b,
        (
            Surface::FromOccurrence { occurrence: a, .. },
            Surface::FromOccurrence { occurrence: b, .. },
        ) => a == b,
        _ => false,
    }
}

fn same_paint_operations<E: ProgramConstraintEvaluatorSetV1>(
    program: &Program<E>,
    index: &IndexedProgramDependenciesV1,
    mut a: usize,
    mut b: usize,
) -> bool {
    loop {
        match (program.paints[a], program.paints[b]) {
            (Paint::Solid { .. }, Paint::Solid { .. }) => return true,
            (Paint::Opacity { opacity: left, .. }, Paint::Opacity { opacity: right, .. }) => {
                let left = program
                    .opacities
                    .iter()
                    .find(|value| value.id == left)
                    .map(|value| value.value.to_bits());
                let right = program
                    .opacities
                    .iter()
                    .find(|value| value.id == right)
                    .map(|value| value.value.to_bits());
                if left.is_none() || left != right {
                    return false;
                }
                let (IndexedPaintDependencyV1::Paint(left), IndexedPaintDependencyV1::Paint(right)) =
                    (index.paint_dependencies[a], index.paint_dependencies[b])
                else {
                    return false;
                };
                a = left;
                b = right;
            }
            _ => return false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compile<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    program: &Program<E>,
    constraints: &[CompiledPointConstraint<ProgramConstraintInvocationOf<E>>],
    factors: &[ComputedFactorV1],
    index: &IndexedProgramDependenciesV1,
) -> Result<Option<PointEquivalenceV1>, ProgramCompileError> {
    fn allocation<T>(_: T) -> ProgramCompileError {
        ProgramCompileError::ResourceExhausted
    }
    let count = component.targets.len();
    if count < 2 {
        return Ok(None);
    }
    let mut occurrences = buffer(count, None).map_err(allocation)?;
    let mut unary = buffer(count, Vec::new()).map_err(allocation)?;
    for (local, &target) in component.targets.iter().enumerate() {
        let Some(occurrence) = own_occurrence(&selection.targets[target], program, index) else {
            continue;
        };
        let mut valid = true;
        for factor in factors
            .iter()
            .filter(|factor| factor.targets.contains(&target))
        {
            match factor.kind {
                ComputedFactorKindV1::Objective(own) if own == target => {}
                ComputedFactorKindV1::Constraint(constraint) => {
                    if let CompiledProgramConstraintBodyV1::VisibleUnary {
                        occurrence: own, ..
                    } = constraints[constraint].body
                    {
                        if own != program.occurrences[occurrence].id {
                            valid = false;
                            break;
                        }
                    } else {
                        valid = false;
                        break;
                    }
                }
                _ => {
                    valid = false;
                    break;
                }
            }
        }
        if valid {
            // An alpha-zero use still declares its own hard requirements even
            // when their physical dependency cone omits this source. Compare
            // the full declaration set, not merely the source-sensitive part.
            for constraint in constraints
                .iter()
                .filter(|constraint| constraint.mode.rejects_candidate())
            {
                if let CompiledProgramConstraintBodyV1::VisibleUnary {
                    occurrence: own,
                    invocation,
                    ..
                } = constraint.body
                {
                    if own == program.occurrences[occurrence].id {
                        let content = program.evaluator.constraint_content(invocation);
                        if !unary[local].contains(&content) {
                            unary[local].try_reserve(1).map_err(allocation)?;
                            unary[local].push(content);
                        }
                    }
                }
            }
            occurrences[local] = Some(occurrence);
        }
    }
    let mut representatives = buffer(count, 0).map_err(allocation)?;
    let mut variables: Vec<usize> = Vec::new();
    variables.try_reserve_exact(count).map_err(allocation)?;
    for local in 0..count {
        let matching = occurrences[local].and_then(|occurrence| {
            variables.iter().copied().find(|&other| {
                let Some(other_occurrence) = occurrences[other] else {
                    return false;
                };
                let a = &program.occurrences[occurrence];
                let b = &program.occurrences[other_occurrence];
                same_intent(
                    &selection.targets[component.targets[local]],
                    &selection.targets[component.targets[other]],
                ) && same_paint_operations(
                    program,
                    index,
                    index.occurrence_paints[occurrence],
                    index.occurrence_paints[other_occurrence],
                ) && a.composition == b.composition
                    && a.context == b.context
                    && same_backdrop(
                        program.surfaces[index.occurrence_surfaces[occurrence]],
                        program.surfaces[index.occurrence_surfaces[other_occurrence]],
                    )
                    && unary[local].len() == unary[other].len()
                    && unary[local]
                        .iter()
                        .all(|content| unary[other].contains(content))
            })
        });
        representatives[local] = matching.unwrap_or(local);
        if matching.is_none() {
            variables.push(local);
        }
    }
    if variables.len() == count {
        return Ok(None);
    }
    let mut retained = Vec::new();
    retained
        .try_reserve_exact(component.constraints.len())
        .map_err(allocation)?;
    for &constraint in &component.constraints {
        let duplicate = match constraints[constraint].body {
            CompiledProgramConstraintBodyV1::VisibleUnary { occurrence, .. } => {
                occurrences.iter().enumerate().any(|(local, own)| {
                    representatives[local] != local
                        && own.is_some_and(|own| program.occurrences[own].id == occurrence)
                })
            }
            _ => false,
        };
        if !duplicate {
            retained.push(constraint);
        }
    }
    Ok(Some(PointEquivalenceV1 {
        representatives: representatives.into_boxed_slice(),
        variables: variables.into_boxed_slice(),
        constraints: retained.into_boxed_slice(),
    }))
}
