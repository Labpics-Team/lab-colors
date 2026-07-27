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
    static ALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
    static REALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
    static DEALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
}

struct TestAllocator;

#[global_allocator]
static TEST_ALLOCATOR: TestAllocator = TestAllocator;

impl TestAllocator {
    fn record(counter: &'static std::thread::LocalKey<Cell<usize>>) {
        let active = COUNT_ALLOCATIONS.try_with(Cell::get).unwrap_or(false);
        if active {
            let _ = counter.try_with(|count| count.set(count.get().saturating_add(1)));
        }
    }
}

// SAFETY: каждая операция без изменений делегирует `System` контракты layout и
// указателя; thread-local наблюдатель считает вызовы, но не владеет памятью.
unsafe impl GlobalAlloc for TestAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::record(&ALLOC_COUNT);
        // SAFETY: `layout` без изменений передаётся от вызывающего allocator-а.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::record(&ALLOC_COUNT);
        // SAFETY: `layout` без изменений передаётся от вызывающего allocator-а.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        Self::record(&DEALLOC_COUNT);
        // SAFETY: `ptr` и `layout` получены от делегированного allocator-а `System`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        Self::record(&REALLOC_COUNT);
        // SAFETY: аргументы без изменений передаются от вызывающего allocator-а.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AllocatorEvents {
    pub(crate) alloc: usize,
    pub(crate) realloc: usize,
    pub(crate) dealloc: usize,
}

fn assert_allocator_measurement_is_active() {
    COUNT_ALLOCATIONS.with(|active| {
        assert!(
            active.get(),
            "allocator checkpoint требует активного измерения"
        );
    });
}

pub(crate) fn reset_allocator_events() {
    assert_allocator_measurement_is_active();
    ALLOC_COUNT.with(|count| count.set(0));
    REALLOC_COUNT.with(|count| count.set(0));
    DEALLOC_COUNT.with(|count| count.set(0));
}

pub(crate) fn current_allocator_events() -> AllocatorEvents {
    assert_allocator_measurement_is_active();
    AllocatorEvents {
        alloc: ALLOC_COUNT.with(Cell::get),
        realloc: REALLOC_COUNT.with(Cell::get),
        dealloc: DEALLOC_COUNT.with(Cell::get),
    }
}

/// Считает каждый вид обращения к allocator-у отдельно в текущем потоке. Это
/// отличает горячий путь без создания памяти от пути, который лишь меняет alloc
/// на realloc либо освобождает прежнее владение внутри транзакции. Работа
/// дочерних потоков намеренно не приписывается измеряемому synchronous path.
pub(crate) fn measured_allocator_events<T>(operation: impl FnOnce() -> T) -> (T, AllocatorEvents) {
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
    reset_allocator_events();
    let disable = DisableMeasurement;
    let result = operation();
    let events = current_allocator_events();
    drop(disable);
    (result, events)
}

/// Считает heap-аллокации `operation` в текущем тестовом потоке. Параллельные
/// тесты не могут исказить измерение.
pub(crate) fn measured_allocations<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    let (result, events) = measured_allocator_events(operation);
    (result, events.alloc.saturating_add(events.realloc))
}

#[test]
fn allocator_event_counter_distinguishes_alloc_realloc_and_dealloc() {
    let (mut buffer, allocated) = measured_allocator_events(|| Vec::<u8>::with_capacity(64));
    assert_eq!(allocated.alloc, 1);
    assert_eq!(allocated.realloc, 0);
    assert_eq!(allocated.dealloc, 0);

    let additional = buffer.capacity() + 1;
    let ((), grown) = measured_allocator_events(|| buffer.reserve_exact(additional));
    assert_eq!(grown.alloc, 0);
    assert_eq!(grown.realloc, 1);
    assert_eq!(grown.dealloc, 0);

    let ((), released) = measured_allocator_events(|| drop(buffer));
    assert_eq!(released.alloc, 0);
    assert_eq!(released.realloc, 0);
    assert_eq!(released.dealloc, 1);

    let layout = Layout::from_size_align(64, 8).unwrap();
    let (zeroed, zeroed_events) = measured_allocator_events(|| {
        // SAFETY: непустой `layout` проверен выше; указатель освобождается тем
        // же global allocator и тем же layout сразу после измерения.
        unsafe { std::alloc::alloc_zeroed(layout) }
    });
    assert!(!zeroed.is_null());
    assert_eq!(zeroed_events.alloc, 1);
    assert_eq!(zeroed_events.realloc, 0);
    assert_eq!(zeroed_events.dealloc, 0);
    // SAFETY: `zeroed` получен от global allocator с точным `layout` и ещё не
    // освобождён.
    unsafe { std::alloc::dealloc(zeroed, layout) };

    let ((), checkpointed) = measured_allocator_events(|| {
        let mut buffer = Vec::<u8>::with_capacity(64);
        assert_eq!(current_allocator_events().alloc, 1);
        reset_allocator_events();
        assert_eq!(current_allocator_events(), AllocatorEvents::default());
        let additional = buffer.capacity() + 1;
        buffer.reserve_exact(additional);
        assert_eq!(current_allocator_events().realloc, 1);
        reset_allocator_events();
        drop(buffer);
        assert_eq!(current_allocator_events().dealloc, 1);
    });
    assert_eq!(
        checkpointed,
        AllocatorEvents {
            alloc: 0,
            realloc: 0,
            dealloc: 1,
        }
    );
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
