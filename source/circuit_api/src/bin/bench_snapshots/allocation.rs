// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::{Cell, RefCell},
    collections::BTreeMap,
};

#[derive(Default)]
struct Tracker {
    pointers: BTreeMap<usize, usize>,
    live: usize,
    peak: usize,
    allocations: usize,
    error: Option<&'static str>,
}

impl Tracker {
    fn allocated(&mut self, pointer: usize, size: usize) {
        if pointer == 0 {
            return;
        }
        if self.pointers.insert(pointer, size).is_some() {
            self.error = Some("allocation address was already live");
            return;
        }
        match self.live.checked_add(size) {
            Some(live) => self.live = live,
            None => self.error = Some("live allocation count overflowed"),
        }
        self.peak = self.peak.max(self.live);
        match self.allocations.checked_add(1) {
            Some(count) => self.allocations = count,
            None => self.error = Some("allocation event count overflowed"),
        }
    }

    fn freed(&mut self, pointer: usize, size: usize) {
        match self.pointers.remove(&pointer) {
            Some(held) if held == size => self.live -= held,
            Some(_) => self.error = Some("deallocation layout differs from allocation"),
            None => self.error = Some("measurement crossed an allocation lifetime boundary"),
        }
    }

    fn reallocated(&mut self, old: usize, old_size: usize, new: usize, new_size: usize) {
        if new != 0 {
            self.freed(old, old_size);
            self.allocated(new, new_size);
        }
    }
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static TRACKER: RefCell<Option<Tracker>> = const { RefCell::new(None) };
}

fn record(update: impl FnOnce(&mut Tracker)) {
    let _ = ENABLED.try_with(|enabled| {
        if enabled.replace(false) {
            let _ = TRACKER.try_with(|slot| {
                if let Some(tracker) = slot.borrow_mut().as_mut() {
                    update(tracker);
                }
            });
            // Tracking is disabled while its own pointer map allocates.
            enabled.set(true);
        }
    });
}

pub struct MeasuredAllocator;

// SAFETY: allocation ownership, layouts and alignment are delegated unchanged
// to System. Bookkeeping never dereferences recorded addresses.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies a valid allocation layout.
        let pointer = unsafe { System.alloc(layout) };
        record(|tracker| tracker.allocated(pointer as usize, layout.size()));
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies a valid allocation layout.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        record(|tracker| tracker.allocated(pointer as usize, layout.size()));
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record(|tracker| tracker.freed(pointer as usize, layout.size()));
        // SAFETY: the caller supplies the live allocation and its layout.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the caller supplies a live allocation and valid new size.
        let new = unsafe { System.realloc(pointer, layout, size) };
        record(|tracker| tracker.reallocated(pointer as usize, layout.size(), new as usize, size));
        new
    }
}

#[derive(Debug)]
pub struct Measurement {
    pub peak_bytes: usize,
    pub allocations: usize,
}

pub struct Window {
    active: bool,
}

impl Window {
    pub fn start() -> Result<Self, &'static str> {
        TRACKER.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_some() {
                return Err("allocation measurements cannot nest");
            }
            *slot = Some(Tracker::default());
            Ok(())
        })?;
        ENABLED.with(|enabled| enabled.set(true));
        Ok(Self { active: true })
    }

    pub fn finish(mut self) -> Result<Measurement, &'static str> {
        ENABLED.with(|enabled| enabled.set(false));
        self.active = false;
        let tracker = TRACKER
            .with(|slot| slot.borrow_mut().take())
            .ok_or("allocation measurement is not active")?;
        if let Some(error) = tracker.error {
            return Err(error);
        }
        if tracker.live != 0 || !tracker.pointers.is_empty() {
            return Err("measured output escaped its allocation window");
        }
        Ok(Measurement {
            peak_bytes: tracker.peak,
            allocations: tracker.allocations,
        })
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        if self.active {
            ENABLED.with(|enabled| enabled.set(false));
            TRACKER.with(|slot| drop(slot.borrow_mut().take()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Tracker, Window};

    #[test]
    fn allocations_and_frees_record_peak_live_bytes() {
        let mut tracker = Tracker::default();
        tracker.allocated(1, 16);
        tracker.allocated(2, 32);
        tracker.freed(1, 16);
        tracker.freed(2, 32);
        assert_eq!(
            (tracker.live, tracker.peak, tracker.allocations),
            (0, 48, 2)
        );
        assert!(tracker.error.is_none());
    }

    #[test]
    fn a_growing_reallocation_counts_only_the_live_delta() {
        let mut tracker = Tracker::default();
        tracker.allocated(1, 16);
        tracker.reallocated(1, 16, 2, 64);
        tracker.freed(2, 64);
        assert_eq!(
            (tracker.live, tracker.peak, tracker.allocations),
            (0, 64, 2)
        );
    }

    #[test]
    fn a_shrinking_reallocation_retains_the_earlier_peak() {
        let mut tracker = Tracker::default();
        tracker.allocated(1, 64);
        tracker.reallocated(1, 64, 1, 16);
        tracker.freed(1, 16);
        assert_eq!((tracker.live, tracker.peak), (0, 64));
    }

    #[test]
    fn failed_allocation_does_not_change_measurements() {
        let mut tracker = Tracker::default();
        tracker.allocated(0, 64);
        assert_eq!((tracker.live, tracker.peak, tracker.allocations), (0, 0, 0));
    }

    #[test]
    fn failed_reallocation_leaves_the_original_allocation_live() {
        let mut tracker = Tracker::default();
        tracker.allocated(1, 16);
        tracker.reallocated(1, 16, 0, 64);
        assert_eq!(
            (tracker.live, tracker.peak, tracker.allocations),
            (16, 16, 1)
        );
        tracker.freed(1, 16);
        assert!(tracker.error.is_none());
    }

    #[test]
    fn an_untracked_free_is_not_reported_as_a_valid_sample() {
        let mut tracker = Tracker::default();
        tracker.freed(1, 16);
        assert!(tracker.error.is_some());
    }

    #[test]
    fn actual_native_allocations_are_measured_without_counting_bookkeeping() {
        let window = Window::start().expect("start");
        let bytes = std::hint::black_box(vec![1_u8; 4096]);
        std::hint::black_box(&bytes);
        drop(bytes);
        let measurement = window.finish().expect("balanced native allocation");
        assert_eq!((measurement.peak_bytes, measurement.allocations), (4096, 1));
    }

    #[test]
    fn nested_windows_are_rejected() {
        let outer = Window::start().expect("start");
        assert!(Window::start().is_err());
        assert_eq!(outer.finish().expect("finish").allocations, 0);
    }
}
