// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    noise_config::{NoiseConfig, NoiseTable},
    shader_types::{Op, ops},
};

// Use the 'real' parts of the Op to store the pauli probabilities.
// For 1q ops, r00 = pI, r01 = pX, r02 = pY, r03 = pZ
// For 2q ops, r00 = pII, r01 = pIX, r02 = pIY, r03 = pIZ
//             r10 = pXI, r11 = pXX, r12 = pXY, r13 = pXZ
//             r20 = pYI, r21 = pYX, r22 = pYY, r23 = pYZ
//             r30 = pZI, r31 = pZX, r32 = pZY, r33 = pZZ
fn set_noise_op_probabilities(noise_table: &NoiseTable<f32>, op: &mut Op) {
    noise_table
        .pauli_strings
        .iter()
        .zip(&noise_table.probabilities)
        .for_each(|(pauli, prob)| match noise_table.qubits {
            1 => match pauli {
                0 => op.r00 = *prob,
                1 => op.r01 = *prob,
                2 => op.r02 = *prob,
                3 => op.r03 = *prob,
                _ => panic!("Invalid pauli string for 1 qubit: {pauli}"),
            },
            2 => match pauli {
                0b_00_00 => op.r00 = *prob,
                0b_00_01 => op.r01 = *prob,
                0b_00_10 => op.r02 = *prob,
                0b_00_11 => op.r03 = *prob,
                0b_01_00 => op.r10 = *prob,
                0b_01_01 => op.r11 = *prob,
                0b_01_10 => op.r12 = *prob,
                0b_01_11 => op.r13 = *prob,
                0b_10_00 => op.r20 = *prob,
                0b_10_01 => op.r21 = *prob,
                0b_10_10 => op.r22 = *prob,
                0b_10_11 => op.r23 = *prob,
                0b_11_00 => op.r30 = *prob,
                0b_11_01 => op.r31 = *prob,
                0b_11_10 => op.r32 = *prob,
                0b_11_11 => op.r33 = *prob,
                _ => panic!("Invalid pauli string for 2 qubits: {pauli}"),
            },
            _ => panic!(
                "Unsupported qubit count in noise table: {}",
                noise_table.qubits
            ),
        });
}

fn get_noise_op(op: &Op, noise_table: &NoiseTable<f32>) -> Op {
    match noise_table.qubits {
        1 => {
            let mut op = Op::new_1q_gate(ops::PAULI_NOISE_1Q, op.q1);
            set_noise_op_probabilities(noise_table, &mut op);
            op
        }
        2 => {
            let mut op = Op::new_2q_gate(ops::PAULI_NOISE_2Q, op.q1, op.q2);
            set_noise_op_probabilities(noise_table, &mut op);
            op
        }
        _ => panic!(
            "Unsupported qubit count in noise table: {}",
            noise_table.qubits
        ),
    }
}

#[must_use]
pub fn get_noise_ops(op: &Op, noise_config: &NoiseConfig<f32, f64>) -> Option<Vec<Op>> {
    let noise_table = match op.id {
        ops::ID => &noise_config.i,
        ops::X => &noise_config.x,
        ops::Y => &noise_config.y,
        ops::Z => &noise_config.z,
        ops::H => &noise_config.h,
        ops::S => &noise_config.s,
        ops::S_ADJ => &noise_config.s_adj,
        ops::T => &noise_config.t,
        ops::T_ADJ => &noise_config.t_adj,
        ops::SX => &noise_config.sx,
        ops::SX_ADJ => &noise_config.sx_adj,
        ops::RX => &noise_config.rx,
        ops::RY => &noise_config.ry,
        ops::RZ => &noise_config.rz,
        ops::CX => &noise_config.cx,
        ops::CZ => &noise_config.cz,
        ops::RXX => &noise_config.rxx,
        ops::RYY => &noise_config.ryy,
        ops::RZZ => &noise_config.rzz,
        ops::SWAP => &noise_config.swap,
        ops::MOVE => &noise_config.mov,
        ops::MRESETZ => &noise_config.mresetz,
        _ => return None,
    };
    if noise_table.is_noiseless() {
        return None;
    }
    let mut results = vec![];
    if noise_table.has_pauli_noise() {
        results.push(get_noise_op(op, noise_table));
    }

    if noise_table.loss > 0.0 {
        if ops::is_2q_op(op.id) {
            // For two-qubit gates, doing loss inline is hard, so just append an Id gate with loss for each qubit
            results.push(Op::new_id_gate(op.q1));
            results.push(Op::new_loss_noise(op.q1, noise_table.loss));
            results.push(Op::new_id_gate(op.q2));
            results.push(Op::new_loss_noise(op.q2, noise_table.loss));
        } else if ops::is_1q_op(op.id) {
            // For one-qubit gates, just add the loss noise on the one qubit operation
            results.push(Op::new_loss_noise(op.q1, noise_table.loss));
        } else {
            panic!("unsupported op for loss noise: {op:?}");
        }
    }
    Some(results)
}
