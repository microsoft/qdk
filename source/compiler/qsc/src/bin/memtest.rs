// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Records the memory usage of the compiler.

use qsc::{CompileUnit, compile};
use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_frontend::compile::PackageStore;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering},
};

/// A wrapper around a memory allocator that tracks allocation amounts.
pub struct AllocationCounter<A: GlobalAlloc> {
    pub allocator: A,
    pub counter: AtomicU64,
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for AllocationCounter<A> {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        self.counter.fetch_add(l.size() as u64, Ordering::SeqCst);
        unsafe { self.allocator.alloc(l) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, l: Layout) {
        unsafe { self.allocator.dealloc(ptr, l) };
        self.counter.fetch_sub(l.size() as u64, Ordering::SeqCst);
    }
}

impl<A: GlobalAlloc> AllocationCounter<A> {
    pub const fn new(allocator: A) -> Self {
        AllocationCounter {
            allocator,
            counter: AtomicU64::new(0),
        }
    }
    pub fn reset(&self) {
        self.counter.store(0, Ordering::SeqCst);
    }
    pub fn read(&self) -> u64 {
        self.counter.load(Ordering::SeqCst)
    }
}

#[global_allocator]
static ALLOCATOR: AllocationCounter<System> = AllocationCounter::new(System);

#[must_use]
pub fn compile_stdlib() -> CompileUnit {
    let store = PackageStore::new(compile::core());
    compile::std(&store, TargetCapabilityFlags::all())
}

fn main() {
    let _stdlib = compile_stdlib();
    let std = ALLOCATOR.read();

    ALLOCATOR.reset();
    println!("{std}");
}
