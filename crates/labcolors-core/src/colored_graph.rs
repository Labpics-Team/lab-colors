//! Точная канонизация ориентированных цветных мультиграфов.
//!
//! Потребитель задаёт содержимое вершин и числовые роли рёбер. Порядок цветов
//! независим от нумерации вершин; равенство цветов совпадает с равенством их байтов.
//! Уточнение разбиений, индивидуализация и доказанные автоморфизмы сохраняют
//! весь граф, включая петли, направления и кратности рёбер. Успешный результат
//! содержит полный канонический прообраз; бюджет поиска не вводится.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CanonicalGraphError {
    ResourceExhausted,
    InternalInvariant,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EdgeV1 {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) role: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ArcV1 {
    direction: u8,
    role: u8,
    neighbour: usize,
}

pub(crate) struct ColoredGraph<Color> {
    colors: Vec<Color>,
    adjacency_offsets: Vec<usize>,
    arcs: Vec<ArcV1>,
    edge_count: usize,
}

impl<Color> ColoredGraph<Color> {
    pub(crate) fn new(colors: Vec<Color>, edges: Vec<EdgeV1>) -> Result<Self, CanonicalGraphError> {
        let offset_count = colors
            .len()
            .checked_add(1)
            .ok_or(CanonicalGraphError::ResourceExhausted)?;
        let mut offsets = Vec::new();
        offsets
            .try_reserve_exact(offset_count)
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        offsets.resize(offset_count, 0_usize);
        for edge in &edges {
            for vertex in [edge.from, edge.to] {
                let end = vertex
                    .checked_add(1)
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                let degree = offsets
                    .get_mut(end)
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                *degree = degree
                    .checked_add(1)
                    .ok_or(CanonicalGraphError::ResourceExhausted)?;
            }
        }
        let mut arc_count = 0_usize;
        for offset in offsets.iter_mut().skip(1) {
            arc_count = arc_count
                .checked_add(*offset)
                .ok_or(CanonicalGraphError::ResourceExhausted)?;
            *offset = arc_count;
        }
        let mut arcs = Vec::new();
        arcs.try_reserve_exact(arc_count)
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        arcs.resize(
            arc_count,
            ArcV1 {
                direction: 0,
                role: 0,
                neighbour: 0,
            },
        );
        // Концы диапазонов служат курсорами заполнения справа налево. После
        // последней дуги каждого узла они уже равны началам его диапазона;
        // отдельный временный буфер курсоров не нужен. Обратный обход рёбер и
        // обеих дуг сохраняет исходный порядок, включая отсортированные списки.
        for edge in edges.iter().rev() {
            for (vertex, direction, neighbour) in [(edge.to, 1, edge.from), (edge.from, 0, edge.to)]
            {
                let end = vertex
                    .checked_add(1)
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                let cursor = offsets
                    .get_mut(end)
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                *cursor = cursor
                    .checked_sub(1)
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                *arcs
                    .get_mut(*cursor)
                    .ok_or(CanonicalGraphError::InternalInvariant)? = ArcV1 {
                    direction,
                    role: edge.role,
                    neighbour,
                };
            }
        }
        offsets.copy_within(1.., 0);
        *offsets
            .last_mut()
            .ok_or(CanonicalGraphError::InternalInvariant)? = arc_count;
        for bounds in offsets.windows(2) {
            ordering::sort(
                arcs.get_mut(bounds[0]..bounds[1])
                    .ok_or(CanonicalGraphError::InternalInvariant)?,
            );
        }
        Ok(Self {
            colors,
            adjacency_offsets: offsets,
            arcs,
            edge_count: edges.len(),
        })
    }

    fn adjacent(&self, vertex: usize) -> Result<&[ArcV1], CanonicalGraphError> {
        let Some([start, end]) = self
            .adjacency_offsets
            .get(vertex..)
            .and_then(|offsets| offsets.get(..2))
        else {
            return Err(CanonicalGraphError::InternalInvariant);
        };
        self.arcs
            .get(*start..*end)
            .ok_or(CanonicalGraphError::InternalInvariant)
    }

    #[cfg(test)]
    pub(crate) fn colors(&self) -> &[Color] {
        &self.colors
    }

    #[cfg(test)]
    pub(crate) fn colors_mut(&mut self) -> &mut [Color] {
        &mut self.colors
    }

    #[cfg(test)]
    pub(crate) fn edge_count(&self) -> usize {
        self.edge_count
    }

    #[cfg(test)]
    pub(crate) fn edge_roles(&self) -> impl Iterator<Item = u8> + '_ {
        self.arcs
            .iter()
            .filter(|arc| arc.direction == 0)
            .map(|arc| arc.role)
    }
}

struct PartitionV1 {
    cells: Vec<Vec<usize>>,
}

/// Клетки однородны по цвету вершины и полным типизированным спискам соседних клеток.
/// Конструктор принадлежит завершённому уточнению разбиения.
struct EquitablePartitionV1(PartitionV1);

