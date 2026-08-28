/// Opaque node index into compiled plan arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeIndexV1(u32);

impl NodeIndexV1 {
    pub(crate) fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub fn raw(self) -> u32 {
        self.0
    }
}

/// A single node in the compiled dependency graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledNodeV1 {
    /// Number of direct dependencies represented by the node's outgoing forward-CSR edges.
    pub dependency_count: u32,
}

/// Maximum number of nodes allowed in a single compiled plan.
const MAX_NODES: usize = 1_000_000;
/// Maximum number of edges allowed in a single compiled plan.
const MAX_EDGES: usize = 10_000_000;

/// Errors produced during compilation of a dependency plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileErrorV1 {
    /// An edge references a node ID not present in the declared node set.
    UnknownNode(u32),
    /// The same directed edge appears more than once.
    DuplicateEdge { from: u32, to: u32 },
    /// The graph contains at least one cycle.
    CycleDetected,
    /// Input exceeds safety bounds (node or edge count).
    InputTooLarge { nodes: usize, edges: usize },
    /// Arithmetic overflow during offset computation.
    OffsetOverflow,
    /// Duplicate node ID in input.
    DuplicateNode(u32),
    /// Scratch buffer provided to `affected_nodes_with_scratch` is too small.
    ScratchBufferTooSmall { needed: usize, got: usize },
    /// Reverse edge index points outside the node array (data corruption).
    CorruptReverseEdge { index: usize, node_count: usize },
}

/// Validate that node and edge counts are within safety bounds before allocation.
pub fn check_compile_bounds(node_count: usize, edge_count: usize) -> Result<(), CompileErrorV1> {
    if node_count > MAX_NODES || edge_count > MAX_EDGES {
        return Err(CompileErrorV1::InputTooLarge {
            nodes: node_count,
            edges: edge_count,
        });
    }
    Ok(())
}

/// Immutable compiled dependency plan with CSR forward edges and packed reverse cone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledDependencyPlanV1 {
    nodes: Box<[CompiledNodeV1]>,
    forward_offsets: Box<[usize]>,
    forward_edges: Box<[NodeIndexV1]>,
    reverse_offsets: Box<[usize]>,
    reverse_edges: Box<[NodeIndexV1]>,
    terminal_outputs: Box<[NodeIndexV1]>,
}

impl CompiledDependencyPlanV1 {
    /// Total number of nodes in the plan.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Terminal output nodes: nodes with zero in-degree (no dependents).
    pub fn terminal_outputs(&self) -> &[NodeIndexV1] {
        &self.terminal_outputs
    }

    /// Forward dependencies of a node (what it depends on).
    /// Returns `None` for out-of-bounds index, `Some(slice)` for valid.
    pub fn dependencies_of(&self, node: NodeIndexV1) -> Option<&[NodeIndexV1]> {
        let i = node.raw() as usize;
        if i + 1 >= self.forward_offsets.len() {
            return None;
        }
        let start = self.forward_offsets[i];
        let end = self.forward_offsets[i + 1];
        Some(&self.forward_edges[start..end])
    }

    /// Reverse dependents of a node (what depends on it).
    /// Returns `None` for out-of-bounds index, `Some(slice)` for valid.
    pub fn dependents_of(&self, node: NodeIndexV1) -> Option<&[NodeIndexV1]> {
        let i = node.raw() as usize;
        if i + 1 >= self.reverse_offsets.len() {
            return None;
        }
        let start = self.reverse_offsets[i];
        let end = self.reverse_offsets[i + 1];
        Some(&self.reverse_edges[start..end])
    }

    /// Check whether a node index is a terminal output.
    fn is_terminal(&self, idx: NodeIndexV1) -> bool {
        self.terminal_outputs.binary_search(&idx).is_ok()
    }

