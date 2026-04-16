// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cmp::min;
use std::mem::size_of;

use bytemuck::{Zeroable, cast_slice};

use crate::bytecode::AdaptiveProgram;
use crate::correlated_noise::NoiseTables;
use crate::gpu_resources::GpuResources;
use crate::noise_config::NoiseConfig;
use crate::noise_mapping::get_noise_ops;
use crate::shader_types::{
    DiagnosticsData, InterpreterState, MAX_BUFFER_SIZE, MAX_QUBIT_COUNT, MAX_QUBITS_PER_WORKGROUP,
    MAX_REGISTERS, MAX_SHOT_ENTRIES, MAX_SHOTS_PER_BATCH, MIN_QUBIT_COUNT, MIN_REGISTERS, Op,
    SIZEOF_SHOTDATA, THREADS_PER_WORKGROUP, Uniforms, WorkgroupCollationBuffer, ops,
};

// On Windows, running larger circuits/shots can hit TDR issues if too many ops are dispatched in one go.
const DEFAULT_MAX_OPS_PER_DISPATCH: i32 = 16;

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
pub struct GpuContext {
    resources: GpuResources,

    program: Vec<Op>,
    program_with_noise: Option<Vec<Op>>,
    noise_config: Option<NoiseConfig<f32, f64>>,
    noise_tables: NoiseTables,

    run_params: RunParams,

    // Adaptive program data (set via set_adaptive_program)
    adaptive_program: Option<AdaptiveProgram>,

    // Indicates if items impacting the Ops have changed and need to be re-uploaded / recompiled
    program_is_dirty: bool,
    // Indicates if the pipeline needs to be recompiled due to changes (i.e., qubit or result count)
    pipeline_is_dirty: bool,
    // Indicates if the noise config has changed (which means the program may be dirty too)
    noise_config_is_dirty: bool,
    // Indicates if the noise tables or adaptive program have changed and need to be re-uploaded to the GPU
    batch_data_is_dirty: bool,
    // Indicates if the context is for an adaptive program.
    is_adaptive: bool,
}

#[derive(Debug, Default)]
pub(crate) struct RunParams {
    pub qubit_count: i32,
    pub result_count: i32,
    pub shot_count: i32,
    pub shots_per_batch: i32,
    pub batch_count: i32,
    pub workgroups_per_shot: i32,
    pub entries_per_thread: i32,
    pub shots_buffer_size: usize,
    pub state_vector_buffer_size: usize,
    pub results_buffer_size: usize,
    pub diagnostics_buffer_size: usize,
    pub download_buffer_size: usize,

