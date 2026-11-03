// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Allocation behavior of the construction path.
//!
//! Two constraints shape this file, and both are load-bearing.
//!
//! A global allocator can be installed only once per binary, so the counter
//! lives in a dedicated integration test rather than in the library.
//!
//! The counter is process-global while the test harness runs tests in parallel
//! threads. A second test in this binary allocates while this one measures, and
//! those allocations land in the count. Serializing with a mutex is not enough,
//! because the harness allocates outside any lock the tests can take. This file
//! therefore contains exactly one test, with its cases run in sequence.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

use qsc_circuit_store::{
    CircuitStore, ControlFlow, PackedOperation, Qubit, SnapshotInstruction, StandardGate,
};

struct CountingAllocator<A> {
    allocator: A,
    allocations: AtomicU64,
    largest_request: AtomicU64,
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.allocations.fetch_add(1, Ordering::SeqCst);
        self.largest_request
            .fetch_max(layout.size() as u64, Ordering::SeqCst);
        unsafe { self.allocator.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.allocator.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        self.allocations.fetch_add(1, Ordering::SeqCst);
        self.largest_request
            .fetch_max(new_size as u64, Ordering::SeqCst);
        unsafe { self.allocator.realloc(ptr, layout, new_size) }
    }
}

impl<A: GlobalAlloc> CountingAllocator<A> {
    const fn new(allocator: A) -> Self {
        Self {
            allocator,
            allocations: AtomicU64::new(0),
            largest_request: AtomicU64::new(0),
        }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator<System> = CountingAllocator::new(System);

/// Counts allocations performed while `body` runs.
fn allocations_during<F: FnOnce()>(body: F) -> u64 {
    ALLOCATOR.allocations.store(0, Ordering::SeqCst);
    body();
    ALLOCATOR.allocations.load(Ordering::SeqCst)
}

#[test]
fn construction_allocation_behavior() {
    steady_state_append_is_allocation_free();
    allocations_track_distinct_patterns_not_gate_count();
    unreserved_growth_is_logarithmic();
    bulk_construction_is_allocation_free_after_warmup();
    sparse_moment_restoration_does_not_allocate_for_unused_qubits();
}

fn sparse_moment_restoration_does_not_allocate_for_unused_qubits() {
    let mut store = CircuitStore::new(1, 0);
    let body = store.new_block();
    store
        .push_into_block(
            body,
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("body");
    let mut snapshot = store.to_snapshot();
    snapshot.qubit_count = 8_000_000;
    snapshot.instructions.push(SnapshotInstruction {
        operation: PackedOperation::ControlFlow(Box::new(ControlFlow::Moment(body))),
        qubits: vec![],
        clbits: vec![],
        params: None,
    });
    let mut message = qsc_snapshot::binary::Message::new_default();
    let mut root = message.init_root::<qsc_snapshot::snapshot_capnp::document::Builder>();
    root.set_version(qsc_snapshot::MODEL_VERSION);
    snapshot
        .write_binary(root.init_store())
        .expect("write binary");
    let bytes =
        qsc_snapshot::binary::encode(qsc_snapshot::Kind::CircuitStore, &message).expect("binary");
    let mut writer =
        qsc_snapshot::text::Writer::new(qsc_snapshot::Kind::CircuitStore).expect("writer");
    snapshot.write_text(&mut writer).expect("write text");
    let text = writer.finish().expect("text");

    ALLOCATOR.largest_request.store(0, Ordering::SeqCst);
    let restored = CircuitStore::from_bytes(&bytes).expect("restore binary");
    let binary_request = ALLOCATOR.largest_request.load(Ordering::SeqCst);
    assert_eq!(restored.qubit_count(), 8_000_000);
    assert!(
        binary_request < 64 * 1024,
        "binary restoration requested {binary_request} bytes"
    );

    ALLOCATOR.largest_request.store(0, Ordering::SeqCst);
    let restored = CircuitStore::from_text(&text).expect("restore text");
    let text_request = ALLOCATOR.largest_request.load(Ordering::SeqCst);
    assert_eq!(restored.qubit_count(), 8_000_000);
    assert!(
        text_request < 64 * 1024,
        "text restoration requested {text_request} bytes"
    );
}

fn steady_state_append_is_allocation_free() {
    let count = 10_000;
    let mut store = CircuitStore::new(2, 0);
    store.reserve(count + 1);

    // Interning boxes a key the first time it sees an operand list, so the
    // steady-state path is only observable after the pattern is warm.
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("warmup should append");

    let allocations = allocations_during(|| {
        for _ in 0..count {
            store
                .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
                .expect("gate should append");
        }
    });

    assert_eq!(
        allocations, 0,
        "appending {count} interned standard gates into reserved capacity should not allocate"
    );
}

fn allocations_track_distinct_patterns_not_gate_count() {
    let patterns: Vec<Vec<Qubit>> = (0..8).map(|i| vec![Qubit(i), Qubit((i + 1) % 8)]).collect();

    let mut counts = Vec::new();
    for gate_count in [1_000usize, 8_000] {
        let mut store = CircuitStore::new(8, 0);
        store.reserve(gate_count);
        let allocations = allocations_during(|| {
            for index in 0..gate_count {
                store
                    .push_standard(StandardGate::Cx, &patterns[index % patterns.len()])
                    .expect("gate should append");
            }
        });
        counts.push(allocations);
    }

    assert_eq!(
        counts[0], counts[1],
        "allocations should track distinct operand patterns, not gate count: {counts:?}"
    );
}

fn unreserved_growth_is_logarithmic() {
    let count = 4_096;
    let mut store = CircuitStore::new(2, 0);
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("warmup should append");

    let allocations = allocations_during(|| {
        for _ in 0..count {
            store
                .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
                .expect("gate should append");
        }
    });

    assert!(
        allocations < 32,
        "growth reallocations should be logarithmic, saw {allocations} for {count} gates"
    );
}

fn bulk_construction_is_allocation_free_after_warmup() {
    let count = 10_000;
    let operands = [Qubit(0), Qubit(1)];
    let mut store = CircuitStore::new(2, 0);
    store.reserve(count + 1);
    store
        .push_standard(StandardGate::Cx, &operands)
        .expect("warmup should append");

    let batch: Vec<&[Qubit]> = (0..count).map(|_| operands.as_slice()).collect();
    let allocations = allocations_during(|| {
        store
            .extend_standard(StandardGate::Cx, batch.iter().copied())
            .expect("batch should append");
    });

    assert_eq!(
        allocations, 0,
        "the batched path should not allocate beyond the caller's own batch"
    );
}
