// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::isa::{Encoding, ISA, Instruction};
use crate::trace::{Trace, instruction_ids::*};

#[test]
fn test_trace_iteration() {
    let mut trace = Trace::new(2);
    trace.add_operation(1, vec![0], vec![]);
    trace.add_operation(2, vec![1], vec![]);

    assert_eq!(trace.deep_iter().count(), 2);
}

#[test]
fn test_nested_blocks() {
    let mut trace = Trace::new(3);
    trace.add_operation(1, vec![0], vec![]);
    let block = trace.add_block(2);
    block.add_operation(2, vec![1], vec![]);
    let block = block.add_block(3);
    block.add_operation(3, vec![2], vec![]);
    trace.add_operation(1, vec![0], vec![]);

    let repetitions = trace.deep_iter().map(|(_, rep)| rep).collect::<Vec<_>>();
    assert_eq!(repetitions.len(), 4);
    assert_eq!(repetitions, vec![1, 2, 6, 1]);
}

#[test]
fn test_depth_simple() {
    let mut trace = Trace::new(2);
    trace.add_operation(1, vec![0], vec![]);
    trace.add_operation(2, vec![1], vec![]);

    // Operations are parallel
    assert_eq!(trace.depth(), 1);

    trace.add_operation(3, vec![0], vec![]);
    // Operation on qubit 0 is sequential to first one
    assert_eq!(trace.depth(), 2);
}

#[test]
fn test_depth_with_blocks() {
    let mut trace = Trace::new(2);
    trace.add_operation(1, vec![0], vec![]); // Depth 1 on q0

    let block = trace.add_block(2);
    block.add_operation(2, vec![1], vec![]); // Depth 1 on q1 * 2 reps = 2

    // Block acts as barrier *only on qubits it touches*.
    // q1 is touched. q0 is not.
    // q0 stays at depth 1.
    // q1 ends at depth 2.

    trace.add_operation(3, vec![0], vec![]);
    // Next op starts at depth 1 (after op 1). Ends at 2.

    assert_eq!(trace.depth(), 2);
}

#[test]
fn test_depth_parallel_blocks() {
    let mut trace = Trace::new(4);

    let block1 = trace.add_block(1);
    block1.add_operation(1, vec![0], vec![]); // q0: 1

    let block2 = trace.add_block(1);
    block2.add_operation(2, vec![1], vec![]); // q1: 1

    // Blocks are parallel
    assert_eq!(trace.depth(), 1);

    trace.add_operation(3, vec![0, 1], vec![]);
    // Dependent on q0 (1) and q1 (1). Start at 1. End at 2.

    assert_eq!(trace.depth(), 2);
}

#[test]
fn test_depth_entangled() {
    let mut trace = Trace::new(2);
    trace.add_operation(1, vec![0], vec![]); // q0: 1
    trace.add_operation(2, vec![1], vec![]); // q1: 1

    trace.add_operation(3, vec![0, 1], vec![]); // q0, q1 synced at 1 -> end at 2

    assert_eq!(trace.depth(), 2);

    trace.add_operation(4, vec![0], vec![]); // q0: 3
    assert_eq!(trace.depth(), 3);
}

#[test]
fn test_psspc_transform() {
    use crate::trace::{PSSPC, TraceTransform};

    let mut trace = Trace::new(3);

    trace.add_operation(T, vec![0], vec![]);
    trace.add_operation(CCX, vec![0, 1, 2], vec![]);
    trace.add_operation(RZ, vec![0], vec![0.1]);
    trace.add_operation(CX, vec![0, 1], vec![]);
    trace.add_operation(RZ, vec![1], vec![0.2]);
    trace.add_operation(MEAS_Z, vec![0], vec![]);

    // Configure PSSPC with 20 T states per rotation, include CCX magic states
    let psspc = PSSPC::new(20, true);

    let transformed = psspc.transform(&trace).expect("Transformation failed");

    assert_eq!(transformed.compute_qubits(), 12);
    assert_eq!(transformed.depth(), 47);

    assert_eq!(transformed.get_resource_state_count(T), 41);
    assert_eq!(transformed.get_resource_state_count(CCX), 1);

    assert!(transformed.base_error() > 0.0);
    // Error is roughly 5e-9 for 20 Ts
    assert!(transformed.base_error() < 1e-8);
}

