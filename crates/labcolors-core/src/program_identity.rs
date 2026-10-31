//! Устойчивый к коллизиям адрес исполняемого содержимого принятой Program.
//!
//! Paint/Surface/Occurrence способны образовывать двудольные графы
//! инцидентности, поэтому одного топологического хеша или уточнения разбиения
//! недостаточно. Модуль строит типизированный цветной граф, канонизирует его без
//! opaque ID и хеширует канонический прообраз.

use super::*;

mod refinement {
    use super::*;

    #[derive(Clone, Copy)]
    struct Cell {
        physical_begin: usize,
        len: usize,
        logical_start: usize,
    }

    #[derive(Clone, Copy)]
    struct Group {
        member_begin: usize,
        member_end: usize,
        len: usize,
        logical_start: usize,
    }

    #[derive(Clone, Copy)]
    struct Split {
        cell: usize,
        group_begin: usize,
        group_end: usize,
        retained: usize,
        largest: usize,
    }

    #[derive(Default)]
    struct Workspace {
        cell_of: Vec<usize>,
        order: Vec<usize>,
        vertex_pos: Vec<usize>,
        cells: Vec<Cell>,
        marked: Vec<bool>,
        dirty: Vec<usize>,
        signatures: Vec<RefinementAtomV1>,
        records: Vec<RefinementRecordV1>,
        members: Vec<usize>,
        groups: Vec<Group>,
        splits: Vec<Split>,
        recycled_singletons: Vec<Vec<usize>>,
        #[cfg(test)]
        trace_starts: Vec<usize>,
    }

    impl Workspace {
        // Every scratch buffer has one owner before the first allocation.
        #[inline(never)]
        fn prepare(
            &mut self,
            graph: &CanonicalGraphV1,
            partition: PartitionV1,
            #[cfg(test)] with_trace: bool,
        ) -> Result<usize, ProgramCompileError> {
            let vertex_count = graph.colors.len();
            self.cell_of = partition_cells(&partition, vertex_count)?;
            self.order
                .try_reserve_exact(vertex_count)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            self.vertex_pos
                .try_reserve_exact(vertex_count)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            self.vertex_pos.resize(vertex_count, 0_usize);
            self.cells
                .try_reserve_exact(vertex_count)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            self.marked
                .try_reserve_exact(vertex_count)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            self.marked.resize(vertex_count, false);
            self.dirty
                .try_reserve_exact(vertex_count)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            let mut non_singletons = 0_usize;
            let mut maximum_records = 0_usize;
            let mut maximum_atoms = 0_usize;
            self.recycled_singletons = partition.cells;
            for vertices in &self.recycled_singletons {
                if vertices.is_empty() {
                    return Err(ProgramCompileError::InternalInvariant);
                }
                let physical_begin = self.order.len();
                let len = vertices.len();
                let mut atom_count = 0_usize;
                if len > 1 {
                    non_singletons += 1;
                    maximum_records = maximum_records.max(len);
                }
                for &vertex in vertices {
                    self.vertex_pos[vertex] = self.order.len();
                    self.order.push(vertex);
                    if len > 1 {
                        self.marked[vertex] = true;
                        self.dirty.push(vertex);
                        let arcs = graph
                            .adjacency
                            .get(vertex)
                            .ok_or(ProgramCompileError::InternalInvariant)?;
                        atom_count = atom_count
                            .checked_add(arcs.len())
                            .ok_or(ProgramCompileError::ResourceExhausted)?;
                    }
                }
                maximum_atoms = maximum_atoms.max(atom_count);
                self.cells.push(Cell {
                    physical_begin,
                    len,
                    logical_start: physical_begin,
                });
            }
            // Keep only buffers whose retained capacity cannot follow a large
            // parent into a tiny descendant. Refinement never merges singletons.
            self.recycled_singletons.retain(|vertices| vertices.len() == 1);
            // Dirty vertices and the clean representative are distinct members
            // of one descendant of an initial cell.
            self.signatures
                .try_reserve_exact(maximum_atoms)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            self.records
                .try_reserve_exact(maximum_records)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            // Staged parents are disjoint; an actual split needs size >= 2.
            self.members
                .try_reserve_exact(vertex_count)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            self.groups
                .try_reserve_exact(vertex_count)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            self.splits
                .try_reserve_exact(vertex_count / 2)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            #[cfg(test)]
            if with_trace {
                self.trace_starts = copy_vertices(&self.cell_of)?;
            }
            Ok(non_singletons)
        }
    }

    fn append_signature(
        graph: &CanonicalGraphV1,
        vertex: usize,
        cell_of: &[usize],
        cells: &[Cell],
        signatures: &mut Vec<RefinementAtomV1>,
        records: &mut Vec<RefinementRecordV1>,
        #[cfg(test)] trace: &mut Option<&mut RefinementTraceV1>,
    ) -> Result<(), ProgramCompileError> {
        let arcs = graph
            .adjacency
            .get(vertex)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        let start = signatures.len();
        #[cfg(test)]
        if let Some(trace) = trace.as_deref_mut() {
            trace.signature_vertices = trace.signature_vertices.saturating_add(1);
        }
        for arc in arcs {
            #[cfg(test)]
            if let Some(trace) = trace.as_deref_mut() {
                trace.signature_arcs = trace.signature_arcs.saturating_add(1);
            }
            let neighbour = *cell_of
                .get(arc.neighbour)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            let neighbour_cell = cells
                .get(neighbour)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            signatures.push(RefinementAtomV1 {
                direction: arc.direction,
                role: arc.role,
                neighbour_cell: neighbour_cell.logical_start,
            });
        }
        let signature = start..signatures.len();
        ordering::sort(&mut signatures[signature.clone()]);
        records.push(RefinementRecordV1 { vertex, signature });
        Ok(())
    }

    fn mark_neighbours(
        graph: &CanonicalGraphV1,
        vertices: &[usize],
        marked: &mut [bool],
        dirty: &mut Vec<usize>,
    ) -> Result<(), ProgramCompileError> {
        for &vertex in vertices {
            let arcs = graph
                .adjacency
                .get(vertex)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            for arc in arcs {
                let mark = marked
                    .get_mut(arc.neighbour)
                    .ok_or(ProgramCompileError::InternalInvariant)?;
                if !*mark {
                    *mark = true;
                    dirty.push(arc.neighbour);
                }
            }
        }
        Ok(())
    }

    fn materialize(
        order: &[usize],
        cells: &[Cell],
        by_start: &mut [usize],
        recycled_singletons: &mut Vec<Vec<usize>>,
    ) -> Result<PartitionV1, ProgramCompileError> {
        // Refinement preserves each input singleton. Reuse its inner buffer
        // and the input outer Vec instead of allocating them again.
        let mut result = core::mem::take(recycled_singletons);
        let additional = cells
            .len()
            .checked_sub(result.len())
            .ok_or(ProgramCompileError::InternalInvariant)?;
        result
            .try_reserve_exact(additional)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        // Logical intervals partition 0..V. Only their starts are read below.
        for (index, cell) in cells.iter().enumerate() {
            let slot = by_start
                .get_mut(cell.logical_start)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            *slot = index;
        }
        let mut cursor = 0;
        let mut completed = 0;
        while cursor < order.len() {
            let index = *by_start
                .get(cursor)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            let cell = cells
                .get(index)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            if cell.logical_start != cursor || cell.len == 0 || completed == cells.len() {
                return Err(ProgramCompileError::InternalInvariant);
            }
            let physical_end = cell
                .physical_begin
                .checked_add(cell.len)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            let vertices = order
                .get(cell.physical_begin..physical_end)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            // The prefix is finalized; the suffix contains spare singletons.
            // Appending a non-singleton moves its displaced spare to the end.
            if cell.len == 1 && completed < result.len() {
                let singleton = &mut result[completed];
                if singleton.len() != 1 {
                    return Err(ProgramCompileError::InternalInvariant);
                }
                singleton[0] = vertices[0];
            } else {
                result.push(copy_vertices(vertices)?);
                let appended = result.len() - 1;
                result.swap(completed, appended);
            }
            completed += 1;
            cursor = cursor
                .checked_add(cell.len)
                .ok_or(ProgramCompileError::InternalInvariant)?;
        }
        if cursor != order.len() || completed != cells.len() || result.len() != completed {
            return Err(ProgramCompileError::InternalInvariant);
        }
        Ok(PartitionV1 { cells: result })
    }

    pub(super) fn refine(
        graph: &CanonicalGraphV1,
        partition: PartitionV1,
        #[cfg(test)] mut trace: Option<&mut RefinementTraceV1>,
    ) -> Result<EquitablePartitionV1, ProgramCompileError> {
        if partition.is_discrete() {
            return Ok(EquitablePartitionV1(partition));
        }
        let mut workspace = Workspace::default();
        let mut non_singletons = workspace.prepare(
            graph,
            partition,
            #[cfg(test)]
            trace.is_some(),
        )?;
        let Workspace {
            cell_of,
            order,
            vertex_pos,
            cells,
            marked,
            dirty,
            signatures,
            records,
            members,
            groups,
            splits,
            recycled_singletons,
            #[cfg(test)]
            trace_starts,
        } = &mut workspace;

        loop {
            members.clear();
            groups.clear();
            splits.clear();
            #[cfg(test)]
            let mut touched_cells = 0_usize;
            // Initially dirty is already grouped by the initial cell IDs.
            // Subsequent rounds sort only their explicit dirty worklist.
            let mut dirty_begin = 0;
            while dirty_begin < dirty.len() {
                let cell_id = cell_of[dirty[dirty_begin]];
                let mut dirty_end = dirty_begin + 1;
                while dirty_end < dirty.len() && cell_of[dirty[dirty_end]] == cell_id {
                    dirty_end += 1;
                }
                let cell = cells[cell_id];
                if cell.len == 1 {
                    dirty_begin = dirty_end;
                    continue;
                }
                #[cfg(test)]
                {
                    touched_cells += 1;
                }
                records.clear();
                signatures.clear();
                let clean_count = cell
                    .len
                    .checked_sub(dirty_end - dirty_begin)
                    .ok_or(ProgramCompileError::InternalInvariant)?;
                let clean_vertex = if clean_count == 0 {
                    None
                } else {
                    // At most the dirty members precede the first clean member.
                    let representative = order[cell.physical_begin..cell.physical_begin + cell.len]
                        .iter()
                        .copied()
                        .find(|&vertex| !marked[vertex])
                        .ok_or(ProgramCompileError::InternalInvariant)?;
                    Some(representative)
                };
                for &vertex in &dirty[dirty_begin..dirty_end] {
                    append_signature(
                        graph,
                        vertex,
                        cell_of,
                        cells,
                        signatures,
                        records,
                        #[cfg(test)]
                        &mut trace,
                    )?;
                }
                if let Some(vertex) = clean_vertex {
                    append_signature(
                        graph,
                        vertex,
                        cell_of,
                        cells,
                        signatures,
                        records,
                        #[cfg(test)]
                        &mut trace,
                    )?;
                }
                ordering::sort_by(records, |left, right| {
                    signatures[left.signature.clone()].cmp(&signatures[right.signature.clone()])
                });
                let group_begin = groups.len();
                let member_begin = members.len();
                let mut retained = None;
                let mut largest = group_begin;
                let mut logical_start = cell.logical_start;
                let mut record_begin = 0;
                while record_begin < records.len() {
                    let mut record_end = record_begin + 1;
                    while record_end < records.len()
                        && signatures[records[record_begin].signature.clone()]
                            == signatures[records[record_end].signature.clone()]
                    {
                        record_end += 1;
                    }
                    let group_index = groups.len();
                    let group_member_begin = members.len();
                    let mut len = 0;
                    for record in &records[record_begin..record_end] {
                        if Some(record.vertex) == clean_vertex {
                            len += clean_count;
                            retained = Some(group_index);
                        } else {
                            members.push(record.vertex);
                            len += 1;
                        }
                    }
                    groups.push(Group {
                        member_begin: group_member_begin,
                        member_end: members.len(),
                        len,
                        logical_start,
                    });
                    if len > groups[largest].len {
                        largest = group_index;
                    }
                    logical_start += len;
                    record_begin = record_end;
                }
                if groups.len() - group_begin > 1 {
                    splits.push(Split {
                        cell: cell_id,
                        group_begin,
                        group_end: groups.len(),
                        retained: retained.unwrap_or(largest),
                        largest,
                    });
                } else {
                    groups.truncate(group_begin);
                    members.truncate(member_begin);
                }
                dirty_begin = dirty_end;
            }
            #[cfg(test)]
            if let Some(trace) = trace.as_deref_mut() {
                trace.skipped_cells = trace
                    .skipped_cells
                    .saturating_add(non_singletons - touched_cells);
            }
            let changed = !splits.is_empty();
            if changed {
                // Every signature above used the same old labels. No old labels
                // are read after this point, so all physical commits are safe.
                for &vertex in dirty.iter() {
                    marked[vertex] = false;
                }
                dirty.clear();
                for split in splits.iter() {
                    let parent = cells[split.cell];
                    let retained = groups[split.retained];
                    let mut tail = parent.physical_begin + retained.len;
                    cells[split.cell] = Cell {
                        physical_begin: parent.physical_begin,
                        len: retained.len,
                        logical_start: retained.logical_start,
                    };
                    non_singletons -= 1;
                    let children = &groups[split.group_begin..split.group_end];
                    for (offset, &group) in children.iter().enumerate() {
                        let group_index = split.group_begin + offset;
                        if group.len > 1 {
                            non_singletons += 1;
                        }
                        if group_index == split.retained {
                            continue;
                        }
                        // The retained group owns every implicit clean member.
                        // Every other group's membership is fully explicit.
                        if group.member_end - group.member_begin != group.len {
                            return Err(ProgramCompileError::InternalInvariant);
                        }
                        let new_cell = cells.len();
                        let end = tail + group.len;
                        for (&vertex, destination) in members[group.member_begin..group.member_end]
                            .iter()
                            .zip(tail..end)
                        {
                            let source = vertex_pos[vertex];
                            let displaced = order[destination];
                            order.swap(source, destination);
                            vertex_pos[vertex] = destination;
                            vertex_pos[displaced] = source;
                            cell_of[vertex] = new_cell;
                        }
                        cells.push(Cell {
                            physical_begin: tail,
                            len: group.len,
                            logical_start: group.logical_start,
                        });
                        // Finalized tail ranges cannot be disturbed by later
                        // groups: their members are disjoint and positions exact.
                        if group_index != split.largest {
                            mark_neighbours(graph, &order[tail..end], marked, dirty)?;
                        }
                        tail = end;
                    }
                    // After every other group moves to its tail interval, the
                    // prefix contains exactly the retained group, without a scan.
                    if tail != parent.physical_begin + parent.len {
                        return Err(ProgramCompileError::InternalInvariant);
                    }
                    if split.retained != split.largest {
                        mark_neighbours(
                            graph,
                            &order[parent.physical_begin..parent.physical_begin + retained.len],
                            marked,
                            dirty,
                        )?;
                    }
                }
            }
            #[cfg(test)]
            if let Some(trace) = trace.as_deref_mut() {
                let snapshot = materialize(order, cells, trace_starts, &mut Vec::new())?;
                trace
                    .rounds
                    .try_reserve(1)
                    .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                trace.rounds.push(snapshot.cells);
            }
            if !changed || non_singletons == 0 || dirty.is_empty() {
                break;
            }
            ordering::sort_by(dirty, |left, right| {
                cell_of[*left].cmp(&cell_of[*right])
            });
        }
        // cell_of is no longer needed as a vertex map. Reuse its V slots as
        // logical-start -> cell ID, avoiding a final allocation or global sort.
        Ok(EquitablePartitionV1(materialize(
            order,
            cells,
            cell_of,
            recycled_singletons,
        )?))
    }
}

const DOMAIN_V9: &[u8] = b"labcolors.program-content-identity.v9\0";
// V9 резервирует фиксированную protocol boundary под type/family tag и полный
// content digest. Каждый writer использует проверяемый push и отвергает
// расширение схемы вместо усечения данных или аллокации на вершину.
const HASH_BOUND_COLOR_BYTES_V9: usize = 1 + 1 + 32;
const COLOR_CAPACITY: usize = HASH_BOUND_COLOR_BYTES_V9;

mod release_tag {
    pub(super) const PROGRAM_SCHEMA_V9: u8 = 9;
    pub(super) const SELECTION_RELEASE_IDENTITY_V1: u8 = 1;
    pub(super) const DECLARED_TOTAL_ORDER_V1: u8 = 1;
    pub(super) const FRESH_FULL_RECHECK_V1: u8 = 1;
    pub(super) const ATOMIC_OBSERVATION_GROUP_V1: u8 = 1;
    pub(super) const ENCODED_PAINT_OUTPUT_ROUTING_V1: u8 = 1;
    pub(super) const FINITE_ATOMIC_PAINT_CANDIDATE_V1: u8 = 1;
    pub(super) const MODELED_POINT_PRESENTATION_V1: u8 = 1;
    pub(super) const FIELD_EFFECT_GRAPH_V1: u8 = 1;
    pub(super) const POINT_ABSENCE_BYPASS_OWN_BACKDROP_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const FAMILY_CERTIFICATE_VERTEX_V1: u8 = 1;
    pub(super) const FAMILY_SEMANTIC_RELEASE_VERTEX_V2: u8 = 2;
    #[cfg(test)]
    pub(super) const MODELED_LCS_OCCURRENCE_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const MODELED_LCS_OCCURRENCE_MUTATION_SENTINEL_V1: u8 = 2;

    pub(super) const IEC_SRGB8_D65_OUTPUT_PROFILE_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const IEC_SRGB8_TO_XYZ_D65_TRANSFORM_V1: u8 = 1;
    pub(super) const CIE1931_TWO_DEGREE_OBSERVER_V1: u8 = 1;
    pub(super) const IEC61966_D65_REFERENCE_WHITE_V1: u8 = 1;
    pub(super) const RELATIVE_Y1_SCALE_V1: u8 = 1;
    pub(super) const XYZ_FRAME_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const MUTATION_SENTINEL_FRAME_V1: u8 = 2;
    pub(super) const CIECAM16_VIEWING_INPUTS_V1: u8 = 1;
    pub(super) const ENCODED_SRGB8_SOURCE_OVER_V1: u8 = 1;
    pub(super) const SURROUND_AVERAGE_V1: u8 = 1;
    pub(super) const SURROUND_DIM_V1: u8 = 2;
    pub(super) const SURROUND_DARK_V1: u8 = 3;

    pub(super) const FIELD_WORKING_ENCODED_SRGB8_PREMULTIPLIED_V1: u8 = 1;
    pub(super) const FIELD_WORKING_LINEAR_SRGB_Q31_PREMULTIPLIED_V1: u8 = 2;
    pub(super) const FIELD_PRECISION_FIXED_Q32_V1: u8 = 1;
    pub(super) const FIELD_PRECISION_BINARY32_V1: u8 = 2;
    pub(super) const FIELD_QUANTIZATION_HALF_UP_SRGB8_V1: u8 = 1;
    pub(super) const FIELD_QUANTIZATION_TIES_EVEN_SRGB8_V1: u8 = 2;
    pub(super) const FIELD_CARRIER_PRESENT_V1: u8 = 1;
    pub(super) const FIELD_CARRIER_CONTRIBUTES_V1: u8 = 2;
    pub(super) const FIELD_CARRIER_SPATIAL_VARIATION_V1: u8 = 3;
    pub(super) const FIELD_GAUSSIAN_BINOMIAL_Q32_V1: u8 = 1;
    pub(super) const FIELD_GAUSSIAN_CLAMP_TO_EDGE_V1: u8 = 1;

