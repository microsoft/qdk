// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

pub mod correlated_noise;
pub mod gpu_context;
pub mod gpu_resources;
pub mod noise_mapping;
pub mod shader_types;

use crate::{
    gpu_context::RunResults, gpu_full_state_simulator::shader_types::Op, noise_config::NoiseConfig,
};

pub fn try_create_gpu_adapter() -> Result<String, String> {
    gpu_context::GpuContext::try_create_adapter()
}

pub fn run_shots_sync(
    qubit_count: i32,
    result_count: i32,
    ops: &[Op],
    noise: &Option<NoiseConfig>,
    shot_count: i32,
    rng_seed: u32,
    start_shot_id: i32,
) -> Result<RunResults, String> {
    futures::executor::block_on(async {
        let mut context = gpu_context::GpuContext::default();

        if let Some(noise_config) = noise {
            context.set_noise_config(noise_config.clone());
        }

        context.set_program(ops, qubit_count, result_count);

        context.run_shots(shot_count, rng_seed, start_shot_id).await
    })
}
