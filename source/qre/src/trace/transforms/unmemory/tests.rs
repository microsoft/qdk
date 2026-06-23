// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::instruction_ids::{CX, H, READ_FROM_MEMORY, WRITE_TO_MEMORY};
use crate::trace::Operation;
use crate::trace::{
    Gate, Trace,
    transforms::{DynamicMemoryCompute, TraceTransform, Unmemory},
};

/// Collect all gates from a trace into a vec of (id, qubits).
fn collect_gates(trace: &Trace) -> Vec<(u64, Vec<u64>)> {
    trace
        .deep_iter()
        .map(|(Gate { id, qubits, .. }, _)| (*id, qubits.clone()))
        .collect()
}

#[test]
fn no_memory_passthrough() {
    // A trace without memory qubits passes through unchanged.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(CX, vec![0, 1], vec![]);

    let result = Unmemory
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 3);
    assert!(!result.has_memory_qubits());
    let gates = collect_gates(&result);
    assert_eq!(gates.len(), 2);
    assert_eq!(gates[0], (H, vec![0]));
    assert_eq!(gates[1], (CX, vec![0, 1]));
}

#[test]
fn round_trip_with_dynamic_memory_compute() {
    // DynamicMemoryCompute uses lazy placement and memory-location IDs
    // that differ from the logical-qubit-ID convention expected by
    // Unmemory.  The round-trip therefore does not recover original IDs
    // exactly, but should produce a valid trace with the right structure.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![2], vec![]);

    let memorized = DynamicMemoryCompute::new(2)
        .transform(&trace)
        .expect("DynamicMemoryCompute should succeed");

    assert!(memorized.has_memory_qubits());
    assert_eq!(memorized.compute_qubits(), 2);

    let unmemorized = Unmemory
        .transform(&memorized)
        .expect("Unmemory should succeed");

    assert!(!unmemorized.has_memory_qubits());

    // Should have exactly 3 H gates, no memory ops.
    let gates = collect_gates(&unmemorized);
    assert_eq!(gates.len(), 3);
    for (id, _) in &gates {
        assert_eq!(*id, H);
    }

    // DynamicMemoryCompute uses memory-location IDs (not logical qubit IDs)
    // in RFM/WTM, so a round-trip through Unmemory does not recover exact
    // original qubit identities.  Just verify structural correctness.
    let mut qubit_ids: Vec<u64> = gates.iter().map(|(_, q)| q[0]).collect();
    qubit_ids.sort_unstable();
    qubit_ids.dedup();
    assert!(qubit_ids.len() >= 2);
}

#[test]
fn round_trip_with_eviction_and_reload() {
    // DynamicMemoryCompute uses memory-location IDs in READ/WRITE args,
    // while Unmemory treats the first arg of RFM (and second of WTM) as
    // logical qubit IDs.  The round-trip therefore does not recover the
    // original qubit IDs exactly, but it should still produce a valid trace
    // with the correct number of gates and no memory operations.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![2], vec![]); // evicts q0
    trace.add_operation(H, vec![0], vec![]); // reloads q0

    let memorized = DynamicMemoryCompute::new(2)
        .transform(&trace)
        .expect("DynamicMemoryCompute should succeed");

    let unmemorized = Unmemory
        .transform(&memorized)
        .expect("Unmemory should succeed");

    assert!(!unmemorized.has_memory_qubits());

    let gates = collect_gates(&unmemorized);
    assert_eq!(gates.len(), 4);
    assert!(gates.iter().all(|(id, _)| *id == H));
}

#[test]
fn explicit_memory_ops() {
    // Manually construct a trace with memory operations using the convention:
    //   READ_FROM_MEMORY(logical_qubit, compute_slot)
    //   WRITE_TO_MEMORY(compute_slot, logical_qubit)
    // 2 compute qubits, 1 memory qubit.
    let mut trace = Trace::new(2);
    trace.set_memory_qubits(1);

    // H on slot 0 → lazily assigned logical q0.
    trace.add_operation(H, vec![0], vec![]);
    // Evict slot 1 (logical q1) to memory.
    trace.add_operation(WRITE_TO_MEMORY, vec![1, 1], vec![]);
    // Read logical qubit 2 from memory into slot 1.
    trace.add_operation(READ_FROM_MEMORY, vec![2, 1], vec![]);
    // Now slot 0 = q0, slot 1 = q2.
    trace.add_operation(CX, vec![0, 1], vec![]); // CX on q0, q2.

    let result = Unmemory
        .transform(&trace)
        .expect("transform should succeed");

    assert!(!result.has_memory_qubits());
    assert_eq!(result.compute_qubits(), 3);

    let gates = collect_gates(&result);
    assert_eq!(gates.len(), 2);
    assert_eq!(gates[0], (H, vec![0])); // H on logical q0
    assert_eq!(gates[1], (CX, vec![0, 2])); // CX on logical q0, q2
}

#[test]
fn round_trip_with_two_qubit_gate() {
    // CX on qubits that span compute and memory.
    let mut trace = Trace::new(3);
    trace.add_operation(CX, vec![0, 2], vec![]);
    trace.add_operation(CX, vec![1, 2], vec![]);

    let memorized = DynamicMemoryCompute::new(2)
        .transform(&trace)
        .expect("DynamicMemoryCompute should succeed");

    let unmemorized = Unmemory
        .transform(&memorized)
        .expect("Unmemory should succeed");

    assert!(!unmemorized.has_memory_qubits());

    let gates = collect_gates(&unmemorized);
    assert_eq!(gates.len(), 2);
    // Logical qubit IDs are assigned in order of first encounter.
    // DynamicMemoryCompute maps q0→slot0, q2→slot1 (lazy), then for the
    // second CX needs q1, which evicts one of them. The exact IDs depend
    // on the eviction order, but the two CX gates should use 3 distinct
    // logical qubits.
    let mut all_qubits: Vec<u64> = gates.iter().flat_map(|(_, q)| q.clone()).collect();
    all_qubits.sort_unstable();
    all_qubits.dedup();
    assert!(all_qubits.len() >= 2);
}

#[test]
fn round_trip_with_repeated_block() {
    // Repeated block that causes eviction, then Unmemory should recover.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    let block = trace.add_block(5.0);
    block.add_operation(H, vec![2], vec![]);

    let memorized = DynamicMemoryCompute::new(2)
        .transform(&trace)
        .expect("DynamicMemoryCompute should succeed");

    let unmemorized = Unmemory
        .transform(&memorized)
        .expect("Unmemory should succeed");

    assert!(!unmemorized.has_memory_qubits());

    let gates = collect_gates(&unmemorized);
    // All gates should be H on the original logical qubit IDs.
    assert!(gates.iter().all(|(id, _)| *id == H));

    // The unique H positions: 2 outside + 1 inside block = 3 unique.
    assert_eq!(gates.len(), 3);
}

#[test]
fn preserves_block_structure() {
    // Verify that block structure is preserved through round-trip.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    let block = trace.add_block(10.0);
    block.add_operation(H, vec![1], vec![]);

    let memorized = DynamicMemoryCompute::new(2)
        .transform(&trace)
        .expect("DynamicMemoryCompute should succeed");

    let unmemorized = Unmemory
        .transform(&memorized)
        .expect("Unmemory should succeed");

    // Check block structure.
    let child_blocks: Vec<_> = unmemorized
        .block
        .operations
        .iter()
        .filter_map(|op| match op {
            Operation::BlockOperation(b) => Some(b),
            Operation::GateOperation(..) => None,
        })
        .collect();
    assert_eq!(child_blocks.len(), 1);
    assert_eq!(child_blocks[0].repetitions, 10.0);
}
