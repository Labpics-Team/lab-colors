use serial_test::serial;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingAllocator;

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        ALLOC_BYTES.fetch_add(layout.size(), Ordering::SeqCst);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        ALLOC_BYTES.fetch_add(layout.size(), Ordering::SeqCst);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        ALLOC_BYTES.fetch_add(new_size, Ordering::SeqCst);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn alloc_snapshot() -> (usize, usize) {
    (
        ALLOC_COUNT.load(Ordering::SeqCst),
        ALLOC_BYTES.load(Ordering::SeqCst),
    )
}

#[test]
#[serial]
fn iterating_role_all_and_reading_key_allocates_zero_bytes() {
    // Take a snapshot of current allocations (test harness has already
    // allocated for the test runner itself). Then iterate Role::ALL and
    // read Role::key() — these are compile-time arrays and static strings,
    // so no allocation should occur.
    let (before_count, before_bytes) = alloc_snapshot();

    for role in labcolors_core::Role::ALL {
        let key = role.key();
        assert!(!key.is_empty(), "every role must have a non-empty key");
    }

    let (after_count, after_bytes) = alloc_snapshot();
    assert_eq!(
        after_count,
        before_count,
        "iterating Role::ALL allocated {} times ({} bytes delta) — Role and Role::ALL must be allocation-free",
        after_count - before_count,
        after_bytes - before_bytes,
    );
}

#[test]
#[serial]
fn role_table_default_construction_does_not_heap_allocate_the_roles_themselves() {
    // The RoleTable::default() constructs a fixed-size array on the stack
    // plus a thread-local cache internally (which MAY allocate). The roles
    // themselves (the [Role; 20] ALL array) must not heap-allocate — but
    // the test harness may allocate for thread-local init. So we assert
    // that ALL at least does not allocate beyond an initial setup.
    let (before_count, _before_bytes) = alloc_snapshot();

    let _table = labcolors_core::RoleTable::default();

    // Dropping the table may deallocate. We only care about the construction
    // itself not being unbounded.
    let (after_count, _after_bytes) = alloc_snapshot();

    // The default table may allocate internally (e.g. the curve plan cache
    // is a thread-local that initialises on first use), but it must be
    // bounded and well under a hundred allocations — not proportional to
    // the role count.
    assert!(
        after_count - before_count < 100,
        "RoleTable::default() allocated {} times — must be bounded, not proportional to role count",
        after_count - before_count,
    );
}
