//! Аллокационный анти-вакуум атомарной операции (#296-C2).
//!
//! Композиция `A → полная валидация политики → B → финальная перепроверка`
//! обязана владеть ровно теми аллокациями, которыми владеют её фазы: терминал A
//! перемещается в исход без копирования матрицы/домена/отношений, валидация
//! политики сканирует на месте, receipt-стриминг остаётся стековым. Поэтому
//! число успешных вызовов аллокатора у атомарного вызова равно числу вызовов
//! одной только A-фазы на байт-идентичном запросе (плюс ноль за B — что уже
//! доказано отдельным zero-alloc гейтом selection).

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

use labcolors_core::Srgb8;
use labcolors_core::wcag22::Wcag22CriterionV1;
use labcolors_core::wcag22_feasibility::explicit::atomic::evaluate_and_select;
use labcolors_core::wcag22_feasibility::explicit::selection::{
    FirstFeasibleInDeclaredOrderV1, PolicyId,
};
use labcolors_core::wcag22_feasibility::explicit::{
    CandidateId, CandidateV1, DomainRequestV1, RequestV1, evaluate,
};
use labcolors_core::wcag22_feasibility::{
    OccurrenceId, RelationId, RelationV1, ResourceProfileIdV1,
};

struct CountingAllocator;

static ALLOCATION_CALLS: AtomicU64 = AtomicU64::new(0);

// SAFETY: все контракты указателей и layout остаются у `System`; счётчик лишь
// наблюдает успешные вызовы alloc/realloc.
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

fn candidate(id: &str, value: u8) -> CandidateV1 {
    CandidateV1::new(
        CandidateId::try_new(id).expect("test candidate ID is non-empty"),
        Srgb8::new([value; 3]),
    )
}

fn request() -> RequestV1 {
    let relation = RelationV1::applicable(
        RelationId::try_new("relation").unwrap(),
        OccurrenceId::try_new("occurrence").unwrap(),
        Wcag22CriterionV1::Sc143TextDefault,
        vec![Srgb8::new([0; 3]), Srgb8::new([1; 3]), Srgb8::new([2; 3])],
    )
    .unwrap();
    RequestV1::try_new(
        DomainRequestV1::try_new(vec![
            candidate("member-a", 255),
            candidate("member-b", 254),
            candidate("member-c", 0),
        ])
        .unwrap(),
        vec![relation],
        ResourceProfileIdV1::Compile,
    )
    .unwrap()
}

fn policy() -> FirstFeasibleInDeclaredOrderV1 {
    FirstFeasibleInDeclaredOrderV1::try_new(
        PolicyId::try_new("brand").unwrap(),
        vec![
            CandidateId::try_new("member-c").unwrap(),
            CandidateId::try_new("member-b").unwrap(),
            CandidateId::try_new("member-a").unwrap(),
        ],
    )
    .unwrap()
}

fn measured_calls(body: impl FnOnce()) -> u64 {
    let before = ALLOCATION_CALLS.load(Ordering::Relaxed);
    body();
    ALLOCATION_CALLS.load(Ordering::Relaxed) - before
}

#[test]
fn combined_operation_allocates_exactly_as_much_as_its_a_phase() {
    // Прогрев: SHA-256 evidence-реестр и lazy-инициализация вне измерения.
    black_box(evaluate(request()).expect("warmup A compiles"));
    black_box(
        evaluate_and_select(request(), policy()).expect("warmup combined operation succeeds"),
    );

    // Запрос и политика строятся ВНЕ измеряемого тела: измеряется сама
    // операция, а не подготовка клиентского входа.
    let a_request = request();
    let a_only = measured_calls(|| {
        black_box(evaluate(a_request).expect("A compiles"));
    });

    let combined_request = request();
    let combined_policy = policy();
    let combined = measured_calls(|| {
        black_box(
            evaluate_and_select(combined_request, combined_policy)
                .expect("combined operation succeeds"),
        );
    });

    assert_eq!(
        combined, a_only,
        "the atomic composition must not copy the sealed A result or allocate \
         during validation, selection or the final recheck"
    );
    assert!(a_only > 0, "anti-vacuum: the A phase itself must allocate");
}
