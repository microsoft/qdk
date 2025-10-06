// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

mod gpu_context;
pub mod pauli_noise;

pub mod per_gate_pauli_noise;

pub mod shader_types;

use crate::gpu_full_state_simulator::{gpu_context::GpuContext, shader_types::Op};

/// Checks if a compatible GPU adapter is available on the system.
///
/// This function attempts to request a GPU adapter to determine if GPU-accelerated
/// quantum simulation is supported. It's useful for capability detection before
/// attempting to run GPU-based simulations.
///
/// # Errors
///
/// Returns `Err(String)` if:
/// - No compatible GPU is found
/// - GPU drivers are missing or not functioning properly
pub fn try_create_gpu_adapter() -> Result<(), String> {
    let _ = futures::executor::block_on(async { GpuContext::get_adapter().await })?;
    Ok(())
}

pub fn run_gpu_simulator(qubits: u32, ops: Vec<Op>) -> Result<Vec<shader_types::Result>, String> {
    futures::executor::block_on(async {
        let mut gpu_context = gpu_context::GpuContext::new(qubits, ops)
            .await
            .map_err(|e| e.to_string())?;
        gpu_context.create_resources();
        Ok(gpu_context.run().await)
    })
}

pub fn run_gpu_shot(qubits: u32, ops: Vec<Op>) -> Result<shader_types::Result, String> {
    let mut results = run_gpu_simulator(qubits, ops)?;
    if results.is_empty() {
        return Err(format!(
            "expected > 0 results from GPU simulator, got {}",
            results.len()
        ));
    }
    // The sampled result is always in index 0
    Ok(results.remove(0))
}

pub fn time_run_gpu_simulator(
    qubits: u32,
    ops: Vec<Op>,
) -> Result<Vec<shader_types::Result>, String> {
    let now = std::time::Instant::now();

    let res = run_gpu_simulator(qubits, ops)?;

    eprintln!("GPU elapsed: {:?}", now.elapsed());

    Ok(res)
}