    pub(super) const EXACT_SRGB8_FAMILY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_IDENTITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_RELEASE_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_CAPABILITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_INTRINSIC_UNARY_FAMILY_V1: u8 = 4;
    pub(super) const EXACT_SRGB8_INTRINSIC_UNARY_IDENTITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_INTRINSIC_UNARY_RELEASE_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_INTRINSIC_UNARY_CAPABILITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_RELATION_FAMILY_V1: u8 = 5;
    pub(super) const EXACT_SRGB8_RELATION_IDENTITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_RELATION_RELEASE_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_RELATION_CAPABILITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_DISTINCTION_RELATION_FAMILY_V1: u8 = 8;
    pub(super) const EXACT_SRGB8_DISTINCTION_RELATION_IDENTITY_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_DISTINCTION_RELATION_RELEASE_V1: u8 = 1;
    pub(super) const EXACT_SRGB8_DISTINCTION_RELATION_CAPABILITY_V1: u8 = 1;
    pub(super) const FAMILY_CATEGORY_RELATION_FAMILY_V1: u8 = 9;
    pub(super) const FAMILY_CATEGORY_RELATION_IDENTITY_V1: u8 = 1;
    pub(super) const FAMILY_CATEGORY_RELATION_RELEASE_V1: u8 = 1;
    pub(super) const FAMILY_CATEGORY_RELATION_CAPABILITY_V1: u8 = 1;
    pub(super) const FAMILY_MEMBERSHIP_FAMILY_V1: u8 = 6;
    pub(super) const FAMILY_MEMBERSHIP_IDENTITY_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const FAMILY_MEMBERSHIP_RELEASE_V1: u8 = 1;
    pub(super) const FAMILY_MEMBERSHIP_RELEASE_V2: u8 = 2;
    pub(super) const FAMILY_MEMBERSHIP_CAPABILITY_V1: u8 = 1;
    #[cfg(test)]
    pub(super) const EXACT_SRGB8_IDENTITY_MUTATION_SENTINEL_V1: u8 = 2;
    #[cfg(test)]
    pub(super) const EXACT_SRGB8_RELEASE_MUTATION_SENTINEL_V1: u8 = 2;
    #[cfg(test)]
    pub(super) const EXACT_SRGB8_CAPABILITY_MUTATION_SENTINEL_V1: u8 = 2;
    pub(super) const WCAG22_SRGB8_FAMILY_V1: u8 = 2;
    pub(super) const WCAG22_SRGB8_IDENTITY_V1: u8 = 1;
    pub(super) const WCAG22_SRGB8_PROFILE_V1: u8 = 1;
    pub(super) const WCAG22_SRGB8_CAPABILITY_V1: u8 = 1;
    pub(super) const WCAG22_SC_1_4_3_TEXT_DEFAULT: u8 = 1;
    pub(super) const WCAG22_SC_1_4_3_TEXT_LARGE_SCALE: u8 = 2;
    pub(super) const WCAG22_SC_1_4_11_UI_COMPONENT_OR_STATE: u8 = 3;
    pub(super) const WCAG22_SC_1_4_11_GRAPHICAL_OBJECT: u8 = 4;
    pub(super) const DECLARED_SRGB8_CLEAN_SET_FAMILY_V1: u8 = 3;
    #[cfg(test)]
    pub(super) const MODELED_LCS_PROBE_FAMILY_V1: u8 = 7;
}

/// Устойчивый к коллизиям адрес канонизированного содержимого Program V9.
///
/// SHA-256 не делает адрес инъективным. Адрес не связывает пространства opaque
/// ID и не подтверждает владельца, поколение либо revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ProgramContentIdentityV9([u8; 32]);

impl ProgramContentIdentityV9 {
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct VertexColorV1 {
    // Длина предшествует payload: сравнение ключей сохраняет порядок V9
    // и выполняется одной операцией над фиксированным массивом.
    bytes: [u8; COLOR_CAPACITY + 1],
}

impl VertexColorV1 {
    fn new(tag: u8) -> Self {
        let mut value = Self {
            bytes: [0; COLOR_CAPACITY + 1],
        };
        value.bytes[0] = 1;
        value.bytes[1] = tag;
        value
    }

    fn len(&self) -> u8 {
        self.bytes[0]
    }

    fn push_u8(&mut self, value: u8) -> Result<(), ProgramCompileError> {
        self.push_bytes(&[value])
    }

    fn push_bytes(&mut self, values: &[u8]) -> Result<(), ProgramCompileError> {
        let start = usize::from(self.len()) + 1;
        let end = start
            .checked_add(values.len())
            .ok_or(ProgramCompileError::InternalInvariant)?;
        self.bytes
            .get_mut(start..end)
            .ok_or(ProgramCompileError::InternalInvariant)?
            .copy_from_slice(values);
        self.bytes[0] =
            u8::try_from(end - 1).map_err(|_| ProgramCompileError::InternalInvariant)?;
        Ok(())
    }

    fn push_u64(&mut self, value: u64) -> Result<(), ProgramCompileError> {
        self.push_bytes(&value.to_be_bytes())
    }

    fn push_srgb8(&mut self, value: Srgb8) -> Result<(), ProgramCompileError> {
        self.push_bytes(&value.bytes())
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[1..=usize::from(self.len())]
    }

    const fn tag(&self) -> u8 {
        self.bytes[1]
    }
}

mod vertex_tag {
    pub(super) const PROGRAM: u8 = 1;
    pub(super) const SOURCE: u8 = 2;
    pub(super) const TARGET_FIXED: u8 = 3;
    pub(super) const TARGET_FINITE: u8 = 4;
    pub(super) const CANDIDATE: u8 = 5;
    pub(super) const OPACITY: u8 = 6;
    pub(super) const PAINT_SOLID: u8 = 7;
    pub(super) const PAINT_OPACITY: u8 = 8;
    pub(super) const OBSERVATION_GROUP: u8 = 9;
    pub(super) const SURFACE_INPUT_PORT: u8 = 10;
    pub(super) const SURFACE_INPUT: u8 = 11;
    pub(super) const SURFACE_FROM_OCCURRENCE: u8 = 12;
    pub(super) const OCCURRENCE: u8 = 13;
    pub(super) const CONSTRAINT_HARD: u8 = 14;
    pub(super) const CONSTRAINT_REPORT_ONLY: u8 = 15;
    pub(super) const OUTPUT: u8 = 16;
    pub(super) const JOINT_SELECTION: u8 = 17;
    pub(super) const JOINT_STATE: u8 = 18;
    pub(super) const JOINT_CHOICE: u8 = 19;
    pub(super) const PRESENTATION_ROOT: u8 = 20;
    pub(super) const PRESENTATION_TARGET: u8 = 21;
    pub(super) const FAMILY: u8 = 22;
    pub(super) const FIELD_INPUT_PREMULTIPLIED_RGBA8: u8 = 23;
    pub(super) const FIELD_INPUT_ENCODED_SRGB8_ALPHA: u8 = 24;
    pub(super) const FIELD_INPUT_OPAQUE_SRGB8: u8 = 25;
    pub(super) const FIELD_OPERATOR_GAUSSIAN_Q32: u8 = 26;
    pub(super) const FIELD_OPERATOR_SOURCE_OVER: u8 = 27;
    pub(super) const FIELD_OPERATOR_SCREEN: u8 = 28;
    pub(super) const FIELD_OPERATOR_LIGHTER: u8 = 29;
}

macro_rules! declare_edge_roles_v1 {
    ($($name:ident = $value:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        #[repr(u8)]
        enum EdgeRoleV1 {
            $($name = $value),+
        }

        #[cfg(test)]
        const EDGE_ROLES_V1: &[EdgeRoleV1] = &[$(EdgeRoleV1::$name),+];
    };
}

declare_edge_roles_v1! {
    ProgramMember = 1,
    TargetSource = 2,
    TargetCandidate = 3,
    SolidTarget = 4,
    OpacitySourcePaint = 5,
    OpacityInput = 6,
    ObservationGroupPort = 7,
    InputSurfacePort = 8,
    DerivedSurfaceOccurrence = 9,
    OccurrenceSubjectPaint = 10,
    OccurrenceBackdropSurface = 11,
    VisibleUnary = 12,
    OutputPaint = 13,
    SelectionState = 14,
    StateChoice = 15,
    ChoiceTarget = 16,
    ChoiceCandidate = 17,
    PresentationRootTerminal = 18,
    PresentationTargetRoot = 19,
    PresentationTargetOccurrence = 20,
    ConstraintPresentationTarget = 21,
    IntrinsicUnary = 22,
    IntrinsicReference = 23,
    IntrinsicCandidate = 24,
    VisibleReference = 25,
    VisibleCandidate = 26,
    ObservationGroup = 27,
    ConstraintFamily = 28,
    FieldInputObservationGroup = 29,
    FieldGaussianSource = 30,
    FieldSourceOverSource = 31,
    FieldSourceOverDestination = 32,
    FieldScreenSource = 33,
    FieldScreenBackdrop = 34,
    FieldLighterSource = 35,
    FieldLighterDestination = 36,
    FieldOutputOperator = 37,
}

#[derive(Debug, Clone, Copy)]
struct EdgeV1 {
    from: usize,
    to: usize,
    role: EdgeRoleV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ArcV1 {
    direction: u8,
    role: EdgeRoleV1,
    neighbour: usize,
}

struct CanonicalGraphV1 {
    colors: Vec<VertexColorV1>,
    adjacency: Vec<Vec<ArcV1>>,
    edge_count: usize,
}

struct GraphBuilderV1 {
    colors: Vec<VertexColorV1>,
    edges: Vec<EdgeV1>,
    root: usize,
}

impl GraphBuilderV1 {
    fn new(root: VertexColorV1) -> Result<Self, ProgramCompileError> {
        let mut colors = Vec::new();
        colors
            .try_reserve_exact(1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        colors.push(root);
        Ok(Self {
            colors,
            edges: Vec::new(),
            root: 0,
        })
    }

    fn add_member(&mut self, color: VertexColorV1) -> Result<usize, ProgramCompileError> {
        self.colors
            .try_reserve(1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        let index = self.colors.len();
        self.colors.push(color);
        self.add_edge(self.root, index, EdgeRoleV1::ProgramMember)?;
        Ok(index)
    }

    fn add_edge(
        &mut self,
        from: usize,
        to: usize,
        role: EdgeRoleV1,
    ) -> Result<(), ProgramCompileError> {
        if from >= self.colors.len() || to >= self.colors.len() {
            return Err(ProgramCompileError::InternalInvariant);
        }
        self.edges
            .try_reserve(1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        self.edges.push(EdgeV1 { from, to, role });
        Ok(())
    }

    fn finish(self) -> Result<CanonicalGraphV1, ProgramCompileError> {
        let mut degrees = Vec::new();
        degrees
            .try_reserve_exact(self.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        degrees.resize(self.colors.len(), 0_usize);
        for edge in &self.edges {
            degrees[edge.from] = degrees[edge.from]
                .checked_add(1)
                .ok_or(ProgramCompileError::ResourceExhausted)?;
            degrees[edge.to] = degrees[edge.to]
                .checked_add(1)
                .ok_or(ProgramCompileError::ResourceExhausted)?;
        }

        let mut adjacency = Vec::new();
        adjacency
            .try_reserve_exact(self.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for degree in degrees {
            let mut arcs = Vec::new();
            arcs.try_reserve_exact(degree)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            adjacency.push(arcs);
        }
        for edge in &self.edges {
            adjacency[edge.from].push(ArcV1 {
                direction: 0,
                role: edge.role,
                neighbour: edge.to,
            });
            adjacency[edge.to].push(ArcV1 {
                direction: 1,
                role: edge.role,
                neighbour: edge.from,
            });
        }
        for arcs in &mut adjacency {
            ordering::sort(arcs);
        }
        Ok(CanonicalGraphV1 {
            colors: self.colors,
            adjacency,
            edge_count: self.edges.len(),
        })
    }
}

struct IdIndexV1<Key> {
    values: Vec<(Key, usize)>,
}

impl<Key> IdIndexV1<Key>
where
    Key: Copy + Ord,
{
    fn new() -> Self {
        Self { values: Vec::new() }
    }

    fn insert(&mut self, key: Key, value: usize) -> Result<(), ProgramCompileError> {
        self.values
            .try_reserve(1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        self.values.push((key, value));
        Ok(())
    }

    fn finish(&mut self) -> Result<(), ProgramCompileError> {
        ordering::sort_by_key(&mut self.values, |(key, _)| *key);
        if self.values.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(ProgramCompileError::InternalInvariant);
        }
        Ok(())
    }

    fn get(&self, key: Key) -> Result<usize, ProgramCompileError> {
        let index = self
            .values
            .binary_search_by_key(&key, |(candidate, _)| *candidate)
            .map_err(|_| ProgramCompileError::InternalInvariant)?;
        Ok(self.values[index].1)
    }
}

fn program_root_color() -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::PROGRAM);
    // Эти теги связывают адрес с версиями исполняемых законов: схемой Program,
    // выбором по полному порядку, финальной перепроверкой, атомарным наблюдением,
    // маршрутизацией кодированного `Paint` в output slot и моделируемым
    // представлением точки. Версии
    // производных возможностей связываются только с теми ограничениями,
    // которые их исполняют.
    for release in [
        release_tag::PROGRAM_SCHEMA_V9,
        release_tag::DECLARED_TOTAL_ORDER_V1,
        release_tag::FRESH_FULL_RECHECK_V1,
        release_tag::ATOMIC_OBSERVATION_GROUP_V1,
        release_tag::ENCODED_PAINT_OUTPUT_ROUTING_V1,
        release_tag::FINITE_ATOMIC_PAINT_CANDIDATE_V1,
        release_tag::MODELED_POINT_PRESENTATION_V1,
        release_tag::FIELD_EFFECT_GRAPH_V1,
    ] {
        color.push_u8(release)?;
    }
    Ok(color)
}

fn joint_selection_color(
    selection: &MaterialisedSelectionV1,
) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::JOINT_SELECTION);
    color.push_u8(release_tag::SELECTION_RELEASE_IDENTITY_V1)?;
    color.push_bytes(selection.release_identity().as_bytes())?;
    Ok(color)
}

fn presentation_target_color(
    target: PointPresentationTargetV1,
) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::PRESENTATION_TARGET);
    color.push_u8(match target.absence_release() {
        PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1 => {
            release_tag::POINT_ABSENCE_BYPASS_OWN_BACKDROP_V1
        }
    })?;
    Ok(color)
}

fn write_signal(color: &mut VertexColorV1, signal: ColorSignal) -> Result<(), ProgramCompileError> {
    let profile = match signal.output_profile() {
        crate::lcs_occurrence::OutputProfileId::Iec61966Srgb8D65V1 => {
            release_tag::IEC_SRGB8_D65_OUTPUT_PROFILE_V1
        }
    };
    color.push_u8(profile)?;
    color.push_srgb8(signal.srgb8())
}

fn source_color(source: Source) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::SOURCE);
    write_signal(&mut color, source.signal())?;
    Ok(color)
}

fn family_color(family: &FamilyDeclarationV2) -> Result<VertexColorV1, ProgramCompileError> {
    let semantic = family.semantic();
    let mut color = VertexColorV1::new(vertex_tag::FAMILY);
    color.push_u8(release_tag::FAMILY_SEMANTIC_RELEASE_VERTEX_V2)?;
    color.push_bytes(semantic.as_bytes())?;
    Ok(color)
}

fn candidate_color(candidate: TargetCandidateV1) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::CANDIDATE);
    let value = candidate.value();
    write_signal(&mut color, ColorSignal::from_srgb8(value.source()))?;
    color.push_u64(value.opacity_bits())?;
    Ok(color)
}

fn opacity_color(input: OpacityInput) -> Result<VertexColorV1, ProgramCompileError> {
    let admitted = crate::composition::AdmittedOpacityV1::new(input.value())
        .map_err(|_| ProgramCompileError::InternalInvariant)?;
    let mut color = VertexColorV1::new(vertex_tag::OPACITY);
    color.push_u64(admitted.bits())?;
    Ok(color)
}

fn write_context(
    color: &mut VertexColorV1,
    context: AppearanceContextId,
) -> Result<(), ProgramCompileError> {
    color.push_u8(match context.schema_release() {
        crate::lcs_occurrence::AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1 => {
            release_tag::CIECAM16_VIEWING_INPUTS_V1
        }
    })?;
    let frame = context.frame();
    color.push_u8(match frame.observer() {
        crate::lcs_occurrence::ObserverProfileId::Cie1931TwoDegreeV1 => {
            release_tag::CIE1931_TWO_DEGREE_OBSERVER_V1
        }
    })?;
    color.push_u8(match frame.reference_white() {
        crate::lcs_occurrence::ReferenceWhiteId::Iec61966D65ChromaticityV1 => {
            release_tag::IEC61966_D65_REFERENCE_WHITE_V1
        }
    })?;
    color.push_u8(match frame.scale() {
        crate::lcs_occurrence::TristimulusScale::RelativeY1 => release_tag::RELATIVE_Y1_SCALE_V1,
    })?;
    color.push_u8(match frame.release() {
        crate::lcs_occurrence::ColorimetricFrameReleaseId::XyzV1 => release_tag::XYZ_FRAME_V1,
        #[cfg(test)]
        crate::lcs_occurrence::ColorimetricFrameReleaseId::MutationSentinelV1 => {
            release_tag::MUTATION_SENTINEL_FRAME_V1
        }
    })?;
    color.push_u64(context.adapting_luminance_cd_m2().to_bits())?;
    color.push_u64(context.background_luminance_ratio().to_bits())?;
    color.push_u8(match context.surround_profile() {
        crate::lcs_occurrence::SurroundProfileId::AverageV1 => release_tag::SURROUND_AVERAGE_V1,
        crate::lcs_occurrence::SurroundProfileId::DimV1 => release_tag::SURROUND_DIM_V1,
        crate::lcs_occurrence::SurroundProfileId::DarkV1 => release_tag::SURROUND_DARK_V1,
    })?;
    Ok(())
}

fn occurrence_color(occurrence: Occurrence) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(vertex_tag::OCCURRENCE);
    color.push_u8(match occurrence.composition() {
        CompositionProfile::EncodedSrgb8SourceOverV1 => release_tag::ENCODED_SRGB8_SOURCE_OVER_V1,
    })?;
    write_context(&mut color, occurrence.context())?;
    Ok(color)
}

fn field_input_color(input: FieldInputDeclarationV1) -> VertexColorV1 {
    VertexColorV1::new(match input.kind() {
        crate::field_effect::FieldInputKindV1::PremultipliedRgba8V1 => {
            vertex_tag::FIELD_INPUT_PREMULTIPLIED_RGBA8
        }
        crate::field_effect::FieldInputKindV1::EncodedSrgb8AlphaV1 => {
            vertex_tag::FIELD_INPUT_ENCODED_SRGB8_ALPHA
        }
        crate::field_effect::FieldInputKindV1::OpaqueSrgb8V1 => {
            vertex_tag::FIELD_INPUT_OPAQUE_SRGB8
        }
    })
}

