// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(dead_code)]

use std::{f32::consts::PI, fmt::Write};

use expect_test::expect;

use crate::{
    run_gpu_simulator,
    shader_types::{Op, ops},
};

/// This code isn't generally safe to use as it gives all states
/// and should only be used in tests
fn write_probabilities(num_qubits: u32, r: &[super::shader_types::Result]) -> String {
    let mut prob_iter = r.iter();
    let mut prob = prob_iter.next();
    let mut prob_str = String::new();
    let mut formatted_results = Vec::with_capacity(2u32.pow(num_qubits) as usize);
    writeln!(&mut prob_str, "Probabilities:").unwrap();
    for i in 0..(2u32.pow(num_qubits)) {
        if let Some(res) = prob
            && res.entry_idx == i
        {
            formatted_results.push((
                format!("{:0width$b}", i, width = num_qubits as usize)
                    .chars()
                    .rev()
                    .collect::<String>(),
                res.probability,
            ));

            prob = prob_iter.next();
        } else {
            formatted_results.push((
                format!("{:0width$b}", i, width = num_qubits as usize)
                    .chars()
                    .rev()
                    .collect::<String>(),
                0.0,
            ));
        }
    }
    formatted_results.sort_by_key(|r| r.0.clone());
    for (bits, prob) in formatted_results {
        writeln!(prob_str, "|{bits}⟩: {prob:.6}").expect("failed to write");
    }
    prob_str
}

fn gate_op(id: u32, q1: u32, q2: u32, q3: u32, angle: f32) -> Op {
    Op {
        id,
        q1,
        q2,
        q3,
        angle,
        padding: [0; 204],
        _00r: 0.0,
        _00i: 0.0,
        _01r: 0.0,
        _01i: 0.0,
        _10r: 0.0,
        _10i: 0.0,
        _11r: 0.0,
        _11i: 0.0,
    }
}

fn m_every_z() -> Op {
    gate_op(ops::MEVERYZ, 0, 0, 0, 0.0)
}

fn two_qubit_gate(id: u32, qubit1: u32, qubit2: u32) -> Op {
    gate_op(id, qubit1, qubit2, 0, 0.0)
}

fn two_qubit_rotation_gate(id: u32, angle: f32, qubit1: u32, qubit2: u32) -> Op {
    gate_op(id, qubit1, qubit2, 0, angle)
}

fn three_qubit_gate(id: u32, qubit1: u32, qubit2: u32, qubit3: u32) -> Op {
    gate_op(id, qubit1, qubit2, qubit3, 0.0)
}

