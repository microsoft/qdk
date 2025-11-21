// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(unused)]

use crate::shader_types;

use super::shader_types::{Op, Result, ops};

use bytemuck::{Pod, Zeroable};
use futures::FutureExt;
use std::{cmp::max, cmp::min, fmt::Write as _, num::NonZeroU64};
use wgpu::{
    Adapter, BindGroup, BindGroupLayout, Buffer, BufferDescriptor, BufferUsages, ComputePipeline,
    Device, Limits, Queue, RequestAdapterError, RequestAdapterOptions, ShaderModule,
};

// Some of these values are to align with WebGPU default limits
// See https://gpuweb.github.io/gpuweb/#limits
const MAX_BUFFER_SIZE: usize = 1 << 30; // 1 GB limit due to some wgpu restrictions
const MAX_QUBIT_COUNT: u32 = 27; // 2^27 * 8 bytes per complex32 = 1 GB buffer limit
const MAX_QUBITS_PER_WORKGROUP: u32 = 18; // Max qubits to be processed by a single workgroup
const THREADS_PER_WORKGROUP: u32 = 32; // 32 gives good occupancy across various GPUs

// Once a shot is big enough to need multiple workgroups, what's the max number of workgroups possible
const MAX_PARTITIONED_WORKGROUPS: usize = 1 << (MAX_QUBIT_COUNT - MAX_QUBITS_PER_WORKGROUP);
const MAX_SHOTS_PER_BATCH: u32 = 65535; // To align with max workgroups per dimension WebGPU default

// Round up circuit qubits if smaller to enable to optimizations re unrolling, etc.
// With min qubit count of 8, this means min 256 entries per shot. Spread across 32 threads = 8 entries per thread.
// With each iteration in each thread processing 2 or 4 entries, that means 2 or 4 iterations per thread minimum.
const MIN_QUBIT_COUNT: u32 = 8;
const MAX_CIRCUIT_OPS: usize = MAX_BUFFER_SIZE / std::mem::size_of::<Op>();
const SIZEOF_SHOTDATA: usize = 640; // Size of ShotData struct on the GPU in bytes
const MAX_SHOT_ENTRIES: usize = MAX_BUFFER_SIZE / SIZEOF_SHOTDATA;

// There is no hard limit here, but for very large circuits we may need to split into multiple dispatches.
// TODO: See if there is a way to query the GPU for max dispatches per submit, or derive it from other limits
const MAX_DISPATCHES_PER_SUBMIT: u32 = 100_000;

const ADAPTER_OPTIONS: RequestAdapterOptions = wgpu::RequestAdapterOptions {
    power_preference: wgpu::PowerPreference::HighPerformance,
    compatible_surface: None,
    force_fallback_adapter: false,
};

pub struct GpuContext {
    device: Device,
    queue: Queue,
    shader_module: ShaderModule,
    bind_group_layout: BindGroupLayout,
    ops: Vec<Op>,
    qubit_count: u32,
    rng_seed: u32,
    resources: Option<GpuResources>,
    run_params: RunParams,
    dbg_capture: bool,
}

struct GpuResources {
    pipeline_init_op: ComputePipeline,
    pipeline_prepare_op: ComputePipeline,
    pipeline_execute_op: ComputePipeline,
    pipeline_execute_2q_op: ComputePipeline,
    pipeline_execute_mz: ComputePipeline,
    bind_group: BindGroup,
    buffers: GpuBuffers,
}

struct GpuBuffers {
    uniform_buffer: Buffer,
    workgroup_collation: Buffer,
    shot_state: Buffer,
    state_vector: Buffer,
    ops_upload: Buffer,
    ops: Buffer,
    results: Buffer,
    diagnostics: Buffer,
    download: Buffer,
}