fn field_operator_color(
    operator: FieldOperatorDeclarationV1,
) -> Result<VertexColorV1, ProgramCompileError> {
    use crate::field_effect::{
        CarrierIntentV1, FieldExecutionProfileV1, FieldPrecisionV1, FieldQuantizationV1,
        FieldWorkingSpaceV1, GaussianEdgeModeV1, GaussianKernelProfileV1,
    };

    let mut color = VertexColorV1::new(match operator.execution() {
        FieldExecutionProfileV1::GaussianBlur { .. } => vertex_tag::FIELD_OPERATOR_GAUSSIAN_Q32,
        FieldExecutionProfileV1::PremultipliedSourceOver { .. } => {
            vertex_tag::FIELD_OPERATOR_SOURCE_OVER
        }
        FieldExecutionProfileV1::EncodedSrgb8ScreenOpaqueBackdrop { .. } => {
            vertex_tag::FIELD_OPERATOR_SCREEN
        }
        FieldExecutionProfileV1::PorterDuffLighter { .. } => vertex_tag::FIELD_OPERATOR_LIGHTER,
    });
    color.push_u8(match operator.working_space() {
        FieldWorkingSpaceV1::EncodedSrgb8PremultipliedV1 => {
            release_tag::FIELD_WORKING_ENCODED_SRGB8_PREMULTIPLIED_V1
        }
        FieldWorkingSpaceV1::LinearSrgbQ31PremultipliedV1 => {
            release_tag::FIELD_WORKING_LINEAR_SRGB_Q31_PREMULTIPLIED_V1
        }
    })?;
    color.push_u8(match operator.precision() {
        FieldPrecisionV1::FixedQ32V1 => release_tag::FIELD_PRECISION_FIXED_Q32_V1,
        FieldPrecisionV1::Binary32V1 => release_tag::FIELD_PRECISION_BINARY32_V1,
    })?;
    color.push_u8(match operator.quantization() {
        FieldQuantizationV1::RoundHalfUpSrgb8V1 => release_tag::FIELD_QUANTIZATION_HALF_UP_SRGB8_V1,
        FieldQuantizationV1::RoundTiesToEvenSrgb8V1 => {
            release_tag::FIELD_QUANTIZATION_TIES_EVEN_SRGB8_V1
        }
    })?;
    color.push_u8(match operator.carrier_intent() {
        CarrierIntentV1::Present => release_tag::FIELD_CARRIER_PRESENT_V1,
        CarrierIntentV1::Contributes => release_tag::FIELD_CARRIER_CONTRIBUTES_V1,
        CarrierIntentV1::SpatialVariation => release_tag::FIELD_CARRIER_SPATIAL_VARIATION_V1,
    })?;
    if let FieldExecutionProfileV1::GaussianBlur {
        kernel,
        css_radius_px,
        edge_mode,
        ..
    } = operator.execution()
    {
        color.push_u8(match kernel {
            GaussianKernelProfileV1::BinomialGaussianQ32V1 => {
                release_tag::FIELD_GAUSSIAN_BINOMIAL_Q32_V1
            }
        })?;
        color.push_u64(u64::from(css_radius_px))?;
        color.push_u8(match edge_mode {
            GaussianEdgeModeV1::ClampToEdgeV1 => release_tag::FIELD_GAUSSIAN_CLAMP_TO_EDGE_V1,
        })?;
    }
    Ok(color)
}

fn wcag_criterion_tag(criterion: Wcag22CriterionV1) -> u8 {
    match criterion {
        Wcag22CriterionV1::Sc143TextDefault => release_tag::WCAG22_SC_1_4_3_TEXT_DEFAULT,
        Wcag22CriterionV1::Sc143TextLargeScale => release_tag::WCAG22_SC_1_4_3_TEXT_LARGE_SCALE,
        Wcag22CriterionV1::Sc1411UiComponentOrState => {
            release_tag::WCAG22_SC_1_4_11_UI_COMPONENT_OR_STATE
        }
        Wcag22CriterionV1::Sc1411GraphicalObject => release_tag::WCAG22_SC_1_4_11_GRAPHICAL_OBJECT,
    }
}

fn constraint_color(
    mode_tag: u8,
    content: ProgramConstraintContentV1,
) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(mode_tag);
    match content {
        ProgramConstraintContentV1::ExactSrgb8 {
            identity,
            release,
            capability,
            expected,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::EXACT_SRGB8_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::ExactConstraintIdentityV1::FinalSrgb8IdentityV1 => {
                    release_tag::EXACT_SRGB8_IDENTITY_V1
                }
                #[cfg(test)]
                crate::constraints::ExactConstraintIdentityV1::MutationSentinelV1 => {
                    release_tag::EXACT_SRGB8_IDENTITY_MUTATION_SENTINEL_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::ExactIdentityReleaseV1::V1 => {
                    release_tag::EXACT_SRGB8_RELEASE_V1
                }
                #[cfg(test)]
                crate::constraints::ExactIdentityReleaseV1::MutationSentinelV1 => {
                    release_tag::EXACT_SRGB8_RELEASE_MUTATION_SENTINEL_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1 => {
                    release_tag::EXACT_SRGB8_CAPABILITY_V1
                }
                #[cfg(test)]
                crate::constraints::ExactIdentityCapabilityV1::MutationSentinelV1 => {
                    release_tag::EXACT_SRGB8_CAPABILITY_MUTATION_SENTINEL_V1
                }
            })?;
            color.push_srgb8(expected)?;
        }
        ProgramConstraintContentV1::Wcag22Srgb8 {
            identity,
            release,
            capability,
            criterion,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::WCAG22_SRGB8_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::Wcag22Srgb8EvaluatorIdentityV1 => {
                    release_tag::WCAG22_SRGB8_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::wcag22::Wcag22ProfileIdV1::Wcag22Srgb8ContrastV1 => {
                    release_tag::WCAG22_SRGB8_PROFILE_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::Wcag22Srgb8CapabilityV1 => {
                    release_tag::WCAG22_SRGB8_CAPABILITY_V1
                }
            })?;
            color.push_u8(wcag_criterion_tag(criterion))?;
        }
        ProgramConstraintContentV1::ExactSrgb8IntrinsicUnary {
            identity,
            release,
            capability,
            expected,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::EXACT_SRGB8_INTRINSIC_UNARY_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::ExactSrgb8IntrinsicUnaryIdentityV1::SourceEqualityV1 => {
                    release_tag::EXACT_SRGB8_INTRINSIC_UNARY_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::ExactSrgb8IntrinsicUnaryReleaseV1::V1 => {
                    release_tag::EXACT_SRGB8_INTRINSIC_UNARY_RELEASE_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::ExactSrgb8IntrinsicUnaryCapabilityV1::EncodedSrgb8PaintSourceV1 => {
                    release_tag::EXACT_SRGB8_INTRINSIC_UNARY_CAPABILITY_V1
                }
            })?;
            color.push_srgb8(expected)?;
        }
        ProgramConstraintContentV1::FamilyMembership {
            identity,
            release,
            capability,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::FAMILY_MEMBERSHIP_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::FamilyMembershipIdentityV1::ExactImageMembershipV1 => {
                    release_tag::FAMILY_MEMBERSHIP_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::FamilyMembershipReleaseV2::V2 => {
                    release_tag::FAMILY_MEMBERSHIP_RELEASE_V2
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::FamilyMembershipCapabilityV1::Iec61966Srgb8D65V1 => {
                    release_tag::FAMILY_MEMBERSHIP_CAPABILITY_V1
                }
            })?;
        }
        ProgramConstraintContentV1::ExactSrgb8Relation {
            identity,
            release,
            capability,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::EXACT_SRGB8_RELATION_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::ExactSrgb8RelationIdentityV1::HomogeneousEndpointEqualityV1 => {
                    release_tag::EXACT_SRGB8_RELATION_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::ExactSrgb8RelationReleaseV1::V1 => {
                    release_tag::EXACT_SRGB8_RELATION_RELEASE_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::ExactSrgb8RelationCapabilityV1::HomogeneousEncodedSrgb8PairV1 => {
                    release_tag::EXACT_SRGB8_RELATION_CAPABILITY_V1
                }
            })?;
        }
        ProgramConstraintContentV1::ExactSrgb8DistinctionRelation {
            identity,
            release,
            capability,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::EXACT_SRGB8_DISTINCTION_RELATION_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::ExactSrgb8DistinctionIdentityV1::HomogeneousEndpointInequalityV1 => {
                    release_tag::EXACT_SRGB8_DISTINCTION_RELATION_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::ExactSrgb8DistinctionReleaseV1::V1 => {
                    release_tag::EXACT_SRGB8_DISTINCTION_RELATION_RELEASE_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::ExactSrgb8DistinctionCapabilityV1::HomogeneousEncodedSrgb8PairV1 => {
                    release_tag::EXACT_SRGB8_DISTINCTION_RELATION_CAPABILITY_V1
                }
            })?;
        }
        ProgramConstraintContentV1::FamilyCategoryRelation {
            identity,
            release,
            capability,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::FAMILY_CATEGORY_RELATION_FAMILY_V1)?;
            color.push_u8(match identity {
                crate::constraints::FamilyCategoryRelationIdentityV1::HomogeneousEndpointCategoryMembershipV1 => {
                    release_tag::FAMILY_CATEGORY_RELATION_IDENTITY_V1
                }
            })?;
            color.push_u8(match release {
                crate::constraints::FamilyCategoryRelationReleaseV1::V1 => {
                    release_tag::FAMILY_CATEGORY_RELATION_RELEASE_V1
                }
            })?;
            color.push_u8(match capability {
                crate::constraints::FamilyCategoryRelationCapabilityV1::HomogeneousIec61966Srgb8D65PairV1 => {
                    release_tag::FAMILY_CATEGORY_RELATION_CAPABILITY_V1
                }
            })?;
        }
        #[cfg(test)]
        ProgramConstraintContentV1::ModeledLcsProbe {
            release,
            applicability: _,
            uncertainty: _,
        } => {
            color.push_u8(release_tag::MODELED_LCS_PROBE_FAMILY_V1)?;
            color.push_u8(match release.modeled_lcs_release() {
                crate::lcs_occurrence::ModeledLcsOccurrenceReleaseId::V1 => {
                    release_tag::MODELED_LCS_OCCURRENCE_V1
                }
                crate::lcs_occurrence::ModeledLcsOccurrenceReleaseId::MutationSentinelV1 => {
                    release_tag::MODELED_LCS_OCCURRENCE_MUTATION_SENTINEL_V1
                }
            })?;
            color.push_u8(match release.transform_release() {
                crate::lcs_occurrence::ColorimetricTransformReleaseId::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 => {
                    release_tag::IEC_SRGB8_TO_XYZ_D65_TRANSFORM_V1
                }
            })?;
        }
        #[cfg(test)]
        ProgramConstraintContentV1::FinalRecheckMutantExactSrgb8 {
            expected,
            applicability: _,
            uncertainty: _,
        } => {
            for tag in [0xFE_u8, 1, 1, 1] {
                color.push_u8(tag)?;
            }
            color.push_srgb8(expected)?;
        }
    }
    Ok(color)
}

fn declared_srgb8_clean_set_constraint_color(
    mode_tag: u8,
) -> Result<VertexColorV1, ProgramCompileError> {
    declared_srgb8_clean_set_constraint_color_for_release(
        mode_tag,
        crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2,
    )
}

#[cfg(test)]
fn clean_set_final_recheck_mutant_release_v1() -> [u8; 32] {
    let mut release = crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2;
    // Инъекция меняет один заранее объявленный бит: тест доказывает, что
    // final-recheck связывает новый release, а не случайно другой дайджест.
    release[0] ^= 1;
    release
}

fn declared_srgb8_clean_set_constraint_color_for_release(
    mode_tag: u8,
    release: [u8; 32],
) -> Result<VertexColorV1, ProgramCompileError> {
    let mut color = VertexColorV1::new(mode_tag);
    color.push_u8(release_tag::DECLARED_SRGB8_CLEAN_SET_FAMILY_V1)?;
    color.push_bytes(&release)?;
    Ok(color)
}

fn presentation_target_vertex(
    targets: &[(PointPresentationTargetV1, usize)],
    target: PointPresentationTargetV1,
) -> Result<usize, ProgramCompileError> {
    let key = (target.root(), target.occurrence());
    let index = targets
        .binary_search_by_key(&key, |(candidate, _)| {
            (candidate.root(), candidate.occurrence())
        })
        .map_err(|_| ProgramCompileError::InternalInvariant)?;
    let (candidate, vertex) = targets[index];
    if candidate != target {
        return Err(ProgramCompileError::InternalInvariant);
    }
    Ok(vertex)
}

fn field_value_vertex(
    inputs: &IdIndexV1<crate::field_effect::FieldInputPortIdV1>,
    operators: &IdIndexV1<crate::field_effect::FieldOperatorInstanceIdV1>,
    value: crate::field_effect::FieldValueRefV1,
) -> Result<usize, ProgramCompileError> {
    match value {
        crate::field_effect::FieldValueRefV1::Input(input) => inputs.get(input),
        crate::field_effect::FieldValueRefV1::Operator(operator) => operators.get(operator),
    }
}

struct ConstraintGraphBindingContextV1<'a, Evaluation> {
    evaluator: &'a Evaluation,
    presentation_targets: &'a [(PointPresentationTargetV1, usize)],
    targets: &'a IdIndexV1<TargetId>,
    families: &'a IdIndexV1<FamilyId>,
    occurrences: &'a IdIndexV1<OccurrenceId>,
    observation_group: usize,
}

fn add_constraint_graph_binding<Evaluation>(
    graph: &mut GraphBuilderV1,
    context: &ConstraintGraphBindingContextV1<'_, Evaluation>,
    mode_tag: u8,
    body: &ProgramConstraintBodyV1<ProgramConstraintInvocationOf<Evaluation>>,
) -> Result<(), ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
{
    let color = match body {
        ProgramConstraintBodyV1::VisibleUnary { invocation, .. } => {
            constraint_color(mode_tag, context.evaluator.constraint_content(*invocation))?
        }
        ProgramConstraintBodyV1::IntrinsicUnary { invocation, .. } => {
            constraint_color(mode_tag, invocation.content())?
        }
        ProgramConstraintBodyV1::IntrinsicRelation { invocation, .. }
        | ProgramConstraintBodyV1::VisibleRelation { invocation, .. } => {
            constraint_color(mode_tag, invocation.content())?
        }
        ProgramConstraintBodyV1::DeclaredSrgb8CleanSet { .. } => {
            declared_srgb8_clean_set_constraint_color(mode_tag)?
        }
        #[cfg(test)]
        ProgramConstraintBodyV1::DeclaredSrgb8CleanSetFinalRecheckMutant { .. } => {
            declared_srgb8_clean_set_constraint_color_for_release(
                mode_tag,
                clean_set_final_recheck_mutant_release_v1(),
            )?
        }
    };
    let vertex = graph.add_member(color)?;
    match body {
        ProgramConstraintBodyV1::VisibleUnary { occurrence, .. } => {
            graph.add_edge(
                vertex,
                context.occurrences.get(*occurrence)?,
                EdgeRoleV1::VisibleUnary,
            )?;
        }
        ProgramConstraintBodyV1::IntrinsicUnary { target, invocation } => {
            graph.add_edge(
                vertex,
                context.targets.get(*target)?,
                EdgeRoleV1::IntrinsicUnary,
            )?;
            if let CoreIntrinsicUnaryInvocationV1::FamilyMembership { family } = invocation {
                graph.add_edge(
                    vertex,
                    context.families.get(*family)?,
                    EdgeRoleV1::ConstraintFamily,
                )?;
            }
        }
        ProgramConstraintBodyV1::IntrinsicRelation {
            relation,
            invocation,
        } => {
            graph.add_edge(
                vertex,
                context.targets.get(relation.reference())?,
                EdgeRoleV1::IntrinsicReference,
            )?;
            for candidate in relation.candidates().iter().copied() {
                graph.add_edge(
                    vertex,
                    context.targets.get(candidate)?,
                    EdgeRoleV1::IntrinsicCandidate,
                )?;
            }
            if let CoreRelationInvocationV1::FamilyCategory { family } = invocation {
                graph.add_edge(
                    vertex,
                    context.families.get(*family)?,
                    EdgeRoleV1::ConstraintFamily,
                )?;
            }
        }
        ProgramConstraintBodyV1::VisibleRelation {
            relation,
            invocation,
        } => {
            graph.add_edge(
                vertex,
                context.occurrences.get(relation.reference())?,
                EdgeRoleV1::VisibleReference,
            )?;
            for candidate in relation.candidates().iter().copied() {
                graph.add_edge(
                    vertex,
                    context.occurrences.get(candidate)?,
                    EdgeRoleV1::VisibleCandidate,
                )?;
            }
            if let CoreRelationInvocationV1::FamilyCategory { family } = invocation {
                graph.add_edge(
                    vertex,
                    context.families.get(*family)?,
                    EdgeRoleV1::ConstraintFamily,
                )?;
            }
        }
        ProgramConstraintBodyV1::DeclaredSrgb8CleanSet { target } => {
            graph.add_edge(
                vertex,
                presentation_target_vertex(context.presentation_targets, *target)?,
                EdgeRoleV1::ConstraintPresentationTarget,
            )?;
        }
        #[cfg(test)]
        ProgramConstraintBodyV1::DeclaredSrgb8CleanSetFinalRecheckMutant { target } => {
            graph.add_edge(
                vertex,
                presentation_target_vertex(context.presentation_targets, *target)?,
                EdgeRoleV1::ConstraintPresentationTarget,
            )?;
        }
    }
    graph.add_edge(
        vertex,
        context.observation_group,
        EdgeRoleV1::ObservationGroup,
    )
}