impl PartitionV1 {
    fn initial<Color: Ord + AsRef<[u8]>>(
        graph: &ColoredGraph<Color>,
    ) -> Result<Self, CanonicalGraphError> {
        let mut order = Vec::new();
        order
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        order.extend(0..graph.colors.len());
        ordering::sort_by(&mut order, |left, right| {
            graph.colors[*left]
                .cmp(&graph.colors[*right])
                .then_with(|| left.cmp(right))
        });

        let mut cells = Vec::new();
        cells
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
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

fn copy_vertices(source: &[usize]) -> Result<Vec<usize>, CanonicalGraphError> {
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(source.len())
        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
    copied.extend_from_slice(source);
    Ok(copied)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RefinementAtomV1 {
    direction: u8,
    role: u8,
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
    built_signatures: usize,
    shared_signatures: usize,
    processed_cells: usize,
    moved_vertices: usize,
}

fn partition_cells(
    partition: &PartitionV1,
    vertex_count: usize,
) -> Result<Vec<usize>, CanonicalGraphError> {
    let mut cell_of = Vec::new();
    partition_cells_into(partition, vertex_count, &mut cell_of)?;
    Ok(cell_of)
}

fn partition_cells_into(
    partition: &PartitionV1,
    vertex_count: usize,
    cell_of: &mut Vec<usize>,
) -> Result<(), CanonicalGraphError> {
    cell_of.clear();
    cell_of
        .try_reserve_exact(vertex_count)
        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
    cell_of.resize(vertex_count, usize::MAX);
    for (cell_index, cell) in partition.cells.iter().enumerate() {
        for &vertex in cell {
            let slot = cell_of
                .get_mut(vertex)
                .ok_or(CanonicalGraphError::InternalInvariant)?;
            if *slot != usize::MAX {
                return Err(CanonicalGraphError::InternalInvariant);
            }
            *slot = cell_index;
        }
    }
    if cell_of.contains(&usize::MAX) {
        return Err(CanonicalGraphError::InternalInvariant);
    }
    Ok(())
}

fn refine_partition<Color: Ord + AsRef<[u8]>>(
    graph: &ColoredGraph<Color>,
    partition: PartitionV1,
) -> Result<EquitablePartitionV1, CanonicalGraphError> {
    refine_partition_seeded_impl(
        graph,
        partition,
        None,
        #[cfg(test)]
        None,
    )
}

struct ActiveRefinementCellV1 {
    vertices: Vec<usize>,
    // Начало непересекающегося интервала в упорядоченном разбиении. Split
    // делит тот же интервал; ранги остальных клеток никогда не меняются.
    rank: usize,
    dirty_head: usize,
}

struct ActiveRefinementV1 {
    cells: Vec<ActiveRefinementCellV1>,
    cell_of: Vec<usize>,
    position: Vec<usize>,
    // MAX означает clean; vertex_count завершает непустой dirty-список.
    dirty_next: Vec<usize>,
    active: Vec<usize>,
    signatures: Vec<RefinementAtomV1>,
    records: Vec<RefinementRecordV1>,
}

impl ActiveRefinementV1 {
    fn new(
        partition: PartitionV1,
        vertex_count: usize,
        all_dirty: bool,
    ) -> Result<Self, CanonicalGraphError> {
        let cell_of = partition_cells(&partition, vertex_count)?;
        let mut position = Vec::new();
        position
            .try_reserve_exact(vertex_count)
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        position.resize(vertex_count, 0);
        let mut dirty_next = Vec::new();
        dirty_next
            .try_reserve_exact(vertex_count)
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        dirty_next.resize(vertex_count, usize::MAX);
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(partition.cells.len())
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        let mut active = Vec::new();
        active
            .try_reserve_exact(if all_dirty {
                partition.cells.iter().filter(|cell| cell.len() > 1).count()
            } else {
                0
            })
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        let mut rank = 0;
        for vertices in partition.cells {
            if vertices.is_empty() {
                return Err(CanonicalGraphError::InternalInvariant);
            }
            let mut dirty_head = vertex_count;
            for (offset, &vertex) in vertices.iter().enumerate() {
                position[vertex] = offset;
                if all_dirty && vertices.len() > 1 {
                    dirty_next[vertex] = dirty_head;
                    dirty_head = vertex;
                }
            }
            if all_dirty && vertices.len() > 1 {
                active.push(cells.len());
            }
            let next_rank = rank + vertices.len();
            cells.push(ActiveRefinementCellV1 {
                vertices,
                rank,
                dirty_head,
            });
            rank = next_rank;
        }
        Ok(Self {
            cells,
            cell_of,
            position,
            dirty_next,
            active,
            signatures: Vec::new(),
            records: Vec::new(),
        })
    }

    fn record<Color: Ord + AsRef<[u8]>>(
        &mut self,
        graph: &ColoredGraph<Color>,
        vertex: usize,
    ) -> Result<(), CanonicalGraphError> {
        let arcs = graph.adjacent(vertex)?;
        let start = self.signatures.len();
        self.signatures
            .try_reserve(arcs.len())
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        for arc in arcs {
            self.signatures.push(RefinementAtomV1 {
                direction: arc.direction,
                role: arc.role,
                neighbour_cell: self.cells[self.cell_of[arc.neighbour]].rank,
            });
        }
        let signature = start..self.signatures.len();
        ordering::sort(&mut self.signatures[signature.clone()]);
        self.records.push(RefinementRecordV1 { vertex, signature });
        Ok(())
    }

    fn group_end(&self, start: usize) -> usize {
        let mut end = start + 1;
        while end < self.records.len()
            && self.signatures[self.records[start].signature.clone()]
                == self.signatures[self.records[end].signature.clone()]
        {
            end += 1;
        }
        end
    }

    fn group_has_clean(&self, start: usize, end: usize, clean: Option<usize>) -> bool {
        self.records[start..end]
            .iter()
            .any(|record| Some(record.vertex) == clean)
    }

    fn refine_cell<Color: Ord + AsRef<[u8]>>(
        &mut self,
        graph: &ColoredGraph<Color>,
        cell: usize,
        #[cfg(test)] trace: Option<&mut RefinementTraceV1>,
    ) -> Result<(), CanonicalGraphError> {
        let end_dirty = self.cell_of.len();
        let mut vertex = self.cells[cell].dirty_head;
        let mut dirty_count = 0;
        while vertex != end_dirty {
            dirty_count += 1;
            vertex = self.dirty_next[vertex];
        }
        let clean_count = self.cells[cell].vertices.len() - dirty_count;
        self.records.clear();
        self.signatures.clear();
        self.records
            .try_reserve_exact(dirty_count + usize::from(clean_count != 0))
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        vertex = self.cells[cell].dirty_head;
        self.cells[cell].dirty_head = end_dirty;
        while vertex != end_dirty {
            let next = self.dirty_next[vertex];
            self.dirty_next[vertex] = usize::MAX;
            let offset = self.position[vertex];
            self.cells[cell].vertices.swap_remove(offset);
            if let Some(&moved) = self.cells[cell].vertices.get(offset) {
                self.position[moved] = offset;
            }
            self.record(graph, vertex)?;
            vertex = next;
        }
        let clean = self.cells[cell].vertices.first().copied();
        if let Some(vertex) = clean {
            self.record(graph, vertex)?;
        }
        #[cfg(test)]
        if let Some(trace) = trace {
            trace.built_signatures += self.records.len();
            trace.shared_signatures += clean_count.saturating_sub(1);
        }
        ordering::sort_by(&mut self.records, |left, right| {
            self.signatures[left.signature.clone()].cmp(&self.signatures[right.signature.clone()])
        });

        if self.group_end(0) == self.records.len() {
            // Равные ключи не разделяют клетку. Возвращаем извлечённые dirty
            // вершины в её прежний буфер, не восстанавливая случайный tie order.
            for record in &self.records {
                if Some(record.vertex) != clean {
                    self.position[record.vertex] = self.cells[cell].vertices.len();
                    self.cells[cell].vertices.push(record.vertex);
                }
            }
            return Ok(());
        }

        // Сначала найдём наибольшую группу. Её вершины сохранят stable cell ID;
        // при равном размере представитель — первая группа по signature.
        let mut largest_start = 0;
        let mut largest_len = 0;
        let mut start = 0;
        while start < self.records.len() {
            let end = self.group_end(start);
            let len = end - start
                + if self.group_has_clean(start, end, clean) {
                    clean_count - 1
                } else {
                    0
                };
            if len > largest_len {
                largest_start = start;
                largest_len = len;
            }
            start = end;
        }

        let mut old_vertices = Some(core::mem::take(&mut self.cells[cell].vertices));
        let mut rank = self.cells[cell].rank;
        start = 0;
        while start < self.records.len() {
            let end = self.group_end(start);
            let has_clean = self.group_has_clean(start, end, clean);
            let len = end - start + if has_clean { clean_count - 1 } else { 0 };
            let mut vertices = if has_clean || (clean.is_none() && start == largest_start) {
                old_vertices
                    .take()
                    .ok_or(CanonicalGraphError::InternalInvariant)?
            } else {
                Vec::new()
            };
            // Долгая цепочка split не удерживает историческую capacity:
            // каждый живой буфер занимает не более двух длин своей клетки.
            if vertices.capacity().saturating_sub(len) > len {
                vertices = copy_vertices(&vertices)?;
            }
            vertices
                .try_reserve_exact(len - vertices.len())
                .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
            for record in &self.records[start..end] {
                if Some(record.vertex) != clean {
                    self.position[record.vertex] = vertices.len();
                    vertices.push(record.vertex);
                }
            }
            if start == largest_start {
                self.cells[cell].vertices = vertices;
                // Старый ID остаётся внутри прежнего непересекающегося
                // интервала. Даже частичная замена рангов старых IDs строго
                // монотонна, поэтому не меняет сравнения signatures других
                // клеток этого раунда. Их vertex -> ID пока остаётся прежним.
                self.cells[cell].rank = rank;
            } else {
                self.cells
                    .try_reserve(1)
                    .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
                self.cells.push(ActiveRefinementCellV1 {
                    vertices,
                    rank,
                    dirty_head: end_dirty,
                });
            }
            rank += len;
            start = end;
        }
        Ok(())
    }

    fn commit_round<Color: Ord + AsRef<[u8]>>(
        &mut self,
        graph: &ColoredGraph<Color>,
        first_new_cell: usize,
    ) -> Result<(), CanonicalGraphError> {
        // До этой границы ВСЕ signatures читали старый cell_of. Ранги старых
        // IDs могли лишь монотонно сдвинуться внутри своих прежних интервалов.
        // Новые IDs принадлежат только меньшим дочерним клеткам. ID наибольшей
        // наследуется, даже если её interval начинается в середине parent.
        for cell in first_new_cell..self.cells.len() {
            for &vertex in &self.cells[cell].vertices {
                self.cell_of[vertex] = cell;
            }
        }
        for cell in first_new_cell..self.cells.len() {
            for offset in 0..self.cells[cell].vertices.len() {
                let vertex = self.cells[cell].vertices[offset];
                self.mark_neighbours_dirty(graph, vertex)?;
            }
        }
        Ok(())
    }

    fn mark_neighbours_dirty<Color: Ord + AsRef<[u8]>>(
        &mut self,
        graph: &ColoredGraph<Color>,
        vertex: usize,
    ) -> Result<(), CanonicalGraphError> {
        for arc in graph.adjacent(vertex)? {
            let neighbour = arc.neighbour;
            let target = self.cell_of[neighbour];
            if self.cells[target].vertices.len() == 1 || self.dirty_next[neighbour] != usize::MAX {
                continue;
            }
            if self.cells[target].dirty_head == self.cell_of.len() {
                self.active
                    .try_reserve(1)
                    .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
                self.active.push(target);
            }
            self.dirty_next[neighbour] = self.cells[target].dirty_head;
            self.cells[target].dirty_head = neighbour;
        }
        Ok(())
    }

    #[cfg(test)]
    fn snapshot(&self) -> Vec<Vec<usize>> {
        let mut ordered = self.cells.iter().collect::<Vec<_>>();
        ordered.sort_unstable_by_key(|cell| cell.rank);
        ordered
            .into_iter()
            .map(|cell| cell.vertices.clone())
            .collect()
    }

    fn finish(mut self) -> Result<EquitablePartitionV1, CanonicalGraphError> {
        let mut cells = core::mem::take(&mut self.cells);
        let mut rank_to_cell = core::mem::take(&mut self.cell_of);
        drop(self);
        // Ранги — начала непересекающихся интервалов, покрывающих все вершины.
        // Карта vertex -> cell больше не нужна: её позиции в началах интервалов
        // связывают rank -> cell. Переход на длину клетки посещает только эти
        // позиции и восстанавливает прежний порядок за один шаг на клетку.
        for (index, cell) in cells.iter().enumerate() {
            *rank_to_cell
                .get_mut(cell.rank)
                .ok_or(CanonicalGraphError::InternalInvariant)? = index;
        }
        let mut ordered = Vec::new();
        ordered
            .try_reserve_exact(cells.len())
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        let mut rank = 0;
        for _ in 0..cells.len() {
            let index = *rank_to_cell
                .get(rank)
                .ok_or(CanonicalGraphError::InternalInvariant)?;
            let cell = cells
                .get_mut(index)
                .ok_or(CanonicalGraphError::InternalInvariant)?;
            if cell.rank != rank || cell.vertices.is_empty() {
                return Err(CanonicalGraphError::InternalInvariant);
            }
            rank = rank
                .checked_add(cell.vertices.len())
                .ok_or(CanonicalGraphError::InternalInvariant)?;
            ordered.push(core::mem::take(&mut cell.vertices));
        }
        if rank != rank_to_cell.len() {
            return Err(CanonicalGraphError::InternalInvariant);
        }
        Ok(EquitablePartitionV1(PartitionV1 { cells: ordered }))
    }
}

fn refine_partition_seeded_impl<Color: Ord + AsRef<[u8]>>(
    graph: &ColoredGraph<Color>,
    partition: PartitionV1,
    individualized: Option<usize>,
    #[cfg(test)] mut trace: Option<&mut RefinementTraceV1>,
) -> Result<EquitablePartitionV1, CanonicalGraphError> {
    if partition.is_discrete() {
        return Ok(EquitablePartitionV1(partition));
    }
    let mut state =
        ActiveRefinementV1::new(partition, graph.colors.len(), individualized.is_none())?;
    if let Some(vertex) = individualized {
        state.mark_neighbours_dirty(graph, vertex)?;
    }
    let mut current = Vec::new();
    loop {
        core::mem::swap(&mut current, &mut state.active);
        let previous_count = state.cells.len();
        #[cfg(test)]
        if let Some(trace) = trace.as_deref_mut() {
            trace.processed_cells += current.len();
            trace.skipped_cells += state
                .cells
                .iter()
                .filter(|cell| cell.vertices.len() > 1)
                .count()
                - current.len();
        }
        for &cell in &current {
            state.refine_cell(
                graph,
                cell,
                #[cfg(test)]
                trace.as_deref_mut(),
            )?;
        }
        current.clear();
        #[cfg(test)]
        if let Some(trace) = trace.as_deref_mut() {
            trace.moved_vertices += state.cells[previous_count..]
                .iter()
                .map(|cell| cell.vertices.len())
                .sum::<usize>();
        }
        state.commit_round(graph, previous_count)?;
        #[cfg(test)]
        if let Some(trace) = trace.as_deref_mut() {
            trace.rounds.push(state.snapshot());
        }
        if state.cells.len() == previous_count || state.cells.len() == graph.colors.len() {
            return state.finish();
        }
        // Пустая очередь представляет точный следующий no-split round:
        // неактивные клетки уже однородны после монотонной замены рангов.
        // Этот раунд не требует обхода графа или его клеток в production.
        if state.active.is_empty() {
            #[cfg(test)]
            if let Some(trace) = trace.as_deref_mut() {
                trace.skipped_cells += state
                    .cells
                    .iter()
                    .filter(|cell| cell.vertices.len() > 1)
                    .count();
                trace.rounds.push(state.snapshot());
            }
            return state.finish();
        }
    }
}

fn individualize(
    partition: &PartitionV1,
    cell_index: usize,
    vertex: usize,
) -> Result<PartitionV1, CanonicalGraphError> {
    let selected = partition
        .cells
        .get(cell_index)
        .ok_or(CanonicalGraphError::InternalInvariant)?;
    if selected.len() < 2 || !selected.contains(&vertex) {
        return Err(CanonicalGraphError::InternalInvariant);
    }

    let mut cells = Vec::new();
    cells
        .try_reserve_exact(
            partition
                .cells
                .len()
                .checked_add(1)
                .ok_or(CanonicalGraphError::ResourceExhausted)?,
        )
        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
    for (index, cell) in partition.cells.iter().enumerate() {
        if index != cell_index {
            cells.push(copy_vertices(cell)?);
            continue;
        }
        let mut singleton = Vec::new();
        singleton
            .try_reserve_exact(1)
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        singleton.push(vertex);
        cells.push(singleton);

        let mut remainder = Vec::new();
        remainder
            .try_reserve_exact(cell.len() - 1)
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        remainder.extend(
            cell.iter()
                .copied()
                .filter(|candidate| *candidate != vertex),
        );
        cells.push(remainder);
    }
    Ok(PartitionV1 { cells })
}

fn refine_individualization<Color: Ord + AsRef<[u8]>>(
    graph: &ColoredGraph<Color>,
    parent: &EquitablePartitionV1,
    cell: usize,
    vertex: usize,
) -> Result<EquitablePartitionV1, CanonicalGraphError> {
    // Parent уже equitable. Выделение вершины меняет signatures только у её
    // соседей; остальные видят монотонное переименование прежних cell ranks.
    // Clean-представитель каждой затронутой клетки сохраняет полный порядок V9.
    refine_partition_seeded_impl(
        graph,
        individualize(&parent.0, cell, vertex)?,
        Some(vertex),
        #[cfg(test)]
        None,
    )
}

struct SearchFrameV1 {
    partition: EquitablePartitionV1,
    branch_cell: Option<usize>,
    explored_candidates: Vec<usize>,
    component_twins: Option<ComponentTwinIndexV1>,
    explored_orbits: Option<ExploredOrbitIndexV1>,
    next_candidate: usize,
    // Порядок минимального листа завершённых дочерних поддеревьев.
    // Полные сериализации живут только во время сравнения: память минимумов
    // O(depth * V + V + E), включая плотные графы.
    best: Option<Vec<usize>>,
}

impl SearchFrameV1 {
    fn new(
        mut partition: EquitablePartitionV1,
        component_twins: Option<ComponentTwinIndexV1>,
    ) -> Self {
        let branch_cell = partition.0.first_non_singleton();
        if let Some(index) = branch_cell {
            // Frame владеет клеткой. Её порядок задаёт очередь кандидатов;
            // отдельный буфер той же последовательности не нужен.
            ordering::sort(&mut partition.0.cells[index]);
        }
        Self {
            best: None,
            partition,
            branch_cell,
            explored_candidates: Vec::new(),
            component_twins,
            explored_orbits: None,
            next_candidate: 0,
        }
    }
}

fn push_u64_bytes(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn usize_as_u64(value: usize) -> Result<u64, CanonicalGraphError> {
    u64::try_from(value).map_err(|_| CanonicalGraphError::ResourceExhausted)
}

struct SerializedLeafV1 {
    preimage: Vec<u8>,
    order: Vec<usize>,
}

fn serialize_leaf<Color: Ord + AsRef<[u8]>>(
    graph: &ColoredGraph<Color>,
    partition: &PartitionV1,
    domain: &[u8],
) -> Result<SerializedLeafV1, CanonicalGraphError> {
    if !partition.is_discrete() || partition.cells.len() != graph.colors.len() {
        return Err(CanonicalGraphError::InternalInvariant);
    }
    let mut order = Vec::new();
    order
        .try_reserve_exact(graph.colors.len())
        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
    for cell in &partition.cells {
        let [vertex] = cell.as_slice() else {
            return Err(CanonicalGraphError::InternalInvariant);
        };
        order.push(*vertex);
    }
    let preimage = serialize_order(graph, &order, domain)?;
    Ok(SerializedLeafV1 { preimage, order })
}

fn serialize_order<Color: Ord + AsRef<[u8]>>(
    graph: &ColoredGraph<Color>,
    order: &[usize],
    domain: &[u8],
) -> Result<Vec<u8>, CanonicalGraphError> {
    if order.len() != graph.colors.len() {
        return Err(CanonicalGraphError::InternalInvariant);
    }
    let mut label_of = Vec::new();
    label_of
        .try_reserve_exact(graph.colors.len())
        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
    label_of.resize(graph.colors.len(), usize::MAX);
    for (label, &vertex) in order.iter().enumerate() {
        let slot = label_of
            .get_mut(vertex)
            .ok_or(CanonicalGraphError::InternalInvariant)?;
        if *slot != usize::MAX {
            return Err(CanonicalGraphError::InternalInvariant);
        }
        *slot = label;
    }
    let edge_bytes = graph
        .edge_count
        .checked_mul(9)
        .ok_or(CanonicalGraphError::ResourceExhausted)?;
    let color_bytes = graph.colors.iter().try_fold(0_usize, |total, color| {
        total
            .checked_add(16)
            .and_then(|value| value.checked_add(color.as_ref().len()))
            .ok_or(CanonicalGraphError::ResourceExhausted)
    })?;
    let capacity = domain
        .len()
        .checked_add(16)
        .and_then(|value| value.checked_add(color_bytes))
        .and_then(|value| value.checked_add(edge_bytes))
        .ok_or(CanonicalGraphError::ResourceExhausted)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(capacity)
        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
    output.extend_from_slice(domain);
    push_u64_bytes(&mut output, usize_as_u64(graph.colors.len())?);
    push_u64_bytes(&mut output, usize_as_u64(graph.edge_count)?);

    let mut outgoing = Vec::new();
    for &vertex in order {
        let color = graph.colors[vertex].as_ref();
        push_u64_bytes(&mut output, usize_as_u64(color.len())?);
        output.extend_from_slice(color);

        let adjacent = graph.adjacent(vertex)?;
        let outgoing_count = adjacent.iter().filter(|arc| arc.direction == 0).count();
        push_u64_bytes(&mut output, usize_as_u64(outgoing_count)?);
        outgoing.clear();
        outgoing
            .try_reserve(outgoing_count)
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        outgoing.extend(
            adjacent
                .iter()
                .filter(|arc| arc.direction == 0)
                .map(|arc| (arc.role, label_of[arc.neighbour])),
        );
        ordering::sort(&mut outgoing);
        for &(role, target) in &outgoing {
            output.push(role);
            push_u64_bytes(&mut output, usize_as_u64(target)?);
        }
    }
    Ok(output)
}

fn equal_leaf_automorphism(
    canonical_order: &[usize],
    equal_order: &[usize],
) -> Result<Vec<usize>, CanonicalGraphError> {
    if canonical_order.len() != equal_order.len() {
        return Err(CanonicalGraphError::InternalInvariant);
    }
    let mut permutation = Vec::new();
    permutation
        .try_reserve_exact(canonical_order.len())
        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
    permutation.resize(canonical_order.len(), usize::MAX);
    for (&from, &to) in canonical_order.iter().zip(equal_order) {
        let slot = permutation
            .get_mut(from)
            .ok_or(CanonicalGraphError::InternalInvariant)?;
        if *slot != usize::MAX || to >= canonical_order.len() {
            return Err(CanonicalGraphError::InternalInvariant);
        }
        *slot = to;
    }
    if permutation.contains(&usize::MAX) {
        return Err(CanonicalGraphError::InternalInvariant);
    }
    Ok(permutation)
}

fn automorphism_preserves_partition(
    permutation: &[usize],
    cell_of: &[usize],
) -> Result<bool, CanonicalGraphError> {
    if permutation.len() != cell_of.len() {
        return Err(CanonicalGraphError::InternalInvariant);
    }
    for (vertex, &image) in permutation.iter().enumerate() {
        let image_cell = cell_of
            .get(image)
            .ok_or(CanonicalGraphError::InternalInvariant)?;
        if cell_of[vertex] != *image_cell {
            return Ok(false);
        }
    }
    Ok(true)
}

#[derive(Clone, Copy, Default)]
struct OrbitRootV1 {
    rank: u8,
    explored: bool,
}

struct OrbitComponentsV1 {
    vertices: Vec<usize>,
    parents: Vec<usize>,
    roots: Vec<OrbitRootV1>,
    explored_count: usize,
}

impl OrbitComponentsV1 {
    fn new(cell_of: &[usize], branch_cell: usize) -> Result<Self, CanonicalGraphError> {
        let vertex_count = cell_of.iter().filter(|&&cell| cell == branch_cell).count();
        let mut vertices = Vec::new();
        vertices
            .try_reserve_exact(vertex_count)
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        vertices.extend(
            cell_of
                .iter()
                .enumerate()
                .filter_map(|(vertex, &cell)| (cell == branch_cell).then_some(vertex)),
        );
        let mut parents = Vec::new();
        parents
            .try_reserve_exact(vertex_count)
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        parents.extend(0..vertex_count);
        let mut roots = Vec::new();
        roots
            .try_reserve_exact(vertex_count)
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        roots.resize(vertex_count, OrbitRootV1::default());
        Ok(Self {
            vertices,
            parents,
            roots,
            explored_count: 0,
        })
    }

    fn root(&mut self, mut vertex: usize) -> usize {
        while self.parents[vertex] != vertex {
            let parent = self.parents[vertex];
            self.parents[vertex] = self.parents[parent];
            vertex = parent;
        }
        vertex
    }

    fn join(&mut self, left: usize, right: usize) {
        let mut left = self.root(left);
        let mut right = self.root(right);
        if left == right {
            return;
        }
        if self.roots[left].rank < self.roots[right].rank {
            core::mem::swap(&mut left, &mut right);
        }
        self.parents[right] = left;
        let explored = self.roots[right].explored;
        self.roots[left].explored |= explored;
        if self.roots[left].rank == self.roots[right].rank {
            // Ранг r требует хотя бы 2^r вершин, поэтому он меньше usize::BITS.
            self.roots[left].rank += 1;
        }
    }

    fn contains_explored(
        &mut self,
        explored: &[usize],
        candidate: usize,
    ) -> Result<bool, CanonicalGraphError> {
        let new_explored = explored
            .get(self.explored_count..)
            .ok_or(CanonicalGraphError::InternalInvariant)?;
        for &vertex in new_explored {
            let vertex = self
                .vertices
                .binary_search(&vertex)
                .map_err(|_| CanonicalGraphError::InternalInvariant)?;
            let root = self.root(vertex);
            self.roots[root].explored = true;
            self.explored_count += 1;
        }
        let candidate = self
            .vertices
            .binary_search(&candidate)
            .map_err(|_| CanonicalGraphError::InternalInvariant)?;
        let root = self.root(candidate);
        Ok(self.roots[root].explored)
    }
}

/// Индекс принадлежит неизменяемому разбиению одного search frame. Глобальный
/// список доказанных автоморфизмов и список исследованных ветвей только растут.
/// Каждый новый автоморфизм проверяется здесь один раз; неподходящий не меняет
/// орбиты. Буферы компонент появляются лишь при первом stabilizer-е.
struct ExploredOrbitIndexV1 {
    vertex_count: usize,
    branch_cell: usize,
    automorphism_count: usize,
    components: Option<OrbitComponentsV1>,
}

impl ExploredOrbitIndexV1 {
    fn new(
        partition: &PartitionV1,
        vertex_count: usize,
        branch_cell: usize,
    ) -> Result<Self, CanonicalGraphError> {
        if branch_cell >= partition.cells.len() {
            return Err(CanonicalGraphError::InternalInvariant);
        }
        Ok(Self {
            vertex_count,
            branch_cell,
            automorphism_count: 0,
            components: None,
        })
    }

    fn contains(
        &mut self,
        partition: &PartitionV1,
        explored: &[usize],
        candidate: usize,
        automorphisms: &[Vec<usize>],
        cell_of: &mut Vec<usize>,
    ) -> Result<bool, CanonicalGraphError> {
        if candidate >= self.vertex_count {
            return Err(CanonicalGraphError::InternalInvariant);
        }
        let new_automorphisms = automorphisms
            .get(self.automorphism_count..)
            .ok_or(CanonicalGraphError::InternalInvariant)?;
        if !new_automorphisms.is_empty() {
            // Scratch общий для всего поиска: после работы другого frame
            // восстанавливаем его только при поступлении новых генераторов.
            partition_cells_into(partition, self.vertex_count, cell_of)?;
            if cell_of[candidate] != self.branch_cell {
                return Err(CanonicalGraphError::InternalInvariant);
            }
        }
        for permutation in new_automorphisms {
            if automorphism_preserves_partition(permutation, cell_of)? {
                if self.components.is_none() {
                    self.components = Some(OrbitComponentsV1::new(cell_of, self.branch_cell)?);
                }
                let components = self
                    .components
                    .as_mut()
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                // У конечной перестановки p обратное действие равно p^(k-1)
                // для некоторого k. Поэтому неориентированные компоненты
                // рёбер v--p(v) точно совпадают с достижимостью прежнего BFS
                // по всем сохраняющим разбиение генераторам, включая их слова.
                // Stabilizer не выводит вершины из branch cell. Остальные
                // компоненты не участвуют в отсечении этого frame и не хранятся.
                for vertex in 0..components.vertices.len() {
                    let image = permutation[components.vertices[vertex]];
                    let image = components
                        .vertices
                        .binary_search(&image)
                        .map_err(|_| CanonicalGraphError::InternalInvariant)?;
                    components.join(vertex, image);
                }
            }
            self.automorphism_count += 1;
        }
        match self.components.as_mut() {
            Some(components) => components.contains_explored(explored, candidate),
            None => Ok(false),
        }
    }
}

/// Орбиты строятся только из отображений, доказанных совпадением полных
/// сериализаций листьев. Эквивалентность сама по себе не предполагается из
/// равенства refinement-сигнатур или принадлежности одной клетке.
fn candidate_is_in_explored_orbit(
    partition: &PartitionV1,
    branch_cell: usize,
    explored: &[usize],
    candidate: usize,
    automorphisms: &[Vec<usize>],
    index: &mut Option<ExploredOrbitIndexV1>,
    cell_of: &mut Vec<usize>,
) -> Result<bool, CanonicalGraphError> {
    if explored.is_empty() || automorphisms.is_empty() {
        return Ok(false);
    }
    if index.is_none() {
        *index = Some(ExploredOrbitIndexV1::new(
            partition,
            automorphisms[0].len(),
            branch_cell,
        )?);
    }
    index
        .as_mut()
        .ok_or(CanonicalGraphError::InternalInvariant)?
        .contains(partition, explored, candidate, automorphisms, cell_of)
}

/// Для каждой вершины хранит клетку, если её компонента после удаления
/// одноэлементных клеток содержит не более одной вершины каждой клетки.
/// Индекс принадлежит одному неизменяемому equitable-разбиению search frame.
struct ComponentTwinIndexV1 {
    unique_cell_of: Vec<usize>,
}

impl ComponentTwinIndexV1 {
    fn new<Color: Ord + AsRef<[u8]>>(
        graph: &ColoredGraph<Color>,
        equitable: &EquitablePartitionV1,
    ) -> Result<Self, CanonicalGraphError> {
        let partition = &equitable.0;
        let mut cell_of = partition_cells(partition, graph.colors.len())?;
        let mut state = Vec::new();
        state
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        state.resize(graph.colors.len(), 0_u8);
        let mut seen_cell = Vec::new();
        seen_cell
            .try_reserve_exact(partition.cells.len())
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        seen_cell.resize(partition.cells.len(), usize::MAX);
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;

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
                for arc in graph.adjacent(vertex)? {
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
fn candidate_is_component_twin<Color: Ord + AsRef<[u8]>>(
    graph: &ColoredGraph<Color>,
    equitable: &EquitablePartitionV1,
    explored: &[usize],
    candidate: usize,
    component_twins: &mut Option<ComponentTwinIndexV1>,
) -> Result<bool, CanonicalGraphError> {
    if explored.is_empty() {
        return Ok(false);
    }
    let candidate_arcs = graph.adjacent(candidate)?;
    for &vertex in explored {
        if graph.colors[vertex] == graph.colors[candidate]
            && graph.adjacent(vertex)? == candidate_arcs
        {
            return Ok(true);
        }
    }
    if component_twins.is_none() {
        *component_twins = Some(ComponentTwinIndexV1::new(graph, equitable)?);
    }
    Ok(component_twins
        .as_ref()
        .ok_or(CanonicalGraphError::InternalInvariant)?
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
fn factor_component_twins<Color: Ord + AsRef<[u8]>>(
    graph: &ColoredGraph<Color>,
    mut equitable: EquitablePartitionV1,
    remaining_branch_expansions: &mut Option<usize>,
    bulk_groups: &mut usize,
) -> Result<(EquitablePartitionV1, Option<ComponentTwinIndexV1>), CanonicalGraphError> {
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
        ordering::sort(&mut equitable.0.cells[branch_cell]);
        let roots = &equitable.0.cells[branch_cell];
        let component_count = roots.len();
        if let Some(remaining) = remaining_branch_expansions {
            // Fault injection считает логические individualize, в том числе
            // выполненные пакетом; при исчерпании не возвращается часть прообраза.
            *remaining = remaining
                .checked_sub(component_count - 1)
                .ok_or(CanonicalGraphError::ResourceExhausted)?;
        }

        let mut component_of = Vec::new();
        component_of
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        component_of.resize(graph.colors.len(), usize::MAX);
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        for (ordinal, &root) in roots.iter().enumerate() {
            if component_of[root] != usize::MAX {
                return Err(CanonicalGraphError::InternalInvariant);
            }
            pending.clear();
            pending.push(root);
            component_of[root] = ordinal;
            let mut cursor = 0;
            while cursor < pending.len() {
                let vertex = pending[cursor];
                cursor += 1;
                for arc in graph.adjacent(vertex)? {
                    let neighbour = arc.neighbour;
                    if twins.unique_cell_of[neighbour] == usize::MAX {
                        continue;
                    }
                    let previous = component_of[neighbour];
                    if previous == usize::MAX {
                        component_of[neighbour] = ordinal;
                        pending.push(neighbour);
                    } else if previous != ordinal {
                        return Err(CanonicalGraphError::InternalInvariant);
                    }
                }
            }
        }

        let mut cells = Vec::new();
        cells
            .try_reserve_exact(graph.colors.len())
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
        let mut ordered = Vec::new();
        ordered
            .try_reserve_exact(component_count)
            .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
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
                return Err(CanonicalGraphError::InternalInvariant);
            }
            ordered.fill(usize::MAX);
            for vertex in cell {
                let slot = ordered
                    .get_mut(component_of[vertex])
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                if *slot != usize::MAX {
                    return Err(CanonicalGraphError::InternalInvariant);
                }
                *slot = vertex;
            }
            for &vertex in &ordered {
                if vertex == usize::MAX {
                    return Err(CanonicalGraphError::InternalInvariant);
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

fn canonical_search_impl<Color: Ord + AsRef<[u8]>>(
    graph: &ColoredGraph<Color>,
    mut remaining_branch_expansions: Option<usize>,
    domain: &[u8],
) -> Result<(Vec<u8>, usize, usize), CanonicalGraphError> {
    let initial = refine_partition(graph, PartitionV1::initial(graph)?)?;
    let mut bulk_groups = 0;
    let (initial, component_twins) = factor_component_twins(
        graph,
        initial,
        &mut remaining_branch_expansions,
        &mut bulk_groups,
    )?;
    let mut stack = Vec::new();
    // Глубину задаёт поиск после refinement, а не число вершин графа.
    // Каждый следующий push также предваряется fallible reserve.
    stack
        .try_reserve_exact(1)
        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
    stack.push(SearchFrameV1::new(initial, component_twins));
    let mut automorphisms: Vec<Vec<usize>> = Vec::new();
    let mut orbit_cell_of = Vec::new();
    let mut leaf_count = 0_usize;

    loop {
        let leaf = stack
            .last()
            .map(|frame| frame.branch_cell.is_none())
            .ok_or(CanonicalGraphError::InternalInvariant)?;
        let candidate = if leaf {
            let frame = stack.pop().ok_or(CanonicalGraphError::InternalInvariant)?;
            let candidate = serialize_leaf(graph, &frame.partition.0, domain)?;
            // Счётчик учитывает только сериализованные листья, не передачу
            // завершённого минимума через предков, и не участвует в admission.
            leaf_count = leaf_count.saturating_add(1);
            candidate
        } else {
            let child = {
                let frame = stack
                    .last_mut()
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                let cell = frame
                    .branch_cell
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                let candidates = &frame.partition.0.cells[cell];
                let mut selected = None;
                while frame.next_candidate < candidates.len() {
                    let candidate = candidates[frame.next_candidate];
                    frame.next_candidate += 1;
                    if candidate_is_component_twin(
                        graph,
                        &frame.partition,
                        &frame.explored_candidates,
                        candidate,
                        &mut frame.component_twins,
                    )? || candidate_is_in_explored_orbit(
                        &frame.partition.0,
                        cell,
                        &frame.explored_candidates,
                        candidate,
                        &automorphisms,
                        &mut frame.explored_orbits,
                        &mut orbit_cell_of,
                    )? {
                        continue;
                    }
                    frame
                        .explored_candidates
                        .try_reserve(1)
                        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
                    frame.explored_candidates.push(candidate);
                    selected = Some(candidate);
                    break;
                }
                match selected {
                    Some(candidate) => {
                        if let Some(remaining) = &mut remaining_branch_expansions {
                            *remaining = remaining
                                .checked_sub(1)
                                .ok_or(CanonicalGraphError::ResourceExhausted)?;
                        }
                        let partition =
                            refine_individualization(graph, &frame.partition, cell, candidate)?;
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
            if let Some((partition, component_twins)) = child {
                stack
                    .try_reserve(1)
                    .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
                stack.push(SearchFrameV1::new(partition, component_twins));
                continue;
            }
            let order = stack
                .pop()
                .ok_or(CanonicalGraphError::InternalInvariant)?
                .best
                .ok_or(CanonicalGraphError::InternalInvariant)?;
            let preimage = serialize_order(graph, &order, domain)?;
            SerializedLeafV1 { preimage, order }
        };
        let Some(parent) = stack.last_mut() else {
            return Ok((candidate.preimage, leaf_count, bulk_groups));
        };
        // Оба сравниваемых листа продолжают одно разбиение parent. При равных
        // полных сериализациях отображение сохраняет его клетки и выбранных
        // предков. Глобальная опорная ветвь для такого доказательства не нужна.
        let comparison = match parent.best.as_deref() {
            Some(current) => candidate
                .preimage
                .cmp(&serialize_order(graph, current, domain)?),
            None => core::cmp::Ordering::Less,
        };
        match comparison {
            core::cmp::Ordering::Less => parent.best = Some(candidate.order),
            core::cmp::Ordering::Equal => {
                let current = parent
                    .best
                    .as_deref()
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                let permutation = equal_leaf_automorphism(current, &candidate.order)?;
                #[cfg(test)]
                assert!(
                    automorphism_preserves_partition(
                        &permutation,
                        &partition_cells(&parent.partition.0, graph.colors.len())?,
                    )?,
                    "equal local minima must preserve their parent partition",
                );
                // Отсечение не требуется для корректности. Храним не более
                // V доказанных автоморфизмов: при длине V это ограничивает
                // память O(V²), а остальные ветви исследуются полностью.
                if permutation.iter().enumerate().any(|(from, to)| from != *to)
                    && automorphisms.len() < graph.colors.len()
                    && !automorphisms.contains(&permutation)
                {
                    automorphisms
                        .try_reserve(1)
                        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
                    automorphisms.push(permutation);
                }
            }
            core::cmp::Ordering::Greater => {}
        }
    }
}

fn canonical_search<Color: Ord + AsRef<[u8]>>(
    graph: &ColoredGraph<Color>,
    domain: &[u8],
) -> Result<(Vec<u8>, usize), CanonicalGraphError> {
    // Число шагов поиска не инвариантно к изоморфизму: после opaque-
    // переименования автоморфизм может обнаружиться другой ветвью. Поэтому
    // динамический лимит сделал бы допуск зависимым от client-owned ID. Точный
    // поиск возвращает только полный прообраз либо типизированный отказ.
    canonical_search_impl(graph, None, domain).map(|(preimage, leaves, _)| (preimage, leaves))
}

#[cfg(test)]
fn canonical_search_with_test_fuel<Color: Ord + AsRef<[u8]>>(
    graph: &ColoredGraph<Color>,
    test_fuel: usize,
    domain: &[u8],
) -> Result<(Vec<u8>, usize), CanonicalGraphError> {
    // Только тестовая инъекция отказа для проверки атомарности. Это не политика
    // допуска: история поиска не инвариантна к alpha-переименованию.
    canonical_search_impl(graph, Some(test_fuel), domain)
        .map(|(preimage, leaves, _)| (preimage, leaves))
}

pub(crate) fn canonical_preimage<Color: Ord + AsRef<[u8]>>(
    graph: &ColoredGraph<Color>,
    domain: &[u8],
) -> Result<Vec<u8>, CanonicalGraphError> {
    canonical_search(graph, domain).map(|(preimage, _)| preimage)
}

pub(crate) mod ordering {
    //! Сортировка без аллокаций для построения и канонизации цветного графа.
    //! Native использует специализацию стандартного алгоритма; WASM — один
    //! индексный цикл, чтобы ограничить размер загружаемого и компилируемого кода.
    use core::cmp::Ordering;

    pub(crate) fn sort<T: Ord>(values: &mut [T]) {
        sort_by(values, Ord::cmp);
    }

    pub(crate) fn sort_by_key<T, K: Ord>(values: &mut [T], mut key: impl FnMut(&T) -> K) {
        sort_by(values, |left, right| key(left).cmp(&key(right)));
    }

    pub(crate) fn sort_by<T>(values: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering) {
        // Индексы и однородные клетки часто уже упорядочены. Проверка использует
        // тот же полный порядок; сортировка нужна только при настоящей инверсии.
        if values.is_sorted_by(|left, right| !compare(left, right).is_gt()) {
            return;
        }
        #[cfg(not(target_family = "wasm"))]
        values.sort_unstable_by(compare);
        #[cfg(target_family = "wasm")]
        shared_sort_by(values, compare);
    }

    #[cfg(any(target_family = "wasm", test))]
    fn shared_sort_by<T>(values: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering) {
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
    #[cfg(any(target_family = "wasm", test))]
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

        fn test_sort_by<T>(
            shared: bool,
            values: &mut [T],
            compare: impl FnMut(&T, &T) -> Ordering,
        ) {
            if shared {
                shared_sort_by(values, compare);
            } else {
                sort_by(values, compare);
            }
        }

        #[test]
        fn all_small_multisets_match_the_standard_library() {
            for (shared, length) in [false, true]
                .into_iter()
                .flat_map(|shared| (0..=8).map(move |length| (shared, length)))
            {
                for mut encoded in 0..3usize.pow(length) {
                    let mut value = Vec::new();
                    for _ in 0..length {
                        value.push(encoded % 3);
                        encoded /= 3;
                    }
                    let mut expected = value.clone();
                    expected.sort_unstable();
                    test_sort_by(shared, &mut value, Ord::cmp);
                    assert_eq!(value, expected);
                    test_sort_by(shared, &mut value, |a, b| b.cmp(a));
                    expected.reverse();
                    assert_eq!(value, expected);
                }
            }
        }

        #[test]
        fn ordered_inputs_need_at_most_one_comparison_per_adjacent_pair() {
            for length in [0_usize, 1, 2, 17, 257, 4096] {
                for repeats in [1, 4, usize::MAX] {
                    for descending in [false, true] {
                        let mut values: Vec<_> = (0..length).map(|index| index / repeats).collect();
                        if descending {
                            values.reverse();
                        }
                        let expected = values.clone();
                        let mut comparisons = 0;
                        let (_, events) = crate::test_support::measured_allocator_events(|| {
                            sort_by(&mut values, |left, right| {
                                comparisons += 1;
                                if descending {
                                    right.cmp(left)
                                } else {
                                    left.cmp(right)
                                }
                            });
                        });
                        assert_eq!(values, expected);
                        assert_eq!(events.alloc + events.realloc + events.dealloc, 0);
                        assert!(
                            comparisons <= length.saturating_sub(1),
                            "ordered length {length}, repeats {repeats}, descending {descending}: {comparisons} comparisons",
                        );
                    }
                }
            }
        }

        #[test]
        fn array_shapes_have_bounded_comparisons_and_no_allocator_events() {
            for (shared, shape) in [false, true]
                .into_iter()
                .flat_map(|shared| (0..5).map(move |shape| (shared, shape)))
            {
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
                    test_sort_by(shared, &mut values, |a, b| {
                        comparisons += 1;
                        a.cmp(b)
                    });
                });
                assert_eq!(values, expected);
                assert_eq!(events.alloc + events.realloc + events.dealloc, 0);
                assert!(
                    comparisons <= 4 * values.len() * 12,
                    "shared {shared}, comparison bound {comparisons}"
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
            for (shared, stop) in [false, true]
                .into_iter()
                .flat_map(|shared| (0..100).map(move |stop| (shared, stop)))
            {
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
                    test_sort_by(shared, &mut records, |a, b| {
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

    use proptest::prelude::*;

    type CanonicalGraphV1 = ColoredGraph<[u8; 1]>;
    const TEST_DOMAIN: &[u8] = b"colored-graph-test\0";
    const TEST_EDGE_ROLES: &[u8] = &[0, 1, 2, 12, 13, 127, 255];

    fn canonical_preimage(graph: &CanonicalGraphV1) -> Result<Vec<u8>, CanonicalGraphError> {
        super::canonical_preimage(graph, TEST_DOMAIN)
    }

    fn canonical_search(graph: &CanonicalGraphV1) -> Result<(Vec<u8>, usize), CanonicalGraphError> {
        super::canonical_search(graph, TEST_DOMAIN)
    }

    fn canonical_search_impl(
        graph: &CanonicalGraphV1,
        fuel: Option<usize>,
    ) -> Result<(Vec<u8>, usize, usize), CanonicalGraphError> {
        super::canonical_search_impl(graph, fuel, TEST_DOMAIN)
    }

    fn canonical_search_with_test_fuel(
        graph: &CanonicalGraphV1,
        fuel: usize,
    ) -> Result<(Vec<u8>, usize), CanonicalGraphError> {
        super::canonical_search_with_test_fuel(graph, fuel, TEST_DOMAIN)
    }

    fn serialize_leaf(
        graph: &CanonicalGraphV1,
        partition: &PartitionV1,
    ) -> Result<SerializedLeafV1, CanonicalGraphError> {
        super::serialize_leaf(graph, partition, TEST_DOMAIN)
    }

    struct GraphBuilderV1 {
        colors: Vec<[u8; 1]>,
        edges: Vec<EdgeV1>,
        root: usize,
    }

    impl GraphBuilderV1 {
        fn new(root: [u8; 1]) -> Result<Self, CanonicalGraphError> {
            let mut colors = Vec::new();
            colors
                .try_reserve_exact(1)
                .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
            colors.push(root);
            Ok(Self {
                colors,
                edges: Vec::new(),
                root: 0,
            })
        }

        fn add_member(&mut self, color: [u8; 1]) -> Result<usize, CanonicalGraphError> {
            self.colors
                .try_reserve(1)
                .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
            let index = self.colors.len();
            self.colors.push(color);
            self.add_edge(self.root, index, 1)?;
            Ok(index)
        }

        fn add_edge(
            &mut self,
            from: usize,
            to: usize,
            role: u8,
        ) -> Result<(), CanonicalGraphError> {
            if from >= self.colors.len() || to >= self.colors.len() {
                return Err(CanonicalGraphError::InternalInvariant);
            }
            self.edges
                .try_reserve(1)
                .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
            self.edges.push(EdgeV1 { from, to, role });
            Ok(())
        }

        fn finish(self) -> Result<CanonicalGraphV1, CanonicalGraphError> {
            ColoredGraph::new(self.colors, self.edges)
        }
    }

    #[test]
    fn graph_adjacency_uses_constant_number_of_buffers() -> Result<(), CanonicalGraphError> {
        for members in [0, 1, 17, 4096] {
            let mut builder = GraphBuilderV1::new([1])?;
            for _ in 0..members {
                builder.add_member([7])?;
            }
            let (graph, events) =
                crate::test_support::measured_allocator_events(|| builder.finish());
            let graph = graph?;
            assert_eq!(graph.edge_count, members);
            assert_eq!(graph.adjacent(0)?.len(), members);
            assert!(
                events.alloc <= 2,
                "{members} members allocated {} adjacency buffers",
                events.alloc,
            );
            assert_eq!(events.realloc, 0);
        }
        Ok(())
    }

    #[test]
    fn flat_adjacency_matches_independent_lists_with_loops_and_parallel_edges()
    -> Result<(), CanonicalGraphError> {
        for members in [0, 1, 2, 17, 257] {
            for reverse in [false, true] {
                let mut builder = GraphBuilderV1::new([1])?;
                for _ in 0..members {
                    builder.add_member([7])?;
                }
                let vertex_count = builder.colors.len();
                for from in 0..vertex_count {
                    builder.add_edge(from, from, 13)?;
                    for (ordinal, &role) in TEST_EDGE_ROLES.iter().enumerate() {
                        let to = (from * 17 + ordinal * 13) % vertex_count;
                        builder.add_edge(from, to, role)?;
                        if ordinal % 3 == 0 {
                            builder.add_edge(from, to, role)?;
                        }
                    }
                }
                if reverse {
                    builder.edges.reverse();
                }
                let mut expected = vec![Vec::new(); vertex_count];
                for edge in &builder.edges {
                    expected[edge.from].push(ArcV1 {
                        direction: 0,
                        role: edge.role,
                        neighbour: edge.to,
                    });
                    expected[edge.to].push(ArcV1 {
                        direction: 1,
                        role: edge.role,
                        neighbour: edge.from,
                    });
                }
                for adjacent in &mut expected {
                    adjacent.sort_unstable();
                }
                let graph = builder.finish()?;
                assert_eq!(graph.adjacency_offsets.len(), vertex_count + 1);
                assert_eq!(graph.arcs.len(), graph.edge_count * 2);
                for (vertex, adjacent) in expected.iter().enumerate() {
                    assert_eq!(graph.adjacent(vertex)?, adjacent);
                }
                for vertex in [vertex_count, usize::MAX] {
                    assert_eq!(
                        graph.adjacent(vertex).err(),
                        Some(CanonicalGraphError::InternalInvariant),
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn flat_adjacency_rejects_dangling_private_edges() -> Result<(), CanonicalGraphError> {
        for vertex in [1, usize::MAX] {
            for incoming in [false, true] {
                let mut builder = GraphBuilderV1::new([1])?;
                builder.edges.push(EdgeV1 {
                    from: if incoming { 0 } else { vertex },
                    to: if incoming { vertex } else { 0 },
                    role: 13,
                });
                assert_eq!(
                    builder.finish().err(),
                    Some(CanonicalGraphError::InternalInvariant),
                );
            }
        }
        Ok(())
    }

    // Независимый алгоритм прежнего пути: направленная достижимость по
    // сохраняющим разбиение перестановкам, без объединения компонент.
    fn breadth_first_orbit_reference(
        partition: &PartitionV1,
        explored: &[usize],
        candidate: usize,
        automorphisms: &[Vec<usize>],
    ) -> Result<bool, CanonicalGraphError> {
        if explored.is_empty() || automorphisms.is_empty() {
            return Ok(false);
        }
        let vertex_count = automorphisms[0].len();
        let cell_of = partition_cells(partition, vertex_count)?;
        if candidate >= vertex_count {
            return Err(CanonicalGraphError::InternalInvariant);
        }
        let mut stabilizers = Vec::new();
        for (index, permutation) in automorphisms.iter().enumerate() {
            if automorphism_preserves_partition(permutation, &cell_of)? {
                stabilizers.push(index);
            }
        }
        if stabilizers.is_empty() {
            return Ok(false);
        }
        let mut seen = vec![false; vertex_count];
        let mut queue = Vec::new();
        for &representative in explored {
            let slot = seen
                .get_mut(representative)
                .ok_or(CanonicalGraphError::InternalInvariant)?;
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
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                if !*slot {
                    *slot = true;
                    queue.push(image);
                }
            }
        }
        Ok(seen[candidate])
    }

    fn all_small_partitions(vertex_count: usize) -> Vec<PartitionV1> {
        fn visit(
            vertex: usize,
            vertex_count: usize,
            cells: &mut Vec<Vec<usize>>,
            partitions: &mut Vec<PartitionV1>,
        ) {
            if vertex == vertex_count {
                partitions.push(PartitionV1 {
                    cells: cells.clone(),
                });
                return;
            }
            for cell in 0..cells.len() {
                cells[cell].push(vertex);
                visit(vertex + 1, vertex_count, cells, partitions);
                cells[cell].pop();
            }
            cells.push(vec![vertex]);
            visit(vertex + 1, vertex_count, cells, partitions);
            cells.pop();
        }
        let mut partitions = Vec::new();
        visit(0, vertex_count, &mut Vec::new(), &mut partitions);
        partitions
    }

    fn all_small_permutations(vertex_count: usize) -> Vec<Vec<usize>> {
        fn visit(cursor: usize, permutation: &mut [usize], output: &mut Vec<Vec<usize>>) {
            if cursor == permutation.len() {
                output.push(permutation.to_vec());
                return;
            }
            for next in cursor..permutation.len() {
                permutation.swap(cursor, next);
                visit(cursor + 1, permutation, output);
                permutation.swap(cursor, next);
            }
        }
        let mut output = Vec::new();
        visit(0, &mut (0..vertex_count).collect::<Vec<_>>(), &mut output);
        output
    }

    #[test]
    fn growing_orbits_match_bfs_for_every_four_vertex_partition_and_generator_pair()
    -> Result<(), CanonicalGraphError> {
        let permutations = all_small_permutations(4);
        for partition in all_small_partitions(4) {
            for (branch_cell, vertices) in partition.cells.iter().enumerate() {
                for first in &permutations {
                    for second in &permutations {
                        let generators = [first.clone(), second.clone()];
                        for rotation in 0..vertices.len() {
                            let mut explored = vertices.clone();
                            explored.rotate_left(rotation);
                            // Чередование роста generators/explored ловит
                            // потерю уже отмеченной орбиты при новом объединении.
                            for split in 1..=explored.len() {
                                let mut index = None;
                                let mut cell_of = Vec::new();
                                for (generator_count, explored_count) in
                                    [(0, 0), (1, 1), (1, split), (2, split), (2, explored.len())]
                                {
                                    for &candidate in vertices {
                                        assert_eq!(
                                            candidate_is_in_explored_orbit(
                                                &partition,
                                                branch_cell,
                                                &explored[..explored_count],
                                                candidate,
                                                &generators[..generator_count],
                                                &mut index,
                                                &mut cell_of,
                                            )?,
                                            breadth_first_orbit_reference(
                                                &partition,
                                                &explored[..explored_count],
                                                candidate,
                                                &generators[..generator_count],
                                            )?,
                                            "cells {:?}, generators {:?}, explored {:?}, candidate {candidate}",
                                            partition.cells,
                                            &generators[..generator_count],
                                            &explored[..explored_count],
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn non_preserving_generators_do_not_form_a_hidden_stabilizer() -> Result<(), CanonicalGraphError>
    {
        let partition = PartitionV1 {
            cells: vec![vec![0, 1], vec![2, 3]],
        };
        // Их композиция сохраняет клетку {0,1}, но прежний алгоритм допускает
        // только генераторы, каждый из которых сам сохраняет всё разбиение.
        let generators = [vec![2, 1, 0, 3], vec![1, 2, 0, 3], vec![1, 0, 2, 3]];
        let mut index = None;
        let mut cell_of = Vec::new();
        assert!(!candidate_is_in_explored_orbit(
            &partition,
            0,
            &[0],
            1,
            &generators[..2],
            &mut index,
            &mut cell_of,
        )?);
        assert!(candidate_is_in_explored_orbit(
            &partition,
            0,
            &[0],
            1,
            &generators,
            &mut index,
            &mut cell_of,
        )?);
        Ok(())
    }

    #[test]
    fn merging_orbits_retains_explored_members_and_queries_do_not_allocate()
    -> Result<(), CanonicalGraphError> {
        let partition = PartitionV1 {
            cells: vec![(0..6).collect()],
        };
        let generators = [
            vec![1, 0, 2, 3, 4, 5],
            vec![0, 1, 3, 2, 5, 4],
            vec![0, 1, 4, 3, 2, 5],
            vec![2, 1, 0, 3, 4, 5],
        ];
        let mut index = None;
        let mut cell_of = Vec::new();
        assert!(candidate_is_in_explored_orbit(
            &partition,
            0,
            &[0],
            1,
            &generators[..3],
            &mut index,
            &mut cell_of,
        )?);
        assert!(!candidate_is_in_explored_orbit(
            &partition,
            0,
            &[0],
            2,
            &generators[..3],
            &mut index,
            &mut cell_of,
        )?);
        let (result, events) = crate::test_support::measured_allocator_events(|| {
            // Последний generator присоединяет меньшую explored-орбиту
            // к большей, до этого не исследованной компоненте.
            if !candidate_is_in_explored_orbit(
                &partition,
                0,
                &[0],
                5,
                &generators,
                &mut index,
                &mut cell_of,
            )? {
                return Err(CanonicalGraphError::InternalInvariant);
            }
            for &candidate in &partition.cells[0] {
                if !candidate_is_in_explored_orbit(
                    &partition,
                    0,
                    &[0, 3],
                    candidate,
                    &generators,
                    &mut index,
                    &mut cell_of,
                )? {
                    return Err(CanonicalGraphError::InternalInvariant);
                }
            }
            Ok(())
        });
        result?;
        assert_eq!(events.alloc + events.realloc + events.dealloc, 0);
        Ok(())
    }

    #[test]
    fn malformed_orbit_permutations_and_vertices_keep_typed_failures() {
        let partition = PartitionV1 {
            cells: vec![vec![0, 1]],
        };
        for (generators, explored, candidate) in [
            (vec![vec![0, 1], vec![0]], vec![0], 1),
            (vec![vec![0, 2]], vec![0], 1),
            (vec![vec![0, 1]], vec![0], 2),
            (vec![vec![0, 1]], vec![2], 1),
        ] {
            let expected =
                breadth_first_orbit_reference(&partition, &explored, candidate, &generators);
            assert_eq!(expected, Err(CanonicalGraphError::InternalInvariant));
            assert_eq!(
                candidate_is_in_explored_orbit(
                    &partition,
                    0,
                    &explored,
                    candidate,
                    &generators,
                    &mut None,
                    &mut Vec::new(),
                ),
                expected,
            );
        }
    }

    #[test]
    fn shared_orbit_scratch_does_not_mix_partition_stabilizers() -> Result<(), CanonicalGraphError>
    {
        let first_partition = PartitionV1 {
            cells: vec![vec![0, 1, 2], vec![3]],
        };
        let second_partition = PartitionV1 {
            cells: vec![vec![0, 1, 3], vec![2]],
        };
        let generators = [vec![1, 0, 2, 3], vec![3, 1, 2, 0]];
        let mut first = None;
        let mut second = None;
        let mut cell_of = Vec::new();
        assert!(candidate_is_in_explored_orbit(
            &first_partition,
            0,
            &[0],
            1,
            &generators[..1],
            &mut first,
            &mut cell_of,
        )?);
        assert!(candidate_is_in_explored_orbit(
            &second_partition,
            0,
            &[0],
            3,
            &generators,
            &mut second,
            &mut cell_of,
        )?);
        assert!(!candidate_is_in_explored_orbit(
            &first_partition,
            0,
            &[0],
            2,
            &generators,
            &mut first,
            &mut cell_of,
        )?);
        // Без нового generator запрос читает DSU второго frame, хотя scratch
        // после предыдущего вызова описывает первое разбиение.
        assert!(candidate_is_in_explored_orbit(
            &second_partition,
            0,
            &[0],
            3,
            &generators,
            &mut second,
            &mut cell_of,
        )?);
        Ok(())
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
            let Ok(left_adjacent) = left.adjacent(vertex) else {
                return false;
            };
            let Ok(right_adjacent) = right.adjacent(image) else {
                return false;
            };
            let mut left_arcs = left_adjacent
                .iter()
                .map(|arc| (arc.direction, arc.role, mapping[arc.neighbour]))
                .collect::<Vec<_>>();
            let mut right_arcs = right_adjacent
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

    fn tiny_bipartite_graph(edge_mask: u8) -> Result<CanonicalGraphV1, CanonicalGraphError> {
        let mut graph = GraphBuilderV1::new([1])?;
        let sources = [graph.add_member([2])?, graph.add_member([2])?];
        let targets = [graph.add_member([3])?, graph.add_member([3])?];
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
                graph.add_edge(target, source, 2)?;
            }
        }
        graph.finish()
    }

    #[test]
    fn canonicalizer_matches_an_independent_tiny_isomorphism_oracle()
    -> Result<(), CanonicalGraphError> {
        let graphs = (0..16)
            .map(tiny_bipartite_graph)
            .collect::<Result<Vec<_>, _>>()?;
        let preimages = graphs
            .iter()
            .map(canonical_preimage)
            .collect::<Result<Vec<_>, _>>()?;

        for left in 0..graphs.len() {
            for right in 0..graphs.len() {
                assert_eq!(
                    preimages[left] == preimages[right],
                    brute_force_isomorphic(&graphs[left], &graphs[right]),
                    "tiny bipartite masks {left:#06b} and {right:#06b}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn exact_automorphism_pruning_prevents_factorial_symmetric_search()
    -> Result<(), CanonicalGraphError> {
        const SYMMETRIC_VERTICES: usize = 12;
        let mut graph = GraphBuilderV1::new([1])?;
        for _ in 0..SYMMETRIC_VERTICES {
            graph.add_member([2])?;
        }
        let graph = graph.finish()?;

        let (_, leaves) = canonical_search(&graph)?;

        assert_eq!(leaves, 1, "exact twin pruning visited extra leaves");
        Ok(())
    }

    #[test]
    fn exhausted_fault_injection_fuel_never_returns_a_partial_preimage()
    -> Result<(), CanonicalGraphError> {
        let mut graph = GraphBuilderV1::new([1])?;
        graph.add_member([2])?;
        graph.add_member([2])?;
        let graph = graph.finish()?;

        assert_eq!(
            canonical_search_with_test_fuel(&graph, 0).unwrap_err(),
            CanonicalGraphError::ResourceExhausted
        );
        Ok(())
    }

    fn relabelled_budget_graph(
        permutation: [usize; 9],
    ) -> Result<CanonicalGraphV1, CanonicalGraphError> {
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

        let mut graph = GraphBuilderV1::new([1])?;
        for _ in 1..permutation.len() {
            graph.add_member([2])?;
        }
        for (from, to) in EDGES {
            graph.add_edge(permutation[from], permutation[to], 2)?;
        }
        graph.finish()
    }

    fn permutation_from_keys<const N: usize>(keys: [u64; N]) -> Vec<usize> {
        let mut images = (0..N).collect::<Vec<_>>();
        ordering::sort_by_key(&mut images, |index| (keys[*index], *index));
        let mut permutation = vec![0];
        permutation.extend(images.into_iter().map(|index| index + 1));
        permutation
    }

    fn small_directed_graph(
        edge_mask: u16,
        permutation: &[usize],
    ) -> Result<CanonicalGraphV1, CanonicalGraphError> {
        let mut graph = GraphBuilderV1::new([1])?;
        for _ in 1..permutation.len() {
            graph.add_member([2])?;
        }
        let mut bit = 0;
        for from in 1..permutation.len() {
            for to in 1..permutation.len() {
                if from != to && edge_mask & (1 << bit) != 0 {
                    graph.add_edge(permutation[from], permutation[to], 2)?;
                }
                if from != to {
                    bit += 1;
                }
            }
        }
        graph.finish()
    }

    #[test]
    fn exact_preimage_is_invariant_under_opaque_relabelling() -> Result<(), CanonicalGraphError> {
        let canonical = relabelled_budget_graph([0, 1, 2, 3, 4, 5, 6, 7, 8])?;
        let renamed = relabelled_budget_graph([0, 1, 2, 6, 3, 5, 4, 7, 8])?;

        let (canonical, _) = canonical_search(&canonical)?;
        let (renamed, _) = canonical_search(&renamed)?;

        assert_eq!(canonical, renamed);
        Ok(())
    }

    fn repeated_component_graph(count: usize) -> Result<CanonicalGraphV1, CanonicalGraphError> {
        let mut graph = GraphBuilderV1::new([1])?;
        for _ in 0..count {
            let source = graph.add_member([2])?;
            let target = graph.add_member([3])?;
            graph.add_edge(target, source, 2)?;
        }
        graph.finish()
    }

    #[test]
    fn repeated_components_are_canonicalized_with_one_leaf() -> Result<(), CanonicalGraphError> {
        let graph = repeated_component_graph(32)?;
        let (preimage, leaves) = canonical_search_with_test_fuel(&graph, 128)?;
        assert!(!preimage.is_empty());
        assert_eq!(leaves, 1);
        Ok(())
    }

    #[test]
    fn component_interchange_checks_direction_role_multiplicity_and_boundary()
    -> Result<(), CanonicalGraphError> {
        for defect in 0..5 {
            let mut graph = GraphBuilderV1::new([1])?;
            let mut sources = Vec::new();
            for index in 0..2 {
                let source = graph.add_member([2])?;
                let target = graph.add_member([3])?;
                sources.push(source);
                let (from, to) = if defect == 1 && index == 0 {
                    (source, target)
                } else {
                    (target, source)
                };
                let role = if defect == 2 && index == 0 { 1 } else { 2 };
                graph.add_edge(from, to, role)?;
                // Совпадающие петли и кратные рёбра допустимы; их различие существенно.
                graph.add_edge(source, source, 2)?;
                graph.add_edge(source, source, 2)?;
                if defect == 3 && index == 0 {
                    graph.add_edge(source, source, 2)?;
                }
                if defect == 4 && index == 0 {
                    graph.add_edge(target, 0, 1)?;
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
    -> Result<(), CanonicalGraphError> {
        let mut graph = GraphBuilderV1::new([1])?;
        let mut roots = Vec::new();
        for length in [3, 6] {
            let mut vertices = Vec::new();
            for index in 0..length {
                let tag = [2, 3, 7][index % 3];
                vertices.push(graph.add_member([tag])?);
            }
            for index in 0..length {
                graph.add_edge(vertices[index], vertices[(index + 1) % length], 2)?;
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
    ) -> Result<(), CanonicalGraphError> {
        let cells = partition_cells(&partition.0, graph.colors.len())?;
        // Независимый oracle объединяет метки концов каждого ребра; обхода
        // компонент и поколений seen_cell здесь нет.
        let mut component = (0..graph.colors.len()).collect::<Vec<_>>();
        for vertex in 0..graph.colors.len() {
            for arc in graph.adjacent(vertex)? {
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
    -> Result<(), CanonicalGraphError> {
        for mask in 0..=0x0fff {
            let graph = small_directed_graph(mask, &[0, 1, 2, 3, 4])?;
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
    ) -> Result<CanonicalGraphV1, CanonicalGraphError> {
        let mut graph = GraphBuilderV1::new([1])?;
        // Цвета второй группы находятся между клетками первой группы.
        let boundary = graph.add_member([60])?;
        for _ in 0..copies {
            let mut vertices = [usize::MAX; 3];
            for index in declaration_order {
                vertices[index] = graph.add_member([[10, 30, 50][index]])?;
            }
            for _ in 0..2 {
                graph.add_edge(vertices[0], vertices[1], 2)?;
                graph.add_edge(vertices[1], vertices[1], 2)?;
            }
            let (from, to) = if reverse_middle {
                (vertices[1], vertices[2])
            } else {
                (vertices[2], vertices[1])
            };
            graph.add_edge(from, to, 1)?;
            graph.add_edge(vertices[0], boundary, 1)?;
            graph.add_edge(boundary, vertices[2], 2)?;
            graph.add_edge(vertices[2], boundary, 2)?;
        }
        for _ in 0..2 {
            let left = graph.add_member([20])?;
            let right = graph.add_member([40])?;
            graph.add_edge(left, right, 2)?;
            graph.add_edge(right, boundary, 1)?;
        }
        graph.finish()
    }

    fn sequential_partition_following(
        graph: &CanonicalGraphV1,
        order: &PartitionV1,
    ) -> Result<EquitablePartitionV1, CanonicalGraphError> {
        let mut rank = vec![usize::MAX; graph.colors.len()];
        for (index, cell) in order.cells.iter().enumerate() {
            let [vertex] = cell.as_slice() else {
                return Err(CanonicalGraphError::InternalInvariant);
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
                .ok_or(CanonicalGraphError::InternalInvariant)?;
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
    -> Result<(), CanonicalGraphError> {
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
    fn bulk_roots_ignore_partition_member_storage_order() -> Result<(), CanonicalGraphError> {
        let graph = interleaved_component_graph(3, [2, 0, 1], true)?;
        let initial = refine_partition(&graph, PartitionV1::initial(&graph)?)?;
        let mut reordered = PartitionV1 {
            cells: initial.0.cells.clone(),
        };
        for cell in &mut reordered.cells {
            cell.reverse();
        }
        let mut first_groups = 0;
        let (first, _) = factor_component_twins(&graph, initial, &mut None, &mut first_groups)?;
        let mut second_groups = 0;
        let (second, _) = factor_component_twins(
            &graph,
            EquitablePartitionV1(reordered),
            &mut None,
            &mut second_groups,
        )?;
        assert_eq!(first_groups, 2);
        assert_eq!(second_groups, first_groups);
        let first_leaf = serialize_leaf(&graph, &first.0)?;
        let second_leaf = serialize_leaf(&graph, &second.0)?;
        assert_eq!(second_leaf.order, first_leaf.order);
        assert_eq!(second_leaf.preimage, first_leaf.preimage);
        Ok(())
    }

    #[test]
    fn repeated_components_use_one_bulk_group_instead_of_search_depth()
    -> Result<(), CanonicalGraphError> {
        let graph = repeated_component_graph(256)?;
        let (preimage, leaves, groups) = canonical_search_impl(&graph, Some(255))?;
        assert!(!preimage.is_empty());
        assert_eq!(leaves, 1);
        assert_eq!(groups, 1);
        assert_eq!(
            canonical_search_impl(&graph, Some(254)).err(),
            Some(CanonicalGraphError::ResourceExhausted),
        );
        Ok(())
    }

    #[test]
    fn interval_rank_finish_matches_comparison_order_without_extra_scratch()
    -> Result<(), CanonicalGraphError> {
        fn check(cells: &[Vec<usize>]) -> Result<(), CanonicalGraphError> {
            let vertex_count = cells.iter().map(Vec::len).sum();
            for permutation in 0..3 {
                let mut state = ActiveRefinementV1::new(
                    PartitionV1 {
                        cells: cells.to_vec(),
                    },
                    vertex_count,
                    true,
                )?;
                match permutation {
                    0 => {}
                    1 => state.cells.reverse(),
                    _ if !state.cells.is_empty() => {
                        let distance = state.cells.len() / 2;
                        state.cells.rotate_left(distance);
                    }
                    _ => {}
                }
                // Между началами интервалов разрешён любой старый vertex map.
                state.cell_of.fill(usize::MAX);
                let mut expected = state.cells.iter().collect::<Vec<_>>();
                expected.sort_unstable_by_key(|cell| cell.rank);
                let expected = PartitionV1 {
                    cells: expected
                        .into_iter()
                        .map(|cell| cell.vertices.clone())
                        .collect(),
                };
                let (actual, events) =
                    crate::test_support::measured_allocator_events(|| state.finish());
                let actual = actual?;
                assert_eq!(actual.0.cells, expected.cells);
                assert_eq!(
                    partition_cells(&actual.0, vertex_count)?,
                    partition_cells(&expected, vertex_count)?,
                );
                assert_eq!(events.alloc, usize::from(!cells.is_empty()));
                assert_eq!(events.realloc, 0);
            }
            Ok(())
        }

        check(&[])?;
        // Все разбиения линейного интервала до девяти вершин: разные длины,
        // singleton и смешанные клетки. Порядок членов также не нормализуется.
        for vertex_count in 1..=9 {
            for cuts in 0..(1_usize << (vertex_count - 1)) {
                let mut cells = Vec::new();
                let mut start = 0;
                for end in 1..=vertex_count {
                    if end == vertex_count || cuts & (1 << (end - 1)) != 0 {
                        cells.push((start..end).rev().collect());
                        start = end;
                    }
                }
                check(&cells)?;
            }
        }
        check(&(0..4096).map(|vertex| vec![vertex]).collect::<Vec<_>>())?;
        check(&[(0..4096).rev().collect()])?;
        Ok(())
    }

    #[test]
    fn interval_rank_finish_rejects_incomplete_or_overlapping_intervals()
    -> Result<(), CanonicalGraphError> {
        for mutation in 0..6 {
            let mut state = ActiveRefinementV1::new(
                PartitionV1 {
                    cells: vec![vec![0, 1], vec![2], vec![3, 4]],
                },
                5,
                true,
            )?;
            match mutation {
                0 => state.cells[1].rank = 0,
                1 => state.cells[1].rank = 1,
                2 => state.cells[1].rank = usize::MAX,
                3 => state.cells[1].vertices.clear(),
                4 => state.cell_of.truncate(4),
                _ => state.cell_of.push(usize::MAX),
            }
            assert_eq!(
                state.finish().err(),
                Some(CanonicalGraphError::InternalInvariant),
                "mutation {mutation}",
            );
        }
        Ok(())
    }

    fn assert_refinement_matches_full(
        graph: &CanonicalGraphV1,
        partition: PartitionV1,
    ) -> Result<(EquitablePartitionV1, RefinementTraceV1), CanonicalGraphError> {
        assert_seeded_refinement_matches_full(graph, partition, None)
    }

    fn assert_seeded_refinement_matches_full(
        graph: &CanonicalGraphV1,
        partition: PartitionV1,
        individualized: Option<usize>,
    ) -> Result<(EquitablePartitionV1, RefinementTraceV1), CanonicalGraphError> {
        let mut expected_rounds = Vec::new();
        let expected = full_refine_partition_reference(
            graph,
            PartitionV1 {
                cells: partition.cells.clone(),
            },
            &mut expected_rounds,
        )?;
        let mut trace = RefinementTraceV1::default();
        let actual =
            refine_partition_seeded_impl(graph, partition, individualized, Some(&mut trace))?;
        assert_eq!(
            ordered_cell_memberships(&actual.0.cells),
            ordered_cell_memberships(&expected.0.cells),
            "ordered final cells",
        );
        assert_eq!(
            trace
                .rounds
                .iter()
                .map(|cells| ordered_cell_memberships(cells))
                .collect::<Vec<_>>(),
            expected_rounds
                .iter()
                .map(|cells| ordered_cell_memberships(cells))
                .collect::<Vec<_>>(),
            "every synchronous round, preserving cell order",
        );
        Ok((actual, trace))
    }

    fn ordered_cell_memberships(cells: &[Vec<usize>]) -> Vec<Vec<usize>> {
        cells
            .iter()
            .map(|cell| {
                let mut members = cell.clone();
                members.sort_unstable();
                members
            })
            .collect()
    }

    fn check_initial_and_individualized_refinement(
        graph: &CanonicalGraphV1,
    ) -> Result<(), CanonicalGraphError> {
        let (partition, _) = assert_refinement_matches_full(graph, PartitionV1::initial(graph)?)?;
        for (cell_index, cell) in partition.0.cells.iter().enumerate() {
            if cell.len() < 2 {
                continue;
            }
            for &vertex in cell {
                assert_seeded_refinement_matches_full(
                    graph,
                    individualize(&partition.0, cell_index, vertex)?,
                    Some(vertex),
                )?;
            }
        }
        Ok(())
    }

    #[test]
    fn incremental_refinement_matches_full_refiner_for_all_small_digraphs()
    -> Result<(), CanonicalGraphError> {
        for mask in 0..=0x0fff {
            let graph = small_directed_graph(mask, &[0, 1, 2, 3, 4])?;
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
    fn individualization_refines_only_affected_cell_members() -> Result<(), CanonicalGraphError> {
        for components in [3, 17, 1024] {
            let graph = repeated_component_graph(components)?;
            let parent = refine_partition(&graph, PartitionV1::initial(&graph)?)?;
            let cell = parent
                .0
                .first_non_singleton()
                .expect("repeated components must leave a non-singleton equitable cell");
            let vertex = parent.0.cells[cell][0];
            let (_, trace) = assert_seeded_refinement_matches_full(
                &graph,
                individualize(&parent.0, cell, vertex)?,
                Some(vertex),
            )?;
            assert_eq!(
                trace.built_signatures, 2,
                "{components} components: only the neighbour and one clean representative",
            );
        }
        Ok(())
    }

    #[test]
    fn mixed_cells_share_clean_signatures_without_changing_synchronous_rounds()
    -> Result<(), CanonicalGraphError> {
        for bidirectional in [false, true] {
            let graph = connected_cycle_graph(64, bidirectional)?;
            let initial = refine_partition(&graph, PartitionV1::initial(&graph)?)?;
            let cell = initial
                .0
                .first_non_singleton()
                .ok_or(CanonicalGraphError::InternalInvariant)?;
            for &vertex in &[1, 32] {
                let (_, trace) = assert_refinement_matches_full(
                    &graph,
                    individualize(&initial.0, cell, vertex)?,
                )?;
                assert!(
                    trace.rounds.len() >= 16,
                    "ring must propagate across rounds"
                );
                assert!(
                    trace.shared_signatures > trace.built_signatures,
                    "mixed cells must construct only changed signatures and one clean representative",
                );
            }
        }
        Ok(())
    }

    #[test]
    fn active_refinement_work_grows_linearly_on_fixed_degree_rings()
    -> Result<(), CanonicalGraphError> {
        for bidirectional in [false, true] {
            let mut previous: Option<(usize, usize)> = None;
            for count in [64, 128, 256] {
                let graph = connected_cycle_graph(count, bidirectional)?;
                let initial = refine_partition(&graph, PartitionV1::initial(&graph)?)?;
                let cell = initial
                    .0
                    .first_non_singleton()
                    .ok_or(CanonicalGraphError::InternalInvariant)?;
                let (_, trace) =
                    assert_refinement_matches_full(&graph, individualize(&initial.0, cell, 1)?)?;
                assert!(
                    trace.rounds.len() >= count / 4,
                    "fixture must propagate through many rounds"
                );
                assert!(
                    trace.processed_cells <= 2 * count,
                    "only the active wave is refined"
                );
                assert!(
                    trace.moved_vertices <= count,
                    "ring vertices enter smaller children once"
                );
                if let Some((processed, moved)) = previous {
                    assert!(
                        trace.processed_cells < 3 * processed,
                        "doubling the ring must not quadruple cell work"
                    );
                    assert!(
                        trace.moved_vertices < 3 * moved,
                        "doubling the ring must not quadruple map updates"
                    );
                }
                previous = Some((trace.processed_cells, trace.moved_vertices));
            }
        }
        Ok(())
    }

    #[test]
    fn incremental_refinement_skips_unchanged_cells_without_skipping_rounds()
    -> Result<(), CanonicalGraphError> {
        for reverse_edges in [false, true] {
            let mut graph = GraphBuilderV1::new([1])?;
            let mut stable = Vec::new();
            for _ in 0..5 {
                stable.push(graph.add_member([20])?);
            }
            let mut path = Vec::new();
            for _ in 0..12 {
                path.push(graph.add_member([10])?);
            }
            for pair in path.windows(2) {
                let (from, to) = if reverse_edges {
                    (pair[1], pair[0])
                } else {
                    (pair[0], pair[1])
                };
                graph.add_edge(from, to, 2)?;
            }
            let graph = graph.finish()?;
            let (partition, trace) =
                assert_refinement_matches_full(&graph, PartitionV1::initial(&graph)?)?;
            assert!(trace.rounds.len() >= 4, "path must propagate across rounds");
            assert!(
                trace.skipped_cells >= trace.rounds.len() - 1,
                "stable cell must execute the optimized branch",
            );
            stable.sort_unstable();
            assert!(
                ordered_cell_memberships(&partition.0.cells).contains(&stable),
                "unchanged cell survives every round, including the terminal round",
            );
            check_initial_and_individualized_refinement(&graph)?;
        }
        Ok(())
    }

    #[test]
    fn incremental_refinement_preserves_middle_and_tied_largest_children()
    -> Result<(), CanonicalGraphError> {
        for (multiplicities, expected_sizes) in [
            ([0, 1, 1, 1, 1, 2], vec![1, 4, 1]),
            ([0, 0, 1, 1, 2, 2], vec![2, 2, 2]),
        ] {
            let mut graph = GraphBuilderV1::new([1])?;
            let mut sources = Vec::new();
            for _ in multiplicities {
                sources.push(graph.add_member([10])?);
            }
            let marker = graph.add_member([30])?;
            for (&source, multiplicity) in sources.iter().zip(multiplicities) {
                for _ in 0..multiplicity {
                    graph.add_edge(source, marker, 2)?;
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
    -> Result<(), CanonicalGraphError> {
        for reverse_edges in [false, true] {
            let mut graph = GraphBuilderV1::new([1])?;
            let mut groups = Vec::new();
            for color in [10, 20, 30, 40, 50] {
                let mut group = Vec::new();
                for _ in 0..4 {
                    group.push(graph.add_member([color])?);
                }
                groups.push(group);
            }
            let marker = graph.add_member([60])?;
            let multiplicities = [(0, 2), (1, 1), (2, 1), (2, 0)];
            for (index, (left_count, right_count)) in multiplicities.into_iter().enumerate() {
                // Group 1 sees splits before and after its iteration position.
                // Group 4 has only one directed incidence to the splitting group.
                for (from, to, role) in [
                    (groups[0][index], groups[1][index], 2),
                    (groups[1][index], groups[2][index], 1),
                    (groups[2][index], groups[3][index], 2),
                    (groups[2][index], groups[3][index], 2),
                    (groups[4][index], groups[0][index], 1),
                ] {
                    let (from, to) = if reverse_edges {
                        (to, from)
                    } else {
                        (from, to)
                    };
                    graph.add_edge(from, to, role)?;
                }
                for _ in 0..left_count {
                    graph.add_edge(groups[0][index], marker, 2)?;
                }
                for _ in 0..right_count {
                    graph.add_edge(groups[2][index], marker, 1)?;
                }
                for _ in 0..2 {
                    graph.add_edge(groups[4][index], groups[4][index], 2)?;
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
    ) -> Result<EquitablePartitionV1, CanonicalGraphError> {
        loop {
            if partition.is_discrete() {
                return Ok(EquitablePartitionV1(partition));
            }
            let cell_of = partition_cells(&partition, graph.colors.len())?;

            let mut next_cells = Vec::new();
            next_cells
                .try_reserve_exact(graph.colors.len())
                .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
            let previous_count = partition.cells.len();
            for cell in partition.cells {
                if cell.len() == 1 {
                    next_cells.push(cell);
                    continue;
                }
                let mut records = Vec::new();
                records
                    .try_reserve_exact(cell.len())
                    .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
                for &vertex in &cell {
                    let arcs = graph.adjacent(vertex)?;
                    let mut signature = Vec::new();
                    signature
                        .try_reserve_exact(arcs.len())
                        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
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
                        .map_err(|_| CanonicalGraphError::ResourceExhausted)?;
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
    ) -> Result<CanonicalGraphV1, CanonicalGraphError> {
        let mut graph = GraphBuilderV1::new([1])?;
        for _ in 0..count {
            graph.add_member([2])?;
        }
        for from in 1..=count {
            let to = from % count + 1;
            graph.add_edge(from, to, 2)?;
            if bidirectional {
                graph.add_edge(to, from, 2)?;
            }
        }
        graph.finish()
    }

    #[test]
    fn search_candidates_ignore_partition_member_storage_order() -> Result<(), CanonicalGraphError>
    {
        let graph = connected_cycle_graph(6, false)?;
        let partition = refine_partition(&graph, PartitionV1::initial(&graph)?)?;
        let (first, events) =
            crate::test_support::measured_allocator_events(|| SearchFrameV1::new(partition, None));
        assert_eq!(events.alloc + events.realloc + events.dealloc, 0);
        let mut reordered = PartitionV1 {
            cells: first.partition.0.cells.clone(),
        };
        // Перестановка членов сохраняет equitable-свойство и порядок клеток.
        for cell in &mut reordered.cells {
            cell.reverse();
        }
        let second = SearchFrameV1::new(EquitablePartitionV1(reordered), None);
        let first_cell = first
            .branch_cell
            .ok_or(CanonicalGraphError::InternalInvariant)?;
        let second_cell = second
            .branch_cell
            .ok_or(CanonicalGraphError::InternalInvariant)?;
        assert_eq!(first.partition.0.cells[first_cell], vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(
            second.partition.0.cells[second_cell],
            first.partition.0.cells[first_cell]
        );
        Ok(())
    }

    #[test]
    fn connected_cycles_keep_unpruned_canonical_bytes() -> Result<(), CanonicalGraphError> {
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
    ) -> Result<Vec<u8>, CanonicalGraphError> {
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
                best.ok_or(CanonicalGraphError::InternalInvariant)
            }
        }
    }

    #[test]
    fn pruning_keeps_exact_bytes_on_every_four_vertex_digraph() -> Result<(), CanonicalGraphError> {
        for mask in 0..=0x0fff {
            let graph = small_directed_graph(mask, &[0, 1, 2, 3, 4])?;
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

    #[test]
    fn orbit_pruning_keeps_exact_bytes_on_every_small_two_role_digraph()
    -> Result<(), CanonicalGraphError> {
        // Шесть возможных направленных дуг: отсутствует, роль A или роль B.
        for mut encoded in 0..3usize.pow(6) {
            let original = encoded;
            let mut graph = GraphBuilderV1::new([1])?;
            for _ in 0..3 {
                graph.add_member([2])?;
            }
            for from in 1..=3 {
                for to in 1..=3 {
                    if from == to {
                        continue;
                    }
                    let role = match encoded % 3 {
                        0 => None,
                        1 => Some(2),
                        _ => Some(12),
                    };
                    encoded /= 3;
                    if let Some(role) = role {
                        graph.add_edge(from, to, role)?;
                    }
                }
            }
            let graph = graph.finish()?;
            assert_eq!(
                canonical_preimage(&graph)?,
                exhaustive_preimage(&graph, PartitionV1::initial(&graph)?)?,
                "typed directed graph {original}",
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
            let permutation: [usize; 9] = permutation
                .try_into()
                .map_err(|_| TestCaseError::fail("generated bijection must have nine vertices"))?;
            let failure = |error| TestCaseError::fail(format!("{error:?}"));
            let baseline = relabelled_budget_graph([0, 1, 2, 3, 4, 5, 6, 7, 8]).map_err(failure)?;
            let renamed = relabelled_budget_graph(permutation).map_err(failure)?;

            prop_assert_eq!(
                canonical_search(&baseline).map_err(failure)?.0,
                canonical_search(&renamed).map_err(failure)?.0,
            );
        }

        #[test]
        fn small_role_directed_graph_preimage_is_invariant_for_generated_bijections(
            edge_mask in 0_u16..=0x0fff,
            keys in proptest::array::uniform4(any::<u64>()),
        ) {
            let failure = |error| TestCaseError::fail(format!("{error:?}"));
            let baseline = small_directed_graph(edge_mask, &[0, 1, 2, 3, 4]).map_err(failure)?;
            let renamed = small_directed_graph(edge_mask, &permutation_from_keys(keys)).map_err(failure)?;

            prop_assert_eq!(
                canonical_search(&baseline).map_err(failure)?.0,
                canonical_search(&renamed).map_err(failure)?.0,
            );
        }
    }
}
