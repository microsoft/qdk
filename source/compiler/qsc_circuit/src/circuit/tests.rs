// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::expect;

/// Converts a 2D grid of operations into a component grid.
///
/// # Arguments
///
/// * `operations` - A 2D vector of operations to be converted.
///
/// # Returns
///
/// A component grid representing the operations.
pub fn op_grid_to_comp_grid(operations: Vec<Vec<Operation>>) -> ComponentGrid {
    let mut component_grid = vec![];
    for col in operations {
        let column = ComponentColumn { components: col };
        component_grid.push(column);
    }
    component_grid
}

fn qubit(id: usize) -> Qubit {
    Qubit {
        id,
        num_results: 0,
        declarations: vec![],
    }
}

fn qubit_with_results(id: usize, num_results: usize) -> Qubit {
    Qubit {
        id,
        num_results,
        declarations: vec![],
    }
}

fn q_reg(id: usize) -> Register {
    Register::quantum(id)
}

fn c_reg(q_id: usize, c_id: usize) -> Register {
    Register::classical(q_id, c_id)
}

fn measurement(q_id: usize, c_id: usize) -> Operation {
    Operation::Measurement(Measurement {
        gate: "Measure".to_string(),
        args: vec![],
        qubits: vec![Register::quantum(q_id)],
        results: vec![Register::classical(q_id, c_id)],
        children: vec![],
        source: None,
    })
}

fn unitary(gate: &str, targets: Vec<Register>) -> Operation {
    Operation::Unitary(Unitary {
        gate: gate.to_string(),
        args: vec![],
        is_adjoint: false,
        controls: vec![],
        targets,
        children: vec![],
        source: None,
        scope_location: None,
    })
}

fn ctl_unitary(gate: &str, targets: Vec<Register>, controls: Vec<Register>) -> Operation {
    Operation::Unitary(Unitary {
        gate: gate.to_string(),
        args: vec![],
        is_adjoint: false,
        controls,
        targets,
        children: vec![],
        source: None,
        scope_location: None,
    })
}

