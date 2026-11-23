// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The unit tests' global allocator, which starts every allocation of
//! alignment 1 (byte buffers, strings) one byte past a 256-byte boundary,
//! unless it is too large to shift: the worst start for the CUDA algebra's
//! host workspace (`src/engine/cuda_algebra/scratch.rs`). What a host slot
//! lends must begin on a 256-byte boundary, so a buffer starting one byte
//! past one skips 255 bytes, every byte the slot holds beyond what it
//! lends. The system allocator often starts large buffers on a boundary,
//! where a slot without those bytes would still pass; under this allocator
//! it never does, so the scratch tests catch it on every host.
//!
//! It is safe for the whole test binary: every allocation is still one the
//! system allocator made, aligned at least as its layout asks, and only
//! allocations of alignment 1, whose start no layout constrains, are moved.
//!
//! It also counts the allocations in one watched range of sizes
//! ([`count_allocations`]), which shows how often a host slot grows: the
//! host memory a slot holds is visible to no fake library.

use std::alloc::{GlobalAlloc, Layout, System};
use std::ops::Range;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Where an allocation of alignment 1 starts in the block holding it: one
/// byte past the block's start, which lies on a [`BOUNDARY`].
const SHIFT: usize = 1;

/// The boundary an allocation of alignment 1 starts [`SHIFT`] bytes past.
pub(super) const BOUNDARY: usize = 256;

struct MisalignedAllocator;

/// The watched range's first size, and its end: an empty range while no
/// range is watched.
static WATCHED_FROM: AtomicUsize = AtomicUsize::new(0);
static WATCHED_TO: AtomicUsize = AtomicUsize::new(0);

/// The allocations made in the watched range since it was set.
static WATCHED_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Held while a range is watched, so two tests never watch at once.
static WATCHING: Mutex<()> = Mutex::new(());

/// How many allocations, by any thread, `run` makes with a size in
/// `sizes`. Tests run in parallel: pick sizes no other test allocates.
pub(super) fn count_allocations(sizes: Range<usize>, run: impl FnOnce()) -> usize {
    let _watching = WATCHING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // The range stays empty (its end at most its start) between stores.
    WATCHED_COUNT.store(0, Ordering::SeqCst);
    WATCHED_FROM.store(sizes.start, Ordering::SeqCst);
    WATCHED_TO.store(sizes.end, Ordering::SeqCst);
    run();
    WATCHED_TO.store(0, Ordering::SeqCst);
    WATCHED_FROM.store(0, Ordering::SeqCst);
    WATCHED_COUNT.load(Ordering::SeqCst)
}

/// Counts an allocation of `size` if the watched range holds it.
fn watch(size: usize) {
    if (WATCHED_FROM.load(Ordering::SeqCst)..WATCHED_TO.load(Ordering::SeqCst)).contains(&size) {
        WATCHED_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

#[global_allocator]
static ALLOCATOR: MisalignedAllocator = MisalignedAllocator;

/// The block that holds an allocation of `layout`, of alignment 1,
/// [`SHIFT`] bytes past its start, or `None` for another alignment or a
/// size too large to shift (beyond `isize::MAX` once rounded to a
/// [`BOUNDARY`]): such a layout is allocated as asked.
fn shifted_block(layout: Layout) -> Option<Layout> {
    if layout.align() != 1 {
        return None;
    }
    let size = layout.size().checked_add(SHIFT)?;
    Layout::from_size_align(size, BOUNDARY).ok()
}

// SAFETY: every pointer returned is either the system allocator's, for the
// layout asked, or `SHIFT` bytes into a block the system allocator made
// `SHIFT` bytes larger, for a layout of alignment 1 only; `dealloc` sees
// the same layout as the allocation (`GlobalAlloc`'s contract) and so
// undoes the same shift. The default `realloc` goes through `alloc` and
// `dealloc`.
unsafe impl GlobalAlloc for MisalignedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        watch(layout.size());
        match shifted_block(layout) {
            // SAFETY: a valid layout, larger than the caller's.
            Some(block) => past_shift(unsafe { System.alloc(block) }),
            // SAFETY: the caller's layout.
            None => unsafe { System.alloc(layout) },
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        watch(layout.size());
        match shifted_block(layout) {
            // SAFETY: a valid layout, larger than the caller's.
            Some(block) => past_shift(unsafe { System.alloc_zeroed(block) }),
            // SAFETY: the caller's layout.
            None => unsafe { System.alloc_zeroed(layout) },
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        match shifted_block(layout) {
            // SAFETY: `alloc` returned `pointer` `SHIFT` bytes into a block
            // of this layout.
            Some(block) => unsafe { System.dealloc(pointer.sub(SHIFT), block) },
            // SAFETY: `alloc` returned the system allocator's pointer for
            // this layout.
            None => unsafe { System.dealloc(pointer, layout) },
        }
    }
}

/// The buffer in a shifted block, [`SHIFT`] bytes past its start; a null
/// pointer (no block) stays null.
fn past_shift(block: *mut u8) -> *mut u8 {
    if block.is_null() {
        block
    } else {
        block.wrapping_add(SHIFT)
    }
}