struct RunParams {
    shots_buffer_size: usize,
    ops_buffer_size: usize,
    state_vector_buffer_size: usize,
    results_buffer_size: usize,
    diagnostics_buffer_size: usize,
    download_buffer_size: usize,
    entries_per_shot: usize,
    entries_per_workgroup: usize,
    entries_per_thread: usize,
    batch_count: usize,
    shots_per_batch: usize,
    shots_count: usize,
    workgroups_per_shot: usize,
    op_count: usize,
    gate_op_count: usize,
    result_count: usize,
}

// ********* The below structure should be kept in sync with the WGSL shader code *********

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Uniforms {
    batch_start_shot_id: u32,
    rng_seed: u32,
}

// The follow data is copied back from the GPU for diagnostics
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct QubitProbabilities {
    zero: f32,
    one: f32,
}

// Each workgroup sums the probabilities for the entries it processed for each qubit
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct WorkgroupSums {
    qubits: [QubitProbabilities; MAX_QUBIT_COUNT as usize],
}

// Once the dispatch for the workgroup processing is done, the results from all workgroups
// for all active shots are collated here for final processing in the next prepare_op step.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct WorkgroupCollationBuffer {
    sums: [WorkgroupSums; MAX_PARTITIONED_WORKGROUPS],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct QubitProbabilityPerThread {
    zero: [f32; MAX_QUBIT_COUNT as usize],
    one: [f32; MAX_QUBIT_COUNT as usize],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct QubitState {
    zero_probability: f32,
    one_probability: f32,
    heat: f32, // -1.0 = lost
    idle_since: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct ShotData {
    shot_id: u32,
    next_op_idx: u32,
    rng_state: [u32; 6], // 6 x u32
    rand_pauli: f32,
    rand_damping: f32,
    rand_dephase: f32,
    rand_measure: f32,
    rand_loss: f32,
    op_type: u32,
    op_idx: u32,
    duration: f32,
    renormalize: f32,
    qubit_is_0_mask: u32,
    qubit_is_1_mask: u32,
    qubits_updated_last_op_mask: u32,
    qubit_state: [QubitState; MAX_QUBIT_COUNT as usize],
    unitary: [f32; 32],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct DiagnosticsData {
    error_code: u32,
    extra1: u32,
    extra2: f32,
    extra3: f32,
    shot: ShotData,
    op: Op,
    qubit_probabilities: [QubitProbabilityPerThread; THREADS_PER_WORKGROUP as usize],
    collation_buffer: WorkgroupCollationBuffer,
}
// TODO: Implement the Display trait for DiagnosticsData for easier debugging output

impl GpuContext {
    pub async fn get_adapter() -> std::result::Result<Adapter, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        // Get high-performance adapter
        let adapter = instance
            .request_adapter(&ADAPTER_OPTIONS)
            .await
            .map_err(|e| e.to_string())?;

        // Validate adapter fits our needs
        Self::validate_adapter_capabilities(&adapter)?;
        Ok(adapter)
    }

    fn validate_adapter_capabilities(adapter: &Adapter) -> std::result::Result<(), String> {
        let downlevel_capabilities = adapter.get_downlevel_capabilities();
        if !downlevel_capabilities
            .flags
            .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
        {
            return Err("Adapter does not support compute shaders".to_string());
        }
        Ok(())
    }

    fn get_required_limits(
        adapter: &Adapter,
        run_params: &RunParams,
    ) -> std::result::Result<wgpu::Limits, String> {
        let adapter_limits = adapter.limits();
        let max_storage_buffer_size = adapter_limits.max_storage_buffer_binding_size as usize;

        let max_required_buffer_size: usize = run_params
            .shots_buffer_size
            .max(run_params.state_vector_buffer_size)
            .max(run_params.ops_buffer_size)
            .max(run_params.results_buffer_size)
            .max(run_params.diagnostics_buffer_size)
            .max(std::mem::size_of::<WorkgroupCollationBuffer>());

        if max_required_buffer_size > max_storage_buffer_size {
            return Err(format!(
                "Required buffer size of {max_required_buffer_size} exceeds maximum GPU \
                buffer size of {max_storage_buffer_size}",
            ));
        }

        let required_limits = wgpu::Limits {
            max_storage_buffer_binding_size: u32::try_from(max_required_buffer_size)
                .expect("MAX_BUFFER_SIZE should fit in u32"),
            ..adapter_limits
        };

        Ok(required_limits)
    }

    pub async fn new(
        qubit_count: u32,
        result_count: u32,
        ops: Vec<Op>,
        shots: u32,
        rng_seed: u32,
        dbg_capture: bool,
    ) -> std::result::Result<Self, String> {
        // Validate the range of inputs
        if ops.is_empty() {
            return Err("Circuit must have at least one operation".to_string());
        }
        if ops.len() > MAX_CIRCUIT_OPS {
            return Err(format!(
                "Operation count {} exceeds maximum supported operation count of {}",
                ops.len(),
                MAX_CIRCUIT_OPS
            ));
        }
        // gate_op_count should filter out any ops with an ID >= 128 and < 256 (noise ops)
        // These get combined and executed with the prior op at run time, so don't count towards dispatches
        let gate_op_count: u32 = ops
            .iter()
            .filter(|op| (op.id) < 128)
            .count()
            .try_into()
            .map_err(|_| "Too many operations")?;
        let op_count: u32 = ops.len().try_into().map_err(|_| "Too many operations")?;

        let qubit_count = qubit_count.max(MIN_QUBIT_COUNT);
        if qubit_count > MAX_QUBIT_COUNT {
            return Err(format!(
                "Qubit count {qubit_count} exceeds maximum supported qubit count of {MAX_QUBIT_COUNT}"
            ));
        }

        // Add space in the results for an 'error code' per shot
        let run_params: RunParams =
            Self::get_params(qubit_count, result_count, op_count, gate_op_count, shots)?;

        let adapter = Self::get_adapter().await?;

        let (device, queue): (Device, Queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                // Note that mappable primary buffers are a native-only feature, but using here to
                // workaround the Xcode bug described in https://github.com/gfx-rs/wgpu/issues/8111.
                // This will need to be revisited for web support.
                required_features: wgpu::Features::MAPPABLE_PRIMARY_BUFFERS,
                required_limits: Self::get_required_limits(&adapter, &run_params)?,
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| e.to_string())?;

        if dbg_capture {
            unsafe {
                device.start_graphics_debugger_capture();
            }
        }

        // Create the shader module and bind group layout
        let raw_shader_src = include_str!("simulator.wgsl");
        let shader_src = raw_shader_src
            .replace("{{QUBIT_COUNT}}", &qubit_count.to_string())
            .replace("{{RESULT_COUNT}}", &run_params.result_count.to_string())
            .replace(
                "{{WORKGROUPS_PER_SHOT}}",
                &run_params.workgroups_per_shot.to_string(),
            )
            .replace(
                "{{ENTRIES_PER_THREAD}}",
                &run_params.entries_per_thread.to_string(),
            )
            .replace(
                "{{THREADS_PER_WORKGROUP}}",
                &THREADS_PER_WORKGROUP.to_string(),
            )
            .replace("{{MAX_QUBIT_COUNT}}", &MAX_QUBIT_COUNT.to_string())
            .replace(
                "{{MAX_QUBITS_PER_WORKGROUP}}",
                &MAX_QUBITS_PER_WORKGROUP.to_string(),
            );

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Simulator Shader Module"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        let bind_group_layout = create_bind_group_layout(&device);

        Ok(GpuContext {
            device,
            queue,
            shader_module,
            bind_group_layout,
            resources: None,
            ops,
            qubit_count,
            rng_seed,
            run_params,
            dbg_capture,
        })
    }

    fn get_params(
        qubit_count: u32,
        result_count: u32,
        op_count: u32,
        gate_op_count: u32,
        shot_count: u32,
    ) -> std::result::Result<RunParams, String> {
        let state_vector_entry_size = std::mem::size_of::<f32>() * 2; // complex f32
        let op_size = std::mem::size_of::<Op>();

        let entries_per_shot: usize = 1 << qubit_count;
        let state_vector_size_per_shot = entries_per_shot * state_vector_entry_size;
        let max_shot_state_vectors = MAX_BUFFER_SIZE / state_vector_size_per_shot;

        // How many of the structures would fit
        let max_shots_in_buffer = min(MAX_SHOT_ENTRIES, max_shot_state_vectors);
        // How many would we allow based on the max shots per batch
        let max_shots_per_batch = min(max_shots_in_buffer, MAX_SHOTS_PER_BATCH as usize);
        // Do that many, of however many shots if less
        let shots_per_batch = min(shot_count as usize, max_shots_per_batch);

        let workgroups_per_shot = if qubit_count <= MAX_QUBITS_PER_WORKGROUP {
            1
        } else {
            1 << (qubit_count - MAX_QUBITS_PER_WORKGROUP)
        };

        let shots_buffer_size = shots_per_batch * SIZEOF_SHOTDATA;
        let ops_buffer_size = op_count as usize * op_size;
        let state_vector_buffer_size = shots_per_batch * state_vector_size_per_shot;
        // Each result is a u32, plus one extra on the end for the shader to set an 'error code' if needed
        let results_buffer_size =
            shots_per_batch * (result_count + 1) as usize * std::mem::size_of::<u32>();

        let diagnostics_buffer_size = 4 * 4 /* initial bytes */
                + 640 + 144 // ShotData + Op
                + (THREADS_PER_WORKGROUP as usize * (8 * MAX_QUBIT_COUNT as usize)) // Workgroup probabilities
                + std::mem::size_of::<WorkgroupCollationBuffer>(); // Collation buffers

        let download_buffer_size = results_buffer_size + diagnostics_buffer_size;

        let batch_count = (shot_count as usize - 1) / shots_per_batch + 1;

        let entries_per_workgroup = entries_per_shot / workgroups_per_shot;

        let entries_per_thread = entries_per_workgroup / THREADS_PER_WORKGROUP as usize;

        Ok(RunParams {
            shots_buffer_size,
            ops_buffer_size,
            state_vector_buffer_size,
            results_buffer_size,
            diagnostics_buffer_size,
            download_buffer_size,
            entries_per_shot,
            entries_per_workgroup,
            entries_per_thread,
            batch_count,
            workgroups_per_shot,
            shots_per_batch,
            shots_count: shot_count as usize,
            op_count: op_count as usize,
            gate_op_count: gate_op_count as usize,
            result_count: (result_count + 1) as usize,
        })
    }

    fn create_buffers(&mut self) -> GpuBuffers {
        let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniforms Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let workgroup_collation = self.device.create_buffer(&wgpu::wgt::BufferDescriptor {
            label: Some("Workgroup Collation Buffer"),
            size: std::mem::size_of::<WorkgroupCollationBuffer>() as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let shot_state = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shot State Buffer"),
            size: self.run_params.shots_buffer_size as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let state_vector = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("StateVector Buffer"),
            size: self.run_params.state_vector_buffer_size as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        // Initialize ops buffer from the circuit using bytemuck
        let (ops_upload, ops) =
            create_ops_buffers(self.run_params.ops_buffer_size, &self.ops, &self.device);

        let results = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Results Buffer"),
            size: self.run_params.results_buffer_size as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let download = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Download buffer"),
            size: self.run_params.download_buffer_size as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let diagnostics = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Diagnostics Buffer"),
            size: self.run_params.diagnostics_buffer_size as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        GpuBuffers {
            uniform_buffer,
            workgroup_collation,
            shot_state,
            state_vector,
            ops_upload,
            ops,
            results,
            diagnostics,
            download,
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn create_resources(&mut self) {
        let buffers = self.create_buffers();

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("StateVector Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.workgroup_collation.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffers.shot_state.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffers.ops.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: buffers.state_vector.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: buffers.results.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: buffers.diagnostics.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: buffers.uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout =
            &self
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("GPU simulator pipeline layout"),
                    bind_group_layouts: &[&self.bind_group_layout],
                    push_constant_ranges: &[],
                });

        let pipeline_init_op =
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("initialize pipeline"),
                    layout: Some(pipeline_layout),
                    module: &self.shader_module,
                    entry_point: Some("initialize"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[],
                        ..Default::default()
                    },
                    cache: None,
                });

        let pipeline_prepare_op =
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("prepare_op pipeline"),
                    layout: Some(pipeline_layout),
                    module: &self.shader_module,
                    entry_point: Some("prepare_op"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[],
                        ..Default::default()
                    },
                    cache: None,
                });

        let pipeline_execute_op =
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("execute_op pipeline"),
                    layout: Some(pipeline_layout),
                    module: &self.shader_module,
                    entry_point: Some("execute_op"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[],
                        ..Default::default()
                    },
                    cache: None,
                });

        let pipeline_execute_2q_op =
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("execute_2q_op pipeline"),
                    layout: Some(pipeline_layout),
                    module: &self.shader_module,
                    entry_point: Some("execute_2q_op"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[],
                        ..Default::default()
                    },
                    cache: None,
                });

        let pipeline_execute_mz =
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("execute_mz pipeline"),
                    layout: Some(pipeline_layout),
                    module: &self.shader_module,
                    entry_point: Some("execute_mz"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[],
                        ..Default::default()
                    },
                    cache: None,
                });

        self.resources = Some(GpuResources {
            pipeline_init_op,
            pipeline_prepare_op,
            pipeline_execute_op,
            pipeline_execute_2q_op,
            pipeline_execute_mz,
            bind_group,
            buffers,
        });
    }

    #[allow(clippy::too_many_lines)]
    pub async fn run(&self) -> Vec<u32> {
        let resources: &GpuResources = self.resources.as_ref().expect("Resources not initialized");

        // Star the upload the ops to the GPU ASAP
        let mut ops_copy_encoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Ops Upload Command Encoder"),
                });

        ops_copy_encoder.copy_buffer_to_buffer(
            &resources.buffers.ops_upload,
            0,
            &resources.buffers.ops,
            0,
            resources.buffers.ops.size(),
        );

        let ops_command_buffer = ops_copy_encoder.finish();
        self.queue.submit([ops_command_buffer]);

        /*
        GPU processing of shots is as follows. Note the following considerations:

        - Shots will be split into batches if they do not all fit in the GPU memory at once
        - Programs will be split into blocks if they are in blocks (i.e., for adaptive circuits), or
          if the size of one block exceeds the limited for dispatches per submit
        - Each op in the program is processed by two compute shaders:
            - prepare_op: prepares any data needed to process the op. This runs on a single thread per shot
              to avoid any parallelization issues with RNG state, noise decisions, etc. The GPU
              ensures that state is consistent between dispatches, so the next steps sees the final
              result of the work done here.
            - execute_op: applies the op to the state vector. This is parallelized across threads
              (and eventually workgroups for >22 qubits), and should only do work that does not
              require any synchronization across workgroups. The next 'prepare_op' step will
              do any cross-shot collation or processing needed after this kernel completes.

        - By making the 'prepare_op' step responsible for generating state such as random numbers and
          noise decisions, and collating the results of parallel processing in prior dispatches,
          we can ensure that the 'execute_op' step is fully parallelizable, and also only require
          one read-only copy of the program to execute in GPU memory when running shots concurrently.

        The overall processing flow is as follows:

        - For each batch of shots (will be 1 if all shots fit in one batch):
            - For each block of ops in the program (will be 1 if all ops fit in one block):
                - For each op in the block:
                    - Dispatch the prepare_op kernel
                    - Dispatch the execute_op kernel
                - Do any end of block processing (e.g., compute & branching for adaptive)
            - Do any output recording for the shots in the batch
        - Return the collated shot results
         */

        let mut results: Vec<u32> = Vec::new();

        // TODO: Just make this an i32 in the first place
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_possible_wrap)]
        let mut shots_remaining: i32 = self.run_params.shots_count as i32;

        for batch_idx in 0..self.run_params.batch_count {
            let shots_this_batch =
                min(shots_remaining, self.run_params.shots_per_batch as i32) as usize;

            // Update the uniforms for this batch
            let uniforms = Uniforms {
                #[allow(clippy::cast_possible_truncation)]
                batch_start_shot_id: (batch_idx * self.run_params.shots_per_batch) as u32,
                rng_seed: self.rng_seed,
            };

            // When this is put directly on the queue, it will be submitted when the next submit occurs, but will run before that submit's work
            self.queue.write_buffer(
                &resources.buffers.uniform_buffer,
                0,
                bytemuck::bytes_of(&uniforms),
            );

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("StateVector Command Encoder"),
                });

            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("StateVector Compute Pass"),
                timestamp_writes: None,
            });

            // TODO: Break into multiple dispatches if too many ops
            if self.run_params.op_count > MAX_DISPATCHES_PER_SUBMIT as usize {
                unimplemented!(
                    "This circuit exceeds the current upper limit of {} operations",
                    MAX_DISPATCHES_PER_SUBMIT
                );
            }

            compute_pass.set_bind_group(0, &resources.bind_group, &[]);

            let prepare_workgroup_count =
                u32::try_from(shots_this_batch).expect("shots_per_batch should fit in u32");
            // Workgroups for execute_op depends on qubit count
            let execute_workgroup_count =
                u32::try_from(self.run_params.workgroups_per_shot * shots_this_batch)
                    .expect("workgroups_per_shot * shots_per_batch should fit in u32");

            // Start by dispatching an 'init' kernel for the batch of shots
            compute_pass.set_pipeline(&resources.pipeline_init_op);
            compute_pass.dispatch_workgroups(execute_workgroup_count, 1, 1);

            // Dispatch the compute shaders for each op for this batch of shots
            for op in &self.ops {
                match op.id {
                    // One qubit gates
                    ops::ID..=ops::RZ | ops::MOVE => {
                        compute_pass.set_pipeline(&resources.pipeline_prepare_op);
                        compute_pass.dispatch_workgroups(prepare_workgroup_count, 1, 1);

                        compute_pass.set_pipeline(&resources.pipeline_execute_op);
                        compute_pass.dispatch_workgroups(execute_workgroup_count, 1, 1);
                    }
                    // Two qubit gates
                    ops::CX..=ops::RZZ => {
                        compute_pass.set_pipeline(&resources.pipeline_prepare_op);
                        compute_pass.dispatch_workgroups(prepare_workgroup_count, 1, 1);

                        compute_pass.set_pipeline(&resources.pipeline_execute_2q_op);
                        compute_pass.dispatch_workgroups(execute_workgroup_count, 1, 1);
                    }
                    ops::MRESETZ | ops::MZ => {
                        // Measurement has its own execute pipeline
                        compute_pass.set_pipeline(&resources.pipeline_prepare_op);
                        compute_pass.dispatch_workgroups(prepare_workgroup_count, 1, 1);

                        compute_pass.set_pipeline(&resources.pipeline_execute_mz);
                        compute_pass.dispatch_workgroups(execute_workgroup_count, 1, 1);
                    }
                    // Skip over noise ops
                    ops::PAULI_NOISE_1Q..=ops::LOSS_NOISE => {}
                    _ => {
                        panic!("Unsupported op ID {}", op.id);
                    }
                }
            }

            drop(compute_pass);

            // Copy the results and diagnostics to the download buffer
            encoder.copy_buffer_to_buffer(
                &resources.buffers.results,
                0,
                &resources.buffers.download,
                0,
                resources.buffers.results.size(),
            );

            encoder.copy_buffer_to_buffer(
                &resources.buffers.diagnostics,
                0,
                &resources.buffers.download,
                resources.buffers.results.size(),
                resources.buffers.diagnostics.size(),
            );

            let command_buffer = encoder.finish();
            self.queue.submit([command_buffer]);

            // Fetching the actual results is a real pain. For details, see:
            // https://github.com/gfx-rs/wgpu/blob/v26/examples/features/src/repeated_compute/mod.rs#L74

            // Cross-platform readback: async map + native poll
            let buffer_slice = resources.buffers.download.slice(..);

            let (sender, receiver) = futures::channel::oneshot::channel();

            buffer_slice.map_async(wgpu::MapMode::Read, move |_| {
                sender
                    .send(())
                    .expect("Unable to download the results buffer from the GPU");
            });

            // On native, drive the GPU and mapping to completion. No-op on the web (where it automatically polls).
            // Retry polling up to 5 times in case of transient failures
            let mut poll_attempts = 0;
            loop {
                match self.device.poll(wgpu::PollType::Wait) {
                    Ok(_) => break,
                    Err(e) => {
                        poll_attempts += 1;
                        assert!(
                            (poll_attempts < 5),
                            "GPU polling failed after 5 attempts: {e}"
                        );
                        eprintln!("GPU poll attempt {poll_attempts} failed: {e}, retrying...");
                    }
                }
            }

            receiver.await.expect("Failed to receive map completion");

            // Read, copy out, and unmap.
            let data = buffer_slice.get_mapped_range();

            // Fetch results and diagnostics from the download buffer
            let result_bytes = self.run_params.results_buffer_size;
            let results_data = &data[..result_bytes];
            let diagnositcs_data = &data[result_bytes..];

            let batch_results: Vec<u32> = bytemuck::cast_slice(results_data).to_vec();

            results.extend(batch_results);

            // TODO: Capture and return only the first populated diagnostics
            let diagnostics = bytemuck::from_bytes::<DiagnosticsData>(diagnositcs_data);

            drop(data);
            resources.buffers.download.unmap();

            shots_remaining -= self.run_params.shots_per_batch as i32;
        }

        // We may have had extra shots if the last batch was not full. Truncate if so.
        results.truncate(self.run_params.shots_count * self.run_params.result_count);

        if self.dbg_capture {
            unsafe {
                self.device.stop_graphics_debugger_capture();
            }
        }

        results
    }
}

fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
    let buffers = [
        // name, index, is_uniform, read_only
        ("WorkgroupCollation", 0, false, false),
        ("ShotState", 1, false, false),
        ("Ops", 2, false, true),
        ("StateVector", 3, false, false),
        ("Results", 4, false, false),
        ("Diagnostics", 5, false, false),
        ("Uniforms", 6, true, false),
    ]
    .into_iter()
    .map(
        |(_, binding, is_uniform, read_only)| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: if is_uniform {
                    wgpu::BufferBindingType::Uniform
                } else {
                    wgpu::BufferBindingType::Storage { read_only }
                },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    )
    .collect::<Vec<_>>();

    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Simulator bind group layout"),
        entries: &buffers,
    })
}

pub fn create_ops_buffers(buffer_size: usize, ops: &[Op], device: &Device) -> (Buffer, Buffer) {
    // If we try to just map and copy the ops buffer directly without the extra upload buffer
    // then Xcode profiling doesn't see the ops due to a bug, so we use an intermediate upload buffer.
    // See https://toji.dev/webgpu-best-practices/buffer-uploads for general best practices on buffer uploads.
    let ops_upload_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Ops Upload Buffer"),
        size: buffer_size as u64,
        usage: BufferUsages::MAP_WRITE | BufferUsages::COPY_SRC,
        mapped_at_creation: true,
    });

    // Upload the ops data and unmap
    ops_upload_buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(bytemuck::cast_slice(ops));
    ops_upload_buffer.unmap();

    // Create the private GPU buffer to copy the ops buffer into.
    let ops_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Ops Buffer"),
        size: buffer_size as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    (ops_upload_buffer, ops_buffer)
}
