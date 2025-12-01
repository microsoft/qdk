// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

mod gpu_controller;

pub mod shader_types;

use crate::gpu_full_state_simulator::shader_types::Op;

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
