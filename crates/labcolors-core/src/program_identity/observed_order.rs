//! A static automorphism may exchange two surface ports. Their observed colours
//! can distinguish them, so a finite search must bind its schedule to the
//! observed *component*. The graph below uses the compiler's own typed edges.
use super::*;
use crate::colored_graph::{ColoredGraph, canonical_form};
use std::collections::HashMap;

#[cfg(test)]
#[path = "observed_order/tests.rs"]
mod tests;

#[derive(Debug)]
pub(crate) struct PeerSearchGraph {
    colors: Vec<VertexColorV1>,
    edges: Vec<EdgeV1>,
    targets: Vec<(usize, usize)>,
    inputs: Vec<(usize, usize)>,
    constraints: Vec<(usize, usize)>,
}

pub(crate) struct ObservedComponentOrder {
    pub(crate) targets: Vec<usize>,
    pub(crate) inputs: Vec<usize>,
    pub(crate) constraints: Vec<usize>,
}

fn bindings<K: Ord + Copy>(
    items: impl ExactSizeIterator<Item = (usize, K)>,
    vertices: &[(K, usize)],
) -> Result<Vec<(usize, usize)>, ProgramCompileError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(items.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for (slot, key) in items {
        let i = vertices
            .binary_search_by_key(&key, |entry| entry.0)
            .map_err(|_| ProgramCompileError::InternalInvariant)?;
        result.push((slot, vertices[i].1));
    }
    Ok(result)
}

impl ProgramIdentityGraphV1 {
    pub(crate) fn peer_component(
        &self,
        targets: impl ExactSizeIterator<Item = (usize, TargetId)>,
        inputs: impl ExactSizeIterator<Item = (usize, SurfaceInputPortId)>,
        constraints: impl ExactSizeIterator<Item = (usize, ConstraintId)>,
    ) -> Result<Option<PeerSearchGraph>, ProgramCompileError> {
        let mut inputs = bindings(inputs, &self.inputs)?;
        let colors = self.graph.colors();
        ordering::sort_by(&mut inputs, |a, b| colors[a.1].cmp(&colors[b.1]));
        // Distinct authored port colours already anchor all relevant inputs.
        if inputs
            .windows(2)
            .all(|pair| colors[pair[0].1] != colors[pair[1].1])
        {
            return Ok(None);
        }
        let mut targets = bindings(targets, &self.computed_targets)?;
        let mut constraints = bindings(constraints, &self.hard)?;
        let mut local = HashMap::new();
        let mut members = Vec::new();
        fn include(
            local: &mut HashMap<usize, usize>,
            members: &mut Vec<usize>,
            vertex: usize,
        ) -> Result<usize, ProgramCompileError> {
            if let Some(&index) = local.get(&vertex) {
                return Ok(index);
            }
            local
                .try_reserve(1)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            members
                .try_reserve(1)
                .map_err(|_| ProgramCompileError::ResourceExhausted)?;
            let index = members.len();
            local.insert(vertex, index);
            members.push(vertex);
            Ok(index)
        }
        for (_, v) in targets
            .iter_mut()
            .chain(&mut constraints)
            .chain(&mut inputs)
        {
            *v = include(&mut local, &mut members, *v)?;
        }
        let mut edges = Vec::new();
        let mut from = 0;
        while from < members.len() {
            for (role, vertex) in self.graph.outgoing(members[from]) {
                // Every constraint refers to the observation group, whose port
                // list also contains unrelated components. Only physical input
                // dependencies below the uses and relations belong here.
                if role == EdgeRoleV1::ObservationGroup as u8 {
                    continue;
                }
                let to = include(&mut local, &mut members, vertex)?;
                edges
                    .try_reserve(1)
                    .map_err(|_| ProgramCompileError::ResourceExhausted)?;
                edges.push(EdgeV1 { from, to, role });
            }
            from += 1;
        }
        // Intern the full static colour exactly once. Runtime sees a compact
        // class per vertex and never traverses another component's program.
        let mut classes =
            try_collect_program(members.len(), members.iter().map(|&vertex| colors[vertex]))?;
        ordering::sort(&mut classes);
        classes.dedup();
        let mut local_colors = Vec::new();
        local_colors
            .try_reserve_exact(members.len())
            .map_err(|_| ProgramCompileError::ResourceExhausted)?;
        for vertex in members {
            let index = classes
                .binary_search(&colors[vertex])
                .map_err(|_| ProgramCompileError::InternalInvariant)?;
            let rank = u32::try_from(index).map_err(|_| ProgramCompileError::ResourceExhausted)?;
            let mut color = VertexColorV1::new(0);
            color.push_bytes(&rank.to_be_bytes())?;
            local_colors.push(color);
        }
        Ok(Some(PeerSearchGraph {
            colors: local_colors,
            edges,
            targets,
            inputs,
            constraints,
        }))
    }
}

