//! Immutable authored composition, evaluated by the same compositor as runtime.

use super::*;

pub(super) struct ReferenceCompositionV1 {
    occurrences: Vec<(OccurrenceId, Srgb8, Srgb8)>,
}

impl ReferenceCompositionV1 {
    pub(super) fn occurrence(&self, id: OccurrenceId) -> Option<(Srgb8, Srgb8)> {
        self.occurrences
            .binary_search_by_key(&id, |entry| entry.0)
            .ok()
            .map(|index| (self.occurrences[index].1, self.occurrences[index].2))
    }
}

pub(super) fn compile<E: ProgramConstraintEvaluatorSetV1>(
    program: &mut Program<E>,
    graph: &CompiledAppearanceGraph,
    bindings: &AdmittedAppearanceBindings,
) -> Result<Option<ReferenceCompositionV1>, ProgramCompileError> {
    let Some(inputs) = &mut program.reference_inputs else {
        return Ok(None);
    };
    let invalid = || {
        ProgramCompileError::InvalidComputedAssignment(
            computed_assignment::ComputedAdmissionErrorV1::InvalidReferenceComposition,
        )
    };
    inputs.sort_unstable_by_key(|entry| entry.0);
    if !inputs
        .iter()
        .map(|entry| entry.0)
        .eq(graph.surface_input_ports())
    {
        return Err(invalid());
    }
    // Finite declarations have no authored source anchor. Arbitrarily choosing
    // one finite member would silently invent the reference composition.
    if program
        .targets
        .iter()
        .any(|target| matches!(target.intent, TargetIntentV1::Finite(_)))
    {
        return Err(invalid());
    }
    let mut workspace = graph.new_workspace().map_err(map_binding_compile_error)?;
    let mut paints = graph
        .prepare_paints_for_surface_cases(
            bindings,
            inputs.iter().map(|entry| entry.0),
            &mut workspace,
        )
        .map_err(map_binding_compile_error)?;
    let evaluation = paints.evaluate_surface_inputs_canonical(|index| inputs[index].1);
    let mut occurrences = Vec::new();
    occurrences
        .try_reserve_exact(program.occurrences.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for occurrence in &program.occurrences {
        let point = evaluation
            .occurrence(occurrence.id)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        occurrences.push((
            occurrence.id,
            Srgb8::new(point.visible()),
            Srgb8::new(point.certificate().backdrop_rgb()),
        ));
    }
    occurrences.sort_unstable_by_key(|entry| entry.0);
    Ok(Some(ReferenceCompositionV1 { occurrences }))
}
