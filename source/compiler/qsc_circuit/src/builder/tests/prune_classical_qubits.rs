// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use qsc_eval::{StackTrace, backend::Tracer, val};

use crate::{
    CircuitTracer, TracerConfig,
    builder::tests::{FakeCompilation, stack_trace},
};

#[test]
fn circuit_trimmed_stays_the_same() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 100,
            source_locations: false,
            group_by_scope: false,
            prune_classical_qubits: true,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&StackTrace::default(), 0);
    builder.qubit_allocate(&StackTrace::default(), 1);

    builder.gate(&StackTrace::default(), "H", false, &[0], &[], None);
    builder.gate(&StackTrace::default(), "X", false, &[1], &[0], None);
    builder.measure(&StackTrace::default(), "MResetZ", 0, &val::Result::Id(0));
    builder.measure(&StackTrace::default(), "MResetZ", 1, &val::Result::Id(1));

    let circuit = builder.finish(&FakeCompilation::default());

    expect![[r#"
        q_0    ── H ──── ● ──── M ──── |0〉 ──
                         │      ╘════════════
        q_1    ───────── X ──── M ──── |0〉 ──
                                ╘════════════
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn circuit_trims_unused_qubit() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 100,
            source_locations: false,
            group_by_scope: false,
            prune_classical_qubits: true,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&StackTrace::default(), 0);
    builder.qubit_allocate(&StackTrace::default(), 1);
    builder.qubit_allocate(&StackTrace::default(), 2);

    builder.gate(&StackTrace::default(), "H", false, &[0], &[], None);
    builder.gate(&StackTrace::default(), "X", false, &[2], &[0], None);
    builder.measure(&StackTrace::default(), "MResetZ", 0, &val::Result::Id(0));
    builder.measure(&StackTrace::default(), "MResetZ", 2, &val::Result::Id(1));

    let circuit = builder.finish(&FakeCompilation::default());

    expect![[r#"
        q_0    ── H ──── ● ──── M ──── |0〉 ──
                         │      ╘════════════
        q_2    ───────── X ──── M ──── |0〉 ──
                                ╘════════════
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn circuit_trims_unused_qubit_with_grouping() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 100,
            source_locations: false,
            group_by_scope: true,
            prune_classical_qubits: true,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&stack_trace(vec![c.user_code_frame("Main", 1)]), 0);
    builder.qubit_allocate(&stack_trace(vec![c.user_code_frame("Main", 2)]), 1);
    builder.qubit_allocate(&stack_trace(vec![c.user_code_frame("Main", 3)]), 2);

    builder.gate(
        &stack_trace(vec![c.user_code_frame("Main", 4)]),
        "H",
        false,
        &[0],
        &[],
        None,
    );
    builder.gate(
        &stack_trace(vec![c.user_code_frame("Main", 5)]),
        "X",
        false,
        &[2],
        &[0],
        None,
    );
    builder.measure(
        &stack_trace(vec![c.user_code_frame("Main", 6)]),
        "MResetZ",
        0,
        &val::Result::Id(0),
    );
    builder.measure(
        &stack_trace(vec![c.user_code_frame("Main", 7)]),
        "MResetZ",
        2,
        &val::Result::Id(1),
    );

    let circuit = builder.finish(&c);

    expect![[r#"
                  ┌──── [Main] ────────────────────────────┐
        q_0    ───┼─────── H ────── ● ──── M ──── |0〉 ─────┼───
                  │                 │      ╘═══════════════╪═══
                  └──────────────   │   ───────────────────┘
                  ┌──── [Main] ──   │   ───────────────────┐
        q_2    ───┼──────────────── X ──── M ──── |0〉 ─────┼───
                  │                        ╘═══════════════╪═══
                  └────────────────────────────────────────┘
    "#]]
    .assert_eq(&circuit.display_with_groups().to_string());
}

#[test]
fn circuit_trims_classical_qubit() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 100,
            source_locations: false,
            group_by_scope: false,
            prune_classical_qubits: true,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&StackTrace::default(), 0);
    builder.qubit_allocate(&StackTrace::default(), 1);
    builder.qubit_allocate(&StackTrace::default(), 2);

    builder.gate(&StackTrace::default(), "H", false, &[0], &[], None);
    builder.gate(&StackTrace::default(), "X", false, &[1], &[], None);
    builder.gate(&StackTrace::default(), "X", false, &[2], &[0], None);
    builder.measure(&StackTrace::default(), "MResetZ", 0, &val::Result::Id(0));
    builder.measure(&StackTrace::default(), "MResetZ", 1, &val::Result::Id(1));
    builder.measure(&StackTrace::default(), "MResetZ", 2, &val::Result::Id(2));
    let circuit = builder.finish(&FakeCompilation::default());

    expect![[r#"
        q_0    ── H ──── ● ──── M ──── |0〉 ──
                         │      ╘════════════
        q_2    ───────── X ──── M ──── |0〉 ──
                                ╘════════════
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn circuit_trims_classical_control_qubit() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 100,
            source_locations: false,
            group_by_scope: false,
            prune_classical_qubits: true,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&StackTrace::default(), 0);
    builder.qubit_allocate(&StackTrace::default(), 1);
    builder.qubit_allocate(&StackTrace::default(), 2);

    builder.gate(&StackTrace::default(), "H", false, &[0], &[], None);
    builder.gate(&StackTrace::default(), "X", false, &[0], &[1], None);
    builder.gate(&StackTrace::default(), "X", false, &[2], &[0], None);
    builder.measure(&StackTrace::default(), "MResetZ", 0, &val::Result::Id(0));
    builder.measure(&StackTrace::default(), "MResetZ", 1, &val::Result::Id(1));
    builder.measure(&StackTrace::default(), "MResetZ", 2, &val::Result::Id(2));
    let circuit = builder.finish(&FakeCompilation::default());

    expect![[r#"
        q_0    ── H ──── ● ──── M ──── |0〉 ──
                         │      ╘════════════
        q_2    ───────── X ──── M ──── |0〉 ──
                                ╘════════════
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn circuit_trims_classical_qubit_when_2q_precedes_superposition() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 100,
            source_locations: false,
            group_by_scope: false,
            prune_classical_qubits: true,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&StackTrace::default(), 0);
    builder.qubit_allocate(&StackTrace::default(), 1);
    builder.qubit_allocate(&StackTrace::default(), 2);

    builder.gate(&StackTrace::default(), "X", false, &[1], &[0], None);
    builder.gate(&StackTrace::default(), "H", false, &[0], &[], None);
    builder.gate(&StackTrace::default(), "X", false, &[2], &[0], None);
    builder.measure(&StackTrace::default(), "MResetZ", 0, &val::Result::Id(0));
    builder.measure(&StackTrace::default(), "MResetZ", 1, &val::Result::Id(1));
    builder.measure(&StackTrace::default(), "MResetZ", 2, &val::Result::Id(2));
    let circuit = builder.finish(&FakeCompilation::default());

    expect![[r#"
        q_0    ── H ──── ● ──── M ──── |0〉 ──
                         │      ╘════════════
        q_2    ───────── X ──── M ──── |0〉 ──
                                ╘════════════
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn target_qubit_trimmed_when_only_one_control_non_classical() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 100,
            source_locations: false,
            group_by_scope: false,
            prune_classical_qubits: true,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&StackTrace::default(), 0);
    builder.qubit_allocate(&StackTrace::default(), 1);
    builder.qubit_allocate(&StackTrace::default(), 2);
    builder.qubit_allocate(&StackTrace::default(), 3);

    builder.gate(&StackTrace::default(), "H", false, &[2], &[], None);
    builder.gate(&StackTrace::default(), "X", false, &[0], &[1, 2], None);
    builder.gate(&StackTrace::default(), "H", false, &[3], &[], None);
    builder.gate(&StackTrace::default(), "X", false, &[1], &[3, 2], None);
    builder.measure(&StackTrace::default(), "MResetZ", 0, &val::Result::Id(0));
    builder.measure(&StackTrace::default(), "MResetZ", 1, &val::Result::Id(1));
    builder.measure(&StackTrace::default(), "MResetZ", 2, &val::Result::Id(2));

    let circuit = builder.finish(&FakeCompilation::default());

    expect![[r#"
        q_1    ───────── X ──── M ──── |0〉 ──
                         │      ╘════════════
        q_2    ── H ──── ● ──── M ──── |0〉 ──
                         │      ╘════════════
        q_3    ── H ──── ● ──────────────────
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn controlled_paulis_become_uncontrolled_when_control_is_known_classical_one() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 100,
            source_locations: false,
            group_by_scope: false,
            prune_classical_qubits: true,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&StackTrace::default(), 0);
    builder.qubit_allocate(&StackTrace::default(), 1);
    builder.qubit_allocate(&StackTrace::default(), 2);
    builder.qubit_allocate(&StackTrace::default(), 3);

    builder.gate(&StackTrace::default(), "X", false, &[0], &[], None);
    builder.gate(&StackTrace::default(), "X", false, &[1], &[0], None);
    builder.gate(&StackTrace::default(), "Y", false, &[2], &[0], None);
    builder.gate(&StackTrace::default(), "Z", false, &[3], &[0], None);
    builder.gate(&StackTrace::default(), "H", false, &[1], &[], None);
    builder.gate(&StackTrace::default(), "H", false, &[2], &[], None);
    builder.gate(&StackTrace::default(), "H", false, &[3], &[], None);

    let circuit = builder.finish(&FakeCompilation::default());

    expect![[r#"
        q_1    ── X ──── H ──
        q_2    ── Y ──── H ──
        q_3    ── Z ──── H ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn ccx_becomes_cx_when_one_control_is_known_classical_one() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 100,
            source_locations: false,
            group_by_scope: false,
            prune_classical_qubits: true,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&StackTrace::default(), 0);
    builder.qubit_allocate(&StackTrace::default(), 1);
    builder.qubit_allocate(&StackTrace::default(), 2);

    builder.gate(&StackTrace::default(), "X", false, &[0], &[], None);
    builder.gate(&StackTrace::default(), "H", false, &[1], &[], None);
    builder.gate(&StackTrace::default(), "H", false, &[2], &[], None);
    builder.gate(&StackTrace::default(), "X", false, &[2], &[0, 1], None);

    let circuit = builder.finish(&FakeCompilation::default());

    expect![[r#"
        q_1    ── H ──── ● ──
        q_2    ── H ──── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn ccx_becomes_cx_when_one_control_is_known_classical_one_with_grouping() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 100,
            source_locations: false,
            group_by_scope: true,
            prune_classical_qubits: true,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&StackTrace::default(), 0);
    builder.qubit_allocate(&StackTrace::default(), 1);
    builder.qubit_allocate(&StackTrace::default(), 2);

    builder.gate(
        &stack_trace(vec![c.user_code_frame("Main", 1)]),
        "X",
        false,
        &[0],
        &[],
        None,
    );
    builder.gate(
        &stack_trace(vec![
            c.user_code_frame("Main", 2),
            c.user_code_frame("Foo", 3),
        ]),
        "H",
        false,
        &[1],
        &[],
        None,
    );
    builder.gate(
        &stack_trace(vec![
            c.user_code_frame("Main", 2),
            c.user_code_frame("Foo", 4),
        ]),
        "H",
        false,
        &[2],
        &[],
        None,
    );
    builder.gate(
        &stack_trace(vec![
            c.user_code_frame("Main", 2),
            c.user_code_frame("Foo", 5),
        ]),
        "X",
        false,
        &[2],
        &[0, 1],
        None,
    );

    let circuit = builder.finish(&c);

    expect![[r#"
                  ┌──── [Main] ────────────────────────────┐
                  │        ┌────── [Foo] ───────────┐      │
        q_1    ───┼────────┼──────── H ───── ● ─────┼──────┼───
        q_2    ───┼────────┼──────── H ───── X ─────┼──────┼───
                  │        └────────────────────────┘      │
                  └────────────────────────────────────────┘
    "#]]
    .assert_eq(&circuit.display_with_groups().to_string());
}

#[test]
fn group_with_no_remaining_operations_is_pruned() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 100,
            source_locations: false,
            group_by_scope: true,
            prune_classical_qubits: true,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&StackTrace::default(), 0);
    builder.qubit_allocate(&StackTrace::default(), 1);
    builder.qubit_allocate(&StackTrace::default(), 2);

    builder.gate(
        &stack_trace(vec![
            c.user_code_frame("Main", 1),
            c.user_code_frame("Bar", 3),
        ]),
        "X",
        false,
        &[0],
        &[],
        None,
    );
    builder.measure(
        &stack_trace(vec![
            c.user_code_frame("Main", 1),
            c.user_code_frame("Bar", 4),
        ]),
        "M",
        0,
        &val::Result::Id(0),
    );
    builder.gate(
        &stack_trace(vec![
            c.user_code_frame("Main", 2),
            c.user_code_frame("Foo", 3),
        ]),
        "H",
        false,
        &[1],
        &[],
        None,
    );
    builder.gate(
        &stack_trace(vec![
            c.user_code_frame("Main", 2),
            c.user_code_frame("Foo", 4),
        ]),
        "H",
        false,
        &[2],
        &[],
        None,
    );
    builder.gate(
        &stack_trace(vec![
            c.user_code_frame("Main", 2),
            c.user_code_frame("Foo", 5),
        ]),
        "X",
        false,
        &[2],
        &[0, 1],
        None,
    );

    let circuit = builder.finish(&c);

    expect![[r#"
                  ┌──── [Main] ────────────────────────────┐
                  │        ┌────── [Foo] ───────────┐      │
        q_1    ───┼────────┼──────── H ───── ● ─────┼──────┼───
        q_2    ───┼────────┼──────── H ───── X ─────┼──────┼───
                  │        └────────────────────────┘      │
                  └────────────────────────────────────────┘
    "#]]
    .assert_eq(&circuit.display_with_groups().to_string());
}
