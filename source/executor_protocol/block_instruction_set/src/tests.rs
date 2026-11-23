// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The crate's tests: one file per module with run-time behaviour, and
//! the fake parts (`fake_parts`) that compose chains on the block set.
//!
//! This module also installs a counting `#[global_allocator]` for the whole
//! test binary, so that the promises on allocations (a built readout
//! parity's, a copied `BlockInstruction`'s) are tested: [`allocations`]
//! counts those made on its own thread, so tests running in parallel do not
//! disturb its count.

mod block_instruction_set;
#[expect(
    clippy::unused_async_trait_impl,
    reason = "a fake part that never waits keeps the trait's `async fn` form, the form the \
              protocol documents; the `core::future::ready` the lint suggests would run the \
              body when the method is called, not when its future is polled"
)]
mod fake_parts;

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
