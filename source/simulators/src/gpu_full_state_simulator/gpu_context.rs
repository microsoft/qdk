// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cmp::min;

use bytemuck::cast_slice;

use crate::correlated_noise::NoiseTables;
use crate::gpu_resources::GpuResources;
use crate::noise_config::NoiseConfig;
use crate::noise_mapping::get_noise_ops;
use crate::shader_types::{
    DiagnosticsData, MAX_BUFFER_SIZE, MAX_QUBIT_COUNT, MAX_QUBITS_PER_WORKGROUP, MAX_SHOT_ENTRIES,
    MAX_SHOTS_PER_BATCH, MIN_QUBIT_COUNT, Op, SIZEOF_SHOTDATA, THREADS_PER_WORKGROUP, Uniforms,
    WorkgroupCollationBuffer, ops,
};

// On Windows, running larger circuits/shots can hit TDR issues if too many ops are dispatched in one go.
const DEFAULT_MAX_OPS_PER_DISPATCH: i32 = 16;

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default)]
pub struct GpuContext {
    resources: GpuResources,

    program: Vec<Op>,
    program_with_noise: Option<Vec<Op>>,
    noise_config: Option<NoiseConfig>,
    noise_tables: NoiseTables,

    run_params: RunParams,

    // Indicates if items impacting the Ops have changed and need to be re-uploaded / recompiled
    program_is_dirty: bool,
    // Indicates if the pipeline needs to be recompiled due to changes (i.e., qubit or result count)
    pipeline_is_dirty: bool,
    // Indicates if the noise config has changed (which means the program may be dirty too)
    noise_config_is_dirty: bool,
    // Indicates if the noise tables have changed and need to be re-uploaded to the GPU
    noise_tables_is_dirty: bool,
}

#[derive(Debug, Default)]
struct RunParams {
    qubit_count: i32,
    result_count: i32,
    shot_count: i32,
    shots_per_batch: i32,
    batch_count: i32,
    workgroups_per_shot: i32,
    entries_per_thread: i32,
    shots_buffer_size: usize,
    state_vector_buffer_size: usize,
    results_buffer_size: usize,
    diagnostics_buffer_size: usize,
    download_buffer_size: usize,
}

#[derive(Debug)]
pub struct RunResults {
    pub shot_results: Vec<Vec<u32>>,
    pub shot_result_codes: Vec<u32>,
    // Box used below lessen size of RunResults struct, as DiagnosticsData is large, and
    // clippy was warning about the size of the Future returned by run_shots otherwise,
    // as the large value may put on the stack which can cause stack overflows.
    // As the DiagnosticsData is only needed if there are errors, we use an Option here, and
    // the value is only created if there are errors (i.e. hopefully rarely).
    pub diagnostics: Option<Box<DiagnosticsData>>,
    pub success: bool,
}

impl GpuContext {
    // See if we can get a GPU adapter on this machine (useful before trying to run tests)
    // Note: This does NOT allocate GPU resources that persist across runs. Run shots for that.
    pub fn try_create_adapter() -> Result<String, String> {
        Ok(format!("{:?}", GpuResources::try_get_adapter()?.get_info()))
    }

    /// Set the program to be run
    pub fn set_program(&mut self, program: &[Op], qubit_count: i32, result_count: i32) {
        // Always allocate a minumum number of qubits to ensure good data alignment, GPU thread usage, etc.
        let qubit_count = qubit_count.max(MIN_QUBIT_COUNT);

        self.program.clear();
        self.program.extend_from_slice(program);

        // If qubit or result count changed, mark pipeline as dirty, as it needs to be recreated
        if qubit_count != self.run_params.qubit_count
            || result_count != self.run_params.result_count
        {
            self.pipeline_is_dirty = true;
        }

        self.run_params.qubit_count = qubit_count;
        self.run_params.result_count = result_count;

        // Mark program as dirty, so we recreate the ops to send to the GPU on next run
        self.program_is_dirty = true;
    }

    fn get_program(&self) -> &[Op] {
        if let Some(noisy_program) = &self.program_with_noise {
            noisy_program
        } else {
            &self.program
        }
    }

    /// Set the noise configuration
    pub fn set_noise_config(&mut self, noise: NoiseConfig) {
        self.noise_config = Some(noise);
        self.noise_config_is_dirty = true;
    }

    /// Add a correlated noise table to the GPU context (will overwrite existing with same name)
    pub fn add_correlated_noise_table(&mut self, name: &str, contents: &str) {
        self.noise_tables.add(name, contents);
        self.noise_tables_is_dirty = true;
    }