#[test]
fn test_lattice_surgery_transform() {
    use crate::trace::{LatticeSurgery, TraceTransform};

    let mut trace = Trace::new(3);

    trace.add_operation(T, vec![0], vec![]);
    trace.add_operation(CX, vec![1, 2], vec![]);
    trace.add_operation(T, vec![0], vec![]);

    assert_eq!(trace.depth(), 2);

    let ls = LatticeSurgery::default();
    let transformed = ls.transform(&trace).expect("Transformation failed");

    assert_eq!(transformed.compute_qubits(), 3);
    assert_eq!(transformed.depth(), 2);

    // Check that we have a LATTICE_SURGERY operation
    // TraceIterator visits the operation definition once, but with a multiplier.
    let ls_ops: Vec<_> = transformed
        .deep_iter()
        .filter(|(gate, _)| gate.id == LATTICE_SURGERY)
        .collect();

    assert_eq!(ls_ops.len(), 1);

    let (gate, mult) = ls_ops[0];
    assert_eq!(gate.id, LATTICE_SURGERY);
    assert_eq!(mult, 2); // Multiplier should carry the repetition count (depth)
}

#[test]
fn test_estimate_simple() {
    let mut trace = Trace::new(1);
    trace.add_operation(T, vec![0], vec![]);

    // Create ISA
    let mut isa = ISA::new();
    isa.add_instruction(Instruction::fixed_arity(
        T,
        Encoding::Logical,
        1,        // arity
        100,      // time
        Some(50), // space
        None,     // length (defaults to arity)
        0.001,    // error_rate
    ));

    let result = trace.estimate(&isa, None).expect("Estimation failed");

    assert!((result.error() - 0.001).abs() <= f64::EPSILON);
    assert_eq!(result.runtime(), 100);
    assert_eq!(result.qubits(), 50);
}

#[test]
fn test_estimate_with_factory() {
    let mut trace = Trace::new(1);
    // Algorithm needs 1000 T states
    trace.increment_resource_state(T, 1000);

    // Some compute runtime to allow factories to run
    trace.add_operation(GENERIC, vec![0], vec![]);

    let mut isa = ISA::new();

    // T factory instruction
    // Produces 1 T state
    isa.add_instruction(Instruction::fixed_arity(
        T,
        Encoding::Logical,
        1,        // arity
        10,       // time to produce 1 state
        Some(50), // space for factory
        None,
        0.0001, // error rate of produced state
    ));

    isa.add_instruction(Instruction::fixed_arity(
        GENERIC,
        Encoding::Logical,
        1,
        1000, // runtime 1000
        Some(200),
        None,
        0.0,
    ));

    let result = trace.estimate(&isa, None).expect("Estimation failed");

    assert_eq!(result.runtime(), 1000);
    assert_eq!(result.qubits(), 700);

    // Check factory result
    let factory_res = result.factories().get(&T).expect("Factory missing");
    assert_eq!(factory_res.copies(), 10);
    assert_eq!(factory_res.runs(), 100);
    assert_eq!(result.factories().len(), 1);
}

#[test]
fn test_trace_display_uses_instruction_names() {
    let mut trace = Trace::new(2);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_operation(CNOT, vec![0, 1], vec![]);
    trace.add_operation(MEAS_Z, vec![0], vec![]);

    let display = format!("{trace}");

    assert!(
        display.contains('H'),
        "Expected 'H' in trace output: {display}"
    );
    assert!(
        display.contains("CNOT"),
        "Expected 'CNOT' in trace output: {display}"
    );
    assert!(
        display.contains("MEAS_Z"),
        "Expected 'MEAS_Z' in trace output: {display}"
    );
}

#[test]
fn test_trace_display_unknown_instruction() {
    let mut trace = Trace::new(1);
    trace.add_operation(0x9999, vec![0], vec![]);

    let display = format!("{trace}");

    assert!(
        display.contains("??"),
        "Expected '??' for unknown instruction in: {display}"
    );
}

#[test]
fn test_block_display_with_repetitions() {
    let mut trace = Trace::new(1);
    let block = trace.add_block(10);
    block.add_operation(H, vec![0], vec![]);

    let display = format!("{trace}");

    assert!(
        display.contains("repeat 10"),
        "Expected 'repeat 10' in: {display}"
    );
    assert!(display.contains('H'), "Expected 'H' in block: {display}");
}

/// Helper to create an ISA with instructions that have known time values.
/// Each entry is (id, arity, time).
fn isa_with_times(entries: &[(u64, u64, u64)]) -> ISA {
    let mut isa = ISA::new();
    for &(id, arity, time) in entries {
        isa.add_instruction(Instruction::fixed_arity(
            id,
            Encoding::Logical,
            arity,
            time,
            Some(1),
            None,
            0.0,
        ));
    }
    isa
}

#[test]
fn test_runtime_single_operation() {
    let mut trace = Trace::new(1);
    trace.add_operation(T, vec![0], vec![]);

    let isa = isa_with_times(&[(T, 1, 100)]);
    let locked = isa.lock();

    assert_eq!(
        trace.runtime(&locked).expect("runtime computation failed"),
        100
    );
}

