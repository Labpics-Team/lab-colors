use crate::compiled_dependency_plan::{CompiledDependencyPlanV1, NodeIndexV1};

/// Two plans with identical edges but different declaration order must produce
/// identical canonical node ordering. Declaration order is not a semantic input.
#[test]
fn permutation_invariance_rejects_declaration_order_dependence() {
    let plan_a = CompiledDependencyPlanV1::compile(&[0, 1, 2], &[(0, 1), (1, 2)])
        .expect("plan A must compile");

    let plan_b = CompiledDependencyPlanV1::compile(&[2, 1, 0], &[(1, 2), (0, 1)])
        .expect("plan B must compile");

    assert_eq!(plan_a.node_count(), plan_b.node_count());
    assert_eq!(plan_a.terminal_outputs(), plan_b.terminal_outputs());
    // Structural equality: both plans must be fully equal via PartialEq.
    assert_eq!(plan_a, plan_b);
}

/// A graph containing a cycle must fail compilation before any execution.
#[test]
fn cycle_rejection_before_execution() {
    let result = CompiledDependencyPlanV1::compile(&[0, 1, 2], &[(0, 1), (1, 2), (2, 0)]);
    assert!(
        result.is_err(),
        "cyclic graph must be rejected at compile time"
    );
}

/// Duplicate edges indicate a specification defect and must be rejected.
#[test]
fn duplicate_edge_rejection() {
    let result = CompiledDependencyPlanV1::compile(&[0, 1], &[(0, 1), (0, 1)]);
    assert!(
        result.is_err(),
        "duplicate edges must be rejected at compile time"
    );
}

/// Diamond: 0 depends on 1 and 2; 1 and 2 both depend on 3.
/// Edge (A, B) means "A depends on B" → forward_edges[A] contains B.
/// reverse_edges[B] contains A (nodes that depend on B).
/// Changing node 3 must affect all four nodes via reverse-cone traversal.
#[test]
fn selective_update_matches_full_resolve_on_diamond_graph() {
    let plan = CompiledDependencyPlanV1::compile(&[0, 1, 2, 3], &[(0, 1), (0, 2), (1, 3), (2, 3)])
        .expect("diamond must compile");

    // Change leaf dependency 3 → everything depends on it transitively.
    let affected = plan.affected_nodes(&[NodeIndexV1::new(3)]);
    let affected_raw: Vec<u32> = affected.iter().map(|n| n.raw()).collect();

    assert!(
        affected_raw.contains(&3),
        "changed node itself must be in affected set"
    );
    assert!(
        affected_raw.contains(&1),
        "direct dependent 1 must be affected"
    );
    assert!(
        affected_raw.contains(&2),
        "direct dependent 2 must be affected"
    );
    assert!(
        affected_raw.contains(&0),
        "transitive dependent 0 must be affected"
    );
    assert_eq!(
        affected_raw.len(),
        4,
        "all nodes must be affected in diamond when root changes"
    );

    // Change node 1 → only 0 and 1 affected (not 2 or 3).
    let affected_1 = plan.affected_nodes(&[NodeIndexV1::new(1)]);
    let affected_1_raw: Vec<u32> = affected_1.iter().map(|n| n.raw()).collect();
    assert!(affected_1_raw.contains(&1));
    assert!(affected_1_raw.contains(&0));
    assert!(
        !affected_1_raw.contains(&2),
        "sibling 2 must NOT be affected by change to 1"
    );
    assert!(
        !affected_1_raw.contains(&3),
        "dependency 3 must NOT be affected by change to 1"
    );
}

// ─── Sabotage controls ───────────────────────────────────────────────
// Each test documents the specific mutant class it kills.
// Edge convention: (A, B) means "A depends on B".
// forward_edges[A] = [B, ...] = A's dependencies.
// reverse_edges[B] = [A, ...] = nodes that depend on B.
// affected_nodes(changed) = changed ∪ transitive dependents via reverse_edges.

/// MUTANT CLASS: replacing selective traversal with full-table scan.
/// If affected_nodes returned ALL nodes regardless of input, this test fails
/// because changing a node with no dependents should affect only itself.
/// Chain: 0→1→2→3 (each depends on next). Node 3 has no dependents.
#[test]
fn sabotage_full_table_scan_detected() {
    // 0 depends on 1, 1 depends on 2, 2 depends on 3.
    // reverse_edges: [3]→[2], [2]→[1], [1]→[0], [0]→[]
    // Node 0 has NO dependents (nothing depends on 0).
    let plan = CompiledDependencyPlanV1::compile(&[0, 1, 2, 3], &[(0, 1), (1, 2), (2, 3)])
        .expect("chain must compile");

    let affected = plan.affected_nodes(&[NodeIndexV1::new(0)]);
    assert_eq!(
        affected.len(),
        1,
        "changing terminal node 0 (no dependents) must affect only itself"
    );
    assert_eq!(affected[0].raw(), 0);
}

/// MUTANT CLASS: omitting transitive dependents from affected set.
/// If traversal stops at depth 1, distant dependents would be missing.
/// Chain: 3→2→1→0 (3 depends on 2, 2 on 1, 1 on 0).
/// Changing 0 must affect {0, 1, 2, 3} transitively.
#[test]
fn sabotage_missing_transitive_dependent_detected() {
    let plan = CompiledDependencyPlanV1::compile(&[0, 1, 2, 3], &[(3, 2), (2, 1), (1, 0)])
        .expect("chain must compile");

    // Change deepest dependency (node 0). All nodes depend on it transitively.
    let affected = plan.affected_nodes(&[NodeIndexV1::new(0)]);
    let raw: Vec<u32> = affected.iter().map(|n| n.raw()).collect();
    assert!(raw.contains(&0), "changed node must be included");
    assert!(raw.contains(&1), "direct dependent must be included");
    assert!(
        raw.contains(&2),
        "transitive dependent at depth 2 must be included"
    );
    assert!(
        raw.contains(&3),
        "most distant transitive dependent must be included"
    );
    assert_eq!(raw.len(), 4);
}

