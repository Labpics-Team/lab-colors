//! Certified views reuse the compiled operations, dependency cones and buffers.

use super::numeric_kernel::{
    CertifiedAppearanceKernel, CertifiedColorResultV1, CertifiedResolvedOccurrenceV1,
};
use super::*;

#[derive(Debug)]
pub(crate) struct PreparedCertifiedAppearancePaints<'program, 'bindings, 'workspace> {
    program: CompiledAppearanceProgram<'program>,
    _bindings: &'bindings AdmittedAppearanceBindings,
    workspace: &'workspace mut CertifiedAppearanceWorkspace,
    scope: Option<&'program CompiledAppearanceConeV1>,
}

impl CompiledAppearanceGraph {
    pub(crate) fn new_certified_workspace(
        &self,
    ) -> Result<CertifiedAppearanceWorkspace, BindingError> {
        CertifiedAppearanceWorkspace::for_program(self.program())
    }

    /// The admitted binding schema fixes the canonical closure indices and the
    /// graph opacity inputs. Each atomic source (RGB,alpha) is supplied without
    /// quantization; prepared paints are reused for all subsequent surface cases.
    pub(crate) fn prepare_certified_surface_cases<'program, 'bindings, 'workspace>(
        &'program self,
        scope: Option<&'program CompiledAppearanceConeV1>,
        bindings: &'bindings AdmittedAppearanceBindings,
        paint_value_at: impl Fn(usize) -> PointPaintValueV1<CertifiedColorResultV1>,
        workspace: &'workspace mut CertifiedAppearanceWorkspace,
    ) -> Result<PreparedCertifiedAppearancePaints<'program, 'bindings, 'workspace>, BindingError>
    {
        let program = self.validated_preparation_program(scope, bindings, workspace)?;
        let order = if let Some(cone) = scope {
            workspace.cone.prepare(self, cone);
            workspace.cone.paints.as_slice()
        } else {
            program.paint_topo
        };
        program.execute_paints_in_order::<CertifiedAppearanceKernel, _, _>(
            order,
            paint_value_at,
            |index| bindings.opacities[index].1,
            &mut workspace.paints,
        );
        Ok(PreparedCertifiedAppearancePaints {
            program,
            _bindings: bindings,
            workspace,
            scope,
        })
    }
}

impl<'program> PreparedCertifiedAppearancePaints<'program, '_, '_> {
    pub(crate) fn evaluate_surface_inputs_canonical(
        &mut self,
        value_at: impl Fn(usize) -> CertifiedColorResultV1,
    ) -> CertifiedAppearanceEvaluationView<'program, '_> {
        let order = if self.scope.is_some() {
            self.workspace.cone.render.as_slice()
        } else {
            self.program.render_topo
        };
        self.program
            .execute_render_in_order::<CertifiedAppearanceKernel, _>(
                order,
                value_at,
                &self.workspace.paints,
                &mut self.workspace.surfaces,
                &mut self.workspace.occurrences,
            );
        CertifiedAppearanceEvaluationView {
            program: self.program,
            workspace: self.workspace,
            scoped: self.scope.is_some(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct CertifiedAppearanceEvaluationView<'program, 'workspace> {
    program: CompiledAppearanceProgram<'program>,
    workspace: &'workspace CertifiedAppearanceWorkspace,
    scoped: bool,
}

impl CertifiedAppearanceEvaluationView<'_, '_> {
    pub(crate) fn occurrence_at(
        &self,
        slot: CompiledOccurrenceSlotV1,
    ) -> Option<&CertifiedResolvedOccurrenceV1> {
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
