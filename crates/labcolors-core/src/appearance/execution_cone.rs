//! Compiler-owned roots share the physical graph; one reusable workspace
//! expands their dependency closure without retaining duplicate prefixes.

use super::*;

#[derive(Debug)]
pub(crate) struct CompiledAppearanceConeV1 {
    pub(super) graph_instance: Arc<()>,
    roots: Box<[CompiledOccurrenceSlotV1]>,
}

impl CompiledAppearanceConeV1 {
    #[cfg(test)]
    pub(crate) fn retained_root_count(&self) -> usize {
        self.roots.len()
    }
}

pub(crate) struct AppearanceConeCompilerV1<'graph> {
    graph: &'graph CompiledAppearanceGraph,
    seen: Vec<bool>,
    roots: Vec<CompiledOccurrenceSlotV1>,
}

fn slots<T: Clone>(len: usize, value: T) -> Result<Vec<T>, BindingError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(len)
        .map_err(|_| BindingError::ResourceExhausted)?;
    result.resize(len, value);
    Ok(result)
}

fn reserved<T>(len: usize) -> Result<Vec<T>, BindingError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(len)
        .map_err(|_| BindingError::ResourceExhausted)?;
    Ok(result)
}

impl<'graph> AppearanceConeCompilerV1<'graph> {
    pub(crate) fn new(graph: &'graph CompiledAppearanceGraph) -> Result<Self, BindingError> {
        Ok(Self {
            graph,
            seen: slots(graph.occurrences.len(), false)?,
            roots: reserved(graph.occurrences.len())?,
        })
    }

    pub(crate) fn occurrence(&mut self, id: OccurrenceId) -> Result<(), BindingError> {
        let slot = self
            .graph
            .bind_occurrence(id)
            .ok_or(BindingError::IncompatibleEvaluationCone)?;
        if !self.seen[slot.index] {
            self.seen[slot.index] = true;
            self.roots.push(slot);
        }
        Ok(())
    }

    pub(crate) fn finish(&mut self) -> Result<CompiledAppearanceConeV1, BindingError> {
        let mut roots = reserved(self.roots.len())?;
        roots.extend_from_slice(&self.roots);
        for slot in self.roots.drain(..) {
            self.seen[slot.index] = false;
        }
        Ok(CompiledAppearanceConeV1 {
            graph_instance: self.graph.instance.clone(),
            roots: roots.into_boxed_slice(),
        })
    }
}

#[derive(Debug)]
pub(super) struct AppearanceExecutionOrderV1 {
    paint_rank: Vec<usize>,
    surface_rank: Vec<usize>,
    occurrence_rank: Vec<usize>,
    occurrence_depth: Vec<usize>,
}

impl CompiledAppearanceGraph {
    // Dependency depth excludes the arbitrary sibling order of the executor.
    // It is computed once in the existing topological pass, not by walking
    // backdrop chains again during every search.
    pub(crate) fn occurrence_dependency_depth(
        &self,
        slot: CompiledOccurrenceSlotV1,
    ) -> Option<usize> {
        self.execution_order
            .occurrence_depth
            .get(slot.index)
            .copied()
    }
}

impl AppearanceExecutionOrderV1 {
    pub(super) fn new(
        paints: &[usize],
        render: &[RenderNode],
        shape: AppearanceWorkspaceShape,
        surfaces: &[CompiledSurfaceSpec],
        occurrences: &[CompiledOccurrenceSpec],
    ) -> Result<Self, BindingError> {
        let mut paint_rank = slots(shape.paints, 0)?;
        let mut surface_rank = slots(shape.surfaces, 0)?;
        let mut occurrence_rank = slots(shape.occurrences, 0)?;
        let mut occurrence_depth = slots(shape.occurrences, 0_usize)?;
        for (rank, &paint) in paints.iter().enumerate() {
            paint_rank[paint] = rank;
        }
        for (rank, node) in render.iter().enumerate() {
            match *node {
                RenderNode::Surface(index) => surface_rank[index] = rank,
                RenderNode::Occurrence(index) => {
                    occurrence_rank[index] = rank;
                    occurrence_depth[index] = match surfaces[occurrences[index].against] {
                        CompiledSurfaceSpec::Input { .. } => 0,
                        CompiledSurfaceSpec::FromOccurrence { occurrence, .. } => occurrence_depth
                            [occurrence]
                            .checked_add(1)
                            .ok_or(BindingError::ResourceExhausted)?,
                    };
                }
            }
        }
        Ok(Self {
            paint_rank,
            surface_rank,
            occurrence_rank,
            occurrence_depth,
        })
    }
}