fn build_graph<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<CanonicalGraphV1, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let mut graph = GraphBuilderV1::new(program_root_color()?)?;
    let mut sources = IdIndexV1::new();
    let mut targets = IdIndexV1::new();
    let mut families = IdIndexV1::new();
    let mut candidates = IdIndexV1::new();
    let mut opacities = IdIndexV1::new();
    let mut paints = IdIndexV1::new();
    let mut ports = IdIndexV1::new();
    let mut surfaces = IdIndexV1::new();
    let mut occurrences = IdIndexV1::new();
    let mut field_inputs = IdIndexV1::new();
    let mut field_operators = IdIndexV1::new();
    let mut presentation_roots = IdIndexV1::new();
    let mut presentation_targets = Vec::new();
    presentation_targets
        .try_reserve_exact(program.presentation_targets.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;

    for source in &program.sources {
        sources.insert(source.id(), graph.add_member(source_color(*source)?)?)?;
    }
    for target in &program.targets {
        let target_color = match target.intent() {
            TargetIntentV1::FixedSource(_) => VertexColorV1::new(vertex_tag::TARGET_FIXED),
            TargetIntentV1::Finite(_) => VertexColorV1::new(vertex_tag::TARGET_FINITE),
        };
        let target_vertex = graph.add_member(target_color)?;
        targets.insert(target.id(), target_vertex)?;
        if let TargetIntentV1::Finite(domain) = target.intent() {
            for candidate in domain.candidates() {
                let vertex = graph.add_member(candidate_color(*candidate)?)?;
                candidates.insert((target.id(), candidate.id()), vertex)?;
            }
        }
    }
    for family in &program.families {
        families.insert(family.id(), graph.add_member(family_color(family)?)?)?;
    }
    for opacity in &program.opacities {
        opacities.insert(opacity.id(), graph.add_member(opacity_color(*opacity)?)?)?;
    }
    for paint in &program.paints {
        let (id, tag) = match paint {
            Paint::Solid { id, .. } => (*id, vertex_tag::PAINT_SOLID),
            Paint::Opacity { id, .. } => (*id, vertex_tag::PAINT_OPACITY),
        };
        paints.insert(id, graph.add_member(VertexColorV1::new(tag))?)?;
    }

    let group = graph.add_member(VertexColorV1::new(vertex_tag::OBSERVATION_GROUP))?;
    for port in &program.observation_group.surface_input_ports {
        ports.insert(
            *port,
            graph.add_member(VertexColorV1::new(vertex_tag::SURFACE_INPUT_PORT))?,
        )?;
    }
    for surface in &program.surfaces {
        let (id, tag) = match surface {
            Surface::Input { id, .. } => (*id, vertex_tag::SURFACE_INPUT),
            Surface::FromOccurrence { id, .. } => (*id, vertex_tag::SURFACE_FROM_OCCURRENCE),
        };
        surfaces.insert(id, graph.add_member(VertexColorV1::new(tag))?)?;
    }
    for occurrence in &program.occurrences {
        occurrences.insert(
            occurrence.id(),
            graph.add_member(occurrence_color(*occurrence)?)?,
        )?;
    }
    for input in program.field.inputs() {
        field_inputs.insert(input.id(), graph.add_member(field_input_color(*input))?)?;
    }
    for operator in program.field.operators() {
        field_operators.insert(
            operator.id(),
            graph.add_member(field_operator_color(*operator)?)?,
        )?;
    }
    for root in &program.presentation_roots {
        presentation_roots.insert(
            root.id(),
            graph.add_member(VertexColorV1::new(vertex_tag::PRESENTATION_ROOT))?,
        )?;
    }
    // `prepare_program` вызывает этот закрытый этап вычисления identity только
    // после сортировки пар `(root, occurrence)` и отклонения дубликатов в
    // `compile_point_presentations`. Повторная дедупликация здесь скрыла бы
    // ошибку декларации и разделила правило admission между двумя источниками истины.
    for target in &program.presentation_targets {
        presentation_targets.push((
            *target,
            graph.add_member(presentation_target_color(*target)?)?,
        ));
    }

    sources.finish()?;
    targets.finish()?;
    families.finish()?;
    candidates.finish()?;
    opacities.finish()?;
    paints.finish()?;
    ports.finish()?;
    surfaces.finish()?;
    occurrences.finish()?;
    field_inputs.finish()?;
    field_operators.finish()?;
    presentation_roots.finish()?;

    for target in &program.targets {
        let target_vertex = targets.get(target.id())?;
        if let TargetIntentV1::FixedSource(source) = target.intent() {
            graph.add_edge(
                target_vertex,
                sources.get(*source)?,
                EdgeRoleV1::TargetSource,
            )?;
        }
        if let TargetIntentV1::Finite(domain) = target.intent() {
            for candidate in domain.candidates() {
                graph.add_edge(
                    target_vertex,
                    candidates.get((target.id(), candidate.id()))?,
                    EdgeRoleV1::TargetCandidate,
                )?;
            }
        }
    }
    for paint in &program.paints {
        match *paint {
            Paint::Solid { id, target } => graph.add_edge(
                paints.get(id)?,
                targets.get(target)?,
                EdgeRoleV1::SolidTarget,
            )?,
            Paint::Opacity {
                id,
                source,
                opacity,
            } => {
                graph.add_edge(
                    paints.get(id)?,
                    paints.get(source)?,
                    EdgeRoleV1::OpacitySourcePaint,
                )?;
                graph.add_edge(
                    paints.get(id)?,
                    opacities.get(opacity)?,
                    EdgeRoleV1::OpacityInput,
                )?;
            }
        }
    }
    for port in &program.observation_group.surface_input_ports {
        graph.add_edge(group, ports.get(*port)?, EdgeRoleV1::ObservationGroupPort)?;
    }
    for input in program.field.inputs() {
        graph.add_edge(
            group,
            field_inputs.get(input.id())?,
            EdgeRoleV1::FieldInputObservationGroup,
        )?;
    }
    for operator in program.field.operators() {
        use crate::field_effect::FieldExecutionProfileV1;

        let vertex = field_operators.get(operator.id())?;
        match operator.execution() {
            FieldExecutionProfileV1::GaussianBlur { source, .. } => graph.add_edge(
                vertex,
                field_value_vertex(&field_inputs, &field_operators, source)?,
                EdgeRoleV1::FieldGaussianSource,
            )?,
            FieldExecutionProfileV1::PremultipliedSourceOver {
                source,
                destination,
            } => {
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, source)?,
                    EdgeRoleV1::FieldSourceOverSource,
                )?;
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, destination)?,
                    EdgeRoleV1::FieldSourceOverDestination,
                )?;
            }
            FieldExecutionProfileV1::EncodedSrgb8ScreenOpaqueBackdrop { source, backdrop } => {
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, source)?,
                    EdgeRoleV1::FieldScreenSource,
                )?;
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, backdrop)?,
                    EdgeRoleV1::FieldScreenBackdrop,
                )?;
            }
            FieldExecutionProfileV1::PorterDuffLighter {
                source,
                destination,
            } => {
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, source)?,
                    EdgeRoleV1::FieldLighterSource,
                )?;
                graph.add_edge(
                    vertex,
                    field_value_vertex(&field_inputs, &field_operators, destination)?,
                    EdgeRoleV1::FieldLighterDestination,
                )?;
            }
        }
    }
    for surface in &program.surfaces {
        match *surface {
            Surface::Input { id, input } => graph.add_edge(
                surfaces.get(id)?,
                ports.get(input)?,
                EdgeRoleV1::InputSurfacePort,
            )?,
            Surface::FromOccurrence { id, occurrence } => graph.add_edge(
                surfaces.get(id)?,
                occurrences.get(occurrence)?,
                EdgeRoleV1::DerivedSurfaceOccurrence,
            )?,
        }
    }
    for occurrence in &program.occurrences {
        graph.add_edge(
            occurrences.get(occurrence.id())?,
            paints.get(occurrence.subject())?,
            EdgeRoleV1::OccurrenceSubjectPaint,
        )?;
        graph.add_edge(
            occurrences.get(occurrence.id())?,
            surfaces.get(occurrence.against())?,
            EdgeRoleV1::OccurrenceBackdropSurface,
        )?;
    }
    for root in &program.presentation_roots {
        graph.add_edge(
            presentation_roots.get(root.id())?,
            occurrences.get(root.terminal())?,
            EdgeRoleV1::PresentationRootTerminal,
        )?;
    }
    for (target, vertex) in &presentation_targets {
        graph.add_edge(
            *vertex,
            presentation_roots.get(target.root())?,
            EdgeRoleV1::PresentationTargetRoot,
        )?;
        graph.add_edge(
            *vertex,
            occurrences.get(target.occurrence())?,
            EdgeRoleV1::PresentationTargetOccurrence,
        )?;
    }

    let constraint_context = ConstraintGraphBindingContextV1 {
        evaluator: &program.evaluator,
        presentation_targets: &presentation_targets,
        targets: &targets,
        families: &families,
        occurrences: &occurrences,
        observation_group: group,
    };
    for constraint in &program.constraints.hard {
        add_constraint_graph_binding(
            &mut graph,
            &constraint_context,
            vertex_tag::CONSTRAINT_HARD,
            constraint.body(),
        )?;
    }
    for constraint in &program.constraints.report_only {
        add_constraint_graph_binding(
            &mut graph,
            &constraint_context,
            vertex_tag::CONSTRAINT_REPORT_ONLY,
            constraint.body(),
        )?;
    }
    for output in &program.outputs {
        let vertex = graph.add_member(VertexColorV1::new(vertex_tag::OUTPUT))?;
        graph.add_edge(vertex, paints.get(output.paint())?, EdgeRoleV1::OutputPaint)?;
    }
    for output in &program.field_outputs {
        let vertex = graph.add_member(VertexColorV1::new(vertex_tag::OUTPUT))?;
        graph.add_edge(
            vertex,
            field_operators.get(output.operator())?,
            EdgeRoleV1::FieldOutputOperator,
        )?;
    }

    if let Some(selection) = &program.joint_selection {
        let selection_vertex = graph.add_member(joint_selection_color(selection)?)?;
        for (state_index, state) in selection.order().states().iter().enumerate() {
            let state_index =
                u64::try_from(state_index).map_err(|_| ProgramCompileError::ResourceExhausted)?;
            let mut state_color = VertexColorV1::new(vertex_tag::JOINT_STATE);
            state_color.push_u64(state_index)?;
            let state_vertex = graph.add_member(state_color)?;
            graph.add_edge(selection_vertex, state_vertex, EdgeRoleV1::SelectionState)?;
            for choice in state.choices() {
                let choice_vertex =
                    graph.add_member(VertexColorV1::new(vertex_tag::JOINT_CHOICE))?;
                graph.add_edge(state_vertex, choice_vertex, EdgeRoleV1::StateChoice)?;
                graph.add_edge(
                    choice_vertex,
                    targets.get(choice.target())?,
                    EdgeRoleV1::ChoiceTarget,
                )?;
                graph.add_edge(
                    choice_vertex,
                    candidates.get((choice.target(), choice.candidate()))?,
                    EdgeRoleV1::ChoiceCandidate,
                )?;
            }
        }
    }

    graph.finish()
}

#[cfg(test)]
pub(crate) fn graph_schema_for_test<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<(Vec<u8>, Vec<u8>), ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let graph = build_graph(program)?;
    let mut vertex_tags = Vec::new();
    vertex_tags
        .try_reserve_exact(graph.colors.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    vertex_tags.extend(graph.colors.iter().map(VertexColorV1::tag));
    ordering::sort(&mut vertex_tags);
    vertex_tags.dedup();

    let mut edge_roles = Vec::new();
    edge_roles
        .try_reserve_exact(graph.edge_count)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for adjacency in &graph.adjacency {
        edge_roles.extend(
            adjacency
                .iter()
                .filter(|arc| arc.direction == 0)
                .map(|arc| arc.role as u8),
        );
    }
    ordering::sort(&mut edge_roles);
    edge_roles.dedup();
    Ok((vertex_tags, edge_roles))
}

#[cfg(test)]
pub(crate) const fn edge_role_count_for_test() -> usize {
    EDGE_ROLES_V1.len()
}

struct PartitionV1 {
    cells: Vec<Vec<usize>>,
}

/// Клетки однородны по цвету вершины и полным типизированным спискам соседних клеток.
/// Конструктор принадлежит завершённому уточнению разбиения.
struct EquitablePartitionV1(PartitionV1);

impl PartitionV1 {
    fn initial(graph: &CanonicalGraphV1) -> Result<Self, ProgramCompileError> {
        let mut order = Vec::new();
        order
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        order.extend(0..graph.colors.len());
        ordering::sort_by(&mut order, |left, right| {
            graph.colors[*left]
                .cmp(&graph.colors[*right])
                .then_with(|| left.cmp(right))
        });

        let mut cells = Vec::new();
        cells
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        let mut start = 0;
        while start < order.len() {
            let mut end = start + 1;
            while end < order.len() && graph.colors[order[start]] == graph.colors[order[end]] {
                end += 1;
            }
            cells.push(copy_vertices(&order[start..end])?);
            start = end;
        }
        Ok(Self { cells })
    }

    fn is_discrete(&self) -> bool {
        self.cells.iter().all(|cell| cell.len() == 1)
    }

    fn first_non_singleton(&self) -> Option<usize> {
        self.cells.iter().position(|cell| cell.len() > 1)
    }
}

