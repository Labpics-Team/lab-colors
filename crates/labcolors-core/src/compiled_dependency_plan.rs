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

/// Immutable compiled dependency plan with CSR forward edges and packed reverse cone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledDependencyPlanV1 {
    pub nodes: Box<[CompiledNodeV1]>,
    pub forward_edges: Box<[NodeIndexV1]>,
    pub reverse_offsets: Box<[usize]>,
    pub reverse_edges: Box<[NodeIndexV1]>,
    pub terminal_outputs: Box<[NodeIndexV1]>,
}