#[test]
fn x_gate() {
    let op = Op::new_x_gate(1);
    let m = m_every_z();
    let ops = vec![op, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 1.000000
        |10⟩: 0.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn y_gate() {
    let op = Op::new_y_gate(0);
    let m = m_every_z();
    let ops = vec![op, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 0.000000
        |10⟩: 1.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn z_gate() {
    let op = Op::new_z_gate(0);
    let m = m_every_z();
    let ops = vec![op, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 1.000000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn rx_gate() {
    let op0 = Op::new_rx_gate(PI, 0);
    let op1 = Op::new_rx_gate(2.0 * PI, 1);
    let m = m_every_z();
    let ops = vec![op0, op1, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 0.000000
        |10⟩: 1.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn ry_gate() {
    let op0 = Op::new_ry_gate(PI, 0);
    let op1 = Op::new_ry_gate(2.0 * PI, 1);
    let m = m_every_z();
    let ops = vec![op0, op1, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 0.000000
        |10⟩: 1.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn rz_gate() {
    let op0 = Op::new_x_gate(0);
    let op1 = Op::new_rz_gate(PI, 0);
    let op2 = Op::new_rz_gate(2.0 * PI, 1);
    let m = m_every_z();
    let ops = vec![op0, op1, op2, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 0.000000
        |10⟩: 1.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn x_is_self_adj() {
    let op = Op::new_x_gate(0);
    let m = m_every_z();
    let ops = vec![op, op, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 1.000000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn sx_gate_twice() {
    let op = Op::new_sx_gate(0);
    let m = m_every_z();
    let ops = vec![op, op, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 0.000000
        |10⟩: 1.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn sx_sxadj() {
    let op0 = Op::new_sx_gate(0);
    let op1 = Op::new_sx_adj_gate(0);
    let m = m_every_z();
    let ops = vec![op0, op1, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 1.000000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn bell() {
    let op0 = Op::new_h_gate(0);
    let op1 = Op::new_cx_gate(0, 1);
    let m = m_every_z();
    let ops = vec![op0, op1, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.500000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 0.500000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn h_gate() {
    let op = Op::new_h_gate(0);
    let m = m_every_z();
    let ops = vec![op, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.500000
        |01⟩: 0.000000
        |10⟩: 0.500000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn s_gate() {
    let op = Op::new_s_gate(0);
    let m = m_every_z();
    let ops = vec![op, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 1.000000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn s_adj_gate() {
    let op = Op::new_s_adj_gate(0);
    let m = m_every_z();
    let ops = vec![op, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 1.000000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn t_gate() {
    let op = Op::new_t_gate(0);
    let m = m_every_z();
    let ops = vec![op, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 1.000000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn t_adj_gate() {
    let op = Op::new_t_adj_gate(0);
    let m = m_every_z();
    let ops = vec![op, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 1.000000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn s_s_adj() {
    let op0 = Op::new_x_gate(0);
    let op1 = Op::new_s_gate(0);
    let op2 = Op::new_s_adj_gate(0);
    let m = m_every_z();
    let ops = vec![op0, op1, op2, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 0.000000
        |10⟩: 1.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn t_t_t_t() {
    let op0 = Op::new_x_gate(0);
    let op1 = Op::new_t_gate(0);
    let op2 = Op::new_t_gate(0);
    let op3 = Op::new_t_gate(0);
    let op4 = Op::new_t_gate(0);
    let m = m_every_z();
    let ops = vec![op0, op1, op2, op3, op4, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 0.000000
        |10⟩: 1.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn cz_gate() {
    let op0 = Op::new_x_gate(0);
    let op1 = Op::new_x_gate(1);
    let op2 = two_qubit_gate(ops::CZ, 0, 1);
    let m = m_every_z();
    let ops = vec![op0, op1, op2, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 1.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn swap_gate() {
    let op0 = Op::new_x_gate(1);
    let op1 = two_qubit_gate(ops::SWAP, 0, 1);
    let m = m_every_z();
    let ops = vec![op0, op1, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 0.000000
        |10⟩: 1.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn rxx_gate() {
    let op0 = two_qubit_rotation_gate(ops::RXX, PI, 0, 1);
    let m = m_every_z();
    let ops = vec![op0, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 1.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn ryy_gate() {
    let op0 = two_qubit_rotation_gate(ops::RYY, PI, 0, 1);
    let m = m_every_z();
    let ops = vec![op0, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 1.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn rzz_gate() {
    let op0 = two_qubit_rotation_gate(ops::RZZ, PI, 0, 1);
    let m = m_every_z();
    let ops = vec![op0, m];
    let r = run_gpu_simulator(2, ops);
    let prob_str = write_probabilities(2, &r);
    expect![[r#"
        Probabilities:
        |00⟩: 1.000000
        |01⟩: 0.000000
        |10⟩: 0.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn h_on_all() {
    let num_qubits = 3;
    let mut ops = Vec::new();
    for i in 0..num_qubits {
        ops.push(Op::new_h_gate(i));
    }
    ops.push(m_every_z());
    let r = run_gpu_simulator(num_qubits, ops);

    let prob_str = write_probabilities(num_qubits, &r);
    expect![[r#"
        Probabilities:
        |000⟩: 0.125000
        |001⟩: 0.125000
        |010⟩: 0.125000
        |011⟩: 0.125000
        |100⟩: 0.125000
        |101⟩: 0.125000
        |110⟩: 0.125000
        |111⟩: 0.125000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
#[ignore = "unimplemented"]
fn ccx_gate_1_1_controls() {
    // Test CCX (Toffoli) gate: controlled-controlled-X
    // Only flips target qubit when both control qubits are |1⟩

    // CCX on |110⟩ should give |111⟩ (target flipped)
    let num_qubits = 3;
    let ops = vec![
        Op::new_x_gate(0),                   // Set qubit 0 to |1⟩
        Op::new_x_gate(1),                   // Set qubit 1 to |1⟩
        three_qubit_gate(ops::CCX, 0, 1, 2), // CCX with controls 0,1 and target 2
        m_every_z(),
    ];
    let r = run_gpu_simulator(num_qubits, ops);
    let prob_str = write_probabilities(num_qubits, &r);
    expect![[r#"
        Probabilities:
        |000⟩: 0.000000
        |001⟩: 0.000000
        |010⟩: 0.000000
        |011⟩: 0.000000
        |100⟩: 0.000000
        |101⟩: 0.000000
        |110⟩: 0.000000
        |111⟩: 1.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
#[ignore = "unimplemented"]
fn ccx_gate_1_1_controls_mixed_order() {
    // Test CCX (Toffoli) gate: controlled-controlled-X
    // Only flips target qubit when both control qubits are |1⟩

    // CCX on |101⟩ should give |111⟩ (target flipped)
    let num_qubits = 3;
    let ops = vec![
        Op::new_x_gate(0),                   // Set qubit 0 to |1⟩
        Op::new_x_gate(2),                   // Set qubit 1 to |1⟩
        three_qubit_gate(ops::CCX, 0, 2, 1), // CCX with controls 0,1 and target 2
        m_every_z(),
    ];
    let r = run_gpu_simulator(num_qubits, ops);
    let prob_str = write_probabilities(num_qubits, &r);
    expect![[r#"
        Probabilities:
        |000⟩: 0.000000
        |001⟩: 0.000000
        |010⟩: 0.000000
        |011⟩: 0.000000
        |100⟩: 0.000000
        |101⟩: 0.000000
        |110⟩: 0.000000
        |111⟩: 1.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
#[ignore = "unimplemented"]
fn ccx_gate_1_0_controls() {
    // Test CCX (Toffoli) gate: controlled-controlled-X
    // Only flips target qubit when both control qubits are |1⟩

    let num_qubits = 3;

    // CCX on |100⟩ should remain |100⟩ (control qubit 1 is |0⟩)
    let ops = vec![
        Op::new_x_gate(0),                   // Set qubit 0 to |1⟩
        three_qubit_gate(ops::CCX, 0, 1, 2), // CCX with controls 0,1 and target 2
        m_every_z(),
    ];
    let r = run_gpu_simulator(num_qubits, ops);
    let prob_str = write_probabilities(num_qubits, &r);
    expect![[r#"
        Probabilities:
        |000⟩: 0.000000
        |001⟩: 0.000000
        |010⟩: 0.000000
        |011⟩: 0.000000
        |100⟩: 1.000000
        |101⟩: 0.000000
        |110⟩: 0.000000
        |111⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
#[ignore = "unimplemented"]
fn ccx_gate_0_1_controls() {
    // Test CCX (Toffoli) gate: controlled-controlled-X
    // Only flips target qubit when both control qubits are |1⟩

    let num_qubits = 3;

    // CCX on |010⟩ should remain |010⟩ (control qubit 0 is |0⟩)
    let ops = vec![
        Op::new_x_gate(1),                   // Set qubit 1 to |1⟩
        three_qubit_gate(ops::CCX, 0, 1, 2), // CCX with controls 0,1 and target 2
        m_every_z(),
    ];
    let r = run_gpu_simulator(num_qubits, ops);
    let prob_str = write_probabilities(num_qubits, &r);
    expect![[r#"
        Probabilities:
        |000⟩: 0.000000
        |001⟩: 0.000000
        |010⟩: 1.000000
        |011⟩: 0.000000
        |100⟩: 0.000000
        |101⟩: 0.000000
        |110⟩: 0.000000
        |111⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn test_gate_utility_functions() {
    // Test basic gate utility functions create correct matrix representations
    let x_gate = Op::new_x_gate(0);
    assert_eq!(x_gate.id, ops::X);
    assert_eq!(x_gate.q1, 0);
    // X gate matrix: [[0, 1], [1, 0]]
    assert!((x_gate._00r - 0.0).abs() < f32::EPSILON);
    assert!((x_gate._01r - 1.0).abs() < f32::EPSILON);
    assert!((x_gate._10r - 1.0).abs() < f32::EPSILON);
    assert!((x_gate._11r - 0.0).abs() < f32::EPSILON);

    let h_gate = Op::new_h_gate(1);
    assert_eq!(h_gate.id, ops::H);
    assert_eq!(h_gate.q1, 1);
    // H gate matrix: [[1/√2, 1/√2], [1/√2, -1/√2]]
    let expected_val = 1.0 / (2.0_f32).sqrt();
    assert!((h_gate._00r - expected_val).abs() < f32::EPSILON);
    assert!((h_gate._01r - expected_val).abs() < f32::EPSILON);
    assert!((h_gate._10r - expected_val).abs() < f32::EPSILON);
    assert!((h_gate._11r - (-expected_val)).abs() < f32::EPSILON);

    let s_gate = Op::new_s_gate(2);
    assert_eq!(s_gate.id, ops::S);
    assert_eq!(s_gate.q1, 2);
    // S gate matrix: [[1, 0], [0, i]]
    assert!((s_gate._00r - 1.0).abs() < f32::EPSILON);
    assert!((s_gate._00i - 0.0).abs() < f32::EPSILON);
    assert!((s_gate._11r - 0.0).abs() < f32::EPSILON);
    assert!((s_gate._11i - 1.0).abs() < f32::EPSILON);

    let t_gate = Op::new_t_gate(3);
    assert_eq!(t_gate.id, ops::T);
    assert_eq!(t_gate.q1, 3);
    // T gate matrix: [[1, 0], [0, e^(iπ/4)]]
    let pi_4 = PI / 4.0;
    assert!((t_gate._00r - 1.0).abs() < f32::EPSILON);
    assert!((t_gate._11r - pi_4.cos()).abs() < f32::EPSILON);
    assert!((t_gate._11i - pi_4.sin()).abs() < f32::EPSILON);
}

#[test]
fn test_rotation_gate_utility_functions() {
    // Test parametric rotation gates
    let angle = PI / 2.0;

    let rx_gate = Op::new_rx_gate(angle, 0);
    assert_eq!(rx_gate.id, ops::RX);
    assert!((rx_gate.angle - angle).abs() < f32::EPSILON);
    // RX(π/2) should have cos(π/4) on diagonal, -i*sin(π/4) off-diagonal
    let half_angle = angle / 2.0;
    assert!((rx_gate._00r - half_angle.cos()).abs() < f32::EPSILON);
    assert!((rx_gate._11r - half_angle.cos()).abs() < f32::EPSILON);
    assert!((rx_gate._01i - (-half_angle.sin())).abs() < f32::EPSILON);
    assert!((rx_gate._10i - (-half_angle.sin())).abs() < f32::EPSILON);

    let ry_operation = Op::new_ry_gate(angle, 1);
    assert_eq!(ry_operation.id, ops::RY);
    assert!((ry_operation.angle - angle).abs() < f32::EPSILON);
    // RY(π/2) should have cos(π/4) on diagonal, ±sin(π/4) off-diagonal
    assert!((ry_operation._00r - half_angle.cos()).abs() < f32::EPSILON);
    assert!((ry_operation._11r - half_angle.cos()).abs() < f32::EPSILON);
    assert!((ry_operation._01r - (-half_angle.sin())).abs() < f32::EPSILON);
    assert!((ry_operation._10r - half_angle.sin()).abs() < f32::EPSILON);

    let rz_op = Op::new_rz_gate(angle, 2);
    assert_eq!(rz_op.id, ops::RZ);
    assert!((rz_op.angle - angle).abs() < f32::EPSILON);
    // RZ(π/2) should have e^(-iπ/4) and e^(iπ/4) on diagonal
    assert!((rz_op._00r - (-half_angle).cos()).abs() < f32::EPSILON);
    assert!((rz_op._00i - (-half_angle).sin()).abs() < f32::EPSILON);
    assert!((rz_op._11r - half_angle.cos()).abs() < f32::EPSILON);
    assert!((rz_op._11i - half_angle.sin()).abs() < f32::EPSILON);
}

#[test]
fn test_x_gate_using_utility() {
    // Test that X gate created with utility function works correctly
    let op = Op::new_x_gate(1);
    let m = m_every_z();
    let operations = vec![op, m];
    let r = run_gpu_simulator(2, operations);
    let prob_str = write_probabilities(2, &r);

    expect![[r#"
        Probabilities:
        |00⟩: 0.000000
        |01⟩: 1.000000
        |10⟩: 0.000000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}

#[test]
fn test_h_gate_using_utility() {
    // Test that H gate created with utility function works correctly
    let op = Op::new_h_gate(0);
    let m = m_every_z();
    let operations = vec![op, m];
    let r = run_gpu_simulator(2, operations);
    let prob_str = write_probabilities(2, &r);

    expect![[r#"
        Probabilities:
        |00⟩: 0.500000
        |01⟩: 0.000000
        |10⟩: 0.500000
        |11⟩: 0.000000
    "#]]
    .assert_eq(&prob_str);
}
