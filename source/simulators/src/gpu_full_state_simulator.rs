// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

mod gpu_controller;

pub mod noise_mapping;
pub mod shader_types;

use crate::{gpu_full_state_simulator::shader_types::Op, noise_config::NoiseConfig};

pub fn try_create_gpu_adapter() -> Result<String, String> {
    let adapter = gpu_controller::GpuContext::get_adapter()?;
    let info = adapter.get_info();
    Ok(format!("{info:?}"))
}

pub fn run_parallel_shots(
    qubits: i32,
    results: i32,
    ops: Vec<Op>,
    shots: i32,
    rng_seed: u32,
) -> Result<Vec<u32>, String> {
    futures::executor::block_on(async {
        let mut controller =
            gpu_controller::GpuContext::new(qubits, results, ops, shots, rng_seed, true)
                .await
                .map_err(|e| e.clone())?;
        controller.create_resources();
        Ok(controller.run().await)
    })
}

pub fn run_shots_with_noise(
    qubits: i32,
    results: i32,
    ops: Vec<Op>,
    shots: i32,
    rng_seed: u32,
    noise: &Option<NoiseConfig>,
) -> Result<Vec<u32>, String> {
    // Only do the mapping if a noise config is provided, else just run the ops directly
    if let Some(noise) = noise {
        let mut noisy_ops: Vec<Op> = Vec::with_capacity(ops.len() + 1);

        for op in ops {
            let mut add_ops: Vec<Op> = vec![op];
            // If there's a NoiseConfig, and we get noise for this op, append it
            if let Some(noise_ops) = noise_mapping::get_noise_ops(&op, noise) {
                add_ops.extend(noise_ops);
            }
            // If it's an MResetZ with noise, change to an Id with noise, followed by MResetZ
            // (This is just simpler to implement than doing noise inline with MResetZ for now)
            if op.id == shader_types::ops::MRESETZ && add_ops.len() > 1 {
                let mz_copy = add_ops[0];
                add_ops[0] = Op::new_id_gate(op.q1);
                add_ops.push(mz_copy);
            }
            // Convert 'mov' ops to identity, and don't add the ops if it's just a
            // single identity (but do add if it has noise)
            if add_ops[0].id == shader_types::ops::MOVE {
                add_ops[0].id = shader_types::ops::ID;
            }
            if add_ops.len() == 1 && add_ops[0].id == shader_types::ops::ID {
                // skip lone identity gates
            } else {
                noisy_ops.extend(add_ops);
            }
        }
        run_parallel_shots(qubits, results, noisy_ops, shots, rng_seed)
    } else {
        run_parallel_shots(qubits, results, ops, shots, rng_seed)
    }
}
