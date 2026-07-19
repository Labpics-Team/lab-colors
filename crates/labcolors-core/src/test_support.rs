//! Общие fail-closed helpers in-crate тестов.
//!
//! Production-адаптеры имеют собственные типизированные границы. Этот модуль
//! существует только под `cfg(test)`, чтобы golden/property тесты не стирали
//! разные причины терминального отказа в один правдоподобный sentinel.

use crate::RoleFailure;

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static COUNT_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static ALLOCATION_COUNT: Cell<usize> = const { Cell::new(0) };
}

struct TestAllocator;

#[global_allocator]
static TEST_ALLOCATOR: TestAllocator = TestAllocator;

impl TestAllocator {
    fn record_allocation() {
        let active = COUNT_ALLOCATIONS.try_with(Cell::get).unwrap_or(false);
        if active {
            let _ = ALLOCATION_COUNT.try_with(|count| count.set(count.get() + 1));
        }
    }
}

// SAFETY: каждая операция без изменений делегирует `System` контракты layout и
// указателя; thread-local наблюдатель считает вызовы, но не владеет памятью.
unsafe impl GlobalAlloc for TestAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::record_allocation();
        // SAFETY: `layout` без изменений передаётся от вызывающего allocator-а.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::record_allocation();
        // SAFETY: `layout` без изменений передаётся от вызывающего allocator-а.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` и `layout` получены от делегированного allocator-а `System`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        Self::record_allocation();
        // SAFETY: аргументы без изменений передаются от вызывающего allocator-а.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Считает heap-аллокации `operation` в текущем тестовом потоке. Параллельные
/// тесты не могут исказить измерение.
pub(crate) fn measured_allocations<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    struct DisableMeasurement;

    impl Drop for DisableMeasurement {
        fn drop(&mut self) {
            let _ = COUNT_ALLOCATIONS.try_with(|active| active.set(false));
        }
    }

    COUNT_ALLOCATIONS.with(|active| {
        assert!(
            !active.replace(true),
            "измерения аллокаций нельзя вкладывать"
        );
    });
    ALLOCATION_COUNT.with(|count| count.set(0));
    let disable = DisableMeasurement;
    let result = operation();
    let count = ALLOCATION_COUNT.with(Cell::get);
    drop(disable);
    (result, count)
}

/// Stable representation of an already-admitted role failure. Admission lives
/// in production; tests only format the typed category and core-owned code.
pub(crate) fn role_failure_repr(failure: &RoleFailure) -> String {
    format!(
        "FAILURE({},{})",
        failure.category().as_str(),
        failure.code()
    )
}
