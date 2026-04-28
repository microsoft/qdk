// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[test]
fn test_trace_iteration() {
    use crate::trace::Trace;

    let mut trace = Trace::new(2);
    trace.add_operation(1, vec![0], vec![]);
    trace.add_operation(2, vec![1], vec![]);

    assert_eq!(trace.deep_iter().count(), 2);
}

#[test]
fn test_nested_blocks() {
    use crate::trace::Trace;

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
    use crate::trace::Trace;

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
    use crate::trace::Trace;

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
    use crate::trace::Trace;

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
    use crate::trace::Trace;

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
    use crate::trace::{PSSPC, Trace, TraceTransform, instruction_ids::*};

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
    use crate::trace::{LatticeSurgery, Trace, TraceTransform, instruction_ids::*};

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
    use crate::isa::{Encoding, ISA, Instruction};
    use crate::trace::{Trace, instruction_ids::*};

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
    use crate::isa::{Encoding, ISA, Instruction};
    use crate::trace::{Trace, instruction_ids::*};

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
    use crate::trace::Trace;
    use crate::trace::instruction_ids::{CNOT, H, MEAS_Z};

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
    use crate::trace::Trace;

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
    use crate::trace::Trace;
    use crate::trace::instruction_ids::H;

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
