//! A static automorphism may exchange two surface ports. Their observed colours
//! can distinguish them, so a finite search must bind its schedule to the
//! observed *component*. The graph below uses the compiler's own typed edges.
use super::*;
use crate::colored_graph::{ColoredGraph, canonical_form};
use std::collections::HashMap;

type Color = [u8; 11];

#[derive(Debug)]
pub(crate) struct PeerSearchGraph {
    colors: Vec<Color>,
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
        inputs.sort_unstable_by_key(|&(_, vertex)| colors[vertex]);
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
        classes.sort_unstable();
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
            let mut color = [0; 11];
            color[1..5].copy_from_slice(&rank.to_be_bytes());
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
                if !distinguished && let Some((earlier, _)) = interchangeable {
                    distinguished = rgb(case, earlier)? != value;
                }
            }
            // Two actual RGB tuples occurring in this column. Distinct extrema
            // already identify most varying ports without a graph per case.
            colors[vertex][5..8].copy_from_slice(&low);
            colors[vertex][8..11].copy_from_slice(&high);
        }
        if !distinguished {
            return Ok(None);
        }
        ties.sort_unstable_by_key(|&(_, v)| colors[v]);
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
        let mut edges =
            try_collect_program(self.edges.len(), self.edges.iter().copied()).map_err(|_| ())?;
        if needs_rows {
            let cells = cases.len().checked_mul(self.inputs.len()).ok_or(())?;
            colors
                .try_reserve_exact(cells.checked_add(cases.len()).ok_or(())?)
                .map_err(|_| ())?;
            edges
                .try_reserve_exact(cells.checked_mul(2).ok_or(())?)
                .map_err(|_| ())?;
            // Case vertices preserve correlations. A multiset of colours for
            // each port loses which backgrounds can actually occur together.
            for &case in cases {
                let row = colors.len();
                let mut color = [0; 11];
                color[0] = 1;
                colors.push(color);
                for &(input, vertex) in &self.inputs {
                    let cell = colors.len();
                    let mut color = [0; 11];
                    color[0] = 2;
                    color[1..4].copy_from_slice(&rgb(case, input)?);
                    colors.push(color);
                    edges.push(EdgeV1 {
                        from: cell,
                        to: row,
                        role: 1,
                    });
                    edges.push(EdgeV1 {
                        from: cell,
                        to: vertex,
                        role: 2,
                    });
                }
            }
        }
        let graph = ColoredGraph::new(colors, edges).map_err(|_| ())?;
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
            ordered.sort_unstable_by_key(|&(_, vertex)| inverse[vertex]);
            try_collect_program(ordered.len(), ordered.into_iter().map(|(slot, _)| slot))
                .map_err(|_| ())
        };
        Ok(Some(ObservedComponentOrder {
            targets: project(&self.targets)?,
            inputs: project(&self.inputs)?,
            constraints: project(&self.constraints)?,
        }))
    }
}
