// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::instruction_ids::{CX, H, READ_FROM_MEMORY, WRITE_TO_MEMORY};
use crate::property_keys::{ALGORITHM_COMPUTE_QUBITS, ALGORITHM_MEMORY_QUBITS};
use crate::trace::{
    Block, Gate, Operation, Property, Trace,
    transforms::{DynamicMemoryCompute, TraceTransform, Unroll},
};

/// Collect all gates from a trace into a vec of (id, qubits, params).
fn collect_gates(trace: &Trace) -> Vec<(u64, Vec<u64>, Vec<f64>)> {
    trace
        .deep_iter()
        .map(|(Gate { id, qubits, params }, _)| (*id, qubits.clone(), params.clone()))
        .collect()
}

/// Count occurrences of a specific instruction in the collected gates.
fn count_instruction(gates: &[(u64, Vec<u64>, Vec<f64>)], instr: u64) -> usize {
    gates.iter().filter(|(id, _, _)| *id == instr).count()
}

/// Return references to top-level `BlockOperation`s in a block.
fn child_blocks(block: &Block) -> Vec<&Block> {
    block
        .operations
        .iter()
        .filter_map(|op| match op {
            Operation::BlockOperation(b) => Some(b),
            Operation::GateOperation(..) => None,
        })
        .collect()
}

/// Assert that transforming `trace` gives the same instruction counts and
/// memory size as transforming its fully unrolled equivalent.
fn assert_matches_unrolled(trace: &Trace, capacity: u64) {
    let unrolled = Unroll.transform(trace).expect("transform should succeed");
    let transform = DynamicMemoryCompute::new(capacity);
    let compact = transform
        .transform(trace)
        .expect("transform should succeed");
    let expected = transform
        .transform(&unrolled)
        .expect("transform should succeed");
    assert_eq!(
        compact.gate_counts(),
        expected.gate_counts(),
        "capacity {capacity}"
    );
    assert_eq!(
        compact.memory_qubits(),
        expected.memory_qubits(),
        "capacity {capacity}"
    );
}

// ---------- Flat trace tests ----------

#[test]
fn no_memory_needed_when_capacity_exceeds_qubits() {
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(CX, vec![0, 1], vec![]);

    let transform = DynamicMemoryCompute::new(5);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 3);
    assert!(!result.has_memory_qubits());
    assert_eq!(collect_gates(&result).len(), 2);
}

#[test]
fn no_memory_needed_when_capacity_equals_qubits() {
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);

    let transform = DynamicMemoryCompute::new(3);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 3);
    assert!(!result.has_memory_qubits());
}

#[test]
fn single_eviction_and_load() {
    // 3 logical qubits, capacity 2.
    // First two distinct qubits (0, 1) are placed lazily into compute.
    // Qubit 2 triggers eviction on third encounter.

    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![2], vec![]); // needs eviction

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 2);
    assert_eq!(result.memory_qubits(), Some(1));

    let gates = collect_gates(&result);

    // H(0), H(1), WRITE_TO_MEMORY, H(mapped slot)
    assert_eq!(gates.len(), 4);
    assert_eq!(gates[0].0, H);
    assert_eq!(gates[1].0, H);
    assert_eq!(gates[2].0, WRITE_TO_MEMORY);
    assert_eq!(gates[3].0, H);
}

#[test]
fn replaces_algorithm_qubit_properties() {
    // Traces from Q# record the pre-placement split, which placement changes.
    let mut trace = Trace::new(3);
    trace.set_property(ALGORITHM_COMPUTE_QUBITS, Property::Int(3));
    trace.set_property(ALGORITHM_MEMORY_QUBITS, Property::Int(0));
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![2], vec![]); // evicts q0

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    let property = |key| result.get_property(key).and_then(Property::as_int);
    assert_eq!(property(ALGORITHM_COMPUTE_QUBITS), Some(2));
    assert_eq!(property(ALGORITHM_MEMORY_QUBITS), Some(1));
}

#[test]
fn qubit_already_in_compute_no_swap() {
    // 3 qubits, capacity 2.  Only touch qubits 0 and 1 — both fit lazily.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(CX, vec![0, 1], vec![]);

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    let gates = collect_gates(&result);
    assert_eq!(gates.len(), 2);
    assert_eq!(gates[0].0, H);
    assert_eq!(gates[1].0, CX);
    assert!(!result.has_memory_qubits());
}