    /// Get the mapping of correlated noise table ids to names and entry counts
    #[must_use]
    pub fn get_correlated_noise_tables(&self) -> Vec<(u32, String, u32)> {
        (0u32..)
            .zip(&self.noise_tables.names)
            .map(|(idx, name)| {
                (
                    idx,
                    name.clone(),
                    self.noise_tables.metadata[idx as usize].entry_count,
                )
            })
            .collect()
    }

    /// Clear the correlated noise tables
    pub fn clear_correlated_noise_tables(&mut self) {
        self.noise_tables = NoiseTables::default();
        self.noise_tables_is_dirty = true;
    }

    pub fn run_shots_sync(
        &mut self,
        shot_count: i32,
        seed: u32,
        start_shot_id: i32,
    ) -> Result<RunResults, String> {
        futures::executor::block_on(self.run_shots(shot_count, seed, start_shot_id))
    }

    /// Run the program for the given number of shots
    #[allow(clippy::too_many_lines)]
    pub async fn run_shots(
        &mut self,
        shot_count: i32,
        seed: u32,
        start_shot_id: i32,
    ) -> Result<RunResults, String> {
        if self.program.is_empty() {
            return Err("No program has been set".to_string());
        }
        // Update all the parameters we need for GPU resources based on current program, noise, shots, etc.
        self.update_run_params(shot_count);
        self.ready_gpu_resources().await?;

        // Get the GPU resources we need for the entire run
        let bind_group = self.resources.get_bind_group()?;
        let kernels = self.resources.get_kernels()?;

        // Use environment variable "QDK_GPU_MAX_OPS_PER_DISPATCH" to override the default if set
        let max_ops_per_dispatch: i32 = std::env::var("QDK_GPU_MAX_OPS_PER_DISPATCH")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(DEFAULT_MAX_OPS_PER_DISPATCH);

        let mut results: Vec<u32> = Vec::new();
        let mut diagnostics: Option<Box<DiagnosticsData>> = None;

        let mut shots_remaining = self.run_params.shot_count;

        for batch_idx in 0..self.run_params.batch_count {
            let shots_this_batch = min(shots_remaining, self.run_params.shots_per_batch);

            // Update the uniforms for this batch
            let uniforms = Uniforms {
                batch_start_shot_id: batch_idx * self.run_params.shots_per_batch + start_shot_id,
                rng_seed: seed,
            };

            // When this is put directly on the queue, it will be submitted when the next submit occurs, but will run before that submit's work
            self.resources.upload_uniform(&uniforms)?;

            let prepare_workgroup_count =
                u32::try_from(shots_this_batch).expect("shots_per_batch should fit in u32");
            // Workgroups for execute_op depends on qubit count
            let execute_workgroup_count =
                u32::try_from(self.run_params.workgroups_per_shot * shots_this_batch)
                    .expect("workgroups_per_shot * shots_per_batch should fit in u32");

            // Split ops into chunks to avoid exceeding max_ops_per_dispatch limit
            // (noise ops don't count toward the limit)
            let mut op_chunks = Vec::new();
            let mut current_chunk = Vec::new();
            let mut non_noise_count = 0;

            for op in self.get_program() {
                // Check if this is a noise op
                let is_noise = matches!(op.id, ops::PAULI_NOISE_1Q..=ops::LOSS_NOISE);

                if !is_noise {
                    non_noise_count += 1;
                    if non_noise_count > max_ops_per_dispatch {
                        // Start a new chunk
                        op_chunks.push(std::mem::take(&mut current_chunk));
                        non_noise_count = 1;
                    }
                }

                current_chunk.push(op);
            }

            // Add the last chunk if it has any ops
            if !current_chunk.is_empty() {
                op_chunks.push(current_chunk);
            }

            // Process each chunk in a separate command buffer to avoid TDR
            for (chunk_idx, chunk) in op_chunks.iter().enumerate() {
                let mut encoder = self.resources.get_encoder("StateVector Command Encoder")?;

                let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("StateVector Compute Pass"),
                    timestamp_writes: None,
                });

                compute_pass.set_bind_group(0, &bind_group, &[]);

                // Start by dispatching an 'init' kernel for the batch of shots (only for first chunk)
                if chunk_idx == 0 {
                    compute_pass.set_pipeline(&kernels.init_op);
                    compute_pass.dispatch_workgroups(execute_workgroup_count, 1, 1);
                }

                // Dispatch the compute shaders for each op in this chunk
                for op in chunk {
                    match op.id {
                        // One qubit gates (and correlated noise ops)
                        ops::ID..=ops::RZ
                        | ops::MOVE
                        | ops::MRESETZ
                        | ops::MZ
                        | ops::CORRELATED_NOISE => {
                            compute_pass.set_pipeline(&kernels.prepare_op);
                            compute_pass.dispatch_workgroups(prepare_workgroup_count, 1, 1);

                            compute_pass.set_pipeline(&kernels.execute_op);
                            compute_pass.dispatch_workgroups(execute_workgroup_count, 1, 1);
                        }
                        // Two qubit gates
                        ops::CX..=ops::RZZ | ops::SWAP => {
                            compute_pass.set_pipeline(&kernels.prepare_op);
                            compute_pass.dispatch_workgroups(prepare_workgroup_count, 1, 1);

                            compute_pass.set_pipeline(&kernels.execute_2q_op);
                            compute_pass.dispatch_workgroups(execute_workgroup_count, 1, 1);
                        }
                        // Skip over simple noise ops
                        ops::PAULI_NOISE_1Q..=ops::LOSS_NOISE => {}
                        _ => {
                            panic!("Unsupported op ID {}", op.id);
                        }
                    }
                }

                drop(compute_pass);

                // Submit this chunk's command buffer
                let chunk_command_buffer = encoder.finish();
                self.resources.submit_command_buffer(chunk_command_buffer)?;
            }

