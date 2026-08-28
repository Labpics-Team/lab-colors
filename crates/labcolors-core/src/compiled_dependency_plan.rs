/// Opaque node index into compiled plan arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeIndexV1(u32);

impl NodeIndexV1 {
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub fn raw(self) -> u32 {
        self.0
    }
}

/// A single node in the compiled dependency graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledNodeV1 {
    /// Number of direct dependencies (reverse cone size for this node).
    pub dependency_count: u32,
}

/// Errors produced during compilation of a dependency plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileErrorV1 {
    /// An edge references a node ID not present in the declared node set.
    UnknownNode(u32),
    /// The same directed edge appears more than once.
    DuplicateEdge { from: u32, to: u32 },
    /// The graph contains at least one cycle.
    CycleDetected,
}

/// Immutable compiled dependency plan with CSR forward edges and packed reverse cone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledDependencyPlanV1 {
    pub nodes: Box<[CompiledNodeV1]>,
    pub forward_edges: Box<[NodeIndexV1]>,
    pub reverse_offsets: Box<[usize]>,
    pub reverse_edges: Box<[NodeIndexV1]>,
    pub terminal_outputs: Box<[NodeIndexV1]>,
}

impl CompiledDependencyPlanV1 {
    /// Compile a dependency graph from raw node IDs and directed edges.
    ///
    /// Nodes are sorted canonically by ID (not declaration order).
    /// Rejects duplicate edges and cycles. Builds CSR forward adjacency
    /// and packed reverse adjacency for efficient cone traversal.
    pub fn compile(node_ids: &[u32], edges: &[(u32, u32)]) -> Result<Self, CompileErrorV1> {
        // Canonical sort: unique, ascending by ID.
        let mut sorted_ids: Vec<u32> = node_ids.to_vec();
        sorted_ids.sort_unstable();
        sorted_ids.dedup();

        // Map external IDs to canonical indices.
        let id_to_index = |id: u32| -> Option<usize> { sorted_ids.binary_search(&id).ok() };

        // Validate edges and check for duplicates.
        let mut edge_pairs: Vec<(usize, usize)> = Vec::with_capacity(edges.len());
        for &(from, to) in edges {
            let fi = id_to_index(from).ok_or(CompileErrorV1::UnknownNode(from))?;
            let ti = id_to_index(to).ok_or(CompileErrorV1::UnknownNode(to))?;
            edge_pairs.push((fi, ti));
        }
        edge_pairs.sort_unstable();
        for w in edge_pairs.windows(2) {
            if w[0] == w[1] {
                return Err(CompileErrorV1::DuplicateEdge {
                    from: sorted_ids[w[0].0],
                    to: sorted_ids[w[0].1],
                });
            }
        }

        let n = sorted_ids.len();

        // Build forward CSR: for each node, list of successors.
        let mut fwd_offsets: Vec<usize> = vec![0; n + 1];
        for &(fi, _ti) in &edge_pairs {
            fwd_offsets[fi + 1] += 1;
        }
        for i in 1..=n {
            fwd_offsets[i] += fwd_offsets[i - 1];
        }
        let mut forward_edges: Vec<NodeIndexV1> = vec![NodeIndexV1::new(0); edge_pairs.len()];
        let mut cursor = fwd_offsets.clone();
        for &(fi, ti) in &edge_pairs {
            let pos = cursor[fi];
            forward_edges[pos] = NodeIndexV1::new(ti as u32);
            cursor[fi] += 1;
        }

        // Topological sort (Kahn's algorithm) — also detects cycles.
        let mut in_degree: Vec<u32> = vec![0; n];
        for &(_fi, ti) in &edge_pairs {
            in_degree[ti] += 1;
        }
        let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
        for (i, &deg) in in_degree.iter().enumerate().take(n) {
            if deg == 0 {
                queue.push_back(i);
            }
        }
        let mut topo_order: Vec<usize> = Vec::with_capacity(n);
        while let Some(node) = queue.pop_front() {
            topo_order.push(node);
            let start = fwd_offsets[node];
            let end = fwd_offsets[node + 1];
            for edge in &forward_edges[start..end] {
                let succ = edge.raw() as usize;
                in_degree[succ] -= 1;
                if in_degree[succ] == 0 {
                    queue.push_back(succ);
                }
            }
        }
        if topo_order.len() != n {
            return Err(CompileErrorV1::CycleDetected);
        }

        // Build packed reverse adjacency: reverse_offsets[i..i+1] gives
        // the slice of reverse_edges containing predecessors of node i.
        // Predecessors stored in topological order for deterministic traversal.
        let mut rev_counts: Vec<usize> = vec![0; n];
        for &(_fi, ti) in &edge_pairs {
            rev_counts[ti] += 1;
        }
        let mut reverse_offsets: Vec<usize> = vec![0; n + 1];
        for i in 0..n {
            reverse_offsets[i + 1] = reverse_offsets[i] + rev_counts[i];
        }
        let mut reverse_edges: Vec<NodeIndexV1> = vec![NodeIndexV1::new(0); edge_pairs.len()];
        let mut rev_cursor = reverse_offsets.clone();
        // Insert predecessors in topological order for determinism.
        let mut pred_lists: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &(fi, ti) in &edge_pairs {
            pred_lists[ti].push(fi);
        }
        // Sort each predecessor list by topological rank.
        let mut topo_rank: Vec<usize> = vec![0; n];
        for (rank, &node) in topo_order.iter().enumerate() {
            topo_rank[node] = rank;
        }
        for preds in &mut pred_lists {
            preds.sort_unstable_by_key(|&p| topo_rank[p]);
        }
        for (ti, preds) in pred_lists.iter().enumerate() {
            for &pi in preds {
                let pos = rev_cursor[ti];
                reverse_edges[pos] = NodeIndexV1::new(pi as u32);
                rev_cursor[ti] += 1;
            }
        }

        // Terminal outputs: nodes with no successors (forward degree == 0).
        let terminal_outputs: Vec<NodeIndexV1> = (0..n)
            .filter(|&i| fwd_offsets[i] == fwd_offsets[i + 1])
            .map(|i| NodeIndexV1::new(i as u32))
            .collect();

        // dependency_count = number of predecessors (reverse cone direct deps).
        let nodes: Vec<CompiledNodeV1> = (0..n)
            .map(|i| CompiledNodeV1 {
                dependency_count: rev_counts[i] as u32,
            })
            .collect();

        Ok(Self {
            nodes: nodes.into_boxed_slice(),
            forward_edges: forward_edges.into_boxed_slice(),
            reverse_offsets: reverse_offsets.into_boxed_slice(),
            reverse_edges: reverse_edges.into_boxed_slice(),
            terminal_outputs: terminal_outputs.into_boxed_slice(),
        })
    }