#[test]
fn multiple_evictions() {
    // 4 logical qubits, capacity 2.
    // Touch qubit 0, then 1 (fills compute), then 2, then 3 — two evictions.
    let mut trace = Trace::new(4);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![2], vec![]);
    trace.add_operation(H, vec![3], vec![]);

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 2);
    assert!(result.has_memory_qubits());

    let gates = collect_gates(&result);
    assert_eq!(count_instruction(&gates, READ_FROM_MEMORY), 0);
    assert_eq!(count_instruction(&gates, WRITE_TO_MEMORY), 2);
}

#[test]
fn reuse_qubit_already_loaded() {
    // Qubit 2 is placed lazily, then reused without eviction.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![2], vec![]);
    trace.add_operation(H, vec![2], vec![]);

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    let gates = collect_gates(&result);
    assert_eq!(count_instruction(&gates, READ_FROM_MEMORY), 0);
    assert_eq!(gates.len(), 2);
}

#[test]
fn error_on_trace_with_memory_qubits() {
    let mut trace = Trace::new(2);
    trace.set_memory_qubits(1);

    let transform = DynamicMemoryCompute::new(2);
    assert!(transform.transform(&trace).is_err());
}

#[test]
fn error_on_gate_arity_exceeds_capacity() {
    let mut trace = Trace::new(3);
    trace.add_operation(42, vec![0, 1, 2], vec![]);

    let transform = DynamicMemoryCompute::new(2);
    assert!(transform.transform(&trace).is_err());
}

#[test]
fn two_qubit_gate_with_eviction() {
    // 3 qubits, capacity 2.  CX on qubits 0 and 2 — both placed lazily.
    // Qubit 1 is never acted upon, so no memory is needed at all.
    let mut trace = Trace::new(3);
    trace.add_operation(CX, vec![0, 2], vec![]);

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert!(!result.has_memory_qubits());

    let gates = collect_gates(&result);
    assert_eq!(count_instruction(&gates, WRITE_TO_MEMORY), 0);
    assert_eq!(count_instruction(&gates, READ_FROM_MEMORY), 0);
    assert_eq!(gates.len(), 1);
    assert_eq!(gates[0].0, CX);
    assert_eq!(gates[0].1.len(), 2);
    assert_ne!(gates[0].1[0], gates[0].1[1]);
}

#[test]
fn empty_trace() {
    let trace = Trace::new(5);
    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 2);
    assert_eq!(collect_gates(&result).len(), 0);
}

#[test]
fn memory_slot_reuse() {
    // q0 and q1 fill compute lazily, q2 evicts q0, then q0 must be read back.
    let mut trace = Trace::new(4);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![2], vec![]);
    trace.add_operation(H, vec![0], vec![]); // q0 was evicted, needs read

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert!(result.has_memory_qubits());
    let gates = collect_gates(&result);
    assert_eq!(gates.iter().filter(|(id, _, _)| *id == H).count(), 4);
    assert_eq!(count_instruction(&gates, WRITE_TO_MEMORY), 2);
    assert_eq!(count_instruction(&gates, READ_FROM_MEMORY), 1);
}

#[test]
fn lazy_placement_with_sparse_qubit_ids() {
    // Qubits 10 and 20 are the only ones used — placed lazily.
    let mut trace = Trace::new(100);
    trace.add_operation(H, vec![10], vec![]);
    trace.add_operation(CX, vec![10, 20], vec![]);

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 2);
    assert!(!result.has_memory_qubits());
    assert_eq!(collect_gates(&result).len(), 2);
}

#[test]
fn evict_and_reload_round_trip() {
    // q0 placed lazily, q1 placed lazily, q2 evicts q0, then q0 must reload.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![2], vec![]); // evicts q0
    trace.add_operation(H, vec![0], vec![]); // reloads q0

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    let gates = collect_gates(&result);
    assert_eq!(count_instruction(&gates, WRITE_TO_MEMORY), 2);
    assert_eq!(count_instruction(&gates, READ_FROM_MEMORY), 1);
    assert_eq!(result.memory_qubits(), Some(1));
}

// ---------- Block tests ----------

#[test]
fn block_with_single_repetition_no_restore() {
    // A block with repetitions=1 behaves like a flat sequence.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    let block = trace.add_block(1);
    block.add_operation(H, vec![1], vec![]);
    block.add_operation(H, vec![2], vec![]); // evicts q0

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    // Block structure is preserved: root has one child block with reps=1.
    let blocks = child_blocks(&result.block);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].repetitions, 1);

    let gates = collect_gates(&result);
    assert_eq!(gates.iter().filter(|(id, _, _)| *id == H).count(), 3);
    assert_eq!(count_instruction(&gates, WRITE_TO_MEMORY), 1);
    assert_eq!(count_instruction(&gates, READ_FROM_MEMORY), 0);
}

