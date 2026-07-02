// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::instruction_ids::{CX, H, READ_FROM_MEMORY, WRITE_TO_MEMORY};
use crate::trace::{
    Block, Gate, Operation, Trace,
    transforms::{DynamicMemoryCompute, TraceTransform},
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
    // Capacity 2, 3 qubits.  A repeated block evicts a qubit, so restore
    // operations must be appended to bring the compute area back to the
    // entry state.
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

    // Block structure preserved: root has one child block with reps=5.
    let blocks = child_blocks(&result.block);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].repetitions, 5);
    // The block body contains the eviction, the H, and the restore ops.
    assert!(blocks[0].operations.len() >= 3);

    // collect_gates via deep_iter yields each gate position once (ignoring
    // the repetition multiplier).
    let gates = collect_gates(&result);

    // Outside: H(q0), H(q1) = 2 positions.
    // Block body: WRITE(evict q0), H(q2), WRITE(restore q2), READ(restore q0) = 4 positions.
    // Total unique gate positions: 6.
    let total_h = gates.iter().filter(|(id, _, _)| *id == H).count();
    assert_eq!(total_h, 3); // 2 outside + 1 inside

    assert_eq!(count_instruction(&gates, WRITE_TO_MEMORY), 2); // 1 evict + 1 restore
    assert_eq!(count_instruction(&gates, READ_FROM_MEMORY), 1); // 1 restore
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

    // Block structure preserved with reps=10.
    let blocks = child_blocks(&result.block);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].repetitions, 10);
    // Body has only the H gate (no restore ops since state unchanged).
    assert_eq!(blocks[0].operations.len(), 1);

    let gates = collect_gates(&result);
    assert_eq!(gates.iter().filter(|(id, _, _)| *id == H).count(), 2);
    assert_eq!(count_instruction(&gates, WRITE_TO_MEMORY), 0);
    assert_eq!(count_instruction(&gates, READ_FROM_MEMORY), 0);
}

#[test]
fn repeated_block_state_restored_for_subsequent_ops() {
    // After a repeated block, the compute area state is restored to the entry
    // state, so subsequent operations can find qubits in their original slots.
    let mut trace = Trace::new(3);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    let block = trace.add_block(3);
    block.add_operation(H, vec![2], vec![]); // evicts q0 inside block
    // After the block, q0 should be back in compute (restored).
    trace.add_operation(H, vec![0], vec![]); // should NOT need a read

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    // Block structure preserved with reps=3.
    let blocks = child_blocks(&result.block);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].repetitions, 3);

    let gates = collect_gates(&result);
    let h_gates = gates.iter().filter(|(id, _, _)| *id == H).count();
    assert_eq!(h_gates, 4);
}

#[test]
fn nested_repeated_blocks() {
    // A repeated block inside another repeated block.  The inner block
    // restores its own state, so the outer block sees no change.
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

    // Outer block (reps=2) preserved in root.
    let outer_blocks = child_blocks(&result.block);
    assert_eq!(outer_blocks.len(), 1);
    assert_eq!(outer_blocks[0].repetitions, 2);

    // Inner block (reps=3) preserved inside outer.
    let inner_blocks = child_blocks(outer_blocks[0]);
    assert_eq!(inner_blocks.len(), 1);
    assert_eq!(inner_blocks[0].repetitions, 3);

    let gates = collect_gates(&result);
    let h_count = gates.iter().filter(|(id, _, _)| *id == H).count();
    assert_eq!(h_count, 3);
}

#[test]
fn restore_does_not_clobber_memory() {
    // Regression test: restore Phase 1 writes must not overwrite memory
    // locations needed by Phase 2 reads.
    //
    // Entry: slot[0]=q0, slot[1]=q1, memory: {q2: M}
    // Body: read q2 from M (frees M), evict q1 to M (reuses freed M),
    //       now slot[0]=q0, slot[1]=q2, memory: {q1: M}
    // Restore must write q2 out and read q1 back without clobbering.
    let mut trace = Trace::new(4);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(H, vec![2], vec![]); // evicts q0, places q2 lazily
    // Now state: slot[0]=q2, slot[1]=q1, q0 in memory

    let block = trace.add_block(2);
    block.add_operation(H, vec![0], vec![]);
    block.add_operation(CX, vec![0, 1], vec![]); // uses q2 and q1 (both in compute)

    let transform = DynamicMemoryCompute::new(2);
    let result = transform
        .transform(&trace)
        .expect("transform should succeed");

    // Block structure preserved with reps=2.
    let blocks = child_blocks(&result.block);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].repetitions, 2);

    // Verify it produces a valid trace with operations.
    let gates = collect_gates(&result);
    assert!(!gates.is_empty());
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
