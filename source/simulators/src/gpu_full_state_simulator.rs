// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

mod gpu_context;
pub mod shader_types;

use crate::gpu_full_state_simulator::shader_types::Op;

#[must_use]
pub fn run_gpu_simulator(qubits: u32, ops: Vec<Op>) -> Vec<shader_types::Result> {
    let qubits = i32::try_from(qubits).expect("num_qubits should fit in u32");
    futures::executor::block_on(async {
        let mut gpu_context = gpu_context::GpuContext::new(qubits, ops).await;
        gpu_context.create_resources();
        gpu_context.run().await
    })
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