    /// Compile a dependency graph from raw node IDs and directed edges.
    ///
    /// Nodes are sorted canonically by ID (not declaration order).
    /// Rejects duplicate edges, duplicate nodes, and cycles. Builds CSR forward adjacency
    /// and packed reverse adjacency for efficient cone traversal.
    pub fn compile(node_ids: &[u32], edges: &[(u32, u32)]) -> Result<Self, CompileErrorV1> {
        // Defense-in-depth: reject oversized inputs before any allocation.
        check_compile_bounds(node_ids.len(), edges.len())?;

        // Canonical sort: unique, ascending by ID. Detect duplicates.
        let mut sorted_ids: Vec<u32> = node_ids.to_vec();
        sorted_ids.sort_unstable();
        // Check for duplicates before dedup.
        for w in sorted_ids.windows(2) {
            if w[0] == w[1] {
                return Err(CompileErrorV1::DuplicateNode(w[0]));
            }
        }

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

        // Build forward CSR: for each node, list of dependencies (successors in edge direction).
        let mut fwd_offsets: Vec<usize> = vec![0; n + 1];
        for &(fi, _ti) in &edge_pairs {
            fwd_offsets[fi + 1] += 1;
        }
        for i in 1..=n {
            fwd_offsets[i] = fwd_offsets[i]
                .checked_add(fwd_offsets[i - 1])
                .ok_or(CompileErrorV1::OffsetOverflow)?;
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
            reverse_offsets[i + 1] = reverse_offsets[i]
                .checked_add(rev_counts[i])
                .ok_or(CompileErrorV1::OffsetOverflow)?;
        }
        let mut reverse_edges: Vec<NodeIndexV1> = vec![NodeIndexV1::new(0); edge_pairs.len()];

        // Sort edge_pairs by (topo_rank[target], topo_rank[source]) for deterministic
        // predecessor ordering without intermediate allocation.
        let mut topo_rank: Vec<usize> = vec![0; n];
        for (rank, &node) in topo_order.iter().enumerate() {
            topo_rank[node] = rank;
        }
        edge_pairs.sort_unstable_by_key(|&(fi, ti)| (topo_rank[ti], topo_rank[fi]));

        // Populate reverse_edges in one pass from sorted edge_pairs.
        let mut rev_cursor = reverse_offsets.clone();
        for &(fi, ti) in &edge_pairs {
            let pos = rev_cursor[ti];
            reverse_edges[pos] = NodeIndexV1::new(fi as u32);
            rev_cursor[ti] += 1;
        }

        // Terminal outputs: nodes with no dependents (nothing depends on them).
        // In our edge convention (A,B) means A depends on B, so forward_edges[A] = [B].
        // Terminal outputs are nodes that nobody depends on → reverse degree == 0.
        let terminal_outputs: Vec<NodeIndexV1> = (0..n)
            .filter(|&i| rev_counts[i] == 0)
            .map(|i| NodeIndexV1::new(i as u32))
            .collect();

        // dependency_count = number of direct dependencies (forward degree).
        let nodes: Vec<CompiledNodeV1> = (0..n)
            .map(|i| CompiledNodeV1 {
                dependency_count: (fwd_offsets[i + 1] - fwd_offsets[i]) as u32,
            })
            .collect();

        Ok(Self {
            nodes: nodes.into_boxed_slice(),
            forward_offsets: fwd_offsets.into_boxed_slice(),
            forward_edges: forward_edges.into_boxed_slice(),
            reverse_offsets: reverse_offsets.into_boxed_slice(),
            reverse_edges: reverse_edges.into_boxed_slice(),
            terminal_outputs: terminal_outputs.into_boxed_slice(),
        })
    }

    /// Return all nodes transitively affected when `changed` nodes are modified.
    /// Uses precompiled reverse adjacency for O(V+E) traversal.
    /// Result is in ascending canonical-index order.
    ///
    /// Allocates internal scratch buffers. For hot-path reuse, see
    /// [`affected_nodes_with_scratch`](Self::affected_nodes_with_scratch).
    ///
    /// # Panics
    /// Panics only if internal data is corrupt (reverse edge index out of range),
    /// which indicates a bug in plan compilation.
    pub fn affected_nodes(&self, changed: &[NodeIndexV1]) -> Vec<NodeIndexV1> {
        let n = self.nodes.len();
        let mut visited = vec![false; n];
        let mut stack: Vec<NodeIndexV1> = Vec::new();
        self.affected_nodes_with_scratch(changed, &mut visited, &mut stack)
            .expect("affected_nodes: scratch buffer allocated to exact size; corruption indicates a bug in plan compilation")
    }