            let (batch_results, batch_diagnostics) =
                self.resources.download_batch_results().await?;

            results.extend(batch_results);
            if batch_diagnostics.error_code != 0 && diagnostics.is_none() {
                diagnostics = Some(Box::new(batch_diagnostics));
            }

            shots_remaining -= self.run_params.shots_per_batch;
        }

        // We may have had extra shots if the last batch was not full. Truncate if so.
        let result_count = self.run_params.result_count;
        let expected_results: usize = (self.run_params.shot_count * (result_count + 1)) // +1 for the error code per shot
            .try_into()
            .expect("Total expected result count should fit in usize");
        results.truncate(expected_results);

        // Split results into measurements and result codes
        let result_count_usize: usize = result_count
            .try_into()
            .expect("Result count should fit in usize");
        let shot_results = results
            .chunks(result_count_usize + 1)
            .map(|chunk| chunk[..result_count_usize].to_vec())
            .collect::<Vec<Vec<u32>>>();
        // Separate out every 3rd entry from results into 'error_codes'
        let shot_result_codes = results
            .chunks(result_count_usize + 1)
            .map(|chunk| chunk[result_count_usize])
            .collect::<Vec<u32>>();

        let success = shot_result_codes.iter().all(|&code| code == 0);

        Ok(RunResults {
            shot_results,
            shot_result_codes,
            diagnostics,
            success,
        })
    }

    async fn ready_gpu_resources(&mut self) -> Result<(), String> {
        // Ensure the device is initialized
        let dbg_capture = std::env::var("QDK_GPU_CAPTURE").is_ok();
        self.resources.ensure_device(dbg_capture).await?;

        if self.program_is_dirty || self.noise_config_is_dirty {
            // Rebuild the program (with noise if needed) and upload to GPU
            if let Some(noise) = &self.noise_config {
                let ops = add_noise_config_to_ops(&self.program, noise);
                self.resources.upload_ops_data(cast_slice(&ops))?;
                self.program_with_noise = Some(ops);
            } else {
                self.resources.upload_ops_data(cast_slice(&self.program))?;
                self.program_with_noise = None;
            }
            self.program_is_dirty = false;
        }

        if self.noise_tables_is_dirty {
            // Re-upload noise tables to GPU (if any)
            let tables = &self.noise_tables;

            if tables.metadata.is_empty() {
                self.resources.free_noise_buffers()?;
            } else {
                self.resources
                    .upload_noise_metadata(cast_slice(&tables.metadata))?;
                self.resources
                    .upload_noise_entries(cast_slice(&tables.entries))?;
            }
        }

        let params = &self.run_params;

        if self.pipeline_is_dirty {
            // The pipeline is marked as dirty if the qubit or result count changed (shot count doesn't impact it)
            self.resources.create_shaders(
                params.qubit_count,
                params.result_count,
                // The next two params are derived from qubit count and result count, so will only change if those do
                params.workgroups_per_shot,
                params.entries_per_thread,
                // The below are constants so will not change from run to run
                THREADS_PER_WORKGROUP,
                MAX_QUBIT_COUNT,
                MAX_QUBITS_PER_WORKGROUP,
            )?;
        }

        self.resources.ensure_run_buffers(
            params.shots_buffer_size,
            params.state_vector_buffer_size,
            params.results_buffer_size,
            params.diagnostics_buffer_size,
        )?;
        Ok(())
    }

    fn update_run_params(&mut self, shot_count: i32) {
        let params = &mut self.run_params;

        // The qubit and result count were already set when the program was set
        params.shot_count = shot_count;

        let entries_per_shot = 1 << params.qubit_count; // 2^n state vector entries per shot for n qubits
        let state_vector_size_per_shot = entries_per_shot * 8; // Each entry is a complex containing 2 * f32

        // Figure out some limits based on buffer size limits, structure sizes, and number of qubits
        let max_shot_state_vectors =
            usize_to_i32(MAX_BUFFER_SIZE / i32_to_usize(state_vector_size_per_shot));
        // How many of the structures would fit
        let max_shots_in_buffer = min(MAX_SHOT_ENTRIES, max_shot_state_vectors);
        // How many would we allow based on the max shots per batch
        let max_shots_per_batch = min(max_shots_in_buffer, MAX_SHOTS_PER_BATCH);

        // So with that... how many shots fit in a batch, and how many batches do we need
        params.shots_per_batch = min(shot_count, max_shots_per_batch);
        params.batch_count = (shot_count - 1) / params.shots_per_batch + 1;

        // Now figure out how to partition processing into GPU workgroups and threads
        params.workgroups_per_shot = if params.qubit_count <= MAX_QUBITS_PER_WORKGROUP {
            1
        } else {
            1 << (params.qubit_count - MAX_QUBITS_PER_WORKGROUP)
        };
        params.entries_per_thread =
            entries_per_shot / params.workgroups_per_shot / THREADS_PER_WORKGROUP;

        // Figure out the buffer sizes we need for all the above
        params.shots_buffer_size = i32_to_usize(params.shots_per_batch) * SIZEOF_SHOTDATA;
        params.state_vector_buffer_size =
            i32_to_usize(params.shots_per_batch * state_vector_size_per_shot);

        // Each result is a u32, plus one extra on the end for the shader to set an 'error code' if needed
        params.results_buffer_size = i32_to_usize(params.shots_per_batch * (params.result_count + 1)) // +1 for error code per shot
                * std::mem::size_of::<u32>();

        params.diagnostics_buffer_size = 4 * 4 /* initial bytes for error codes and ad-hoc data */
                + SIZEOF_SHOTDATA + std::mem::size_of::<Op>() // ShotData + Op
                + (THREADS_PER_WORKGROUP * 8 * MAX_QUBIT_COUNT) as usize // Workgroup probabilities
                + std::mem::size_of::<WorkgroupCollationBuffer>(); // Collation buffers

        // Finally, the download buffer needs to hold both results and diagnostics
        params.download_buffer_size = params.results_buffer_size + params.diagnostics_buffer_size;
    }
}