fn copy_vertices(source: &[usize]) -> Result<Vec<usize>, ProgramCompileError> {
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(source.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    copied.extend_from_slice(source);
    Ok(copied)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RefinementAtomV1 {
    direction: u8,
    role: EdgeRoleV1,
    neighbour_cell: usize,
}

struct RefinementRecordV1 {
    vertex: usize,
    signature: core::ops::Range<usize>,
}

#[cfg(test)]
#[derive(Default)]
struct RefinementTraceV1 {
    rounds: Vec<Vec<Vec<usize>>>,
    skipped_cells: usize,
    signature_vertices: usize,
    signature_arcs: usize,
}

fn partition_cells(
    partition: &PartitionV1,
    vertex_count: usize,
) -> Result<Vec<usize>, ProgramCompileError> {
    let mut cell_of = Vec::new();
    cell_of
        .try_reserve_exact(vertex_count)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    cell_of.resize(vertex_count, usize::MAX);
    for (cell_index, cell) in partition.cells.iter().enumerate() {
        for &vertex in cell {
            let slot = cell_of
                .get_mut(vertex)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            if *slot != usize::MAX {
                return Err(ProgramCompileError::InternalInvariant);
            }
            *slot = cell_index;
        }
    }
    if cell_of.contains(&usize::MAX) {
        return Err(ProgramCompileError::InternalInvariant);
    }
    Ok(cell_of)
}

fn refine_partition(
    graph: &CanonicalGraphV1,
    partition: PartitionV1,
) -> Result<EquitablePartitionV1, ProgramCompileError> {
    refine_partition_impl(
        graph,
        partition,
        #[cfg(test)]
        None,
    )
}

fn refine_partition_impl(
    graph: &CanonicalGraphV1,
    partition: PartitionV1,
    #[cfg(test)] trace: Option<&mut RefinementTraceV1>,
) -> Result<EquitablePartitionV1, ProgramCompileError> {
    refinement::refine(
        graph,
        partition,
        #[cfg(test)]
        trace,
    )
}

fn individualize(
    partition: &PartitionV1,
    cell_index: usize,
    vertex: usize,
) -> Result<PartitionV1, ProgramCompileError> {
    let selected = partition
        .cells
        .get(cell_index)
        .ok_or(ProgramCompileError::InternalInvariant)?;
    if selected.len() < 2 || !selected.contains(&vertex) {
        return Err(ProgramCompileError::InternalInvariant);
    }

    let mut cells = Vec::new();
    cells
        .try_reserve_exact(
            partition
                .cells
                .len()
                .checked_add(1)
                .ok_or(ProgramCompileError::ResourceExhausted)?,
        )
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (index, cell) in partition.cells.iter().enumerate() {
        if index != cell_index {
            cells.push(copy_vertices(cell)?);
            continue;
        }
        let mut singleton = Vec::new();
        singleton
            .try_reserve_exact(1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        singleton.push(vertex);
        cells.push(singleton);

        let mut remainder = Vec::new();
        remainder
            .try_reserve_exact(cell.len() - 1)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        remainder.extend(
            cell.iter()
                .copied()
                .filter(|candidate| *candidate != vertex),
        );
        cells.push(remainder);
    }
    Ok(PartitionV1 { cells })
}

struct SearchFrameV1 {
    partition: EquitablePartitionV1,
    branch_cell: Option<usize>,
    candidates: Vec<usize>,
    explored_candidates: Vec<usize>,
    component_twins: Option<ComponentTwinIndexV1>,
    next_candidate: usize,
    leaf_pending: bool,
}

impl SearchFrameV1 {
    fn new(
        partition: EquitablePartitionV1,
        component_twins: Option<ComponentTwinIndexV1>,
    ) -> Result<Self, ProgramCompileError> {
        let branch_cell = partition.0.first_non_singleton();
        let candidates = match branch_cell {
            Some(index) => copy_vertices(&partition.0.cells[index])?,
            None => Vec::new(),
        };
        Ok(Self {
            leaf_pending: branch_cell.is_none(),
            partition,
            branch_cell,
            candidates,
            explored_candidates: Vec::new(),
            component_twins,
            next_candidate: 0,
        })
    }
}

fn push_u64_bytes(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn usize_as_u64(value: usize) -> Result<u64, ProgramCompileError> {
    u64::try_from(value).map_err(|_| ProgramCompileError::ResourceExhausted)
}

struct SerializedLeafV1 {
    preimage: Vec<u8>,
    order: Vec<usize>,
}

fn serialize_leaf(
    graph: &CanonicalGraphV1,
    partition: &PartitionV1,
) -> Result<SerializedLeafV1, ProgramCompileError> {
    if !partition.is_discrete() || partition.cells.len() != graph.colors.len() {
        return Err(ProgramCompileError::InternalInvariant);
    }
    let mut order = Vec::new();
    order
        .try_reserve_exact(graph.colors.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    let mut label_of = Vec::new();
    label_of
        .try_reserve_exact(graph.colors.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    label_of.resize(graph.colors.len(), usize::MAX);
    for (label, cell) in partition.cells.iter().enumerate() {
        let [vertex] = cell.as_slice() else {
            return Err(ProgramCompileError::InternalInvariant);
        };
        label_of[*vertex] = label;
        order.push(*vertex);
    }

    let edge_bytes = graph
        .edge_count
        .checked_mul(9)
        .ok_or(ProgramCompileError::ResourceExhausted)?;
    let color_bytes = graph.colors.iter().try_fold(0_usize, |total, color| {
        total
            .checked_add(16)
            .and_then(|value| value.checked_add(color.as_slice().len()))
            .ok_or(ProgramCompileError::ResourceExhausted)
    })?;
    let capacity = DOMAIN_V9
        .len()
        .checked_add(16)
        .and_then(|value| value.checked_add(color_bytes))
        .and_then(|value| value.checked_add(edge_bytes))
        .ok_or(ProgramCompileError::ResourceExhausted)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(capacity)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    output.extend_from_slice(DOMAIN_V9);
    push_u64_bytes(&mut output, usize_as_u64(graph.colors.len())?);
    push_u64_bytes(&mut output, usize_as_u64(graph.edge_count)?);

    let mut outgoing = Vec::new();
    for cell in &partition.cells {
        let vertex = cell[0];
        let color = graph.colors[vertex];
        push_u64_bytes(&mut output, u64::from(color.len()));
        output.extend_from_slice(color.as_slice());

        let outgoing_count = graph.adjacency[vertex]
            .iter()
            .filter(|arc| arc.direction == 0)
            .count();
        push_u64_bytes(&mut output, usize_as_u64(outgoing_count)?);
        outgoing.clear();
        outgoing
            .try_reserve(outgoing_count)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        outgoing.extend(
            graph.adjacency[vertex]
                .iter()
                .filter(|arc| arc.direction == 0)
                .map(|arc| (arc.role, label_of[arc.neighbour])),
        );
        ordering::sort(&mut outgoing);
        for &(role, target) in &outgoing {
            output.push(role as u8);
            push_u64_bytes(&mut output, usize_as_u64(target)?);
        }
    }
    Ok(SerializedLeafV1 {
        preimage: output,
        order,
    })
}

fn equal_leaf_automorphism(
    canonical_order: &[usize],
    equal_order: &[usize],
) -> Result<Vec<usize>, ProgramCompileError> {
    if canonical_order.len() != equal_order.len() {
        return Err(ProgramCompileError::InternalInvariant);
    }
    let mut permutation = Vec::new();
    permutation
        .try_reserve_exact(canonical_order.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    permutation.resize(canonical_order.len(), usize::MAX);
    for (&from, &to) in canonical_order.iter().zip(equal_order) {
        let slot = permutation
            .get_mut(from)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        if *slot != usize::MAX || to >= canonical_order.len() {
            return Err(ProgramCompileError::InternalInvariant);
        }
        *slot = to;
    }
    if permutation.contains(&usize::MAX) {
        return Err(ProgramCompileError::InternalInvariant);
    }
    Ok(permutation)
}

fn automorphism_preserves_partition(
    permutation: &[usize],
    cell_of: &[usize],
) -> Result<bool, ProgramCompileError> {
    if permutation.len() != cell_of.len() {
        return Err(ProgramCompileError::InternalInvariant);
    }
    for (vertex, &image) in permutation.iter().enumerate() {
        let image_cell = cell_of
            .get(image)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        if cell_of[vertex] != *image_cell {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
thread_local! {
    static EXPLORED_ORBIT_BUILDS: core::cell::Cell<usize> =
        const { core::cell::Cell::new(0) };
}

/// Проверяет, лежит ли `candidate` в орбите уже исследованной ветви при
/// автоморфизмах, стабилизирующих текущее упорядоченное разбиение. Отсечение
/// точное: отображение сохраняется лишь после совпадения полных сериализаций
/// листьев, доказывающего автоморфизм графа.
fn candidate_is_in_explored_orbit(
    partition: &PartitionV1,
    explored: &[usize],
    candidate: usize,
    automorphisms: &[Vec<usize>],
    orbit: &mut Option<Vec<bool>>,
) -> Result<bool, ProgramCompileError> {
    if explored.is_empty() || automorphisms.is_empty() {
        return Ok(false);
    }
    let vertex_count = automorphisms[0].len();
    if candidate >= vertex_count {
        return Err(ProgramCompileError::InternalInvariant);
    }
    if let Some(seen) = orbit.as_deref() {
        if seen.is_empty() {
            return Ok(false);
        }
        return seen
            .get(candidate)
            .copied()
            .ok_or(ProgramCompileError::InternalInvariant);
    }

    #[cfg(test)]
    EXPLORED_ORBIT_BUILDS.with(|builds| builds.set(builds.get() + 1));

    let cell_of = partition_cells(partition, vertex_count)?;
    let mut stabilizers = Vec::new();
    stabilizers
        .try_reserve_exact(automorphisms.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (index, permutation) in automorphisms.iter().enumerate() {
        if automorphism_preserves_partition(permutation, &cell_of)? {
            stabilizers.push(index);
        }
    }
    if stabilizers.is_empty() {
        // Пустой bitmap запоминает отсутствие подходящих стабилизаторов.
        *orbit = Some(Vec::new());
        return Ok(false);
    }

    let mut seen = Vec::new();
    seen.try_reserve_exact(vertex_count)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    seen.resize(vertex_count, false);
    let mut queue = Vec::new();
    queue
        .try_reserve_exact(vertex_count)
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for &representative in explored {
        let slot = seen
            .get_mut(representative)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        if !*slot {
            *slot = true;
            queue.push(representative);
        }
    }
    let mut cursor = 0;
    while cursor < queue.len() {
        let vertex = queue[cursor];
        cursor += 1;
        for &index in &stabilizers {
            let image = automorphisms[index][vertex];
            let slot = seen
                .get_mut(image)
                .ok_or(ProgramCompileError::InternalInvariant)?;
            if !*slot {
                *slot = true;
                queue.push(image);
            }
        }
    }
    let contains = seen[candidate];
    *orbit = Some(seen);
    Ok(contains)
}

/// Для каждой вершины хранит клетку, если её компонента после удаления
/// одноэлементных клеток содержит не более одной вершины каждой клетки.
/// Индекс принадлежит одному неизменяемому equitable-разбиению search frame.
struct ComponentTwinIndexV1 {
    unique_cell_of: Vec<usize>,
}

impl ComponentTwinIndexV1 {
    fn new(
        graph: &CanonicalGraphV1,
        equitable: &EquitablePartitionV1,
    ) -> Result<Self, ProgramCompileError> {
        let partition = &equitable.0;
        let mut cell_of = partition_cells(partition, graph.colors.len())?;
        let mut state = Vec::new();
        state
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        state.resize(graph.colors.len(), 0_u8);
        let mut seen_cell = Vec::new();
        seen_cell
            .try_reserve_exact(partition.cells.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        seen_cell.resize(partition.cells.len(), usize::MAX);
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;

        for root in 0..graph.colors.len() {
            if state[root] != 0 || partition.cells[cell_of[root]].len() == 1 {
                continue;
            }
            pending.clear();
            pending.push(root);
            state[root] = 1;
            let mut unique = true;
            let mut cursor = 0;
            while cursor < pending.len() {
                let vertex = pending[cursor];
                cursor += 1;
                let cell = cell_of[vertex];
                if seen_cell[cell] == root {
                    unique = false;
                }
                seen_cell[cell] = root;
                for arc in &graph.adjacency[vertex] {
                    let neighbour = arc.neighbour;
                    if state[neighbour] == 0 && partition.cells[cell_of[neighbour]].len() != 1 {
                        state[neighbour] = 1;
                        pending.push(neighbour);
                    }
                }
            }
            if unique {
                for &vertex in &pending {
                    state[vertex] = 2;
                }
            }
        }
        for (cell, state) in cell_of.iter_mut().zip(state) {
            if state != 2 {
                *cell = usize::MAX;
            }
        }
        Ok(Self {
            unique_cell_of: cell_of,
        })
    }

    fn interchangeable(&self, explored: &[usize], candidate: usize) -> bool {
        let cell = self.unique_cell_of[candidate];
        cell != usize::MAX
            && explored.iter().any(|&representative| {
                candidate != representative && self.unique_cell_of[representative] == cell
            })
    }
}

/// В устойчивом разбиении две компоненты с единственной вершиной каждой клетки
/// совпадают по направлениям, ролям и кратностям рёбер. Соответствие по клеткам
/// задаёт их точную транспозицию; одноэлементные клетки фиксируют внешнюю границу.
/// Свойство вычисляется один раз для всего frame, а не заново для каждой ветви.
fn candidate_is_component_twin(
    graph: &CanonicalGraphV1,
    equitable: &EquitablePartitionV1,
    explored: &[usize],
    candidate: usize,
    component_twins: &mut Option<ComponentTwinIndexV1>,
) -> Result<bool, ProgramCompileError> {
    if explored.is_empty() {
        return Ok(false);
    }
    let candidate_arcs = &graph.adjacency[candidate];
    if explored.iter().any(|&vertex| {
        graph.colors[vertex] == graph.colors[candidate]
            && graph.adjacency[vertex] == *candidate_arcs
    }) {
        return Ok(true);
    }
    if component_twins.is_none() {
        *component_twins = Some(ComponentTwinIndexV1::new(graph, equitable)?);
    }
    Ok(component_twins
        .as_ref()
        .ok_or(ProgramCompileError::InternalInvariant)?
        .interchangeable(explored, candidate))
}

/// Удаляет всю глубину выбора взаимозаменяемых компонент.
/// Применимость требует unique-компонент для КАЖДОЙ вершины первой ветвящейся
/// клетки: совпадения одной пары недостаточно, например у циклов длины 3 и 6.
///
/// Individualize ставит выбранную вершину перед остатком её клетки. Для
/// соответствующих вершин двух unique-компонент списки дуг имеют одинаковые
/// длины, роли и кратности. Индексы соседей выбранной компоненты после split
/// не больше соответствующих индексов остатка; при соседнем split хотя бы один
/// строго меньше. Поэтому её отсортированная signature строго меньше, и refine
/// ставит её вершину перед остатком. По связности это распространяется на всю
/// компоненту. Повтор выбора даёт singleton-клетки в едином порядке компонент.
/// Любой такой порядок связан точным автоморфизмом и сохраняет байты V9.
fn factor_component_twins(
    graph: &CanonicalGraphV1,
    mut equitable: EquitablePartitionV1,
    remaining_branch_expansions: &mut Option<usize>,
    bulk_groups: &mut usize,
) -> Result<(EquitablePartitionV1, Option<ComponentTwinIndexV1>), ProgramCompileError> {
    loop {
        let Some(branch_cell) = equitable.0.first_non_singleton() else {
            return Ok((equitable, None));
        };
        let twins = ComponentTwinIndexV1::new(graph, &equitable)?;
        if equitable.0.cells[branch_cell]
            .iter()
            .any(|&vertex| twins.unique_cell_of[vertex] != branch_cell)
        {
            return Ok((equitable, Some(twins)));
        }
        let roots = copy_vertices(&equitable.0.cells[branch_cell])?;
        let component_count = roots.len();
        if let Some(remaining) = remaining_branch_expansions {
            // Fault injection считает логические individualize, в том числе
            // выполненные пакетом; при исчерпании не возвращается часть прообраза.
            *remaining = remaining
                .checked_sub(component_count - 1)
                .ok_or(ProgramCompileError::ResourceExhausted)?;
        }

        let mut component_of = Vec::new();
        component_of
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        component_of.resize(graph.colors.len(), usize::MAX);
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for (ordinal, &root) in roots.iter().enumerate() {
            if component_of[root] != usize::MAX {
                return Err(ProgramCompileError::InternalInvariant);
            }
            pending.clear();
            pending.push(root);
            component_of[root] = ordinal;
            let mut cursor = 0;
            while cursor < pending.len() {
                let vertex = pending[cursor];
                cursor += 1;
                for arc in &graph.adjacency[vertex] {
                    let neighbour = arc.neighbour;
                    if twins.unique_cell_of[neighbour] == usize::MAX {
                        continue;
                    }
                    let previous = component_of[neighbour];
                    if previous == usize::MAX {
                        component_of[neighbour] = ordinal;
                        pending.push(neighbour);
                    } else if previous != ordinal {
                        return Err(ProgramCompileError::InternalInvariant);
                    }
                }
            }
        }

        let mut cells = Vec::new();
        cells
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        let mut ordered = Vec::new();
        ordered
            .try_reserve_exact(component_count)
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        ordered.resize(component_count, usize::MAX);
        for cell in equitable.0.cells {
            if cell
                .iter()
                .all(|&vertex| component_of[vertex] == usize::MAX)
            {
                cells.push(cell);
                continue;
            }
            if cell.len() != component_count {
                return Err(ProgramCompileError::InternalInvariant);
            }
            ordered.fill(usize::MAX);
            for vertex in cell {
                let slot = ordered
                    .get_mut(component_of[vertex])
                    .ok_or(ProgramCompileError::InternalInvariant)?;
                if *slot != usize::MAX {
                    return Err(ProgramCompileError::InternalInvariant);
                }
                *slot = vertex;
            }
            for &vertex in &ordered {
                if vertex == usize::MAX {
                    return Err(ProgramCompileError::InternalInvariant);
                }
                cells.push(copy_vertices(&[vertex])?);
            }
        }
        // У остальных non-singleton клеток нет дуг к этим компонентам.
        // Изменение индексов сохраняет их порядок и не требует нового refine.
        equitable = EquitablePartitionV1(PartitionV1 { cells });
        *bulk_groups = bulk_groups.saturating_add(1);
    }
}

fn canonical_search_impl(
    graph: &CanonicalGraphV1,
    mut remaining_branch_expansions: Option<usize>,
) -> Result<(Vec<u8>, usize, usize), ProgramCompileError> {
    let initial = refine_partition(graph, PartitionV1::initial(graph)?)?;
    let mut bulk_groups = 0;
    let (initial, component_twins) = factor_component_twins(
        graph,
        initial,
        &mut remaining_branch_expansions,
        &mut bulk_groups,
    )?;
    let mut stack = Vec::new();
    stack
        .try_reserve_exact(graph.colors.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    stack.push(SearchFrameV1::new(initial, component_twins)?);
    let mut best: Option<SerializedLeafV1> = None;
    let mut automorphisms: Vec<Vec<usize>> = Vec::new();
    let mut leaf_count = 0_usize;

    while !stack.is_empty() {
        let leaf = stack
            .last()
            .map(|frame| frame.leaf_pending)
            .ok_or(ProgramCompileError::InternalInvariant)?;
        if leaf {
            let candidate = {
                let frame = stack
                    .last_mut()
                    .ok_or(ProgramCompileError::InternalInvariant)?;
                frame.leaf_pending = false;
                serialize_leaf(graph, &frame.partition.0)?
            };
            stack.pop();
            // Диагностический счётчик не участвует в admission: насыщение не
            // влияет на выбранный прообраз даже у недостижимо большого дерева.
            leaf_count = leaf_count.saturating_add(1);
            match &best {
                None => best = Some(candidate),
                Some(current) if candidate.preimage < current.preimage => best = Some(candidate),
                Some(current) if candidate.preimage == current.preimage => {
                    let permutation = equal_leaf_automorphism(&current.order, &candidate.order)?;
                    // Отсечение не требуется для корректности. Храним не более
                    // V доказанных автоморфизмов: при длине V это ограничивает
                    // память O(V²), а остальные ветви исследуются полностью.
                    if permutation.iter().enumerate().any(|(from, to)| from != *to)
                        && automorphisms.len() < graph.colors.len()
                        && !automorphisms.contains(&permutation)
                    {
                        automorphisms
                            .try_reserve(1)
                            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                        automorphisms.push(permutation);
                    }
                }
                Some(_) => {}
            }
            continue;
        }

        let child = {
            let frame = stack
                .last_mut()
                .ok_or(ProgramCompileError::InternalInvariant)?;
            let mut selected = None;
            // Разбиение, представители и автоморфизмы неизменны до выхода
            // из этого прохода. После дочерней ветви орбита строится заново.
            let mut explored_orbit = None;
            while frame.next_candidate < frame.candidates.len() {
                let candidate = frame.candidates[frame.next_candidate];
                frame.next_candidate += 1;
                if candidate_is_component_twin(
                    graph,
                    &frame.partition,
                    &frame.explored_candidates,
                    candidate,
                    &mut frame.component_twins,
                )? || candidate_is_in_explored_orbit(
                    &frame.partition.0,
                    &frame.explored_candidates,
                    candidate,
                    &automorphisms,
                    &mut explored_orbit,
                )? {
                    continue;
                }
                frame
                    .explored_candidates
                    .try_reserve(1)
                    .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                frame.explored_candidates.push(candidate);
                selected = Some(candidate);
                break;
            }
            drop(explored_orbit);
            match selected {
                Some(candidate) => {
                    if let Some(remaining) = &mut remaining_branch_expansions {
                        *remaining = remaining
                            .checked_sub(1)
                            .ok_or(ProgramCompileError::ResourceExhausted)?;
                    }
                    let cell = frame
                        .branch_cell
                        .ok_or(ProgramCompileError::InternalInvariant)?;
                    let partition = refine_partition(
                        graph,
                        individualize(&frame.partition.0, cell, candidate)?,
                    )?;
                    Some(factor_component_twins(
                        graph,
                        partition,
                        &mut remaining_branch_expansions,
                        &mut bulk_groups,
                    )?)
                }
                None => None,
            }
        };
        match child {
            Some((partition, component_twins)) => {
                stack
                    .try_reserve(1)
                    .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                stack.push(SearchFrameV1::new(partition, component_twins)?);
            }
            None => {
                stack.pop();
            }
        }
    }
    let best = best.ok_or(ProgramCompileError::InternalInvariant)?;
    Ok((best.preimage, leaf_count, bulk_groups))
}

fn canonical_search(graph: &CanonicalGraphV1) -> Result<(Vec<u8>, usize), ProgramCompileError> {
    // Число шагов поиска не инвариантно к изоморфизму: после opaque-
    // переименования автоморфизм может обнаружиться другой ветвью. Поэтому
    // динамический лимит сделал бы допуск зависимым от client-owned ID. Точный
    // поиск возвращает только полный прообраз либо типизированный отказ.
    canonical_search_impl(graph, None).map(|(preimage, leaves, _)| (preimage, leaves))
}

#[cfg(test)]
fn canonical_search_with_test_fuel(
    graph: &CanonicalGraphV1,
    test_fuel: usize,
) -> Result<(Vec<u8>, usize), ProgramCompileError> {
    // Только тестовая инъекция отказа для проверки атомарности. Это не политика
    // допуска: история поиска не инвариантна к alpha-переименованию.
    canonical_search_impl(graph, Some(test_fuel)).map(|(preimage, leaves, _)| (preimage, leaves))
}

fn canonical_preimage(graph: &CanonicalGraphV1) -> Result<Vec<u8>, ProgramCompileError> {
    canonical_search(graph).map(|(preimage, _)| preimage)
}

pub(super) fn compile_program_content_identity_v9<Evaluation>(
    program: &Program<Evaluation>,
) -> Result<ProgramContentIdentityV9, ProgramCompileError>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let graph = build_graph(program)?;
    let preimage = canonical_preimage(&graph)?;
    let digest = crate::sha256::digest(&preimage);
    Ok(ProgramContentIdentityV9(*digest.as_bytes()))
}

mod ordering {
    //! Allocation-free heapsort for cold Program identity construction.
    //! Keep the binary small without changing runtime or candidate-order sorting.
    use core::cmp::Ordering;

    pub(crate) fn sort<T: Ord>(values: &mut [T]) {
        sort_by(values, Ord::cmp);
    }

    pub(crate) fn sort_by_key<T, K: Ord>(values: &mut [T], mut key: impl FnMut(&T) -> K) {
        sort_by(values, |left, right| key(left).cmp(&key(right)));
    }

    pub(crate) fn sort_by<T>(values: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering) {
        sort_indices(values.len(), &mut |left, right, swap| {
            if swap {
                values.swap(left, right);
                false
            } else {
                compare(&values[left], &values[right]).is_lt()
            }
        });
    }

    // Один цикл сортировки обслуживает все типы записей холодного компилятора.
    // Передача операций по индексам сохраняет владение элементами и обходится без аллокаций.
    #[inline(never)]
    fn sort_indices(n: usize, operation: &mut dyn FnMut(usize, usize, bool) -> bool) {
        fn sift(
            mut root: usize,
            end: usize,
            operation: &mut dyn FnMut(usize, usize, bool) -> bool,
        ) {
            while root < end / 2 {
                let mut child = root * 2 + 1;
                if child + 1 < end && operation(child, child + 1, false) {
                    child += 1;
                }
                if !operation(root, child, false) {
                    break;
                }
                operation(root, child, true);
                root = child;
            }
        }
        for root in (0..n / 2).rev() {
            sift(root, n, operation);
        }
        for end in (1..n).rev() {
            operation(0, end, true);
            sift(0, end, operation);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::cell::Cell;
        use std::rc::Rc;

        #[test]
        fn all_small_multisets_match_the_standard_library() {
            for length in 0..=8 {
                for mut encoded in 0..3usize.pow(length) {
                    let mut value = Vec::new();
                    for _ in 0..length {
                        value.push(encoded % 3);
                        encoded /= 3;
                    }
                    let mut expected = value.clone();
                    expected.sort_unstable();
                    sort(&mut value);
                    assert_eq!(value, expected);
                    sort_by(&mut value, |a, b| b.cmp(a));
                    expected.reverse();
                    assert_eq!(value, expected);
                }
            }
        }

        #[test]
        fn array_shapes_have_bounded_comparisons_and_no_allocator_events() {
            for shape in 0..5 {
                let mut values = [0u32; 4096];
                for (i, value) in values.iter_mut().enumerate() {
                    *value = match shape {
                        0 => i as u32,
                        1 => 4095 - i as u32,
                        2 => (i.min(4095 - i)) as u32,
                        3 => 7,
                        _ => (i as u32).wrapping_mul(2_654_435_761),
                    };
                }
                let mut expected = values;
                expected.sort_unstable();
                let mut comparisons = 0usize;
                let (_, events) = crate::test_support::measured_allocator_events(|| {
                    sort_by(&mut values, |a, b| {
                        comparisons += 1;
                        a.cmp(b)
                    });
                });
                assert_eq!(values, expected);
                assert_eq!(events.alloc + events.realloc + events.dealloc, 0);
                assert!(
                    comparisons <= 4 * values.len() * 12,
                    "comparison bound {comparisons}"
                );
            }
        }

        #[test]
        fn projected_keys_keep_owned_records_and_their_complete_payload() {
            let mut records = vec![(2, "b"), (1, "a"), (2, "c"), (0, "z")];
            sort_by_key(&mut records, |row| (row.0, row.1));
            assert_eq!(records, vec![(0, "z"), (1, "a"), (2, "b"), (2, "c")]);
            let mut empty: [(); 0] = [];
            sort(&mut empty);
            let mut zero_sized = [(); 257];
            sort(&mut zero_sized);
        }

        #[test]
        fn comparator_failure_never_drops_or_duplicates_an_element() {
            #[derive(Debug)]
            struct Owned {
                id: usize,
                drops: Rc<Cell<usize>>,
            }
            impl Drop for Owned {
                fn drop(&mut self) {
                    self.drops.set(self.drops.get() + 1);
                }
            }
            for stop in 0..100 {
                let drops = Rc::new(Cell::new(0));
                let mut records: Vec<_> = (0..17)
                    .rev()
                    .map(|id| Owned {
                        id,
                        drops: Rc::clone(&drops),
                    })
                    .collect();
                let mut calls = 0;
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    sort_by(&mut records, |a, b| {
                        if calls == stop {
                            panic!("controlled comparator failure");
                        }
                        calls += 1;
                        a.id.cmp(&b.id)
                    });
                }));
                assert_eq!(drops.get(), 0);
                let mut ids: Vec<_> = records.iter().map(|row| row.id).collect();
                ids.sort_unstable();
                assert_eq!(ids, (0..17).collect::<Vec<_>>());
                drop(records);
                assert_eq!(drops.get(), 17);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joint_selection_vertex_binds_tag_and_exact_release_identity_bytes() {
        let key = crate::selection_release::SelectionCandidateKeyV1::new(
            b"only".to_vec().into_boxed_slice(),
        );
        let release = crate::selection_release::SelectionReleaseV1::new(
            9,
            vec![vec![key.clone()].into_boxed_slice()].into_boxed_slice(),
        );
        let admitted = crate::selection_release::admit_selection_release_v1(release)
            .expect("single-key release must admit");
        let materialised = crate::selection_release::materialise_joint_selection_v1(
            &admitted,
            &[(
                JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(
                    TargetId::new(1),
                    TargetCandidateId::new(2),
                )]),
                key,
            )],
        )
        .expect("single binding must materialise");

        let color = joint_selection_color(&materialised).expect("identity color must fit");

        assert_eq!(color.as_slice().len(), 1 + 1 + 32);
        assert_eq!(color.as_slice()[0], vertex_tag::JOINT_SELECTION);
        assert_eq!(
            color.as_slice()[1],
            release_tag::SELECTION_RELEASE_IDENTITY_V1,
        );
        assert_eq!(
            &color.as_slice()[2..],
            materialised.release_identity().as_bytes(),
        );
    }

    #[test]
    fn family_vertex_codec_binds_semantic_release_without_opaque_id_or_artifact_receipt() {
        assert_eq!(release_tag::FAMILY_CERTIFICATE_VERTEX_V1, 1);
        assert_eq!(release_tag::FAMILY_MEMBERSHIP_RELEASE_V1, 1);
        assert_eq!(release_tag::FAMILY_SEMANTIC_RELEASE_VERTEX_V2, 2);
        assert_eq!(release_tag::FAMILY_MEMBERSHIP_RELEASE_V2, 2);
        let expected = crate::family::SemanticFamilyReleaseIdV2::from_digest([0xA5; 32]);
        let family = FamilyDeclarationV2::new(FamilyId::new(u32::MAX), expected);

        let color = family_color(&family).unwrap();

        assert_eq!(color.as_slice()[0], vertex_tag::FAMILY);
        assert_eq!(
            color.as_slice()[1],
            release_tag::FAMILY_SEMANTIC_RELEASE_VERTEX_V2
        );
        assert_eq!(&color.as_slice()[2..], expected.as_bytes());
    }

    #[test]
    fn clean_set_final_recheck_mutant_release_changes_exactly_one_declared_bit() {
        let source = crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2;
        let mutant = clean_set_final_recheck_mutant_release_v1();

        assert_eq!(mutant[0], source[0] ^ 1);
        assert_eq!(&mutant[1..], &source[1..]);
    }

    #[test]
    fn declared_clean_set_constraint_color_binds_every_release_digest_byte() {
        let release = crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2;
        let baseline = declared_srgb8_clean_set_constraint_color_for_release(
            vertex_tag::CONSTRAINT_HARD,
            release,
        )
        .unwrap();

        for byte_index in 0..release.len() {
            let mut mutant = release;
            mutant[byte_index] ^= 1;
            assert_ne!(
                baseline,
                declared_srgb8_clean_set_constraint_color_for_release(
                    vertex_tag::CONSTRAINT_HARD,
                    mutant,
                )
                .unwrap(),
                "release digest byte {byte_index} escaped identity",
            );
        }
    }
    use proptest::prelude::*;

    #[test]
    fn evaluator_descriptor_and_certificate_metadata_have_one_source_of_truth() {
        let evaluator = crate::constraints::ExactSrgb8IdentityV1;
        let expected = Srgb8::new([0x12, 0x34, 0x56]);
        let content = evaluator.program_constraint_content_v1(expected);
        let ProgramConstraintContentV1::ExactSrgb8 {
            identity,
            release,
            capability,
            expected: described_expected,
            applicability,
            uncertainty,
        } = content
        else {
            panic!("exact evaluator must describe its own exact invocation");
        };
        assert_eq!(
            identity,
            <crate::constraints::ExactSrgb8IdentityV1 as Evaluator<ProgramPointTargetV1>>::identity(
                &evaluator
            )
        );
        assert_eq!(
            release,
            <crate::constraints::ExactSrgb8IdentityV1 as Evaluator<ProgramPointTargetV1>>::release(
                &evaluator
            )
        );
        assert_eq!(
            capability,
            <crate::constraints::ExactSrgb8IdentityV1 as Evaluator<
                ProgramPointTargetV1,
            >>::capability(&evaluator)
        );
        assert_eq!(described_expected, expected);

        let baseline = constraint_color(vertex_tag::CONSTRAINT_HARD, content).unwrap();
        for mutant in [
            ProgramConstraintContentV1::ExactSrgb8 {
                identity: crate::constraints::ExactConstraintIdentityV1::MutationSentinelV1,
                release,
                capability,
                expected,
                applicability,
                uncertainty,
            },
            ProgramConstraintContentV1::ExactSrgb8 {
                identity,
                release: crate::constraints::ExactIdentityReleaseV1::MutationSentinelV1,
                capability,
                expected,
                applicability,
                uncertainty,
            },
            ProgramConstraintContentV1::ExactSrgb8 {
                identity,
                release,
                capability: crate::constraints::ExactIdentityCapabilityV1::MutationSentinelV1,
                expected,
                applicability,
                uncertainty,
            },
        ] {
            assert_ne!(
                constraint_color(vertex_tag::CONSTRAINT_HARD, mutant).unwrap(),
                baseline
            );
        }
    }

    #[test]
    fn modeled_lcs_release_mutation_affects_only_lcs_aware_constraint_content() {
        fn mutate_modeled_lcs_release(
            content: ProgramConstraintContentV1,
        ) -> ProgramConstraintContentV1 {
            match content {
                ProgramConstraintContentV1::ModeledLcsProbe {
                    release,
                    applicability,
                    uncertainty,
                } => ProgramConstraintContentV1::ModeledLcsProbe {
                    release: release.with_modeled_lcs_release_for_test(
                        crate::lcs_occurrence::ModeledLcsOccurrenceReleaseId::MutationSentinelV1,
                    ),
                    applicability,
                    uncertainty,
                },
                encoded_only => encoded_only,
            }
        }

        let exact = crate::constraints::ExactSrgb8IdentityV1
            .program_constraint_content_v1(Srgb8::new([0x12, 0x34, 0x56]));
        let wcag = crate::constraints::Wcag22Srgb8V1
            .program_constraint_content_v1(Wcag22CriterionV1::Sc143TextDefault);
        for encoded_only in [exact, wcag] {
            assert_eq!(
                constraint_color(vertex_tag::CONSTRAINT_HARD, encoded_only).unwrap(),
                constraint_color(
                    vertex_tag::CONSTRAINT_HARD,
                    mutate_modeled_lcs_release(encoded_only),
                )
                .unwrap(),
                "an encoded-only descriptor has no LCS release capability to mutate",
            );
        }

        let lcs = crate::constraints::LcsProbeProgramEvaluatorV1.program_constraint_content_v1();
        assert_ne!(
            constraint_color(vertex_tag::CONSTRAINT_HARD, lcs).unwrap(),
            constraint_color(vertex_tag::CONSTRAINT_HARD, mutate_modeled_lcs_release(lcs),)
                .unwrap(),
            "an LCS-aware descriptor must bind the modeled occurrence release",
        );
    }

    fn context_color(context: AppearanceContextId) -> VertexColorV1 {
        let mut color = VertexColorV1::new(vertex_tag::OCCURRENCE);
        write_context(&mut color, context).unwrap();
        color
    }

    #[test]
    fn every_appearance_context_coordinate_and_frame_release_is_content_bound() {
        use crate::lcs_occurrence::{
            AdaptingLuminanceCdM2, AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio,
            IEC_SRGB_D65_XYZ_FRAME_V1, MUTATION_SENTINEL_XYZ_FRAME_V1, SurroundProfileId,
        };

        let make = |frame, adapting_luminance, background_ratio, surround| {
            AppearanceContextId::from_inputs(
                AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
                frame,
                AdaptingLuminanceCdM2::try_new(adapting_luminance).unwrap(),
                BackgroundLuminanceRatio::try_new(background_ratio).unwrap(),
                surround,
            )
        };
        let baseline = context_color(make(
            IEC_SRGB_D65_XYZ_FRAME_V1,
            64.0,
            0.2,
            SurroundProfileId::AverageV1,
        ));
        for mutant in [
            make(
                IEC_SRGB_D65_XYZ_FRAME_V1,
                32.0,
                0.2,
                SurroundProfileId::AverageV1,
            ),
            make(
                IEC_SRGB_D65_XYZ_FRAME_V1,
                64.0,
                0.1,
                SurroundProfileId::AverageV1,
            ),
            make(
                IEC_SRGB_D65_XYZ_FRAME_V1,
                64.0,
                0.2,
                SurroundProfileId::DimV1,
            ),
            make(
                MUTATION_SENTINEL_XYZ_FRAME_V1,
                64.0,
                0.2,
                SurroundProfileId::AverageV1,
            ),
        ] {
            assert_ne!(context_color(mutant), baseline);
        }
    }

    #[test]
    fn every_wcag_criterion_has_distinct_constraint_content() {
        let evaluator = crate::constraints::Wcag22Srgb8V1;
        let mut colors = Vec::new();
        for criterion in [
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22CriterionV1::Sc143TextLargeScale,
            Wcag22CriterionV1::Sc1411UiComponentOrState,
            Wcag22CriterionV1::Sc1411GraphicalObject,
        ] {
            colors.push(
                constraint_color(
                    vertex_tag::CONSTRAINT_HARD,
                    evaluator.program_constraint_content_v1(criterion),
                )
                .unwrap(),
            );
        }
        ordering::sort(&mut colors);
        colors.dedup();
        assert_eq!(colors.len(), 4);
    }

    fn mapping_preserves_graph(
        left: &CanonicalGraphV1,
        right: &CanonicalGraphV1,
        mapping: &[usize],
    ) -> bool {
        if left.colors.len() != right.colors.len() || left.edge_count != right.edge_count {
            return false;
        }
        for (vertex, &image) in mapping.iter().enumerate() {
            if left.colors[vertex] != right.colors[image] {
                return false;
            }
            let mut left_arcs = left.adjacency[vertex]
                .iter()
                .map(|arc| (arc.direction, arc.role, mapping[arc.neighbour]))
                .collect::<Vec<_>>();
            let mut right_arcs = right.adjacency[image]
                .iter()
                .map(|arc| (arc.direction, arc.role, arc.neighbour))
                .collect::<Vec<_>>();
            ordering::sort(&mut left_arcs);
            ordering::sort(&mut right_arcs);
            if left_arcs != right_arcs {
                return false;
            }
        }
        true
    }

    fn visit_mappings(
        left: &CanonicalGraphV1,
        right: &CanonicalGraphV1,
        mapping: &mut [usize],
        cursor: usize,
    ) -> bool {
        if cursor == mapping.len() {
            return mapping_preserves_graph(left, right, mapping);
        }
        for candidate in cursor..mapping.len() {
            mapping.swap(cursor, candidate);
            let color_matches = left.colors[cursor] == right.colors[mapping[cursor]];
            if color_matches && visit_mappings(left, right, mapping, cursor + 1) {
                mapping.swap(cursor, candidate);
                return true;
            }
            mapping.swap(cursor, candidate);
        }
        false
    }

    fn brute_force_isomorphic(left: &CanonicalGraphV1, right: &CanonicalGraphV1) -> bool {
        if left.colors.len() != right.colors.len() {
            return false;
        }
        let mut mapping = (0..left.colors.len()).collect::<Vec<_>>();
        visit_mappings(left, right, &mut mapping, 0)
    }

    fn tiny_bipartite_graph(edge_mask: u8) -> CanonicalGraphV1 {
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM)).unwrap();
        let sources = [(); 2].map(|()| {
            graph
                .add_member(VertexColorV1::new(vertex_tag::SOURCE))
                .unwrap()
        });
        let targets = [(); 2].map(|()| {
            graph
                .add_member(VertexColorV1::new(vertex_tag::TARGET_FIXED))
                .unwrap()
        });
        for (bit, (target, source)) in [
            (targets[0], sources[0]),
            (targets[0], sources[1]),
            (targets[1], sources[0]),
            (targets[1], sources[1]),
        ]
        .into_iter()
        .enumerate()
        {
            if edge_mask & (1 << bit) != 0 {
                graph
                    .add_edge(target, source, EdgeRoleV1::TargetSource)
                    .unwrap();
            }
        }
        graph.finish().unwrap()
    }

    #[test]
    fn canonicalizer_matches_an_independent_tiny_isomorphism_oracle() {
        let graphs = (0..16).map(tiny_bipartite_graph).collect::<Vec<_>>();
        let preimages = graphs
            .iter()
            .map(|graph| canonical_preimage(graph).unwrap())
            .collect::<Vec<_>>();

        for left in 0..graphs.len() {
            for right in 0..graphs.len() {
                assert_eq!(
                    preimages[left] == preimages[right],
                    brute_force_isomorphic(&graphs[left], &graphs[right]),
                    "tiny bipartite masks {left:#06b} and {right:#06b}"
                );
            }
        }
    }

    #[test]
    fn exact_automorphism_pruning_prevents_factorial_symmetric_search() {
        const SYMMETRIC_VERTICES: usize = 12;
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM)).unwrap();
        for _ in 0..SYMMETRIC_VERTICES {
            graph
                .add_member(VertexColorV1::new(vertex_tag::SOURCE))
                .unwrap();
        }
        let graph = graph.finish().unwrap();

        let (_, leaves) = canonical_search(&graph).unwrap();

        assert_eq!(leaves, 1, "exact twin pruning visited extra leaves");
    }

    #[test]
    fn exhausted_fault_injection_fuel_never_returns_a_partial_preimage() {
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM)).unwrap();
        graph
            .add_member(VertexColorV1::new(vertex_tag::SOURCE))
            .unwrap();
        graph
            .add_member(VertexColorV1::new(vertex_tag::SOURCE))
            .unwrap();
        let graph = graph.finish().unwrap();

        assert_eq!(
            canonical_search_with_test_fuel(&graph, 0).unwrap_err(),
            ProgramCompileError::ResourceExhausted
        );
    }

    fn relabelled_budget_graph(permutation: [usize; 9]) -> CanonicalGraphV1 {
        const EDGES: [(usize, usize); 16] = [
            (1, 2),
            (2, 1),
            (3, 4),
            (8, 1),
            (4, 3),
            (1, 8),
            (6, 4),
            (6, 7),
            (7, 6),
            (5, 6),
            (5, 3),
            (8, 2),
            (7, 5),
            (4, 7),
            (3, 5),
            (2, 8),
        ];

        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM)).unwrap();
        for _ in 1..permutation.len() {
            graph
                .add_member(VertexColorV1::new(vertex_tag::SOURCE))
                .unwrap();
        }
        for (from, to) in EDGES {
            graph
                .add_edge(permutation[from], permutation[to], EdgeRoleV1::TargetSource)
                .unwrap();
        }
        graph.finish().unwrap()
    }

    fn permutation_from_keys<const N: usize>(keys: [u64; N]) -> Vec<usize> {
        let mut images = (0..N).collect::<Vec<_>>();
        ordering::sort_by_key(&mut images, |index| (keys[*index], *index));
        let mut permutation = vec![0];
        permutation.extend(images.into_iter().map(|index| index + 1));
        permutation
    }

    fn small_directed_graph(edge_mask: u16, permutation: &[usize]) -> CanonicalGraphV1 {
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM)).unwrap();
        for _ in 1..permutation.len() {
            graph
                .add_member(VertexColorV1::new(vertex_tag::SOURCE))
                .unwrap();
        }
        let mut bit = 0;
        for from in 1..permutation.len() {
            for to in 1..permutation.len() {
                if from != to && edge_mask & (1 << bit) != 0 {
                    graph
                        .add_edge(permutation[from], permutation[to], EdgeRoleV1::TargetSource)
                        .unwrap();
                }
                if from != to {
                    bit += 1;
                }
            }
        }
        graph.finish().unwrap()
    }

    #[test]
    fn exact_preimage_is_invariant_under_opaque_relabelling() {
        let canonical = relabelled_budget_graph([0, 1, 2, 3, 4, 5, 6, 7, 8]);
        let renamed = relabelled_budget_graph([0, 1, 2, 6, 3, 5, 4, 7, 8]);

        let (canonical, _) = canonical_search(&canonical).unwrap();
        let (renamed, _) = canonical_search(&renamed).unwrap();

        assert_eq!(canonical, renamed);
    }

    #[test]
    fn fixed_color_key_preserves_length_first_order_and_bounds() -> Result<(), ProgramCompileError>
    {
        let mut colors = Vec::new();
        for length in 0..COLOR_CAPACITY {
            for tag in [0, 1, 128, 255] {
                let mut color = VertexColorV1::new(tag);
                let payload = vec![tag ^ length as u8; length];
                color.push_bytes(&payload)?;
                assert_eq!(usize::from(color.len()), length + 1);
                assert_eq!(color.as_slice()[0], tag);
                assert_eq!(&color.as_slice()[1..], payload);
                colors.push(color);
            }
        }
        for left in &colors {
            for right in &colors {
                assert_eq!(
                    left.cmp(right),
                    (left.len(), left.as_slice()).cmp(&(right.len(), right.as_slice()))
                );
            }
        }
        let mut full = *colors
            .last()
            .ok_or(ProgramCompileError::InternalInvariant)?;
        let before = full;
        assert_eq!(full.push_u8(1), Err(ProgramCompileError::InternalInvariant));
        assert_eq!(
            full.push_u64(1),
            Err(ProgramCompileError::InternalInvariant)
        );
        assert_eq!(full, before);
        Ok(())
    }

    fn repeated_component_graph(count: usize) -> Result<CanonicalGraphV1, ProgramCompileError> {
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM))?;
        for _ in 0..count {
            let source = graph.add_member(VertexColorV1::new(vertex_tag::SOURCE))?;
            let target = graph.add_member(VertexColorV1::new(vertex_tag::TARGET_FIXED))?;
            graph.add_edge(target, source, EdgeRoleV1::TargetSource)?;
        }
        graph.finish()
    }

    #[test]
    fn repeated_components_are_canonicalized_with_one_leaf() -> Result<(), ProgramCompileError> {
        let graph = repeated_component_graph(32)?;
        let (preimage, leaves) = canonical_search_with_test_fuel(&graph, 128)?;
        assert!(!preimage.is_empty());
        assert_eq!(leaves, 1);
        Ok(())
    }

    #[test]
    fn component_interchange_checks_direction_role_multiplicity_and_boundary()
    -> Result<(), ProgramCompileError> {
        for defect in 0..5 {
            let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM))?;
            let mut sources = Vec::new();
            for index in 0..2 {
                let source = graph.add_member(VertexColorV1::new(vertex_tag::SOURCE))?;
                let target = graph.add_member(VertexColorV1::new(vertex_tag::TARGET_FIXED))?;
                sources.push(source);
                let (from, to) = if defect == 1 && index == 0 {
                    (source, target)
                } else {
                    (target, source)
                };
                let role = if defect == 2 && index == 0 {
                    EdgeRoleV1::ProgramMember
                } else {
                    EdgeRoleV1::TargetSource
                };
                graph.add_edge(from, to, role)?;
                // Совпадающие петли и кратные рёбра допустимы; их различие существенно.
                graph.add_edge(source, source, EdgeRoleV1::TargetSource)?;
                graph.add_edge(source, source, EdgeRoleV1::TargetSource)?;
                if defect == 3 && index == 0 {
                    graph.add_edge(source, source, EdgeRoleV1::TargetSource)?;
                }
                if defect == 4 && index == 0 {
                    graph.add_edge(target, 0, EdgeRoleV1::ProgramMember)?;
                }
            }
            let graph = graph.finish()?;
            let partition = refine_partition(&graph, PartitionV1::initial(&graph)?)?;
            assert_eq!(
                candidate_is_component_twin(
                    &graph,
                    &partition,
                    &sources[..1],
                    sources[1],
                    &mut None,
                )?,
                defect == 0,
                "changed incidence {defect}",
            );
        }
        Ok(())
    }

    #[test]
    fn equitable_components_with_repeated_cells_are_not_interchangeable()
    -> Result<(), ProgramCompileError> {
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM))?;
        let mut roots = Vec::new();
        for length in [3, 6] {
            let mut vertices = Vec::new();
            for index in 0..length {
                let tag = [
                    vertex_tag::SOURCE,
                    vertex_tag::TARGET_FIXED,
                    vertex_tag::PAINT_SOLID,
                ][index % 3];
                vertices.push(graph.add_member(VertexColorV1::new(tag))?);
            }
            for index in 0..length {
                graph.add_edge(
                    vertices[index],
                    vertices[(index + 1) % length],
                    EdgeRoleV1::TargetSource,
                )?;
            }
            roots.push(vertices[0]);
        }
        let graph = graph.finish()?;
        let partition = refine_partition(&graph, PartitionV1::initial(&graph)?)?;
        assert!(
            partition
                .0
                .cells
                .iter()
                .any(|cell| cell.contains(&roots[0]) && cell.contains(&roots[1]))
        );
        assert!(!candidate_is_component_twin(
            &graph,
            &partition,
            &roots[..1],
            roots[1],
            &mut None,
        )?);
        assert!(!candidate_is_component_twin(
            &graph,
            &partition,
            &roots[1..],
            roots[0],
            &mut None,
        )?);
        let original_cells = partition.0.cells.clone();
        let mut groups = 0;
        let (unfactored, _) = factor_component_twins(&graph, partition, &mut None, &mut groups)?;
        assert_eq!(
            groups, 0,
            "one unique component cannot authorize the whole cell"
        );
        assert_eq!(unfactored.0.cells, original_cells);
        Ok(())
    }

    fn component_index_matches_union_oracle(
        graph: &CanonicalGraphV1,
        partition: &EquitablePartitionV1,
    ) -> Result<(), ProgramCompileError> {
        let cells = partition_cells(&partition.0, graph.colors.len())?;
        // Независимый oracle объединяет метки концов каждого ребра; обхода
        // компонент и поколений seen_cell здесь нет.
        let mut component = (0..graph.colors.len()).collect::<Vec<_>>();
        for (vertex, arcs) in graph.adjacency.iter().enumerate() {
            for arc in arcs {
                if partition.0.cells[cells[vertex]].len() == 1
                    || partition.0.cells[cells[arc.neighbour]].len() == 1
                {
                    continue;
                }
                let from = component[vertex];
                let to = component[arc.neighbour];
                for label in &mut component {
                    if *label == from {
                        *label = to;
                    }
                }
            }
        }
        let index = ComponentTwinIndexV1::new(graph, partition)?;
        for vertex in 0..graph.colors.len() {
            let unique = partition.0.cells[cells[vertex]].len() != 1
                && partition.0.cells.iter().all(|cell| {
                    cell.iter()
                        .filter(|&&other| component[other] == component[vertex])
                        .count()
                        <= 1
                });
            assert_eq!(
                index.unique_cell_of[vertex],
                if unique { cells[vertex] } else { usize::MAX },
                "component classification at vertex {vertex}",
            );
        }
        Ok(())
    }

    #[test]
    fn component_index_matches_connectivity_after_refinement_and_individualization()
    -> Result<(), ProgramCompileError> {
        for mask in 0..=0x0fff {
            let graph = small_directed_graph(mask, &[0, 1, 2, 3, 4]);
            let partition = refine_partition(&graph, PartitionV1::initial(&graph)?)?;
            component_index_matches_union_oracle(&graph, &partition)?;
            if let Some(cell) = partition.0.first_non_singleton() {
                for &vertex in &partition.0.cells[cell] {
                    let child =
                        refine_partition(&graph, individualize(&partition.0, cell, vertex)?)?;
                    component_index_matches_union_oracle(&graph, &child)?;
                }
            }
        }
        for count in 1..=5 {
            let graph = repeated_component_graph(count)?;
            let partition = refine_partition(&graph, PartitionV1::initial(&graph)?)?;
            component_index_matches_union_oracle(&graph, &partition)?;
        }
        Ok(())
    }

    fn interleaved_component_graph(
        copies: usize,
        declaration_order: [usize; 3],
        reverse_middle: bool,
    ) -> Result<CanonicalGraphV1, ProgramCompileError> {
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM))?;
        // Цвета второй группы находятся между клетками первой группы.
        let boundary = graph.add_member(VertexColorV1::new(60))?;
        for _ in 0..copies {
            let mut vertices = [usize::MAX; 3];
            for index in declaration_order {
                vertices[index] = graph.add_member(VertexColorV1::new([10, 30, 50][index]))?;
            }
            for _ in 0..2 {
                graph.add_edge(vertices[0], vertices[1], EdgeRoleV1::TargetSource)?;
                graph.add_edge(vertices[1], vertices[1], EdgeRoleV1::TargetSource)?;
            }
            let (from, to) = if reverse_middle {
                (vertices[1], vertices[2])
            } else {
                (vertices[2], vertices[1])
            };
            graph.add_edge(from, to, EdgeRoleV1::ProgramMember)?;
            graph.add_edge(vertices[0], boundary, EdgeRoleV1::ProgramMember)?;
            graph.add_edge(boundary, vertices[2], EdgeRoleV1::TargetSource)?;
            graph.add_edge(vertices[2], boundary, EdgeRoleV1::TargetSource)?;
        }
        for _ in 0..2 {
            let left = graph.add_member(VertexColorV1::new(20))?;
            let right = graph.add_member(VertexColorV1::new(40))?;
            graph.add_edge(left, right, EdgeRoleV1::TargetSource)?;
            graph.add_edge(right, boundary, EdgeRoleV1::ProgramMember)?;
        }
        graph.finish()
    }

    fn sequential_partition_following(
        graph: &CanonicalGraphV1,
        order: &PartitionV1,
    ) -> Result<EquitablePartitionV1, ProgramCompileError> {
        let mut rank = vec![usize::MAX; graph.colors.len()];
        for (index, cell) in order.cells.iter().enumerate() {
            let [vertex] = cell.as_slice() else {
                return Err(ProgramCompileError::InternalInvariant);
            };
            rank[*vertex] = index;
        }
        let mut partition =
            full_refine_partition_reference(graph, PartitionV1::initial(graph)?, &mut Vec::new())?;
        while let Some(cell) = partition.0.first_non_singleton() {
            // Выбор вершины допустим в исходном полном поиске; все последующие
            // split и их порядок вычисляет прежний refine без пакетной ветви.
            let vertex = partition.0.cells[cell]
                .iter()
                .copied()
                .min_by_key(|&vertex| rank[vertex])
                .ok_or(ProgramCompileError::InternalInvariant)?;
            partition = full_refine_partition_reference(
                graph,
                individualize(&partition.0, cell, vertex)?,
                &mut Vec::new(),
            )?;
        }
        Ok(partition)
    }

    #[test]
    fn bulk_components_match_sequential_refinement_and_exhaustive_preimages()
    -> Result<(), ProgramCompileError> {
        for copies in [2, 3, 4] {
            for declaration_order in [[0, 1, 2], [2, 0, 1], [1, 2, 0]] {
                for reverse_middle in [false, true] {
                    let graph =
                        interleaved_component_graph(copies, declaration_order, reverse_middle)?;
                    let initial = refine_partition(&graph, PartitionV1::initial(&graph)?)?;
                    let mut groups = 0;
                    let (bulk, _) =
                        factor_component_twins(&graph, initial, &mut None, &mut groups)?;
                    assert_eq!(
                        groups, 2,
                        "both independent component groups must be factored"
                    );
                    let sequential = sequential_partition_following(&graph, &bulk.0)?;
                    assert_eq!(bulk.0.cells, sequential.0.cells);
                    let (actual, leaves, searched_groups) = canonical_search_impl(&graph, None)?;
                    assert_eq!(
                        searched_groups, 2,
                        "search must use the production bulk path"
                    );
                    assert_eq!(leaves, 1);
                    assert_eq!(
                        actual,
                        exhaustive_preimage(&graph, PartitionV1::initial(&graph)?)?,
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn repeated_components_use_one_bulk_group_instead_of_search_depth()
    -> Result<(), ProgramCompileError> {
        let graph = repeated_component_graph(256)?;
        let (preimage, leaves, groups) = canonical_search_impl(&graph, Some(255))?;
        assert!(!preimage.is_empty());
        assert_eq!(leaves, 1);
        assert_eq!(groups, 1);
        assert_eq!(
            canonical_search_impl(&graph, Some(254)).err(),
            Some(ProgramCompileError::ResourceExhausted),
        );
        Ok(())
    }

    fn ordered_cell_sets(mut cells: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
        for cell in &mut cells {
            ordering::sort(cell);
        }
        cells
    }

    fn split_rounds(rounds: Vec<Vec<Vec<usize>>>) -> Vec<Vec<Vec<usize>>> {
        let mut rounds = rounds
            .into_iter()
            .map(ordered_cell_sets)
            .collect::<Vec<_>>();
        rounds.dedup();
        rounds
    }

    fn assert_refinement_matches_full(
        graph: &CanonicalGraphV1,
        partition: PartitionV1,
    ) -> Result<(EquitablePartitionV1, RefinementTraceV1), ProgramCompileError> {
        let mut expected_rounds = Vec::new();
        let expected = full_refine_partition_reference(
            graph,
            PartitionV1 {
                cells: partition.cells.clone(),
            },
            &mut expected_rounds,
        )?;
        let mut trace = RefinementTraceV1::default();
        let actual = refine_partition_impl(graph, partition, Some(&mut trace))?;
        assert_eq!(
            ordered_cell_sets(actual.0.cells.clone()),
            ordered_cell_sets(expected.0.cells),
            "ordered final cell sets",
        );
        assert_eq!(
            split_rounds(trace.rounds.clone()),
            split_rounds(expected_rounds),
            "every synchronous split round",
        );
        Ok((actual, trace))
    }

    fn check_initial_and_individualized_refinement(
        graph: &CanonicalGraphV1,
    ) -> Result<(), ProgramCompileError> {
        let (partition, _) = assert_refinement_matches_full(graph, PartitionV1::initial(graph)?)?;
        for (cell_index, cell) in partition.0.cells.iter().enumerate() {
            if cell.len() < 2 {
                continue;
            }
            for &vertex in cell {
                assert_refinement_matches_full(
                    graph,
                    individualize(&partition.0, cell_index, vertex)?,
                )?;
            }
        }
        Ok(())
    }

    #[test]
    fn equal_signature_heapsort_preserves_one_left_rotation() {
        for count in 0..=64 {
            let mut actual = (0..count).collect::<Vec<_>>();
            let expected = (1..count)
                .chain((count != 0).then_some(0))
                .collect::<Vec<_>>();
            ordering::sort_by(&mut actual, |_, _| core::cmp::Ordering::Equal);
            assert_eq!(actual, expected, "equal-key cell length {count}");
        }
    }

    #[test]
    fn incremental_refinement_matches_full_refiner_for_all_small_digraphs()
    -> Result<(), ProgramCompileError> {
        for mask in 0..=0x0fff {
            let graph = small_directed_graph(mask, &[0, 1, 2, 3, 4]);
            check_initial_and_individualized_refinement(&graph)?;
            let mut reversed = PartitionV1::initial(&graph)?;
            for cell in &mut reversed.cells {
                cell.reverse();
            }
            assert_refinement_matches_full(&graph, reversed)?;
        }
        Ok(())
    }

    #[test]
    fn incremental_refinement_skips_unchanged_cells_without_skipping_rounds()
    -> Result<(), ProgramCompileError> {
        for reverse_edges in [false, true] {
            let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM))?;
            let mut stable = Vec::new();
            for _ in 0..5 {
                stable.push(graph.add_member(VertexColorV1::new(20))?);
            }
            let mut path = Vec::new();
            for _ in 0..12 {
                path.push(graph.add_member(VertexColorV1::new(10))?);
            }
            for pair in path.windows(2) {
                let (from, to) = if reverse_edges {
                    (pair[1], pair[0])
                } else {
                    (pair[0], pair[1])
                };
                graph.add_edge(from, to, EdgeRoleV1::TargetSource)?;
            }
            let graph = graph.finish()?;
            let (partition, trace) =
                assert_refinement_matches_full(&graph, PartitionV1::initial(&graph)?)?;
            assert!(trace.rounds.len() >= 4, "path must propagate across rounds");
            assert!(
                trace.skipped_cells >= trace.rounds.len() - 1,
                "stable cell must execute the optimized branch",
            );
            ordering::sort(&mut stable);
            assert!(
                ordered_cell_sets(partition.0.cells).contains(&stable),
                "the unchanged cell retains exactly its members",
            );
            check_initial_and_individualized_refinement(&graph)?;
        }
        Ok(())
    }

    #[test]
    fn incremental_refinement_preserves_middle_and_tied_largest_children()
    -> Result<(), ProgramCompileError> {
        for (multiplicities, expected_sizes) in [
            ([0, 1, 1, 1, 1, 2], vec![1, 4, 1]),
            ([0, 0, 1, 1, 2, 2], vec![2, 2, 2]),
        ] {
            let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM))?;
            let mut sources = Vec::new();
            for _ in multiplicities {
                sources.push(graph.add_member(VertexColorV1::new(10))?);
            }
            let marker = graph.add_member(VertexColorV1::new(30))?;
            for (&source, multiplicity) in sources.iter().zip(multiplicities) {
                for _ in 0..multiplicity {
                    graph.add_edge(source, marker, EdgeRoleV1::TargetSource)?;
                }
            }
            let graph = graph.finish()?;
            let (_, trace) = assert_refinement_matches_full(&graph, PartitionV1::initial(&graph)?)?;
            let sizes = trace.rounds[0]
                .iter()
                .filter(|cell| cell.iter().all(|vertex| sources.contains(vertex)))
                .map(Vec::len)
                .collect::<Vec<_>>();
            assert_eq!(sizes, expected_sizes, "representative must not move groups");
            check_initial_and_individualized_refinement(&graph)?;
        }
        Ok(())
    }

    #[test]
    fn incremental_refinement_preserves_typed_simultaneous_splits_and_cycles()
    -> Result<(), ProgramCompileError> {
        for reverse_edges in [false, true] {
            let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM))?;
            let mut groups = Vec::new();
            for color in [10, 20, 30, 40, 50] {
                let mut group = Vec::new();
                for _ in 0..4 {
                    group.push(graph.add_member(VertexColorV1::new(color))?);
                }
                groups.push(group);
            }
            let marker = graph.add_member(VertexColorV1::new(60))?;
            let multiplicities = [(0, 2), (1, 1), (2, 1), (2, 0)];
            for (index, (left_count, right_count)) in multiplicities.into_iter().enumerate() {
                // Group 1 sees splits before and after its iteration position.
                // Group 4 has only one directed incidence to the splitting group.
                for (from, to, role) in [
                    (groups[0][index], groups[1][index], EdgeRoleV1::TargetSource),
                    (
                        groups[1][index],
                        groups[2][index],
                        EdgeRoleV1::ProgramMember,
                    ),
                    (groups[2][index], groups[3][index], EdgeRoleV1::TargetSource),
                    (groups[2][index], groups[3][index], EdgeRoleV1::TargetSource),
                    (
                        groups[4][index],
                        groups[0][index],
                        EdgeRoleV1::ProgramMember,
                    ),
                ] {
                    let (from, to) = if reverse_edges {
                        (to, from)
                    } else {
                        (from, to)
                    };
                    graph.add_edge(from, to, role)?;
                }
                for _ in 0..left_count {
                    graph.add_edge(groups[0][index], marker, EdgeRoleV1::TargetSource)?;
                }
                for _ in 0..right_count {
                    graph.add_edge(groups[2][index], marker, EdgeRoleV1::ProgramMember)?;
                }
                for _ in 0..2 {
                    graph.add_edge(groups[4][index], groups[4][index], EdgeRoleV1::TargetSource)?;
                }
            }
            let graph = graph.finish()?;
            let (partition, trace) =
                assert_refinement_matches_full(&graph, PartitionV1::initial(&graph)?)?;
            assert!(trace.rounds[0].iter().any(|cell| {
                cell.len() == 4 && cell.iter().all(|vertex| groups[1].contains(vertex))
            }));
            assert!(partition.0.cells.iter().all(|cell| {
                cell.len() == 1 || cell.iter().all(|vertex| !groups[1].contains(vertex))
            }));
            check_initial_and_individualized_refinement(&graph)?;
        }
        for count in 3..=16 {
            for bidirectional in [false, true] {
                check_initial_and_individualized_refinement(&connected_cycle_graph(
                    count,
                    bidirectional,
                )?)?;
            }
        }
        for reverse_middle in [false, true] {
            check_initial_and_individualized_refinement(&interleaved_component_graph(
                3,
                [2, 0, 1],
                reverse_middle,
            )?)?;
        }
        Ok(())
    }

    #[test]
    fn sparse_refinement_preserves_clean_middle_and_largest_dirty_children()
    -> Result<(), ProgramCompileError> {
        fn ordered_members(round: &[Vec<usize>], members: &[usize]) -> Vec<Vec<usize>> {
            round
                .iter()
                .filter(|cell| cell.iter().any(|vertex| members.contains(vertex)))
                .map(|cell| {
                    assert!(
                        cell.iter().all(|vertex| members.contains(vertex)),
                        "a child must remain inside its original color cell",
                    );
                    let mut normalized = cell.clone();
                    normalized.sort_unstable();
                    normalized
                })
                .collect()
        }

        for largest_is_dirty in [false, true] {
            let count = if largest_is_dirty { 9 } else { 6 };
            let mut builder = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM))?;
            let mut sources = Vec::new();
            let mut receivers = Vec::new();
            for _ in 0..count {
                sources.push(builder.add_member(VertexColorV1::new(vertex_tag::SOURCE))?);
            }
            for _ in 0..count {
                receivers.push(builder.add_member(VertexColorV1::new(vertex_tag::TARGET_FIXED))?);
            }
            let marker = builder.add_member(VertexColorV1::new(vertex_tag::OUTPUT))?;

            if largest_is_dirty {
                // A has five sources; B has four. Every source has exactly two
                // receiver incidences, so only the marker distinguishes A and B
                // in the first synchronous round. A sorts before B.
                for &source in &sources[..5] {
                    builder.add_edge(source, marker, EdgeRoleV1::TargetSource)?;
                }
                // Receiver signatures: 3 x (A,A), 4 x (A,B), 2 x (B,B).
                // The clean child has size 3; the largest dirty child has size 4.
                for (receiver, [left, right]) in [
                    [0, 1],
                    [1, 2],
                    [2, 0],
                    [3, 5],
                    [3, 6],
                    [4, 7],
                    [4, 8],
                    [5, 6],
                    [7, 8],
                ]
                .into_iter()
                .enumerate()
                {
                    builder.add_edge(
                        sources[left],
                        receivers[receiver],
                        EdgeRoleV1::TargetSource,
                    )?;
                    builder.add_edge(
                        sources[right],
                        receivers[receiver],
                        EdgeRoleV1::TargetSource,
                    )?;
                }
            } else {
                // Marker multiplicities order the source children as [1,4,1].
                // Their matching receiver children inherit that same order.
                for (index, multiplicity) in [0, 1, 1, 1, 1, 2].into_iter().enumerate() {
                    builder.add_edge(sources[index], receivers[index], EdgeRoleV1::TargetSource)?;
                    for _ in 0..multiplicity {
                        builder.add_edge(sources[index], marker, EdgeRoleV1::TargetSource)?;
                    }
                }
            }

            let graph = builder.finish()?;
            let expected_source_children = if largest_is_dirty {
                vec![sources[..5].to_vec(), sources[5..].to_vec()]
            } else {
                vec![vec![sources[5]], sources[1..5].to_vec(), vec![sources[0]]]
            };
            let expected_receiver_children = if largest_is_dirty {
                vec![
                    receivers[..3].to_vec(),
                    receivers[3..7].to_vec(),
                    receivers[7..].to_vec(),
                ]
            } else {
                vec![
                    vec![receivers[5]],
                    receivers[1..5].to_vec(),
                    vec![receivers[0]],
                ]
            };
            let expected_dirty_receivers = if largest_is_dirty {
                receivers[3..].to_vec()
            } else {
                vec![receivers[0], receivers[5]]
            };
            let expected_clean_receivers = if largest_is_dirty {
                receivers[..3].to_vec()
            } else {
                receivers[1..5].to_vec()
            };
            let expected_receiver_sizes = if largest_is_dirty {
                vec![3, 4, 2]
            } else {
                vec![1, 4, 1]
            };

            // Put dirty vertices at different physical ends. Logical child order
            // and memberships must not depend on these within-cell permutations.
            for permutation in 0..4 {
                let mut initial = PartitionV1::initial(&graph)?;
                for cell in &mut initial.cells {
                    if permutation & 1 != 0 {
                        cell.reverse();
                    }
                    if permutation & 2 != 0 && cell.len() > 1 {
                        cell.rotate_left(1);
                    }
                }
                let initial_cell_count = initial.cells.len();
                let (_, trace) = assert_refinement_matches_full(&graph, initial)?;
                assert!(
                    trace.rounds.len() >= 2,
                    "source split must propagate: dirty={largest_is_dirty}, permutation={permutation}",
                );
                let first_sources = ordered_members(&trace.rounds[0], &sources);
                assert_eq!(
                    first_sources, expected_source_children,
                    "first source split"
                );
                assert_eq!(
                    ordered_members(&trace.rounds[0], &receivers),
                    vec![receivers.clone()],
                    "receivers must remain unsplit until the next synchronous round",
                );
                assert_eq!(
                    trace.rounds[0].len(),
                    initial_cell_count + expected_source_children.len() - 1,
                    "only the source cell may split in the first round",
                );

                // Derive the complete next-round dirty set from the actual split.
                let largest_source = first_sources
                    .iter()
                    .max_by_key(|cell| cell.len())
                    .expect("the source cell has children");
                let dirty_receivers = receivers
                    .iter()
                    .copied()
                    .filter(|&receiver| {
                        graph.adjacency[receiver].iter().any(|arc| {
                            sources.contains(&arc.neighbour)
                                && !largest_source.contains(&arc.neighbour)
                        })
                    })
                    .collect::<Vec<_>>();
                let clean_receivers = receivers
                    .iter()
                    .copied()
                    .filter(|receiver| !dirty_receivers.contains(receiver))
                    .collect::<Vec<_>>();
                assert_eq!(
                    dirty_receivers, expected_dirty_receivers,
                    "actual dirty set"
                );
                assert_eq!(
                    clean_receivers, expected_clean_receivers,
                    "actual clean set"
                );
                assert_eq!(
                    ordered_members(&trace.rounds[1], &sources),
                    expected_source_children,
                    "the second round must still use the unsplit receiver snapshot",
                );
                let second_receivers = ordered_members(&trace.rounds[1], &receivers);
                assert_eq!(
                    second_receivers, expected_receiver_children,
                    "second-round receiver children",
                );
                assert_eq!(
                    second_receivers.iter().map(Vec::len).collect::<Vec<_>>(),
                    expected_receiver_sizes,
                    "ordered child cardinalities",
                );
                let clean_child = second_receivers
                    .iter()
                    .position(|cell| cell == &clean_receivers)
                    .expect("all clean receivers must form one child");
                assert_eq!(
                    clean_child,
                    if largest_is_dirty { 0 } else { 1 },
                    "physical ownership must preserve logical child order",
                );
                let (largest_child, largest_receivers) = second_receivers
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, cell)| cell.len())
                    .expect("the receiver cell has children");
                assert_eq!(
                    largest_child, 1,
                    "the unique largest child is in the middle"
                );

                if largest_is_dirty {
                    assert!(
                        largest_receivers
                            .iter()
                            .all(|vertex| dirty_receivers.contains(vertex)),
                        "the largest child must consist entirely of dirty vertices",
                    );
                    assert!(
                        trace.rounds.len() >= 3,
                        "the receiver split must propagate back into the sources",
                    );
                    assert_eq!(
                        ordered_members(&trace.rounds[2], &sources),
                        vec![
                            sources[..3].to_vec(),
                            sources[3..5].to_vec(),
                            sources[5..].to_vec(),
                        ],
                        "the retained source cell must support a later [3,2] split",
                    );
                } else {
                    assert_eq!(
                        largest_receivers, &clean_receivers,
                        "largest-child selection must count the complete clean group",
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn sparse_refinement_visits_only_changed_signatures() -> Result<(), ProgramCompileError> {
        for count in [32_usize, 128] {
            for reverse_edges in [false, true] {
                let mut builder = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM))?;
                let vertices = (0..count)
                    .map(|_| builder.add_member(VertexColorV1::new(vertex_tag::SOURCE)))
                    .collect::<Result<Vec<_>, _>>()?;
                for pair in vertices.windows(2) {
                    let (from, to) = if reverse_edges {
                        (pair[1], pair[0])
                    } else {
                        (pair[0], pair[1])
                    };
                    builder.add_edge(from, to, EdgeRoleV1::TargetSource)?;
                }
                let graph = builder.finish()?;
                let (partition, trace) =
                    assert_refinement_matches_full(&graph, PartitionV1::initial(&graph)?)?;
                assert!(
                    partition.0.is_discrete(),
                    "the path must reach its complete refinement"
                );
                // После первого прохода следующий split затрагивает два конца
                // остатка пути и одного представителя неизменившихся вершин.
                let work_bound = count + 3 * count.div_ceil(2);
                assert!(
                    trace.signature_vertices >= count,
                    "the first round must execute"
                );
                assert!(
                    trace.signature_vertices <= work_bound,
                    "{} signatures for a path of {count} vertices; bound {work_bound}",
                    trace.signature_vertices,
                );
                assert!(
                    trace.signature_arcs >= 2 * count,
                    "the signatures must inspect edges"
                );
                assert!(
                    trace.signature_arcs <= 3 * trace.signature_vertices,
                    "a path member has at most three incident typed arcs",
                );
            }
        }
        Ok(())
    }

    #[test]
    fn refinement_reuses_initial_singletons_without_per_vertex_allocations()
    -> Result<(), ProgramCompileError> {
        let mut allocation_counts = Vec::new();
        for count in [32_usize, 128] {
            let mut builder = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM))?;
            let vertices = (0..count + 2)
                .map(|_| builder.add_member(VertexColorV1::new(vertex_tag::SOURCE)))
                .collect::<Result<Vec<_>, _>>()?;
            builder.add_edge(vertices[count], vertices[0], EdgeRoleV1::TargetSource)?;
            builder.add_edge(vertices[count + 1], vertices[1], EdgeRoleV1::TargetSource)?;
            let graph = builder.finish()?;
            let mut cells = Vec::with_capacity(count + 2);
            cells.push(vec![0]);
            cells.extend(vertices[..count].iter().map(|&vertex| vec![vertex]));
            cells.push(vertices[count..].to_vec());
            let initial = PartitionV1 { cells };
            let expected = full_refine_partition_reference(
                &graph,
                PartitionV1 {
                    cells: initial.cells.clone(),
                },
                &mut Vec::new(),
            )?;
            let (actual, events) =
                crate::test_support::measured_allocator_events(|| refine_partition(&graph, initial));
            let actual = actual?;
            assert!(actual.0.is_discrete(), "the remaining pair must split");
            assert_eq!(actual.0.cells, expected.0.cells, "complete ordered result");
            allocation_counts.push(events.alloc + events.realloc);
        }
        assert!(
            allocation_counts[1] <= allocation_counts[0],
            "more retained singletons must not require more allocations: {allocation_counts:?}",
        );
        Ok(())
    }

    // Full-round implementation frozen from 5bf2d944. Only the record type,
    // function name and round trace differ from that source.
    struct FullRefinementRecordV1 {
        vertex: usize,
        signature: Vec<RefinementAtomV1>,
    }

    fn full_refine_partition_reference(
        graph: &CanonicalGraphV1,
        mut partition: PartitionV1,
        rounds: &mut Vec<Vec<Vec<usize>>>,
    ) -> Result<EquitablePartitionV1, ProgramCompileError> {
        loop {
            if partition.is_discrete() {
                return Ok(EquitablePartitionV1(partition));
            }
            let cell_of = partition_cells(&partition, graph.colors.len())?;

            let mut next_cells = Vec::new();
            next_cells
                .try_reserve_exact(graph.colors.len())
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            let previous_count = partition.cells.len();
            for cell in partition.cells {
                if cell.len() == 1 {
                    next_cells.push(cell);
                    continue;
                }
                let mut records = Vec::new();
                records
                    .try_reserve_exact(cell.len())
                    .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                for &vertex in &cell {
                    let arcs = graph
                        .adjacency
                        .get(vertex)
                        .ok_or(ProgramCompileError::InternalInvariant)?;
                    let mut signature = Vec::new();
                    signature
                        .try_reserve_exact(arcs.len())
                        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                    for arc in arcs {
                        signature.push(RefinementAtomV1 {
                            direction: arc.direction,
                            role: arc.role,
                            neighbour_cell: cell_of[arc.neighbour],
                        });
                    }
                    ordering::sort(&mut signature);
                    records.push(FullRefinementRecordV1 { vertex, signature });
                }
                ordering::sort_by(&mut records, |left, right| {
                    left.signature.cmp(&right.signature)
                });

                let mut start = 0;
                while start < records.len() {
                    let mut end = start + 1;
                    while end < records.len() && records[start].signature == records[end].signature
                    {
                        end += 1;
                    }
                    let mut split = Vec::new();
                    split
                        .try_reserve_exact(end - start)
                        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                    split.extend(records[start..end].iter().map(|record| record.vertex));
                    next_cells.push(split);
                    start = end;
                }
            }

            rounds.push(next_cells.clone());
            if next_cells.len() == previous_count {
                partition.cells = next_cells;
                return Ok(EquitablePartitionV1(partition));
            }
            partition.cells = next_cells;
        }
    }

    fn connected_cycle_graph(
        count: usize,
        bidirectional: bool,
    ) -> Result<CanonicalGraphV1, ProgramCompileError> {
        let mut graph = GraphBuilderV1::new(VertexColorV1::new(vertex_tag::PROGRAM))?;
        for _ in 0..count {
            graph.add_member(VertexColorV1::new(vertex_tag::SOURCE))?;
        }
        for from in 1..=count {
            let to = from % count + 1;
            graph.add_edge(from, to, EdgeRoleV1::TargetSource)?;
            if bidirectional {
                graph.add_edge(to, from, EdgeRoleV1::TargetSource)?;
            }
        }
        graph.finish()
    }

    #[test]
    fn connected_cycles_build_one_orbit_per_unchanged_search_pass()
    -> Result<(), ProgramCompileError> {
        // Любой ненулевой сдвиг цикла простой длины порождает всю орбиту.
        // После двух листьев оставшиеся ветви используют один и тот же bitmap.
        for count in [5, 17] {
            let graph = connected_cycle_graph(count, false)?;
            let expected = exhaustive_preimage(&graph, PartitionV1::initial(&graph)?)?;
            EXPLORED_ORBIT_BUILDS.with(|builds| builds.set(0));

            let (actual, leaves) = canonical_search(&graph)?;
            let builds = EXPLORED_ORBIT_BUILDS.with(core::cell::Cell::get);

            assert_eq!(leaves, 2, "directed prime cycle of length {count}");
            assert_eq!(builds, 1, "repeated orbit construction for cycle {count}");
            assert_eq!(actual, expected);
        }
        Ok(())
    }

    #[test]
    fn connected_cycles_keep_unpruned_canonical_bytes() -> Result<(), ProgramCompileError> {
        for count in 3..=9 {
            for bidirectional in [false, true] {
                let graph = connected_cycle_graph(count, bidirectional)?;
                let (actual, leaves) = canonical_search(&graph)?;
                assert!(leaves > 1, "connected cycles must exercise exact search");
                assert_eq!(
                    actual,
                    exhaustive_preimage(&graph, PartitionV1::initial(&graph)?)?,
                    "cycle length {count}, bidirectional {bidirectional}",
                );
            }
        }
        Ok(())
    }

    fn exhaustive_preimage(
        graph: &CanonicalGraphV1,
        partition: PartitionV1,
    ) -> Result<Vec<u8>, ProgramCompileError> {
        let partition = full_refine_partition_reference(graph, partition, &mut Vec::new())?.0;
        match partition.first_non_singleton() {
            None => Ok(serialize_leaf(graph, &partition)?.preimage),
            Some(cell) => {
                let mut best: Option<Vec<u8>> = None;
                for &vertex in &partition.cells[cell] {
                    let preimage =
                        exhaustive_preimage(graph, individualize(&partition, cell, vertex)?)?;
                    if best.as_ref().is_none_or(|current| preimage < *current) {
                        best = Some(preimage);
                    }
                }
                best.ok_or(ProgramCompileError::InternalInvariant)
            }
        }
    }

    #[test]
    fn pruning_keeps_exact_bytes_on_every_four_vertex_digraph() -> Result<(), ProgramCompileError> {
        for mask in 0..=0x0fff {
            let graph = small_directed_graph(mask, &[0, 1, 2, 3, 4]);
            assert_eq!(
                canonical_preimage(&graph)?,
                exhaustive_preimage(&graph, PartitionV1::initial(&graph)?)?,
                "directed edge mask {mask:#05x}",
            );
        }
        for count in 1..=5 {
            let graph = repeated_component_graph(count)?;
            assert_eq!(
                canonical_preimage(&graph)?,
                exhaustive_preimage(&graph, PartitionV1::initial(&graph)?)?,
                "components {count}",
            );
        }
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn hostile_graph_preimage_is_invariant_for_generated_bijections(
            keys in proptest::array::uniform8(any::<u64>()),
        ) {
            let permutation = permutation_from_keys(keys);
            let permutation: [usize; 9] = permutation.try_into().unwrap();
            let baseline = relabelled_budget_graph([0, 1, 2, 3, 4, 5, 6, 7, 8]);
            let renamed = relabelled_budget_graph(permutation);

            prop_assert_eq!(
                canonical_search(&baseline).unwrap().0,
                canonical_search(&renamed).unwrap().0,
            );
        }

        #[test]
        fn small_role_directed_graph_preimage_is_invariant_for_generated_bijections(
            edge_mask in 0_u16..=0x0fff,
            keys in proptest::array::uniform4(any::<u64>()),
        ) {
            let baseline = small_directed_graph(edge_mask, &[0, 1, 2, 3, 4]);
            let renamed = small_directed_graph(edge_mask, &permutation_from_keys(keys));

            prop_assert_eq!(
                canonical_search(&baseline).unwrap().0,
                canonical_search(&renamed).unwrap().0,
            );
        }
    }
}