    // Adaptive program parameters.
    pub num_registers: usize,
    pub num_instructions: usize,
    pub num_blocks: usize,
    pub num_functions: usize,
    pub num_phi_entries: usize,
    pub num_switch_cases: usize,
    pub num_call_args: usize,
    // Noise table sizes for BatchData (minimum 1, since WGSL arrays must have length ≥ 1).
    pub noise_table_count: usize,
    pub noise_entry_count: usize,
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

impl Default for GpuContext {
    fn default() -> Self {
        Self {
            // Start dirty so the batch_data buffer gets uploaded on first run,
            // even when no noise is configured (shader always needs the buffer).
            batch_data_is_dirty: true,
            resources: GpuResources::default(),
            program: Vec::new(),
            program_with_noise: None,
            noise_config: None,
            noise_tables: NoiseTables::default(),
            run_params: RunParams::default(),
            adaptive_program: None,
            program_is_dirty: false,
            pipeline_is_dirty: false,
            noise_config_is_dirty: false,
            is_adaptive: false,
        }
    }
}

impl GpuContext {
    // See if we can get a GPU adapter on this machine (useful before trying to run tests)
    // Note: This does NOT allocate GPU resources that persist across runs. Run shots for that.
    pub async fn try_create_adapter() -> Result<String, String> {
        Ok(format!(
            "{:?}",
            GpuResources::try_get_adapter().await?.get_info()
        ))
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
            || self.adaptive_program.is_some()
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
    pub fn set_noise_config(&mut self, noise: NoiseConfig<f32, f64>) {
        self.add_correlated_noise_table_from_noise_config(&noise);
        self.noise_config = Some(noise);
        self.noise_config_is_dirty = true;
    }

    /// Add a correlated noise table to the GPU context (will overwrite existing with same name)
    pub fn add_correlated_noise_table(&mut self, name: &str, contents: &str) {
        self.noise_tables.add(name, contents);
        self.batch_data_is_dirty = true;
    }

    /// Add a correlated noise table to the GPU context (will overwrite existing with same name)
    pub fn add_correlated_noise_table_from_noise_config(
        &mut self,
        noise_config: &NoiseConfig<f32, f64>,
    ) {
        self.noise_tables.load_from_noise_config(noise_config);
        self.batch_data_is_dirty = true;
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
        self.batch_data_is_dirty = true;
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
                        | ops::CX..=ops::RZZ
                        | ops::CY
                        | ops::SWAP
                        | ops::CORRELATED_NOISE => {
                            compute_pass.set_pipeline(&kernels.prepare_op);
                            compute_pass.dispatch_workgroups(prepare_workgroup_count, 1, 1);

                            compute_pass.set_pipeline(&kernels.execute_op);
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
                diagnostics = Some(batch_diagnostics);
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
        self.resources.ensure_device(dbg_capture, false).await?;

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

        if self.batch_data_is_dirty {
            // Track noise table sizes. If they changed, the shader templates need to be
            // recompiled (NOISE_TABLE_COUNT / NOISE_ENTRY_COUNT are compile-time constants).
            let new_table_count = self.noise_tables.metadata.len().max(1);
            let new_entry_count = self.noise_tables.entries.len().max(1);
            if new_table_count != self.run_params.noise_table_count
                || new_entry_count != self.run_params.noise_entry_count
            {
                self.run_params.noise_table_count = new_table_count;
                self.run_params.noise_entry_count = new_entry_count;
                self.pipeline_is_dirty = true;
            }

            // Upload combined noise batch_data buffer
            let noise_metadata_bytes: &[u8] = cast_slice(&self.noise_tables.metadata);
            let noise_entries_bytes: &[u8] = cast_slice(&self.noise_tables.entries);
            let noise_table_padded_size = self.run_params.noise_table_count * 16;
            let noise_entry_padded_size = self.run_params.noise_entry_count * 16;
            let total_size = noise_table_padded_size + noise_entry_padded_size;
            let mut batch_buf = vec![0u8; total_size];
            batch_buf[..noise_metadata_bytes.len()].copy_from_slice(noise_metadata_bytes);
            batch_buf[noise_table_padded_size..noise_table_padded_size + noise_entries_bytes.len()]
                .copy_from_slice(noise_entries_bytes);
            self.resources.upload_batch_data(&batch_buf)?;
            self.batch_data_is_dirty = false;
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
                // These only change if the size of the noise table changes
                params.noise_table_count,
                params.noise_entry_count,
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
        // In adaptive mode, each shot's GPU struct includes the interpreter state + registers.
        // WGSL requires struct size to be a multiple of the struct's alignment (8 bytes due to vec2f).
        let gpu_shot_size = if self.is_adaptive {
            let raw = SIZEOF_SHOTDATA + size_of::<InterpreterState>() + params.num_registers * 4;
            (raw + 7) & !7 // round up to 8-byte alignment
        } else {
            SIZEOF_SHOTDATA
        };
        params.shots_buffer_size = i32_to_usize(params.shots_per_batch) * gpu_shot_size;
        params.state_vector_buffer_size =
            i32_to_usize(params.shots_per_batch * state_vector_size_per_shot);

        // Each result is a u32, plus one extra on the end for the shader to set an 'error code' if needed
        params.results_buffer_size = i32_to_usize(params.shots_per_batch * (params.result_count + 1)) // +1 for error code per shot
                * size_of::<u32>();

        params.diagnostics_buffer_size = 6 * 4 /* error_code, termination_count, extra1, extra2, extra3, padding */
                + gpu_shot_size + size_of::<Op>() // ShotData (GPU-side, may include interp+registers) + Op
                + (THREADS_PER_WORKGROUP * 8 * MAX_QUBIT_COUNT) as usize // Workgroup probabilities
                + size_of::<WorkgroupCollationBuffer>(); // Collation buffers

        // Finally, the download buffer needs to hold both results and diagnostics
        params.download_buffer_size = params.results_buffer_size + params.diagnostics_buffer_size;
    }

    pub fn switch_to_base(&mut self) {
        if self.is_adaptive {
            // Preserve noise tables loaded before the mode switch
            let noise_tables = std::mem::take(&mut self.noise_tables);
            let batch_data_is_dirty = self.batch_data_is_dirty;
            *self = Self::default();
            self.noise_tables = noise_tables;
            self.batch_data_is_dirty = batch_data_is_dirty;
        }
    }
}

// Adaptive Profile implementations.
impl GpuContext {
    #[must_use]
    pub fn adaptive() -> Self {
        Self {
            resources: GpuResources::adaptive(),
            is_adaptive: true,
            // Start dirty so the batch_data buffer gets uploaded on first run,
            // even when no noise is configured (shader always needs the buffer).
            batch_data_is_dirty: true,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn is_adaptive(&self) -> bool {
        self.is_adaptive
    }

    pub fn swith_to_adaptive(&mut self) {
        if !self.is_adaptive {
            // Preserve noise tables loaded before the mode switch
            let noise_tables = std::mem::take(&mut self.noise_tables);
            let batch_data_is_dirty = self.batch_data_is_dirty;
            *self = Self::adaptive();
            self.noise_tables = noise_tables;
            self.batch_data_is_dirty = batch_data_is_dirty;
        }
    }

    pub fn set_adaptive_program(&mut self, program: AdaptiveProgram) -> Result<(), String> {
        self.program.clear();
        let num_qubits = u32_to_i32(program.num_qubits);

        // Always allocate a minumum number of qubits to ensure good data alignment, GPU thread usage, etc.
        let qubit_count = num_qubits.max(MIN_QUBIT_COUNT);
        let result_count = u32_to_i32(program.num_results);

        // Dynamic register file sizing: use at least MIN_REGISTERS to avoid
        // tiny buffers, but grow to match the program's actual requirement.
        let num_registers = (program.num_registers as usize).max(MIN_REGISTERS);

        if num_registers > MAX_REGISTERS {
            return Err(format!(
                "MAX_REGISTERS for adaptive program exceeded: {num_registers} > {MAX_REGISTERS}"
            ));
        }

        if self.adaptive_program.is_none()
            || qubit_count != self.run_params.qubit_count
            || self.run_params.result_count != result_count
            || self.run_params.num_registers != num_registers
            || self.run_params.num_instructions != program.instructions.len()
            || self.run_params.num_blocks != program.block_table.len()
            || self.run_params.num_functions != program.function_table.len()
            || self.run_params.num_phi_entries != program.phi_entries.len()
            || self.run_params.num_switch_cases != program.switch_cases.len()
            || self.run_params.num_call_args != program.call_args.len()
        {
            self.pipeline_is_dirty = true;
        }

        self.run_params.qubit_count = qubit_count;
        self.run_params.result_count = result_count;
        self.run_params.num_registers = num_registers;
        self.run_params.num_instructions = program.instructions.len();
        self.run_params.num_blocks = program.block_table.len();
        self.run_params.num_functions = program.function_table.len();
        self.run_params.num_phi_entries = program.phi_entries.len();
        self.run_params.num_switch_cases = program.switch_cases.len();
        self.run_params.num_call_args = program.call_args.len();

        self.adaptive_program = Some(program);
        self.program_is_dirty = true;
        Ok(())
    }

    /// Upload the combined batch data buffer (noise tables + noise entries + program bytes)
    /// to GPU binding 7.
    fn upload_batch_data(&mut self) -> Result<(), String> {
        let program = self
            .adaptive_program
            .as_ref()
            .ok_or("No adaptive program has been set")?;

        let noise_metadata_bytes: &[u8] = cast_slice(&self.noise_tables.metadata);
        let noise_entries_bytes: &[u8] = cast_slice(&self.noise_tables.entries);

        // Pad noise arrays to the template sizes (max(count, 1) * element_size)
        // so that the GPU struct layout matches the fixed-size WGSL arrays.
        let noise_table_padded_size = self.run_params.noise_table_count * 16; // NoiseTableMetadata is 16 bytes
        let noise_entry_padded_size = self.run_params.noise_entry_count * 16; // NoiseTableEntry is 16 bytes

        let program_bytes: Vec<u8> = [
            cast_slice(&program.instructions),
            cast_slice(&program.block_table),
            cast_slice(&program.function_table),
            cast_slice(&program.phi_entries),
            cast_slice(&program.switch_cases),
            cast_slice(&program.call_args),
        ]
        .concat();

        // Build the combined batch_data buffer: [noise_tables | noise_entries | program]
        let total_size = noise_table_padded_size + noise_entry_padded_size + program_bytes.len();
        let mut batch_data = vec![0u8; total_size];

        batch_data[..noise_metadata_bytes.len()].copy_from_slice(noise_metadata_bytes);
        let entries_offset = noise_table_padded_size;
        batch_data[entries_offset..entries_offset + noise_entries_bytes.len()]
            .copy_from_slice(noise_entries_bytes);
        let program_offset = entries_offset + noise_entry_padded_size;
        batch_data[program_offset..program_offset + program_bytes.len()]
            .copy_from_slice(&program_bytes);

        self.resources.upload_batch_data(&batch_data)?;
        self.resources
            .upload_ops_data(cast_slice(&program.quantum_ops))?;

        Ok(())
    }

    /// Prepares the GPU for running multiple instances of adaptive simulations in parallel.
    async fn ready_gpu_resources_adaptive(&mut self) -> Result<(), String> {
        // Ensure the device is initialized
        let dbg_capture = std::env::var("QDK_GPU_CAPTURE").is_ok();
        self.resources.ensure_device(dbg_capture, true).await?;

        // Track noise table sizes. If they changed, the shader templates need to be
        // recompiled (NOISE_TABLE_COUNT / NOISE_ENTRY_COUNT are compile-time constants).
        if self.batch_data_is_dirty {
            let new_table_count = self.noise_tables.metadata.len().max(1);
            let new_entry_count = self.noise_tables.entries.len().max(1);
            if new_table_count != self.run_params.noise_table_count
                || new_entry_count != self.run_params.noise_entry_count
            {
                self.run_params.noise_table_count = new_table_count;
                self.run_params.noise_entry_count = new_entry_count;
                self.pipeline_is_dirty = true;
            }
        }

        if self.pipeline_is_dirty {
            let params = &self.run_params;
            self.resources.create_shaders_adaptive(params)?;
        }

        if self.program_is_dirty || self.noise_config_is_dirty || self.batch_data_is_dirty {
            // Rebuild the quantum ops pool (with noise if needed) and upload to GPU
            if self.noise_config_is_dirty
                && let Some(noise) = &self.noise_config
            {
                let program = self
                    .adaptive_program
                    .as_mut()
                    .ok_or("No adaptive program has been set")?;
                let (noisy_ops, index_map) = add_noise_to_adaptive_ops(&program.quantum_ops, noise);
                // Patch bytecode instructions that reference quantum op indices.
                // OP_QUANTUM_GATE (0x10), OP_MEASURE (0x11), OP_RESET (0x12)
                // all store the op pool index in `aux0`.
                for instr in &mut program.instructions {
                    let primary = instr.opcode & 0xFF;
                    if primary == 0x10 || primary == 0x11 || primary == 0x12 {
                        instr.aux0 = index_map[instr.aux0 as usize];
                    }
                }
                program.quantum_ops = noisy_ops;
            }
            // Upload the combined batch_data buffer (noise + program) to binding 7
            self.upload_batch_data()?;
            self.program_is_dirty = false;
            self.noise_config_is_dirty = false;
            self.batch_data_is_dirty = false;
        }

        let params = &self.run_params;

        self.resources.ensure_run_buffers(
            params.shots_buffer_size,
            params.state_vector_buffer_size,
            params.results_buffer_size,
            params.diagnostics_buffer_size,
        )?;

        Ok(())
    }

    /// Run the adaptive program for the given number of shots (blocking)
    pub fn run_adaptive_shots_sync(
        &mut self,
        shot_count: i32,
        seed: u32,
        start_shot_id: i32,
    ) -> Result<RunResults, String> {
        futures::executor::block_on(self.run_adaptive_shots(shot_count, seed, start_shot_id))
    }

    #[allow(clippy::too_many_lines)]
    pub async fn run_adaptive_shots(
        &mut self,
        shot_count: i32,
        seed: u32,
        start_shot_id: i32,
    ) -> Result<RunResults, String> {
        const MAX_ROUNDS_PER_BATCH: u32 = 100_000;

        // 1. Update run params.
        self.update_run_params(shot_count);

        // 2. Prepare GPU resources.
        self.ready_gpu_resources_adaptive().await?;

        // Get the GPU resources we need for the entire run
        let bind_group = self.resources.get_bind_group_adaptive()?;

        let mut results: Vec<u32> = Vec::new();
        let mut diagnostics: Option<Box<DiagnosticsData>> = None;
        let mut shots_remaining = self.run_params.shot_count;

        let program = self
            .adaptive_program
            .as_ref()
            .ok_or("No adaptive program has been set")?;

        let entry_block = program.entry_block;
        // Entry instruction offset. PC stands for Program Counter, aka Instruction Pointer.
        let entry_pc = program.block_table[entry_block as usize].instr_offset;

        // 3. For each batch:
        for batch_idx in 0..self.run_params.batch_count {
            // 3.1 Schedule batch of shots.
            let shots_this_batch = min(shots_remaining, self.run_params.shots_per_batch);
            let shots_usize = i32_to_usize(shots_this_batch);
            let execute_workgroup_count =
                u32::try_from(self.run_params.workgroups_per_shot * shots_this_batch)
                    .expect("workgroups_per_shot * shots_per_batch should fit in u32");
            let shots_this_batch: u32 = shots_this_batch
                .try_into()
                .expect("the number of shots in this batch should fit in a u32");
            shots_remaining -= self.run_params.shots_per_batch;

            // Update the uniforms for this batch
            // When this is put directly on the queue, it will be submitted when the next submit occurs, but will run before that submit's work
            self.resources.upload_uniform(&Uniforms {
                batch_start_shot_id: batch_idx * self.run_params.shots_per_batch + start_shot_id,
                rng_seed: seed,
            })?;

            // Zero the diagnostics header (error_code + termination_count) for this batch
            self.resources.reset_diagnostics_header()?;

            // Initialize state vectors and shot data via the init kernel.
            // The init kernel zeros and configures the base ShotData fields per shot.
            {
                let kernels = self.resources.get_kernels()?;
                let mut encoder = self.resources.get_encoder("Adaptive Init Encoder")?;
                let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Adaptive Init Pass"),
                    timestamp_writes: None,
                });
                compute_pass.set_bind_group(0, &bind_group, &[]);
                compute_pass.set_pipeline(&kernels.init_op);
                compute_pass.dispatch_workgroups(execute_workgroup_count, 1, 1);
                drop(compute_pass);
                self.resources.submit_command_buffer(encoder.finish())?;
            }

            // Upload interpreter state + registers into the shots buffer.
            // Each shot's interp data starts at SIZEOF_SHOTDATA offset within the shot region.
            // The init kernel already initialized the base ShotData fields, so we only
            // write the interp portion per shot to avoid overwriting them.
            {
                let num_registers = self.run_params.num_registers;
                let interp_bytes_per_shot = size_of::<InterpreterState>() + num_registers * 4;
                let gpu_shot_size = (SIZEOF_SHOTDATA + interp_bytes_per_shot + 7) & !7;

                let mut interp = InterpreterState::zeroed();
                interp.pc = entry_pc;
                interp.current_block_id = entry_block;
                let interp_bytes =
                    cast_slice::<InterpreterState, u8>(std::slice::from_ref(&interp));

                // Build per-shot data: [InterpreterState bytes | zeroed registers]
                let mut shot_interp_data = vec![0u8; interp_bytes_per_shot];
                shot_interp_data[..interp_bytes.len()].copy_from_slice(interp_bytes);

                for i in 0..shots_usize {
                    let offset = i * gpu_shot_size + SIZEOF_SHOTDATA;
                    self.resources
                        .upload_interpreter_data_to_shots(&shot_interp_data, offset)?;
                }
            }

            // 3.2 Wait until the batch is finished.
            //     We try to resume the computation `ROUNDS_PER_BATCH` times.
            let kernels = self.resources.get_kernels()?;
            for _ in 0..MAX_ROUNDS_PER_BATCH {
                let mut encoder = self.resources.get_encoder("Adaptive Batch Encoder")?;

                // Dispatch a round of computation.
                // Phase 1: interpret classical bytecode (1 thread per shot)
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("Adaptive Classical Pass"),
                        timestamp_writes: None,
                    });
                    pass.set_bind_group(0, &bind_group, &[]);
                    pass.set_pipeline(&kernels.interpret_classical);
                    pass.dispatch_workgroups(shots_this_batch, 1, 1);
                }

                // Phase 2: prepare the pending quantum op per shot
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("Adaptive Prepare Pass"),
                        timestamp_writes: None,
                    });
                    pass.set_bind_group(0, &bind_group, &[]);
                    pass.set_pipeline(&kernels.prepare_op);
                    pass.dispatch_workgroups(shots_this_batch, 1, 1);
                }