fn add_noise_config_to_ops(ops: &[Op], noise: &NoiseConfig) -> Vec<Op> {
    let mut noisy_ops: Vec<Op> = Vec::with_capacity(ops.len() + 1);

    for op in ops {
        let mut add_ops: Vec<Op> = vec![*op];
        // If there's a NoiseConfig, and we get noise for this op, append it
        if let Some(noise_ops) = get_noise_ops(op, noise) {
            add_ops.extend(noise_ops);
        }
        // If it's an MResetZ with noise, change to an Id with noise, followed by MResetZ
        // (This is just simpler to implement than doing noise inline with MResetZ for now)
        if op.id == ops::MRESETZ && add_ops.len() > 1 {
            let mz_copy = add_ops[0];
            add_ops[0] = Op::new_id_gate(op.q1);
            add_ops.push(mz_copy);
        }
        // Convert 'mov' ops to identity, and don't add the ops if it's just a
        // single identity (but do add if it has noise)
        if add_ops[0].id == ops::MOVE {
            add_ops[0].id = ops::ID;
        }
        if add_ops.len() == 1 && add_ops[0].id == ops::ID {
            // skip lone identity gates
        } else {
            noisy_ops.extend(add_ops);
        }
    }

    noisy_ops
}

// Helpers to ease conversion boilerplate where it should always succeed
fn usize_to_i32(value: usize) -> i32 {
    i32::try_from(value).expect("Value {value} can't convert to i32")
}

fn i32_to_usize(value: i32) -> usize {
    usize::try_from(value).expect("Value {value} can't convert to usize")
}
