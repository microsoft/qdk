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
        metadata: None,
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
        metadata: None,
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
        metadata: None,
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
    let component_grid = operation_list_to_grid(operations, qubits.len());
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
    let component_grid = operation_list_to_grid(operations, qubits.len());
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
    let component_grid = operation_list_to_grid(operations, qubits.len());
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
            metadata: None,
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
            metadata: None,
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