#[test]
fn repeated_block_adds_restore_ops() {
    // Capacity 2, 3 qubits.  Only the first iteration of the repeated block
    // evicts a qubit; later iterations find q2 in compute.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]); // lazy place q0
    trace.add_operation(H, vec![1], vec![]); // lazy place q1
    let block = trace.add_block(5);
    block.add_operation(H, vec![2], vec![]); // evicts q0, lazy places q2

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert!(result.has_memory_qubits());

    // The first iteration is emitted separately; the remaining four share
    // one body.
    let blocks = child_blocks(&result.block);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].repetitions, 1);
    assert_eq!(blocks[1].repetitions, 4);
    assert_matches_unrolled(&trace, 2);
}

#[test]
fn repeated_block_no_change_no_restore() {
    // If the repeated block doesn't change the compute layout, no restore
    // operations are needed.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    let block = trace.add_block(10);
    block.add_operation(H, vec![0], vec![]); // q0 already in compute

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    // The first iteration is emitted separately; the remaining nine share
    // one body.
    let blocks = child_blocks(&result.block);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].repetitions, 1);
    assert_eq!(blocks[1].repetitions, 9);
    // Body has only the H gate (no restore ops since state unchanged).
    assert_eq!(blocks[1].operations.len(), 1);

    let gates = collect_gates(&result);
    assert_eq!(gates.iter().filter(|(id, _, _)| *id == H).count(), 3);
    assert_eq!(count_instruction(&gates, WRITE_TO_MEMORY), 0);
    assert_eq!(count_instruction(&gates, READ_FROM_MEMORY), 0);
}

#[test]
fn repeated_block_state_restored_for_subsequent_ops() {
    // After a repeated block, the compute area holds what the unrolled loop
    // leaves behind, so the final H(q0) reads q0 back from memory.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    let block = trace.add_block(3);
    block.add_operation(H, vec![2], vec![]); // evicts q0 inside block
    trace.add_operation(H, vec![0], vec![]);

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    // The first iteration is emitted separately; the remaining two share one
    // body.
    let blocks = child_blocks(&result.block);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].repetitions, 1);
    assert_eq!(blocks[1].repetitions, 2);
    assert_matches_unrolled(&trace, 2);
}

#[test]
fn zero_repetition_block_does_not_change_placement() {
    let mut trace = Trace::new(2);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_block(0).add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![0], vec![]);

    assert_matches_unrolled(&trace, 1);
}

#[test]
fn nested_repeated_blocks() {
    // A repeated block inside another repeated block.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    let outer = trace.add_block(2);
    let inner = outer.add_block(3);
    inner.add_operation(H, vec![2], vec![]); // evicts q0

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert!(result.has_memory_qubits());
    assert_matches_unrolled(&trace, 2);
}

#[test]
fn repeated_blocks_match_fully_unrolled_lru_traffic() {
    // At most of these capacities, compute slots permute between iterations.
    let mut trace = Trace::new(6);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    let outer = trace.add_block(5);
    outer.add_operation(CX, vec![2, 3], vec![]);
    let inner = outer.add_block(3);
    inner.add_operation(H, vec![4], vec![]);
    inner.add_operation(CX, vec![4, 5], vec![]);
    outer.add_operation(H, vec![0], vec![]);

    for capacity in 2..6 {
        assert_matches_unrolled(&trace, capacity);
    }
}

#[test]
fn restore_does_not_clobber_memory() {
    // A repeated block that reads an evicted qubit back and reuses its freed
    // memory location.
    let mut trace = Trace::new(4);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![2], vec![]); // evicts q0, places q2 lazily
    // Now state: slot[0]=q2, slot[1]=q1, q0 in memory

    let block = trace.add_block(2);
    block.add_operation(H, vec![0], vec![]);
    block.add_operation(CX, vec![0, 1], vec![]); // uses q2 and q1 (both in compute)

    assert_matches_unrolled(&trace, 2);
}

// ---------- Percentage-based capacity tests ----------

#[test]
fn percentage_50_percent_of_4_gives_capacity_2() {
    // 50% of 4 qubits = 2 compute slots.
    let mut trace = Trace::new(4);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![2], vec![]);
    trace.add_operation(H, vec![3], vec![]);

    let transform = DynamicMemoryCompute::with_percentage(0.5);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 2);
    assert!(result.has_memory_qubits());
}