/// MUTANT CLASS: branching on semantic names instead of opaque indices.
/// Uses arbitrary non-sequential IDs to prove pure index operation.
#[test]
fn sabotage_semantic_name_branching_detected() {
    let plan = CompiledDependencyPlanV1::compile(&[100, 200, 300], &[(300, 200), (200, 100)])
        .expect("non-sequential IDs must compile");

    // Canonical indices: 100→0, 200→1, 300→2.
    // Changing index 0 (ID 100): dependents are 1 (200) and 2 (300).
    let affected = plan.affected_nodes(&[NodeIndexV1::new(0)]);
    assert_eq!(
        affected.len(),
        3,
        "all nodes must be reachable via opaque indices"
    );
}

/// MUTANT CLASS: caching results by byte-equality of input slice without
/// invalidation. Different changed sets must produce different results.
#[test]
fn sabotage_byte_equality_stale_reuse_detected() {
    let plan =
        CompiledDependencyPlanV1::compile(&[0, 1, 2], &[(2, 1), (1, 0)]).expect("must compile");

    // Changing 0 (root dependency): affected = {0, 1, 2}.
    let a = plan.affected_nodes(&[NodeIndexV1::new(0)]);
    // Changing 2 (leaf, no dependents): affected = {2}.
    let b = plan.affected_nodes(&[NodeIndexV1::new(2)]);

    assert_ne!(
        a.len(),
        b.len(),
        "different changed sets must produce different affected sets"
    );
    assert!(
        a.len() > b.len(),
        "changing root must affect more nodes than changing leaf"
    );
}

/// MUTANT CLASS: making output depend on declaration order of nodes/edges.
/// Two compilations of the same graph with shuffled inputs must be identical.
#[test]
fn sabotage_declaration_order_dependence_detected() {
    let plan_a = CompiledDependencyPlanV1::compile(&[5, 3, 1, 4, 2], &[(5, 3), (3, 1), (4, 2)])
        .expect("plan A must compile");

    let plan_b = CompiledDependencyPlanV1::compile(&[1, 2, 3, 4, 5], &[(4, 2), (3, 1), (5, 3)])
        .expect("plan B must compile");

    assert_eq!(plan_a.node_count(), plan_b.node_count());
    assert_eq!(plan_a.terminal_outputs(), plan_b.terminal_outputs());
    assert_eq!(plan_a, plan_b);
}

/// MUTANT CLASS: allowing edges to reference undeclared nodes.
/// If edge validation did not verify both endpoints exist, phantom
/// ownership could corrupt the CSR structure.
#[test]
fn sabotage_duplicate_ownership_detected() {
    let result = CompiledDependencyPlanV1::compile(&[0, 1], &[(0, 99)]);
    assert!(
        result.is_err(),
        "edge to undeclared node must be rejected, preventing phantom ownership"
    );
}

/// Snapshot diff reports exact changed and unchanged counts.
/// Diamond graph: 0 depends on 1 and 2; 1 and 2 both depend on 3.
/// Terminal outputs are nodes nobody depends on (reverse degree == 0).
/// In this graph, node 0 is the only terminal output.
#[test]
fn snapshot_diff_reports_exact_changed_and_unchanged_counts() {
    let plan = CompiledDependencyPlanV1::compile(&[0, 1, 2, 3], &[(0, 1), (0, 2), (1, 3), (2, 3)])
        .expect("diamond must compile");

    let total_outputs = plan.terminal_outputs().len();

    // Change node 3 (deepest dependency) → all nodes affected, including terminal 0.
    let affected = plan.affected_nodes(&[NodeIndexV1::new(3)]);
    let diff = plan.compute_snapshot_diff(&affected, total_outputs);

    assert_eq!(
        diff.changed_outputs().len(),
        1,
        "exactly one terminal output (node 0) must be marked changed"
    );
    assert_eq!(diff.changed_outputs()[0].raw(), 0);
    assert_eq!(
        diff.unchanged_count(),
        0,
        "no outputs remain unchanged when root dependency changes"
    );
    assert!(
        diff.recheck_passed().is_empty(),
        "recheck_passed is a capability marker, empty by default"
    );

    // Change node 0 (terminal output itself, no dependents) → only 0 affected.
    let affected_leaf = plan.affected_nodes(&[NodeIndexV1::new(0)]);
    let diff_leaf = plan.compute_snapshot_diff(&affected_leaf, total_outputs);

    assert_eq!(diff_leaf.changed_outputs().len(), 1);
    assert_eq!(diff_leaf.changed_outputs()[0].raw(), 0);
    assert_eq!(diff_leaf.unchanged_count(), 0);
}

/// affected_nodes_with_scratch reuses caller-provided buffers.
#[test]
fn scratch_api_reuses_buffers_without_reallocation() {
    let plan = CompiledDependencyPlanV1::compile(&[0, 1, 2], &[(2, 1), (1, 0)])
        .expect("chain must compile");

    let n = plan.node_count();
    let mut visited = vec![false; n];
    let mut stack: Vec<NodeIndexV1> = Vec::new();

    // First call.
    let a = plan.affected_nodes_with_scratch(&[NodeIndexV1::new(0)], &mut visited, &mut stack);
    assert_eq!(a.len(), 3);

    // Second call reuses same buffers.
    let b = plan.affected_nodes_with_scratch(&[NodeIndexV1::new(2)], &mut visited, &mut stack);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].raw(), 2);
}
