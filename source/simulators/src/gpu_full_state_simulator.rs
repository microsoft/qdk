// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

mod gpu_context;
pub mod shader_types;

use crate::{gpu_full_state_simulator::shader_types::Op, shader_types::ops};

use rand::{Rng, rngs::StdRng};

#[must_use]
pub fn run_gpu_simulator(qubits: u32, ops: Vec<Op>) -> Vec<shader_types::Result> {
    let qubits = i32::try_from(qubits).expect("num_qubits should fit in u32");
    futures::executor::block_on(async {
        let mut gpu_context = gpu_context::GpuContext::new(qubits, ops).await;
        gpu_context.create_resources();
        gpu_context.run().await
    })
}

pub fn run_gpu_simulator_with_pauli_noise(
    qubits: u32,
    ops: Vec<Op>,
    noise: [f64; 3],
    rng: &mut StdRng,
) -> Vec<shader_types::Result> {
    let ops = apply_pauli_noise(ops, rng, noise);
    run_gpu_simulator(qubits, ops)
}

#[must_use]
pub fn time_run_gpu_simulator(qubits: u32, ops: Vec<Op>) -> Vec<shader_types::Result> {
    let qubits = i32::try_from(qubits).expect("num_qubits should fit in u32");

    let now = std::time::Instant::now();
    let res = futures::executor::block_on(async {
        let mut gpu_context = gpu_context::GpuContext::new(qubits, ops).await;
        gpu_context.create_resources();
        gpu_context.run().await
    });

    eprintln!("GPU elapsed: {:?}", now.elapsed());

    res
}

fn apply_pauli_noise(mut ops: Vec<Op>, rng: &mut StdRng, noise: [f64; 3]) -> Vec<Op> {
    let mut new_ops = Vec::with_capacity(ops.len());
    for op in ops.drain(..) {
        new_ops.push(op);
        match op.id {
            ops::ID..=ops::CZ => {
                if let Some(noise_op) = get_noise_op(rng, &noise, op.q1) {
                    new_ops.push(noise_op);
                }
            }
            ops::RXX..=ops::RZZ | ops::SWAP => {
                if let Some(noise_op) = get_noise_op(rng, &noise, op.q1) {
                    new_ops.push(noise_op);
                }
                if let Some(noise_op) = get_noise_op(rng, &noise, op.q2) {
                    new_ops.push(noise_op);
                }
            }
            ops::CCX => {
                if let Some(noise_op) = get_noise_op(rng, &noise, op.q1) {
                    new_ops.push(noise_op);
                }
                if let Some(noise_op) = get_noise_op(rng, &noise, op.q2) {
                    new_ops.push(noise_op);
                }
                if let Some(noise_op) = get_noise_op(rng, &noise, op.q3) {
                    new_ops.push(noise_op);
                }
            }
            _ => {}
        }
    }
    new_ops
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