    /// Return all nodes transitively affected when `changed` nodes are modified,
    /// using caller-provided scratch buffers to avoid repeated allocation.
    ///
    /// `visited` must have length >= `self.node_count()` and will be used as a
    /// bitset (values are reset internally before each call).
    /// `stack` is reused as the DFS work stack and cleared internally.
    ///
    /// Returns `Err` if the scratch buffer is too small or if reverse edge data
    /// is corrupt (index out of range). Both conditions indicate a caller error
    /// or data corruption respectively — never silently ignored.
    ///
    /// Callers can reuse both buffers across multiple calls.
    pub fn affected_nodes_with_scratch(
        &self,
        changed: &[NodeIndexV1],
        visited: &mut [bool],
        stack: &mut Vec<NodeIndexV1>,
    ) -> Result<Vec<NodeIndexV1>, CompileErrorV1> {
        let n = self.nodes.len();
        if visited.len() < n {
            return Err(CompileErrorV1::ScratchBufferTooSmall {
                needed: n,
                got: visited.len(),
            });
        }
        // Reset visited bitset.
        for v in visited.iter_mut().take(n) {
            *v = false;
        }
        stack.clear();

        for &idx in changed {
            let i = idx.raw() as usize;
            if i < n && !visited[i] {
                visited[i] = true;
                stack.push(idx);
            }
        }

        let mut result_indices: Vec<usize> = Vec::new();
        while let Some(node) = stack.pop() {
            let ni = node.raw() as usize;
            result_indices.push(ni);
            let start = self.reverse_offsets[ni];
            let end = self.reverse_offsets[ni + 1];
            for j in start..end {
                let dep = self.reverse_edges[j];
                let di = dep.raw() as usize;
                if di >= n {
                    return Err(CompileErrorV1::CorruptReverseEdge {
                        index: di,
                        node_count: n,
                    });
                }
                if !visited[di] {
                    visited[di] = true;
                    stack.push(dep);
                }
            }
        }

        // Sort by index for deterministic output.
        result_indices.sort_unstable();
        Ok(result_indices
            .into_iter()
            .map(|i| NodeIndexV1::new(i as u32))
            .collect())
    }

    /// Compute the diff between a full resolve and the current state.
    /// `affected` is the output of `affected_nodes`.
    pub fn compute_snapshot_diff(&self, affected: &[NodeIndexV1]) -> ResolvedSnapshotDiffV1 {
        let total_outputs = self.terminal_outputs.len();
        let changed: Vec<NodeIndexV1> = affected
            .iter()
            .copied()
            .filter(|&idx| self.is_terminal(idx))
            .collect();
        debug_assert!(
            total_outputs >= changed.len(),
            "total_outputs ({total_outputs}) < changed terminal count ({})",
            changed.len()
        );
        let unchanged_count = total_outputs.saturating_sub(changed.len());
        ResolvedSnapshotDiffV1 {
            changed_outputs: changed.into_boxed_slice(),
            unchanged_count,
            recheck_passed: Box::new([]),
        }
    }
}

/// Typed diff produced by selective update. Records which outputs changed
/// and how many were unchanged, enabling proof-bound evidence reuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSnapshotDiffV1 {
    /// Indices of outputs whose values changed in this update.
    changed_outputs: Box<[NodeIndexV1]>,
    /// Count of outputs that remained unchanged and can reuse prior evidence.
    unchanged_count: usize,
    /// Outputs whose retained evidence passed cheap recheck and need no re-resolve.
    /// Capability marker — actual recheck logic belongs to Session/runtime, not the plan.
    recheck_passed: Box<[NodeIndexV1]>,
}

impl ResolvedSnapshotDiffV1 {
    /// Indices of terminal outputs whose values changed in this update.
    pub fn changed_outputs(&self) -> &[NodeIndexV1] {
        &self.changed_outputs
    }

    /// Count of terminal outputs that remained unchanged and can reuse prior evidence.
    pub fn unchanged_count(&self) -> usize {
        self.unchanged_count
    }

    /// Outputs whose retained evidence passed cheap recheck and need no re-resolve.
    /// Empty by default — actual recheck logic belongs to Session/runtime.
    pub fn recheck_passed(&self) -> &[NodeIndexV1] {
        &self.recheck_passed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_rejects_oversized_node_input() {
        let over_max_nodes = MAX_NODES + 1;
        assert_eq!(
            check_compile_bounds(over_max_nodes, 0),
            Err(CompileErrorV1::InputTooLarge {
                nodes: over_max_nodes,
                edges: 0,
            })
        );
    }

    #[test]
    fn compile_rejects_oversized_edge_input() {
        let over_max_edges = MAX_EDGES + 1;
        assert_eq!(
            check_compile_bounds(2, over_max_edges),
            Err(CompileErrorV1::InputTooLarge {
                nodes: 2,
                edges: over_max_edges,
            })
        );
    }

    #[test]
    fn compile_accepts_valid_small_graph() {
        let node_ids = [1u32, 2, 3];
        let edges = [(1, 2), (2, 3)];
        let plan = CompiledDependencyPlanV1::compile(&node_ids, &edges).unwrap();
        assert_eq!(plan.node_count(), 3);
    }
}
