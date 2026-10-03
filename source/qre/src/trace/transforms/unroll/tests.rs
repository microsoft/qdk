// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::instruction_ids::{CX, H, RZ};
use crate::property_keys::ALGORITHM_COMPUTE_QUBITS;
use crate::trace::{
    Gate, Operation, Property, Trace,
    transforms::{TraceTransform, Unroll},
};

#[test]
fn expands_nested_blocks_in_execution_order() {
    let mut trace = Trace::new(2);
    trace.add_operation(H, vec![0], vec![]);
    let outer = trace.add_block(2);
    outer.add_operation(CX, vec![0, 1], vec![]);
    outer.add_block(3).add_operation(RZ, vec![1], vec![0.5]);

    let result = Unroll.transform(&trace).expect("transform should succeed");

    assert!(
        result
            .block
            .operations
            .iter()
            .all(|op| matches!(op, Operation::GateOperation(..)))
    );
    let gates: Vec<(u64, &[u64], &[f64])> = result
        .walk_iter()
        .map(|gate| (gate.id(), gate.qubits(), gate.params()))
        .collect();
    let cx = (CX, &[0, 1][..], &[][..]);
    let rz = (RZ, &[1][..], &[0.5][..]);
    assert_eq!(
        gates,
        vec![(H, &[0][..], &[][..]), cx, rz, rz, rz, cx, rz, rz, rz]
    );
}

#[test]
fn drops_blocks_with_zero_repetitions() {
    let mut trace = Trace::new(1);
    trace.add_operation(H, vec![0], vec![]);
    trace.add_block(0).add_operation(RZ, vec![0], vec![0.5]);

    let result = Unroll.transform(&trace).expect("transform should succeed");

    let ids: Vec<u64> = result.walk_iter().map(Gate::id).collect();
    assert_eq!(ids, vec![H]);
}

#[test]
fn keeps_trace_metadata() {
    let mut trace = Trace::new(2);
    trace.set_memory_qubits(3);
    trace.set_property(ALGORITHM_COMPUTE_QUBITS, Property::Int(2));
    trace.add_block(2).add_operation(H, vec![0], vec![]);

    let result = Unroll.transform(&trace).expect("transform should succeed");

    assert_eq!(result.compute_qubits(), 2);
    assert_eq!(result.memory_qubits(), Some(3));
    assert_eq!(
        result
            .get_property(ALGORITHM_COMPUTE_QUBITS)
            .and_then(Property::as_int),
        Some(2)
    );
}
