//! Allocation anti-vacuum for explicit selection (#296-B).
//!
//! Construction and feasibility compilation own the variable-size storage.
//! Once both sealed source and client policy exist, selection must only scan
//! and stream its one-row receipt; its public call therefore allocates nothing.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

use labcolors_core::Srgb8;
use labcolors_core::wcag22::Wcag22CriterionV1;
use labcolors_core::wcag22_feasibility::explicit::selection::{
    FirstFeasibleInDeclaredOrderV1, PolicyId, select,
};
use labcolors_core::wcag22_feasibility::explicit::{
    CandidateId, CandidateV1, DomainRequestV1, RequestV1, evaluate,
};
use labcolors_core::wcag22_feasibility::{
    OccurrenceId, RelationId, RelationV1, ResourceProfileIdV1,
};

struct CountingAllocator;

static ALLOCATION_CALLS: AtomicU64 = AtomicU64::new(0);

// SAFETY: all pointer and layout contracts remain owned by `System`; the
// counter only observes successful allocation and reallocation calls.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            ALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, size) };
        if !replacement.is_null() {
            ALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
        }
        replacement
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

fn candidate(index: usize) -> CandidateV1 {
    CandidateV1::new(
        CandidateId::try_new(format!("candidate-{index:02}"))
            .expect("fixture candidate ID is non-empty"),
        Srgb8::new([255; 3]),
    )
}

#[test]
fn public_selection_allocates_nothing_after_source_and_policy_construction() {
    let candidates: Vec<_> = (0..17).map(candidate).collect();
    let order = candidates
        .iter()
        .rev()
        .map(|candidate| candidate.candidate_id().clone())
        .collect();
    let relations = vec![
        RelationV1::applicable(
            RelationId::try_new("contrast").unwrap(),
            OccurrenceId::try_new("text").unwrap(),
            Wcag22CriterionV1::Sc143TextDefault,
            vec![Srgb8::new([0; 3]), Srgb8::new([1; 3]), Srgb8::new([2; 3])],
        )
        .unwrap(),
    ];
    let feasibility = evaluate(
        RequestV1::try_new(
            DomainRequestV1::try_new(candidates).unwrap(),
            relations,
            ResourceProfileIdV1::Compile,
        )
        .unwrap(),
    )
    .expect("fixture compiles");
    let policy = FirstFeasibleInDeclaredOrderV1::try_new(
        PolicyId::try_new("allocation-proof").unwrap(),
        order,
    )
    .unwrap();

    // Positive control: this binary's allocator probe must observe a real heap
    // allocation before it is trusted to certify the production call below.
    ALLOCATION_CALLS.store(0, Ordering::SeqCst);
    let control = black_box(Box::new([0_u8; 257]));
    assert!(ALLOCATION_CALLS.load(Ordering::SeqCst) > 0);
    drop(control);

    ALLOCATION_CALLS.store(0, Ordering::SeqCst);
    let outcome = black_box(select(
        feasibility.selection_source().expect("fixture is feasible"),
        policy,
    ));
    let allocation_calls = ALLOCATION_CALLS.load(Ordering::SeqCst);

    let outcome = outcome.expect("selection succeeds");
    let selected = outcome.selected().expect("declared order selects");
    assert_eq!(selected.candidate().candidate_id().as_str(), "candidate-16");
    assert_eq!(selected.final_verification().verified_applicable_edges(), 3);
    assert_eq!(
        allocation_calls, 0,
        "selection allocated after preconstruction"
    );
}