impl PeerSearchGraph {
    pub(crate) fn order(
        &self,
        cases: &[usize],
        scenarios: NonEmptyScenarioSetV1<'_>,
    ) -> Result<Option<ObservedComponentOrder>, ()> {
        let rgb = |case: usize, input: usize| {
            scenarios
                .physical_values(case)
                .and_then(|v| v.get(input))
                .map(|v| v.srgb8().bytes())
                .ok_or(())
        };
        let Some(graph) = self.observed_graph(cases, rgb)? else {
            return Ok(None);
        };
        let form =
            canonical_form(&graph, b"labcolors.local-observed-peer-order.v1\0").map_err(|_| ())?;
        drop(form.preimage);
        let rank = form.order;
        let mut inverse = Vec::new();
        inverse.try_reserve_exact(rank.len()).map_err(|_| ())?;
        inverse.resize(rank.len(), 0);
        for (index, vertex) in rank.into_iter().enumerate() {
            inverse[vertex] = index;
        }
        let project = |bindings: &[(usize, usize)]| -> Result<Vec<usize>, ()> {
            let mut ordered =
                try_collect_program(bindings.len(), bindings.iter().copied()).map_err(|_| ())?;
            ordering::sort_by_key(&mut ordered, |&(_, vertex)| inverse[vertex]);
            try_collect_program(ordered.len(), ordered.into_iter().map(|(slot, _)| slot))
                .map_err(|_| ())
        };
        Ok(Some(ObservedComponentOrder {
            targets: project(&self.targets)?,
            inputs: project(&self.inputs)?,
            constraints: project(&self.constraints)?,
        }))
    }