#[test]
fn deserialize_circuit() {
    let contents = r#"
{
  "qubits": [ { "id": 0 }, { "id": 1 } ],
  "componentGrid": [
    {
      "components": [
        { "kind": "unitary", "gate": "H", "targets": [{ "qubit": 0 }] },
        { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 1 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "Z", "targets": [{ "qubit": 0 }] }
      ]
    },
    {
      "components": [
        { "kind": "unitary", "gate": "X", "targets": [{ "qubit": 1 }], "controls": [{ "qubit": 0 }] }
      ]
    }
  ]
}"#;

    let c = serde_json::from_str::<Circuit>(contents).expect("Was not able to deserialize");

    expect![[r#"
        q_0    ── H ──── Z ──── ● ──
        q_1    ── X ─────────── X ──
    "#]]
    .assert_eq(&c.to_string());
}

#[test]
fn empty() {
    let c = Circuit {
        qubits: vec![],
        component_grid: vec![],
    };
    expect![[""]].assert_eq(&c.to_string());
}

#[test]
fn no_gates() {
    let c = Circuit {
        qubits: vec![qubit(0), qubit(1)],
        component_grid: vec![],
    };

    expect![[r"
        q_0
        q_1
    "]]
    .assert_eq(&c.to_string());
}

#[test]
fn bell() {
    let operations = vec![
        unitary("H", vec![q_reg(0)]),
        ctl_unitary("X", vec![q_reg(1)], vec![q_reg(0)]),
        measurement(0, 0),
        measurement(1, 0),
    ];
    let qubits = vec![qubit_with_results(0, 1), qubit_with_results(1, 1)];
    let component_grid = operation_list_to_grid(operations, &qubits, true);
    let c = Circuit {
        qubits,
        component_grid,
    };

    expect![[r"
        q_0    ── H ──── ● ──── M ──
                         │      ╘═══
        q_1    ───────── X ──── M ──
                                ╘═══
    "]]
    .assert_eq(&c.to_string());
}

#[test]
fn control_classical() {
    let operations = vec![
        measurement(0, 0),
        ctl_unitary("X", vec![q_reg(2)], vec![c_reg(0, 0)]),
        ctl_unitary("X", vec![q_reg(2)], vec![q_reg(0)]),
    ];
    let qubits = vec![qubit_with_results(0, 1), qubit(1), qubit(2)];
    let component_grid = operation_list_to_grid(operations, &qubits, true);
    let c = Circuit {
        qubits,
        component_grid,
    };

    expect![[r"
        q_0    ── M ─────────── ● ──
                  ╘═════ ● ═════╪═══
        q_1    ──────────┼──────┼───
        q_2    ───────── X ──── X ──
    "]]
    .assert_eq(&c.to_string());
}

#[test]
fn two_measurements() {
    let operations = vec![measurement(0, 0), measurement(0, 1)];
    let qubits = vec![qubit_with_results(0, 2)];
    let component_grid = operation_list_to_grid(operations, &qubits, true);
    let c = Circuit {
        qubits,
        component_grid,
    };

    expect![[r"
        q_0    ── M ──── M ──
                  ╘══════╪═══
                         ╘═══
    "]]
    .assert_eq(&c.to_string());
}

#[test]
fn left_align_operations() {
    let qubits = vec![qubit_with_results(0, 1), qubit(1), qubit(2)];
    let operations = vec![
        measurement(0, 0),
        ctl_unitary("X", vec![q_reg(0)], vec![]),
        ctl_unitary("X", vec![q_reg(2)], vec![]),
        ctl_unitary("X", vec![q_reg(1)], vec![]),
        ctl_unitary("X", vec![q_reg(1)], vec![q_reg(0)]),
        ctl_unitary("X", vec![q_reg(1)], vec![q_reg(0)]),
    ];
    // let component_grid = operation_list_to_grid(operations, &qubits, false);
    let component_grid = operation_list_to_grid(operations, &qubits, false);
    let c = Circuit {
        qubits,
        component_grid,
    };

    expect![[r#"
        q_0    ── M ──── X ──── ● ──── ● ──
                  ╘═════════════╪══════╪═══
        q_1    ── X ─────────── X ──── X ──
        q_2    ── X ───────────────────────
    "#]]
    .assert_eq(&c.to_string());
}

#[test]
fn with_args() {
    let c = Circuit {
        qubits: vec![qubit(0)],
        component_grid: op_grid_to_comp_grid(vec![vec![Operation::Unitary(Unitary {
            gate: "rx".to_string(),
            args: vec!["1.5708".to_string()],
            is_adjoint: false,
            controls: vec![],
            targets: vec![Register::quantum(0)],
            children: vec![],
            source: None,
            scope_location: None,
        })]]),
    };

    expect![[r"
        q_0    ─ rx(1.5708) ──
    "]]
    .assert_eq(&c.to_string());
}

#[test]
fn two_targets() {
    let c = Circuit {
        qubits: vec![qubit(0), qubit(1), qubit(2)],
        component_grid: op_grid_to_comp_grid(vec![vec![Operation::Unitary(Unitary {
            gate: "rzz".to_string(),
            args: vec!["1.0000".to_string()],
            is_adjoint: false,
            controls: vec![],
            targets: vec![Register::quantum(0), Register::quantum(2)],
            children: vec![],
            source: None,
            scope_location: None,
        })]]),
    };

    expect![[r"
        q_0    ─ rzz(1.0000) ─
        q_1    ───────┆───────
        q_2    ─ rzz(1.0000) ─
    "]]
    .assert_eq(&c.to_string());
}

#[test]
fn respect_column_info() {
    let c = Circuit {
        qubits: vec![qubit(0), qubit(1)],
        component_grid: op_grid_to_comp_grid(vec![
            vec![unitary("X", vec![q_reg(0)])],
            vec![unitary("Y", vec![q_reg(0)]), unitary("S", vec![q_reg(1)])],
            vec![unitary("Z", vec![q_reg(0)])],
        ]),
    };

    expect![[r#"
        q_0    ── X ──── Y ──── Z ──
        q_1    ───────── S ─────────
    "#]]
    .assert_eq(&c.to_string());
}

#[test]
fn classical_controlled_group() {
    let c = Circuit {
        qubits: vec![qubit_with_results(0, 1), qubit_with_results(1, 1)],
        component_grid: vec![
            ComponentColumn {
                components: vec![unitary("H", vec![q_reg(0)])],
            },
            ComponentColumn {
                components: vec![measurement(0, 0)],
            },
            ComponentColumn {
                components: vec![measurement(1, 0)],
            },
            ComponentColumn {
                components: vec![Component::Unitary(Unitary {
                    gate: "group".into(),
                    args: vec![],
                    children: vec![
                        ComponentColumn {
                            components: vec![unitary("X", vec![q_reg(0)])],
                        },
                        ComponentColumn {
                            components: vec![unitary("Y", vec![q_reg(1)])],
                        },
                    ],
                    targets: vec![
                        Register {
                            qubit: 0,
                            result: None,
                        },
                        Register {
                            qubit: 1,
                            result: None,
                        },
                    ],
                    controls: vec![Register {
                        qubit: 1,
                        result: Some(0),
                    }],
                    is_adjoint: false,
                    source: None,
                })],
            },
            ComponentColumn {
                components: vec![unitary("Z", vec![q_reg(0)])],
            },
        ],
    };

    expect![[r#"
        q_0    ── H ──── M ────────── [[ ─── [group] ─── X ────────── ]] ──── Z ──
                         ╘══════════════════════╪═════════════════════════════════
        q_1    ──────────────── M ─── [[ ─── [group] ────────── Y ─── ]] ─────────
                                ╘══════════════ ● ════════════════════════════════
    "#]]
    .assert_eq(&c.to_string());
}

#[test]
fn group_two_qubits() {
    let qubits = vec![qubit(0), qubit(1), qubit(2)];
    let operations = vec![
        unitary("X", vec![q_reg(0)]),
        unitary("Y", vec![q_reg(1)]),
        unitary("Z", vec![q_reg(2)]),
    ];
    let component_grid = operation_list_to_grid(operations.clone(), &qubits, true);
    let c = Circuit {
        qubits: qubits.clone(),
        component_grid,
    };

    expect![[r#"
        q_0    ── X ──
        q_1    ── Y ──
        q_2    ── Z ──
    "#]]
    .assert_eq(&c.to_string());

    let (operations, qubits) = group_qubits(operations, qubits, &[0, 1]);
    let component_grid = operation_list_to_grid(operations, &qubits, true);
    let c = Circuit {
        qubits,
        component_grid,
    };
    expect![[r#"
        q_0    ─ X (q[0]) ─── Y (q[1]) ──
        q_2    ───── Z ──────────────────
    "#]]
    .assert_eq(&c.to_string());
}

#[test]
fn group_two_qubits_both_control_and_target() {
    let qubits = vec![qubit(0), qubit(1)];
    let operations = vec![
        ctl_unitary("X", vec![q_reg(0)], vec![q_reg(1)]),
        ctl_unitary("X", vec![q_reg(1)], vec![q_reg(0)]),
    ];
    let component_grid = operation_list_to_grid(operations.clone(), &qubits, false);
    let c = Circuit {
        qubits: qubits.clone(),
        component_grid,
    };

    expect![[r#"
        q_0    ── X ──── ● ──
        q_1    ── ● ──── X ──
    "#]]
    .assert_eq(&c.to_string());

    let (operations, qubits) = group_qubits(operations, qubits, &[0, 1]);
    let component_grid = operation_list_to_grid(operations, &qubits, false);
    let c = Circuit {
        qubits,
        component_grid,
    };
    expect![[r#"
        q_0    ─ CX (q[1, 0]) ─── CX (q[0, 1]) ──
    "#]]
    .assert_eq(&c.to_string());
}

#[test]
fn group_two_qubits_both_control_and_target_external() {
    let qubits = vec![qubit(0), qubit(1), qubit(2)];
    let operations = vec![
        ctl_unitary("X", vec![q_reg(0)], vec![q_reg(1)]),
        ctl_unitary("X", vec![q_reg(2)], vec![q_reg(0)]),
        ctl_unitary("X", vec![q_reg(0)], vec![q_reg(2)]),
    ];
    let component_grid = operation_list_to_grid(operations.clone(), &qubits, false);
    let c = Circuit {
        qubits: qubits.clone(),
        component_grid,
    };

    // TODO: group 0 and 1

    expect![[r#"
        q_0    ── X ──── ● ──── X ──
        q_1    ── ● ─────┼──────┼───
        q_2    ───────── X ──── ● ──
    "#]]
    .assert_eq(&c.to_string());

    let (operations, qubits) = group_qubits(operations, qubits, &[0, 1]);
    let component_grid = operation_list_to_grid(operations, &qubits, false);
    let c = Circuit {
        qubits,
        component_grid,
    };
    expect![[r#"
        q_0    ─ CX (q[1, 0]) ─────── ● ────── X (q[0]) ──
        q_2    ────────────────── X (q[0]) ─────── ● ─────
    "#]]
    .assert_eq(&c.to_string());
}

#[test]
fn group_two_qubits_both_control_and_target_external_double_control() {
    let qubits = vec![qubit(0), qubit(1), qubit(2)];
    let operations = vec![
        ctl_unitary("X", vec![q_reg(0)], vec![q_reg(1)]),
        ctl_unitary("X", vec![q_reg(2)], vec![q_reg(0)]),
        ctl_unitary("X", vec![q_reg(0)], vec![q_reg(1), q_reg(2)]),
    ];
    let component_grid = operation_list_to_grid(operations.clone(), &qubits, false);
    let c = Circuit {
        qubits: qubits.clone(),
        component_grid,
    };

    // TODO: group 0 and 1

    expect![[r#"
        q_0    ── X ──── ● ──── X ──
        q_1    ── ● ─────┼───── ● ──
        q_2    ───────── X ──── ● ──
    "#]]
    .assert_eq(&c.to_string());

    let (operations, qubits) = group_qubits(operations, qubits, &[0, 1]);
    let component_grid = operation_list_to_grid(operations, &qubits, false);
    let c = Circuit {
        qubits,
        component_grid,
    };
    expect![[r#"
        q_0    ─ CX (q[1, 0]) ─────── ● ────── CX (q[1, 0]) ──
        q_2    ────────────────── X (q[0]) ───────── ● ───────
    "#]]
    .assert_eq(&c.to_string());
}

#[test]
fn group_two_qubits_measurements() {
    let qubits = vec![qubit_with_results(0, 1), qubit(1), qubit(2)];
    let operations = vec![
        unitary("H", vec![q_reg(1)]),
        ctl_unitary("X", vec![q_reg(0)], vec![q_reg(1)]),
        ctl_unitary("X", vec![q_reg(2)], vec![q_reg(0)]),
        measurement(0, 0),
    ];
    let component_grid = operation_list_to_grid(operations.clone(), &qubits, false);
    let c = Circuit {
        qubits: qubits.clone(),
        component_grid,
    };

    // TODO: group 0 and 1

    expect![[r#"
        q_0    ───────── X ──── ● ──── M ──
                         │      │      ╘═══
        q_1    ── H ──── ● ─────┼──────────
        q_2    ──────────────── X ─────────
    "#]]
    .assert_eq(&c.to_string());

    let (operations, qubits) = group_qubits(operations, qubits, &[0, 1]);
    let component_grid = operation_list_to_grid(operations, &qubits, false);
    let c = Circuit {
        qubits,
        component_grid,
    };
    expect![[r#"
        q_0    ─ H (q[1]) ─── CX (q[1, 0]) ─────── ● ─────── M ──
                                                   │         ╘═══
        q_2    ─────────────────────────────── X (q[0]) ─────────
    "#]]
    .assert_eq(&c.to_string());
}

#[test]
fn group_two_qubits_grouped_operations() {
    let qubits = vec![qubit_with_results(0, 1), qubit(1), qubit(2)];
    let mut group_box = unitary("box", vec![q_reg(0), q_reg(1)]);
    *group_box.children_mut() = vec![ComponentColumn {
        components: vec![unitary("H", vec![q_reg(1)])],
    }];
    let operations = vec![
        unitary("H", vec![q_reg(1)]),
        ctl_unitary("X", vec![q_reg(0)], vec![q_reg(1)]),
        ctl_unitary("X", vec![q_reg(2)], vec![q_reg(0)]),
        group_box,
        measurement(0, 0),
    ];
    let component_grid = operation_list_to_grid(operations.clone(), &qubits, false);
    let c = Circuit {
        qubits: qubits.clone(),
        component_grid,
    };

    expect![[r#"
        q_0    ───────── X ──── ● ─── [[ ─── [box] ───────── ]] ──── M ──
                         │      │              ┆                     ╘═══
        q_1    ── H ──── ● ─────┼──── [[ ─── [box] ─── H ─── ]] ─────────
        q_2    ──────────────── X ───────────────────────────────────────
    "#]]
    .assert_eq(&c.to_string());

    let (operations, qubits) = group_qubits(operations, qubits, &[0, 1]);
    let component_grid = operation_list_to_grid(operations, &qubits, false);
    let c = Circuit {
        qubits,
        component_grid,
    };
    expect![[r#"
        q_0    ─ H (q[1]) ─── CX (q[1, 0]) ─────── ● ────── [[ ─── [box (q[0, 1])] ── H (q[1]) ─── ]] ──── M ──
                                                   │                                                       ╘═══
        q_2    ─────────────────────────────── X (q[0]) ───────────────────────────────────────────────────────
    "#]]
    .assert_eq(&c.to_string());
}

#[test]
fn group_two_qubits_grouped_operations_exceeds_register() {
    let qubits = vec![qubit_with_results(0, 1), qubit(1), qubit(2)];
    let mut group_box = unitary("box", vec![q_reg(0), q_reg(1), q_reg(2)]);
    *group_box.children_mut() = vec![ComponentColumn {
        components: vec![
            unitary("H", vec![q_reg(0)]),
            unitary("H", vec![q_reg(1)]),
            unitary("H", vec![q_reg(2)]),
        ],
    }];
    let operations = vec![
        unitary("H", vec![q_reg(1)]),
        ctl_unitary("X", vec![q_reg(0)], vec![q_reg(1)]),
        ctl_unitary("X", vec![q_reg(2)], vec![q_reg(0)]),
        group_box,
        measurement(0, 0),
    ];
    let component_grid = operation_list_to_grid(operations.clone(), &qubits, false);
    let c = Circuit {
        qubits: qubits.clone(),
        component_grid,
    };

    expect![[r#"
        q_0    ───────── X ──── ● ─── [[ ─── [box] ─── H ─── ]] ──── M ──
                         │      │              ┆                     ╘═══
        q_1    ── H ──── ● ─────┼──── [[ ─── [box] ─── H ─── ]] ─────────
                                │              ┆
        q_2    ──────────────── X ─── [[ ─── [box] ─── H ─── ]] ─────────
    "#]]
    .assert_eq(&c.to_string());

    let (operations, qubits) = group_qubits(operations, qubits, &[0, 1]);
    let component_grid = operation_list_to_grid(operations, &qubits, false);
    let c = Circuit {
        qubits,
        component_grid,
    };
    expect![[r#"
        q_0    ─ H (q[1]) ─── CX (q[1, 0]) ─────── ● ────── [[ ─── [box (q[0, 1])] ── H (q[0]) ─── H (q[1]) ─── ]] ──── M ──
                                                   │                      ┆                                             ╘═══
        q_2    ─────────────────────────────── X (q[0]) ─── [[ ─── [box (q[0, 1])] ────── H ─────────────────── ]] ─────────
    "#]]
    .assert_eq(&c.to_string());
}