#[test]
fn percentage_100_percent_returns_clone() {
    // 100% means all qubits fit — no memory needed.
    let mut trace = Trace::new(4);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);

    let transform = DynamicMemoryCompute::with_percentage(1.0);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 4);
    assert!(!result.has_memory_qubits());
}

#[test]
fn percentage_floors_to_whole_number() {
    // 30% of 10 = 3.0 → capacity 3.
    let mut trace = Trace::new(10);
    trace.add_operation(H, vec![0], vec![]);

    let transform = DynamicMemoryCompute::with_percentage(0.3);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 3);
}

#[test]
fn percentage_clamps_to_at_least_one() {
    // A very small percentage on a small trace should give at least 1.
    let mut trace = Trace::new(2);
    trace.add_operation(H, vec![0], vec![]);

    let transform = DynamicMemoryCompute::with_percentage(0.01);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 1);
}

// ---------- Eviction strategy tests ----------

use super::EvictionStrategy;

#[test]
fn lru_evicts_least_recently_used() {
    // Capacity 2, 3 qubits.  Access q0, q1, then q2.
    // LRU should evict q0 (least recently used) when q2 arrives.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    // Touch q0 again to make q1 the least recently used.
    trace.add_operation(H, vec![0], vec![]);
    // Now access q2 — LRU should evict q1 (not q0).
    trace.add_operation(CX, vec![0, 2], vec![]);

    let transform = DynamicMemoryCompute::new(2).with_strategy(EvictionStrategy::LeastRecentlyUsed);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    let gates = collect_gates(&result);
    // Only 1 eviction needed (for q2), and it should evict q1.
    assert_eq!(count_instruction(&gates, WRITE_TO_MEMORY), 1);
    // The CX gate uses q0 and q2 — both should be in compute.
    // If q0 were evicted instead, there would be an extra read.
    assert_eq!(count_instruction(&gates, READ_FROM_MEMORY), 0);
}

#[test]
fn lfu_evicts_least_frequently_used() {
    // Capacity 2, 3 qubits.  Access q0 three times, q1 once, then q2.
    // LFU should evict q1 (least frequent) when q2 arrives.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![0], vec![]);
    // Now q0 has freq 3, q1 has freq 1.  Requesting q2 should evict q1.
    trace.add_operation(CX, vec![0, 2], vec![]);

    let transform =
        DynamicMemoryCompute::new(2).with_strategy(EvictionStrategy::LeastFrequentlyUsed);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    let gates = collect_gates(&result);
    assert_eq!(count_instruction(&gates, WRITE_TO_MEMORY), 1);
    assert_eq!(count_instruction(&gates, READ_FROM_MEMORY), 0);
}

#[test]
fn first_available_evicts_first_candidate() {
    // Same setup but with FirstAvailable — just verifying it works.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![2], vec![]);

    let transform = DynamicMemoryCompute::new(2).with_strategy(EvictionStrategy::FirstAvailable);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 2);
    assert_eq!(result.memory_qubits(), Some(1));
}

#[test]
fn lru_fewer_memory_ops_than_first_available() {
    // Pattern where LRU should produce fewer memory ops than FirstAvailable:
    // Access q0, q1, then repeatedly access q0 and q2.
    // LRU knows q0 was recently used and evicts q1.
    // FirstAvailable might evict q0 (first slot) causing extra reads.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![0], vec![]); // touch q0
    trace.add_operation(H, vec![2], vec![]); // evicts q1 (LRU) or q0 (first)
    trace.add_operation(H, vec![0], vec![]); // q0 in compute for LRU, needs read for first

    let lru_transform =
        DynamicMemoryCompute::new(2).with_strategy(EvictionStrategy::LeastRecentlyUsed);
    let lru_result = lru_transform
        .transform(&trace)
        .expect("LRU transform should succeed");

    let first_transform =
        DynamicMemoryCompute::new(2).with_strategy(EvictionStrategy::FirstAvailable);
    let first_result = first_transform
        .transform(&trace)
        .expect("FirstAvailable transform should succeed");

    let lru_gates = collect_gates(&lru_result);
    let first_gates = collect_gates(&first_result);

    let lru_reads = count_instruction(&lru_gates, READ_FROM_MEMORY);
    let first_reads = count_instruction(&first_gates, READ_FROM_MEMORY);

    // LRU should need fewer or equal reads.
    assert!(lru_reads <= first_reads);
}
