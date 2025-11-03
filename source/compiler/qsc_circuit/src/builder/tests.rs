// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::expect;

#[test]
fn exceed_max_operations() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 2,
            source_locations: false,
            loop_detection: false,
            group_scopes: false,
            collapse_qubit_registers: false,
        },
        &[],
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(&[], "X", false, &[0], &[], None);
    builder.gate(&[], "X", false, &[0], &[], None);
    builder.gate(&[], "X", false, &[0], &[], None);

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
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 2,
            source_locations: false,
            loop_detection: false,
            group_scopes: false,
            collapse_qubit_registers: false,
        },
        &[],
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(&[], "X", false, &[0], &[], None);
    builder.measure(&[], "M", 0, &(0.into()));
    builder.gate(&[], "X", false, &[0], &[], None);

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