#[test]
fn test_runtime_parallel_operations() {
    let mut trace = Trace::new(2);
    trace.add_operation(T, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);

    let isa = isa_with_times(&[(T, 1, 100), (H, 1, 50)]);
    let locked = isa.lock();

    // Parallel: runtime is the max of the two = 100
    assert_eq!(
        trace.runtime(&locked).expect("runtime computation failed"),
        100
    );
}

#[test]
fn test_runtime_sequential_operations() {
    let mut trace = Trace::new(1);
    trace.add_operation(T, vec![0], vec![]);
    trace.add_operation(H, vec![0], vec![]);

    let isa = isa_with_times(&[(T, 1, 100), (H, 1, 50)]);
    let locked = isa.lock();

    // Sequential on qubit 0: 100 + 50 = 150
    assert_eq!(
        trace.runtime(&locked).expect("runtime computation failed"),
        150
    );
}

#[test]
fn test_runtime_with_repeated_block() {
    let mut trace = Trace::new(1);
    let block = trace.add_block(5);
    block.add_operation(T, vec![0], vec![]);

    let isa = isa_with_times(&[(T, 1, 100)]);
    let locked = isa.lock();

    // Block depth = 100, repeated 5 times = 500
    assert_eq!(
        trace.runtime(&locked).expect("runtime computation failed"),
        500
    );
}

#[test]
fn test_runtime_nested_blocks() {
    let mut trace = Trace::new(1);
    let outer = trace.add_block(3);
    let inner = outer.add_block(2);
    inner.add_operation(H, vec![0], vec![]);

    let isa = isa_with_times(&[(H, 1, 10)]);
    let locked = isa.lock();

    // Inner: 10 * 2 = 20, outer: 20 * 3 = 60
    assert_eq!(
        trace.runtime(&locked).expect("runtime computation failed"),
        60
    );
}

#[test]
fn test_runtime_multi_qubit_gate() {
    let mut trace = Trace::new(2);
    trace.add_operation(CX, vec![0, 1], vec![]);

    let isa = isa_with_times(&[(CX, 2, 200)]);
    let locked = isa.lock();

    assert_eq!(
        trace.runtime(&locked).expect("runtime computation failed"),
        200
    );
}

#[test]
fn test_runtime_sequential_after_multi_qubit() {
    let mut trace = Trace::new(2);
    trace.add_operation(CX, vec![0, 1], vec![]);
    trace.add_operation(T, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);

    let isa = isa_with_times(&[(CX, 2, 200), (T, 1, 100), (H, 1, 50)]);
    let locked = isa.lock();

    // CX occupies both qubits to time 200
    // T on q0: 200 + 100 = 300
    // H on q1: 200 + 50 = 250
    // max = 300
    assert_eq!(
        trace.runtime(&locked).expect("runtime computation failed"),
        300
    );
}

#[test]
fn test_runtime_empty_trace() {
    let trace = Trace::new(1);

    let isa = ISA::new();
    let locked = isa.lock();

    assert_eq!(
        trace.runtime(&locked).expect("runtime computation failed"),
        0
    );
}

#[test]
fn test_runtime_block_parallel_to_operation() {
    let mut trace = Trace::new(2);
    // Block on q0
    let block = trace.add_block(4);
    block.add_operation(T, vec![0], vec![]);
    // Operation on q1 (parallel to block)
    trace.add_operation(H, vec![1], vec![]);

    let isa = isa_with_times(&[(T, 1, 10), (H, 1, 50)]);
    let locked = isa.lock();

    // Block: 10 * 4 = 40 on q0
    // H: 50 on q1
    // max = 50
    assert_eq!(
        trace.runtime(&locked).expect("runtime computation failed"),
        50
    );
}

#[test]
fn test_runtime_missing_instruction_returns_error() {
    let mut trace = Trace::new(1);
    trace.add_operation(T, vec![0], vec![]);

    // ISA has no T instruction
    let isa = ISA::new();
    let locked = isa.lock();

    assert!(trace.runtime(&locked).is_err());
}

#[test]
fn test_runtime_mixed_sequential_and_parallel() {
    let mut trace = Trace::new(3);
    // q0: T(100) -> CX(200) on q0,q1
    // q1: H(50) -> CX(200) on q0,q1
    // q2: T(100)
    trace.add_operation(T, vec![0], vec![]);
    trace.add_operation(H, vec![1], vec![]);
    trace.add_operation(T, vec![2], vec![]);
    trace.add_operation(CX, vec![0, 1], vec![]);

    let isa = isa_with_times(&[(T, 1, 100), (H, 1, 50), (CX, 2, 200)]);
    let locked = isa.lock();

    // q0: T ends at 100, CX starts at max(100, 50)=100, ends at 300
    // q1: H ends at 50, CX starts at 100, ends at 300
    // q2: T ends at 100
    // max = 300
    assert_eq!(
        trace.runtime(&locked).expect("runtime computation failed"),
        300
    );
}
