// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod logical_stack_trace;

use super::known_gate_spec;

#[test]
fn recognizes_pauli_measurement_intrinsics() {
    let mx = known_gate_spec("__quantum__qis__mx__body")
        .expect("MX should be recognized as a measurement gate");
    let my = known_gate_spec("__quantum__qis__my__body")
        .expect("MY should be recognized as a measurement gate");

    assert_eq!(mx.name, "MX");
    assert_eq!(my.name, "MY");
}
