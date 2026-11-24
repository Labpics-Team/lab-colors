//! Continuous views reuse the compiled operations, dependency cones and buffers.

use super::numeric_kernel::{ContinuousAppearanceKernel, ContinuousResolvedOccurrenceV1};
use super::*;

#[derive(Debug)]
pub(crate) struct PreparedContinuousAppearancePaints<'program, 'bindings, 'workspace> {
    program: CompiledAppearanceProgram<'program>,
    _bindings: &'bindings AdmittedAppearanceBindings,
    workspace: &'workspace mut ContinuousAppearanceWorkspace,
    scope: Option<&'program CompiledAppearanceConeV1>,
}

impl CompiledAppearanceGraph {
    pub(crate) fn new_continuous_workspace(
        &self,
    ) -> Result<ContinuousAppearanceWorkspace, BindingError> {
        ContinuousAppearanceWorkspace::for_program(self.program())
    }

    /// The admitted binding schema fixes the canonical closure indices and the
    /// graph opacity inputs. Each atomic source (RGB,alpha) is supplied without
    /// quantization; prepared paints are reused for all subsequent surface cases.
    pub(crate) fn prepare_continuous_surface_cases<'program, 'bindings, 'workspace>(
        &'program self,
        scope: Option<&'program CompiledAppearanceConeV1>,
        bindings: &'bindings AdmittedAppearanceBindings,
        paint_value_at: impl Fn(usize) -> ContinuousPointPaintValueV1,
        workspace: &'workspace mut ContinuousAppearanceWorkspace,
    ) -> Result<PreparedContinuousAppearancePaints<'program, 'bindings, 'workspace>, BindingError>
    {
        if scope.is_some_and(|cone| !Arc::ptr_eq(&self.instance, &cone.graph_instance)) {
            return Err(BindingError::IncompatibleEvaluationCone);
        }
        let program = self.program();
        if bindings
            .graph_instance
            .as_ref()
            .is_some_and(|instance| Arc::ptr_eq(instance, &self.instance))
        {
            workspace.validate_shape(program)?;
        } else {
            program.validate_admitted_workspace(bindings, workspace)?;
        }
        let order = if let Some(cone) = scope {
            workspace.cone.prepare(self, cone);
            workspace.cone.paints.as_slice()
        } else {
            program.paint_topo
        };
        program.execute_paints_in_order::<ContinuousAppearanceKernel, _, _>(
            order,
            paint_value_at,
            |index| bindings.opacities[index].1,
            &mut workspace.paints,
        );
        Ok(PreparedContinuousAppearancePaints {
            program,
            _bindings: bindings,
            workspace,
            scope,
        })
    }
}

impl<'program> PreparedContinuousAppearancePaints<'program, '_, '_> {
    pub(crate) fn evaluate_surface_inputs_canonical(
        &mut self,
        value_at: impl Fn(usize) -> ContinuousSrgbV1,
    ) -> ContinuousAppearanceEvaluationView<'program, '_> {
        let order = if self.scope.is_some() {
            self.workspace.cone.render.as_slice()
        } else {
            self.program.render_topo
        };
        self.program
            .execute_render_in_order::<ContinuousAppearanceKernel, _>(
                order,
                value_at,
                &self.workspace.paints,
                &mut self.workspace.surfaces,
                &mut self.workspace.occurrences,
            );
        ContinuousAppearanceEvaluationView {
            program: self.program,
            workspace: self.workspace,
            scoped: self.scope.is_some(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ContinuousAppearanceEvaluationView<'program, 'workspace> {
    program: CompiledAppearanceProgram<'program>,
    workspace: &'workspace ContinuousAppearanceWorkspace,
    scoped: bool,
}

impl ContinuousAppearanceEvaluationView<'_, '_> {
    pub(crate) fn occurrence_at(
        &self,
        slot: CompiledOccurrenceSlotV1,
    ) -> Option<&ContinuousResolvedOccurrenceV1> {
        if self.scoped
            && self
                .workspace
                .cone
                .occurrence_seen
                .get(slot.index)
                .is_none_or(|seen| !*seen)
        {
            return None;
        }
        let spec = self.program.occurrences.get(slot.index)?;
        if spec.id != slot.id {
            return None;
        }
        let occurrence = self.workspace.occurrences.get(slot.index)?.as_ref()?;
        (occurrence.id == slot.id).then_some(occurrence)
    }
}

#[cfg(test)]
mod tests;
