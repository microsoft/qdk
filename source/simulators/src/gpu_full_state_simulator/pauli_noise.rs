// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{gpu_full_state_simulator::shader_types::Op, shader_types::ops};

use num_bigint::BigUint;
use rand::{Rng, rngs::StdRng};

#[allow(clippy::too_many_lines)]
pub fn apply_pauli_noise_with_loss(
    ops: Vec<Op>,
    rng: &mut StdRng,
    noise: [f64; 3],
    loss: Option<f64>,
) -> Vec<Op> {
    let mut lost_qubits = BigUint::from(0u64);
    let mut ops = ops;
    let mut new_ops = Vec::with_capacity(ops.len());

    for op in ops.drain(..) {
        match op.id {
            ops::X..=ops::RZ | ops::ID => {
                if !is_qubit_lost(op.q1, &mut lost_qubits) {
                    new_ops.push(op);
                }
                if let Some(noise_op) = apply_noise(op.q1, &mut lost_qubits, noise, loss, rng) {
                    new_ops.push(noise_op);
                }
            }
            ops::CX..=ops::CZ | ops::SWAP => {
                if !is_qubit_lost(op.q1, &mut lost_qubits)
                    && !is_qubit_lost(op.q2, &mut lost_qubits)
                {
                    new_ops.push(op);
                }
                if let Some(noise_op) = apply_noise(op.q1, &mut lost_qubits, noise, loss, rng) {
                    new_ops.push(noise_op);
                }
                if let Some(noise_op) = apply_noise(op.q2, &mut lost_qubits, noise, loss, rng) {
                    new_ops.push(noise_op);
                }
            }
            ops::RXX..=ops::RZZ => {
                match (
                    is_qubit_lost(op.q1, &mut lost_qubits),
                    is_qubit_lost(op.q2, &mut lost_qubits),
                ) {
                    (true, true) => {}
                    (true, false) => {
                        let op = match op.id {
                            ops::RXX => Op::new_rx_gate(op.angle, op.q2),
                            ops::RYY => Op::new_ry_gate(op.angle, op.q2),
                            ops::RZZ => Op::new_rz_gate(op.angle, op.q2),
                            _ => unreachable!(""),
                        };
                        new_ops.push(op);
                    }
                    (false, true) => {
                        let op = match op.id {
                            ops::RXX => Op::new_rx_gate(op.angle, op.q1),
                            ops::RYY => Op::new_ry_gate(op.angle, op.q1),
                            ops::RZZ => Op::new_rz_gate(op.angle, op.q1),
                            _ => unreachable!(""),
                        };
                        new_ops.push(op);
                    }
                    (false, false) => new_ops.push(op),
                }
                if let Some(noise_op) = apply_noise(op.q1, &mut lost_qubits, noise, loss, rng) {
                    new_ops.push(noise_op);
                }
                if let Some(noise_op) = apply_noise(op.q2, &mut lost_qubits, noise, loss, rng) {
                    new_ops.push(noise_op);
                }
            }
            ops::CCX => {
                match (
                    is_qubit_lost(op.q1, &mut lost_qubits),
                    is_qubit_lost(op.q2, &mut lost_qubits),
                    is_qubit_lost(op.q3, &mut lost_qubits),
                ) {
                    (true, true, _) | (_, _, true) => {
                        // If the target qubit is lost or both controls are lost, skip the operation.
                    }

                    // When only one control is lost, use the other to do a singly controlled X.
                    (true, false, false) => {
                        let op = Op::new_cx_gate(op.q2, op.q3);
                        new_ops.push(op);
                    }
                    (false, true, false) => {
                        let op = Op::new_cx_gate(op.q1, op.q3);
                        new_ops.push(op);
                    }

                    // No qubits lost, execute normally.
                    (false, false, false) => {
                        new_ops.push(op);
                    }
                }
                if let Some(noise_op) = apply_noise(op.q1, &mut lost_qubits, noise, loss, rng) {
                    new_ops.push(noise_op);
                }
                if let Some(noise_op) = apply_noise(op.q2, &mut lost_qubits, noise, loss, rng) {
                    new_ops.push(noise_op);
                }
                if let Some(noise_op) = apply_noise(op.q3, &mut lost_qubits, noise, loss, rng) {
                    new_ops.push(noise_op);
                }
            }
            ops::RESET | ops::MRESETZ => {
                // Applying noise before measurement
                if let Some(noise_op) = apply_noise(op.q1, &mut lost_qubits, noise, loss, rng) {
                    new_ops.push(noise_op);
                }
                // TODO: is this valid in base profile or just adaptive?
                // if is_qubit_lost(op.q1, &mut lost_qubits) {
                //     // If the qubit is lost, we cannot measure it.
                //     // Mark it as no longer lost so it becomes usable again, since
                //     // measurement will "reload" the qubit.
                //     lost_qubits.set_bit(u64::from(op.q1), false);
                // }

                //todo!("do mresetz");

                // Applying noise after reset
                if let Some(noise_op) = apply_noise(op.q1, &mut lost_qubits, noise, loss, rng) {
                    new_ops.push(noise_op);
                }
            }
            _ => {
                // mz, meveryz, matrix, matix_2q
                new_ops.push(op);
            }
        }
    }
    new_ops
}

fn is_qubit_lost(q: u32, lost_qubits: &mut BigUint) -> bool {
    lost_qubits.bit(u64::from(q))
}

fn apply_noise(
    q: u32,
    lost_qubits: &mut BigUint,
    noise: [f64; 3],
    loss: Option<f64>,
    rng: &mut StdRng,
) -> Option<Op> {
    if is_qubit_lost(q, lost_qubits) {
        return None;
    }
    if let Some(loss) = loss {
        // First, check for loss.
        let p = rng.gen_range(0.0..1.0);
        if p < loss {
            // The qubit is lost, so we reset it.
            // It is not safe to release the qubit here, as that may
            // interfere with later operations (gates or measurements)
            // or even normal qubit release at end of scope.

            // TODO: mresetz
            // if self.sim.measure(q) {
            //     self.sim.x(q);
            // }

            // Mark the qubit as lost.
            lost_qubits.set_bit(u64::from(q), true);
            return None;
        }
    }

    get_noise_op(rng, &noise, q)
}

fn get_noise_op(rng: &mut StdRng, noise: &[f64; 3], qubit: u32) -> Option<Op> {
    let p = rng.gen_range(0.0..1.0);
    if p >= noise[2] {
        None
    } else if p < noise[0] {
        Some(Op::new_x_gate(qubit))
    } else if p < noise[1] {
        Some(Op::new_y_gate(qubit))
    } else {
        Some(Op::new_z_gate(qubit))
    }
}
