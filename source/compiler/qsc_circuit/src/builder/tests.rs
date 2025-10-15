// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::circuit::GenerationMethod;

use super::*;
use expect_test::expect;

#[test]
fn exceed_max_operations() {
    let mut builder = CircuitBuilder::new(Config {
        max_operations: 2,
        generation_method: GenerationMethod::ClassicalEval,
        locations: false,
    });

    let tracer: &mut dyn Tracer = &mut builder;
    tracer.qubit_allocate(0, None);

    tracer.gate("X", false, GateInputs::with_targets(vec![0]), vec![], None);
    tracer.gate("X", false, GateInputs::with_targets(vec![0]), vec![], None);
    tracer.gate("X", false, GateInputs::with_targets(vec![0]), vec![], None);

    let circuit = builder.finish(None);

    // The current behavior is to silently truncate the circuit
    // if it exceeds the maximum allowed number of operations.
    expect![[r#"
        q_0    ── X ──── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn exceed_max_operations_deferred_measurements() {
    let mut builder = CircuitBuilder::new(Config {
        max_operations: 2,
        generation_method: GenerationMethod::ClassicalEval,
        locations: false,
    });

    // TODO: ugh...
    let tracer: &mut dyn Tracer = &mut builder;
    tracer.qubit_allocate(0, None);

    tracer.gate("X", false, GateInputs::with_targets(vec![0]), vec![], None);
    tracer.m(0, &(0.into()), None);
    tracer.gate("X", false, GateInputs::with_targets(vec![0]), vec![], None);

    let circuit = builder.finish(None);

    // The current behavior is to silently truncate the circuit
    // if it exceeds the maximum allowed number of operations.
    // The second X will be dropped.
    expect![[r#"
        q_0    ── X ──── M ──
                         ╘═══
    "#]]
    .assert_eq(&circuit.to_string());
}