    /// Return all nodes transitively affected when `changed` nodes are modified.
    /// Uses precompiled reverse adjacency for O(V+E) traversal.
    /// Result is in deterministic topological order (ascending index).
    pub fn affected_nodes(&self, changed: &[NodeIndexV1]) -> Vec<NodeIndexV1> {
        let n = self.nodes.len();
        let mut visited = vec![false; n];
        let mut stack: Vec<usize> = Vec::new();

        for &idx in changed {
            let i = idx.raw() as usize;
            if i < n && !visited[i] {
                visited[i] = true;
                stack.push(i);
            }
        }

        // BFS/DFS through reverse edges: find all transitive dependents.
        // Reverse edges of node X = nodes that depend on X (predecessors in
        // the "depends-on" direction are successors in "affected-by").
        // Wait — our edges are (from, to) meaning "from depends on to"?
        // No: edges are (dependency, dependent)? Let's clarify:
        // In compile, edge (fi, ti) means fi -> ti in forward_edges.
        // Forward = fi's successors. So fi depends on ti? Or fi feeds ti?
        // Convention: edge (A, B) means "A must be computed before B",
        // i.e., B depends on A. Forward edges of A include B.
        // Reverse edges of B include A.
        // If A changes, B is affected. So we traverse FORWARD from changed nodes.
        // But the method says "reverse-cone traversal through precompiled reverse_edges".
        // Re-reading: reverse_edges[X] = predecessors of X in the forward graph.
        // If changed = {A}, affected = everything reachable via forward edges from A.
        // That uses forward_edges, not reverse_edges.
        //
        // Actually the spec says "reverse-cone traversal" which typically means:
        // given changed outputs, find all inputs that could have contributed.
        // But for selective UPDATE, we want downstream dependents.
        // Let me re-read: "affected_nodes" = nodes that need recomputation.
        // If node X changed, everything that depends on X needs recomputation.
        // "Depends on X" = X appears in their forward dependency list.
        // Equivalently, they appear in X's... hmm.
        //
        // Edge (A, B): A -> B in forward. B depends on A.
        // If A changed → B is affected → traverse forward from A.
        // Forward CSR already gives us this efficiently.
        //
        // But the task says "reverse-cone traversal through precompiled reverse_edges".
        // Maybe the convention is inverted: edge (A,B) means A depends on B.
        // Then forward_edges[A] contains B (A's dependencies).
        // If B changed, A is affected. We need "who depends on B" = reverse_edges[B].
        // That matches "reverse-cone traversal through reverse_edges".
        //
        // Let's go with: edge (from, to) means "from depends on to".
        // forward_edges[from] = [to, ...] = from's dependencies.
        // reverse_edges[to] = [from, ...] = nodes that depend on to.
        // If `changed` contains to, affected = transitive closure via reverse_edges.

        // With this interpretation, the traversal is correct:
        // Start from changed nodes, follow reverse_edges to find dependents.
        // But wait — reverse_edges[to] = predecessors in forward graph = nodes whose
        // forward_edges include to = nodes that depend on to. Yes, this is right.

        let mut result_indices: Vec<usize> = Vec::new();
        while let Some(node) = stack.pop() {
            result_indices.push(node);
            let start = self.reverse_offsets[node];
            let end = self.reverse_offsets[node + 1];
            for j in start..end {
                let dep = self.reverse_edges[j].raw() as usize;
                if !visited[dep] {
                    visited[dep] = true;
                    stack.push(dep);
                }
            }
        }

        // Sort by index for deterministic topological output.
        result_indices.sort_unstable();
        result_indices
            .into_iter()
            .map(|i| NodeIndexV1::new(i as u32))
            .collect()
    }
}
