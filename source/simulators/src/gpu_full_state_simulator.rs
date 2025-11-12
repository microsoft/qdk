// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

mod gpu_controller;

pub mod shader_types;

use crate::gpu_full_state_simulator::shader_types::Op;

pub fn try_create_gpu_adapter() -> Result<(), String> {
    let _ = futures::executor::block_on(async { gpu_controller::GpuContext::get_adapter().await })?;
    Ok(())
}

pub fn run_parallel_shots(
    qubits: u32,
    results: u32,
    ops: Vec<Op>,
    shots: u32,
) -> Result<Vec<u32>, String> {
    futures::executor::block_on(async {
        let mut controller = gpu_controller::GpuContext::new(qubits, results, ops, shots, true)
            .await
            .map_err(|e| e.to_string())?;
        controller.create_resources();
        Ok(controller.run().await)
    })
}
