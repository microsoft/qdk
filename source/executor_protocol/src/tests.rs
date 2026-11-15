// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The crate's tests: one file per module with run-time behaviour.
//!
//! This module also installs a counting `#[global_allocator]` for the whole
//! test binary, so that the promises on allocations (`ShotSet`'s,
//! `BatchReadouts`', a built readout parity's and a copied `Instruction`'s) are tested:
//! [`allocations`] counts those made on its own thread, so tests running
//! in parallel do not disturb its count.

mod admission;
mod batch_readouts;
mod composition;
mod fake_parts;
mod instruction_set;
mod runner;
mod seed;
mod shot_set;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// The allocations made on this thread while [`allocations`] counts.
    static ALLOCATED: Cell<Option<usize>> = const { Cell::new(None) };
}

/// The system allocator, counting the allocations [`allocations`] asks for,
/// so that a test checks what promises to allocate nothing.
struct CountingAllocator;

impl CountingAllocator {
    fn count() {
        // `try_with`: the thread's counter may be gone as the thread ends.
        let _ = ALLOCATED.try_with(|count| count.set(count.get().map(|n| n + 1)));
    }
}

// SAFETY: each method passes its arguments to the system allocator as it is.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::count();
        // SAFETY: the caller's guarantees, passed on.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::count();
        // SAFETY: the caller's guarantees, passed on.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        Self::count();
        // SAFETY: the caller's guarantees, passed on.
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's guarantees, passed on.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// The allocations `f` makes on this thread.
fn allocations(f: impl FnOnce()) -> usize {
    ALLOCATED.with(|count| count.set(Some(0)));
    f();
    ALLOCATED.with(Cell::take).expect("counting until now")
}
