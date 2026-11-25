// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The qubits of a call on any number of them ([`QirQubits`]) allocate
//! nothing up to four, and are allocated past four.
//!
//! Why this test exists: `QirQubits` holds up to four qubits in place
//! for speed. Correlated noise, its only call, took 25 ns on one qubit
//! when its qubits were boxed, and 6 ns held in place (the machine alone,
//! fastest of 7 runs). A change back to boxed qubits would pass every
//! other test and only slow the machine; this test catches it.
//!
//! Why it uses an allocator of its own: Rust offers no other way to
//! count allocations, so the test installs a global allocator that
//! delegates each to the system's and counts those of the threads that
//! ask it to. It exists only in this test binary, and the binary holds
//! only this test, so that no other test runs under it; the library
//! and everything it ships use the system's allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use qdk_simulators::qir::QirQubits;

thread_local! {
    /// The allocations made on this thread, while it counts them.
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

/// The system's allocator, counting the allocations of the threads that
/// count them (see [`allocations`]).
struct Counting;

impl Counting {
    fn count() {
        // `try_with`: a thread allocates while its locals are torn down.
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get().map(|count| count + 1)));
    }
}

// SAFETY: each method delegates to `System`, with the caller's arguments,
// so it keeps `GlobalAlloc`'s contract as `System` does.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::count();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::count();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        Self::count();
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// What `run` returns, and the allocations it made on this thread.
fn allocations<T>(run: impl FnOnce() -> T) -> (T, usize) {
    ALLOCATIONS.with(|count| count.set(Some(0)));
    let output = black_box(run());
    let count = ALLOCATIONS.with(Cell::take);
    (output, count.expect("this thread counted its allocations"))
}

/// Calls on few qubits, the common case: none, one, and four, the most
/// held in place, which allocate nothing; and correlated noise on five
/// qubits, one more than fit in place, which are allocated.
#[test]
fn collected_qubits_allocate_only_past_four() {
    for len in [0, 1, 4, 5] {
        let (qubits, allocations) = allocations(|| (0..len).collect::<QirQubits>());
        assert!(qubits.iter().copied().eq(0..len), "{qubits:?}");
        assert_eq!(
            allocations > 0,
            len > 4,
            "{len} qubits: {allocations} allocations"
        );
    }
}
