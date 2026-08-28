use crate::compiled_dependency_plan::CompiledDependencyPlanV1;

/// Two plans with identical edges but different declaration order must produce
/// identical canonical node ordering. Declaration order is not a semantic input.
#[test]
fn permutation_invariance_rejects_declaration_order_dependence() {
    // Plan A: declare nodes 0,1,2 with edges 0->1, 1->2
    let plan_a = CompiledDependencyPlanV1::compile(
        &[0, 1, 2],
        &[(0, 1), (1, 2)],
    )
    .expect("plan A must compile");

    // Plan B: same graph, declared in reverse node order and shuffled edges
    let plan_b = CompiledDependencyPlanV1::compile(
        &[2, 1, 0],
        &[(1, 2), (0, 1)],
    )
    .expect("plan B must compile");

    assert_eq!(
        plan_a.nodes, plan_b.nodes,
        "node metadata must be identical regardless of declaration order"
    );
    assert_eq!(
        plan_a.forward_edges, plan_b.forward_edges,
        "forward CSR must be identical regardless of declaration order"
    );
    assert_eq!(
        plan_a.reverse_offsets, plan_b.reverse_offsets,
        "reverse offsets must be identical regardless of declaration order"
    );
    assert_eq!(
        plan_a.reverse_edges, plan_b.reverse_edges,
        "reverse edges must be identical regardless of declaration order"
    );
}

/// A graph containing a cycle must fail compilation before any execution.
/// Cycles are structural defects, not runtime conditions.
#[test]
fn cycle_rejection_before_execution() {
    let result = CompiledDependencyPlanV1::compile(
        &[0, 1, 2],
        &[(0, 1), (1, 2), (2, 0)],
    );
    assert!(result.is_err(), "cyclic graph must be rejected at compile time");
}

/// Duplicate edges indicate a specification defect and must be rejected.
/// Silent deduplication would hide upstream bugs.
#[test]
fn duplicate_edge_rejection() {
    let result = CompiledDependencyPlanV1::compile(
        &[0, 1],
        &[(0, 1), (0, 1)],
    );
    assert!(result.is_err(), "duplicate edges must be rejected at compile time");
}