    fn observed_graph(
        &self,
        cases: &[usize],
        rgb: impl Fn(usize, usize) -> Result<[u8; 3], ()>,
    ) -> Result<Option<ColoredGraph<VertexColorV1>>, ()> {
        let mut colors =
            try_collect_program(self.colors.len(), self.colors.iter().copied()).map_err(|_| ())?;
        let mut ties =
            try_collect_program(self.inputs.len(), self.inputs.iter().copied()).map_err(|_| ())?;
        let mut distinguished = false;
        let mut previous = None;
        for &(input, vertex) in &self.inputs {
            let mut low = [255; 3];
            let mut high = [0; 3];
            let interchangeable =
                previous.filter(|&(_, other)| self.colors[vertex] == self.colors[other]);
            previous = Some((input, vertex));
            for &case in cases {
                let value = rgb(case, input)?;
                low = low.min(value);
                high = high.max(value);
                if let Some((earlier, _)) = interchangeable.filter(|_| !distinguished) {
                    distinguished = rgb(case, earlier)? != value;
                }
            }
            // Two actual RGB tuples occurring in this column. Distinct extrema
            // already identify most varying ports without a graph per case.
            colors[vertex].push_bytes(&low).map_err(|_| ())?;
            colors[vertex].push_bytes(&high).map_err(|_| ())?;
        }
        if !distinguished {
            return Ok(None);
        }
        ordering::sort_by(&mut ties, |a, b| colors[a.1].cmp(&colors[b.1]));
        let mut needs_rows = false;
        for pair in ties.windows(2) {
            let (a, va) = pair[0];
            let (b, vb) = pair[1];
            if colors[va] != colors[vb] {
                continue;
            }
            for &case in cases {
                if rgb(case, a)? != rgb(case, b)? {
                    needs_rows = true;
                    break;
                }
            }
            if needs_rows {
                break;
            }
        }
        if needs_rows && refine_distinct_marginals(&mut colors, &ties, cases, &rgb)? {
            needs_rows = false;
        }
        let mut edges =
            try_collect_program(self.edges.len(), self.edges.iter().copied()).map_err(|_| ())?;
        if needs_rows {
            let channels = self.inputs.len().checked_mul(3).ok_or(())?;
            colors
                .try_reserve_exact(channels.checked_add(cases.len()).ok_or(())?)
                .map_err(|_| ())?;
            edges
                .try_reserve_exact(
                    channels
                        .checked_mul(cases.len().checked_add(1).ok_or(())?)
                        .ok_or(())?,
                )
                .map_err(|_| ())?;
            // Each row points to three fixed channel vertices per input. Edge
            // roles hold the bytes, so the canonicalizer only partitions
            // cases + 3 * inputs new vertices for a complete RGB matrix.
            let channels_start = colors.len();
            for &(_, vertex) in &self.inputs {
                for tag in 2..=4 {
                    let channel = colors.len();
                    colors.push(VertexColorV1::new(tag));
                    edges.push(EdgeV1 {
                        from: channel,
                        to: vertex,
                        role: 0,
                    });
                }
            }
            for &case in cases {
                let row = colors.len();
                colors.push(VertexColorV1::new(1));
                for (i, &(input, _)) in self.inputs.iter().enumerate() {
                    for (channel, role) in rgb(case, input)?.into_iter().enumerate() {
                        edges.push(EdgeV1 {
                            from: row,
                            to: channels_start + 3 * i + channel,
                            role,
                        });
                    }
                }
            }
        }
        ColoredGraph::new(colors, edges).map(Some).map_err(|_| ())
    }
}

/// A row permutation preserves the multiset of every input's RGB tuples.
/// Refine tied input colours by those exact multisets. If two remaining tied
/// columns differ on any row, their correlation with the other columns is
/// essential: retain the original graph and encode the complete joint rows.
/// Otherwise every colour-preserving automorphism of the refined static
/// graph preserves the observation matrix pointwise on the input columns.
fn refine_distinct_marginals(
    colors: &mut [VertexColorV1],
    ties: &[(usize, usize)],
    cases: &[usize],
    rgb: &impl Fn(usize, usize) -> Result<[u8; 3], ()>,
) -> Result<bool, ()> {
    let mut refinements = Vec::new();
    refinements.try_reserve_exact(ties.len()).map_err(|_| ())?;
    for group in ties.chunk_by(|a, b| colors[a.1] == colors[b.1]) {
        if group.len() == 1 {
            refinements.push((group[0].1, 0_u32));
            continue;
        }
        // The temporary matrix for this one tied class stores only RGB bytes.
        let mut columns = Vec::new();
        columns.try_reserve_exact(group.len()).map_err(|_| ())?;
        for &(input, vertex) in group {
            let mut values = Vec::new();
            values.try_reserve_exact(cases.len()).map_err(|_| ())?;
            for &case in cases {
                values.push(rgb(case, input)?);
            }
            ordering::sort(&mut values);
            columns.push((input, vertex, values));
        }
        ordering::sort_by(&mut columns, |a, b| a.2.cmp(&b.2));
        let mut rank = 0_u32;
        for (index, (input, vertex, values)) in columns.iter().enumerate() {
            if let Some((previous, _, previous_values)) = index.checked_sub(1).map(|i| &columns[i])
            {
                if values != previous_values {
                    rank = rank.checked_add(1).ok_or(())?;
                } else {
                    for &case in cases {
                        if rgb(case, *input)? != rgb(case, *previous)? {
                            // Do not install any partial refinement on this path.
                            return Ok(false);
                        }
                    }
                }
            }
            refinements.push((*vertex, rank));
        }
    }
    for (vertex, rank) in refinements {
        colors[vertex]
            .push_bytes(&rank.to_be_bytes())
            .map_err(|_| ())?;
    }
    Ok(true)
}