#[derive(Debug)]
pub(super) struct AppearanceConeWorkspaceV1 {
    pub(super) paint_seen: Vec<bool>,
    pub(super) surface_seen: Vec<bool>,
    pub(super) occurrence_seen: Vec<bool>,
    pub(super) paints: Vec<usize>,
    pub(super) render: Vec<RenderNode>,
    pub(super) occurrences: Vec<usize>,
}

impl AppearanceConeWorkspaceV1 {
    pub(super) fn new(shape: AppearanceWorkspaceShape) -> Result<Self, BindingError> {
        Ok(Self {
            paint_seen: slots(shape.paints, false)?,
            surface_seen: slots(shape.surfaces, false)?,
            occurrence_seen: slots(shape.occurrences, false)?,
            paints: reserved(shape.paints)?,
            render: reserved(
                shape
                    .surfaces
                    .checked_add(shape.occurrences)
                    .ok_or(BindingError::ResourceExhausted)?,
            )?,
            occurrences: reserved(shape.occurrences)?,
        })
    }

    fn clear(&mut self) {
        for index in self.paints.drain(..) {
            self.paint_seen[index] = false;
        }
        for node in self.render.drain(..) {
            match node {
                RenderNode::Surface(index) => self.surface_seen[index] = false,
                RenderNode::Occurrence(index) => self.occurrence_seen[index] = false,
            }
        }
        self.occurrences.clear();
    }

    fn paint(&mut self, program: CompiledAppearanceProgram<'_>, mut index: usize) {
        while !self.paint_seen[index] {
            self.paint_seen[index] = true;
            self.paints.push(index);
            match program.paints[index] {
                CompiledPaintSpec::Input { .. } => break,
                CompiledPaintSpec::Opacity { source, .. } => index = source,
            }
        }
    }

    pub(super) fn prepare(
        &mut self,
        graph: &CompiledAppearanceGraph,
        cone: &CompiledAppearanceConeV1,
    ) {
        self.clear();
        let program = graph.program();
        for root in &cone.roots {
            let mut node = RenderNode::Occurrence(root.index);
            loop {
                node = match node {
                    RenderNode::Occurrence(index) => {
                        if self.occurrence_seen[index] {
                            break;
                        }
                        self.occurrence_seen[index] = true;
                        self.occurrences.push(index);
                        self.render.push(node);
                        let subject = program.occurrences[index].subject;
                        self.paint(program, subject);
                        RenderNode::Surface(program.occurrences[index].against)
                    }
                    RenderNode::Surface(index) => {
                        if self.surface_seen[index] {
                            break;
                        }
                        self.surface_seen[index] = true;
                        self.render.push(node);
                        match program.surfaces[index] {
                            CompiledSurfaceSpec::Input { .. } => break,
                            CompiledSurfaceSpec::FromOccurrence { occurrence, .. } => {
                                RenderNode::Occurrence(occurrence)
                            }
                        }
                    }
                };
            }
        }
        // Sorting only visited nodes preserves the original physical order;
        // no pass through the unrelated global topology is required.
        self.paints
            .sort_unstable_by_key(|&index| graph.execution_order.paint_rank[index]);
        self.render.sort_unstable_by_key(|node| match *node {
            RenderNode::Surface(index) => graph.execution_order.surface_rank[index],
            RenderNode::Occurrence(index) => graph.execution_order.occurrence_rank[index],
        });
        self.occurrences.sort_unstable();
    }
}

impl CompiledAppearanceGraph {
    /// Roots and bindings retain their graph identity. Every scoped pass builds
    /// fresh membership in the already allocated workspace and hides other slots.
    pub(crate) fn prepare_scoped_surface_cases<'program, 'bindings, 'workspace>(
        &'program self,
        cone: &'program CompiledAppearanceConeV1,
        bindings: &'bindings AdmittedAppearanceBindings,
        workspace: &'workspace mut AppearanceWorkspace,
    ) -> Result<PreparedAppearancePaints<'program, 'bindings, 'workspace>, BindingError> {
        if !Arc::ptr_eq(&self.instance, &cone.graph_instance) {
            return Err(BindingError::IncompatibleEvaluationCone);
        }
        let program = self.program();
        if bindings
            .graph_instance
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(owner, &self.instance))
        {
            workspace.validate_shape(program)?;
        } else {
            program.validate_admitted_workspace(bindings, workspace)?;
        }
        workspace.cone.prepare(self, cone);
        program.execute_paints_in_order::<ByteAppearanceKernel, _, _>(
            &workspace.cone.paints,
            |index| bindings.paint_inputs[index].1,
            |index| bindings.opacities[index].1,
            &mut workspace.paints,
        );
        Ok(PreparedAppearancePaints {
            graph_instance: &self.instance,
            program,
            _bindings: bindings,
            workspace,
            scope: Some(cone),
        })
    }
}
