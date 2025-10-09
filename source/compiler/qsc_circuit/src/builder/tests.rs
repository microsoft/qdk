// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::circuit::GenerationMethod;

use super::*;
use expect_test::expect;

#[test]
fn exceed_max_operations() {
    let mut builder = Builder::new(Config {
        max_operations: 2,
        loop_detection: false,
        generation_method: GenerationMethod::ClassicalEval,
        group_scopes: false,
        collapse_qubit_registers: false,
    });

    let tracer: &mut dyn TracingBackend<usize> = &mut builder;
    tracer.qubit_allocate(0);

    tracer.x(0);
    tracer.x(0);
    tracer.x(0);

    let circuit = builder.finish();

    // The current behavior is to silently truncate the circuit
    // if it exceeds the maximum allowed number of operations.
    expect![[r#"
        q_0    ── X ──── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn exceed_max_operations_deferred_measurements() {
    let mut builder = Builder::new(Config {
        max_operations: 2,
        loop_detection: false,
        generation_method: GenerationMethod::ClassicalEval,
        group_scopes: false,
        collapse_qubit_registers: false,
    });

    // TODO: ugh...
    let tracer: &mut dyn TracingBackend<usize> = &mut builder;
    tracer.qubit_allocate(0);

    tracer.x(0);
    tracer.m(0, &0);
    tracer.x(0);

    let circuit = builder.finish();

    // The current behavior is to silently truncate the circuit
    // if it exceeds the maximum allowed number of operations.
    // The second X will be dropped.
    expect![[r#"
        q_0    ── X ──── M ──
                         ╘═══
    "#]]
    .assert_eq(&circuit.to_string());
}