                // Phase 3: execute the quantum op (parallel state-vector kernel)
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("Adaptive Execute Pass"),
                        timestamp_writes: None,
                    });
                    pass.set_bind_group(0, &bind_group, &[]);
                    pass.set_pipeline(&kernels.execute_op);
                    pass.dispatch_workgroups(execute_workgroup_count, 1, 1);
                }

                // Check for termination.
                let terminated = self.resources.download_termination_count().await?;
                if terminated >= shots_this_batch {
                    break;
                }

                self.resources.submit_command_buffer(encoder.finish())?;
            }

            // 3.3 Aggregate results.
            let (batch_results, batch_diagnostics) =
                self.resources.download_batch_results().await?;

            results.extend(batch_results);
            if batch_diagnostics.error_code != 0 && diagnostics.is_none() {
                diagnostics = Some(batch_diagnostics);
            }
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
}

fn add_noise_config_to_ops(ops: &[Op], noise: &NoiseConfig<f32, f64>) -> Vec<Op> {
    let mut noisy_ops: Vec<Op> = Vec::with_capacity(ops.len() + 1);

    for op in ops {
        let mut add_ops: Vec<Op> = vec![*op];
        // If there's a NoiseConfig, and we get noise for this op, append it
        if let Some(noise_ops) = get_noise_ops(op, noise) {
            add_ops.extend(noise_ops);
        }
        // If it's an MResetZ, MZ, or ResetZ with noise, change to an Id with noise, followed by the original op
        // (This is just simpler to implement than doing noise inline with measure/reset for now)
        if (op.id == ops::MRESETZ || op.id == ops::MZ || op.id == ops::RESETZ) && add_ops.len() > 1
        {
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

/// Expand the adaptive quantum op pool with noise ops.
///
/// For each original op, this emits the op itself and then any Pauli/loss
/// noise ops from the `NoiseConfig`.  Unlike `add_noise_config_to_ops` used
/// by the non-adaptive path, this does *not* reorder measure/reset ops or
/// drop identity gates, because the adaptive bytecode interpreter references
/// each op by index and handles measure/reset at the instruction level.
///
/// Returns `(expanded_ops, index_map)` where `index_map[old_idx]` gives
/// the new index of the original op in the expanded pool.
fn add_noise_to_adaptive_ops(
    ops_pool: &[Op],
    noise: &NoiseConfig<f32, f64>,
) -> (Vec<Op>, Vec<u32>) {
    let mut noisy_ops: Vec<Op> = Vec::with_capacity(ops_pool.len() * 2);
    let mut index_map: Vec<u32> = Vec::with_capacity(ops_pool.len());

    for op in ops_pool {
        // Record the new index for this original op
        #[allow(clippy::cast_possible_truncation)]
        let new_idx = noisy_ops.len() as u32;
        index_map.push(new_idx);

        noisy_ops.push(*op);

        // Append any noise ops (pauli + loss) from the config
        if let Some(noise_ops) = get_noise_ops(op, noise) {
            noisy_ops.extend(noise_ops);
        }
    }

    (noisy_ops, index_map)
}

// Helpers to ease conversion boilerplate where it should always succeed
fn usize_to_i32(value: usize) -> i32 {
    i32::try_from(value).expect("Value {value} can't convert to i32")
}

fn i32_to_usize(value: i32) -> usize {
    usize::try_from(value).expect("Value {value} can't convert to usize")
}

fn u32_to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or_else(|_| panic!("{value} should fit in a i32"))
}
