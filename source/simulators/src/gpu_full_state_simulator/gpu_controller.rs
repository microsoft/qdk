// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(unused)]

use super::shader_types::{Op, Result};

use futures::FutureExt;
use rand::distributions::uniform;
use std::{cmp::max, cmp::min, num::NonZeroU64};
use wgpu::{
    Adapter, BindGroup, BindGroupLayout, Buffer, BufferDescriptor, BufferUsages, ComputePipeline,
    Device, Limits, Queue, RequestAdapterError, ShaderModule,
};

// Some of these values are to align with WebGPU default limits
// See https://gpuweb.github.io/gpuweb/#limits
const MAX_BUFFER_SIZE: usize = 1 << 30; // 1 GB limit due to some wgpu restrictions
const MAX_QUBIT_COUNT: u32 = 27; // 2^27 * 8 bytes per complex32 = 1 GB buffer limit
const MAX_QUBITS_PER_WORKGROUP: u32 = 22; // Max qubits to be processed by a single workgroup

// Once a shot is big enough to need multiple workgroups, what's the max number of workgroups possible
const MAX_PARTITIONED_WORKGROUPS: usize = 1 << (MAX_QUBIT_COUNT - MAX_QUBITS_PER_WORKGROUP);
const MAX_SHOTS_PER_BATCH: u32 = 65535; // To align with max workgroups per dimension WebGPU default
const THREADS_PER_WORKGROUP: u32 = 32; // 32 gives good occupancy across various GPUs

// Round up circuit qubits if smaller to enable to optimizations re unrolling, etc.
// With min qubit count of 8, this means min 256 entries per shot. Spread across 32 threads = 8 entries per thread.
// With each iteration in each thread processing 2 or 4 entries, that means 2 or 4 iterations per thread minimum.
const MIN_QUBIT_COUNT: u32 = 8;
const MAX_CIRCUIT_OPS: usize = MAX_BUFFER_SIZE / std::mem::size_of::<Op>();
const SIZEOF_SHOTDATA: usize = 1024; // Size of ShotData struct on the GPU in bytes
const MAX_SHOT_ENTRIES: usize = MAX_BUFFER_SIZE / SIZEOF_SHOTDATA;

// There is no hard limit here, but for very large circuits we may need to split into multiple dispatches.
// TODO: See if there is a way to query the GPU for max dispatches per submit, or derive it from other limits
const MAX_DISPATCHES_PER_SUBMIT: u32 = 10000;

pub struct GpuContext {
    device: Device,
    queue: Queue,
    shader_module: ShaderModule,
    bind_group_layout: BindGroupLayout,
    ops: Vec<Op>,
    qubit_count: u32,
    resources: Option<GpuResources>,
    run_params: RunParams,
    dbg_capture: bool,
}

struct GpuResources {
    pipeline_prepare_op: ComputePipeline,
    pipeline_execute_op: ComputePipeline,
    bind_group: BindGroup,
    buffers: GpuBuffers,
}

struct GpuBuffers {
    workgroup_collation: Buffer,
    shot_state: Buffer,
    state_vector: Buffer,
    ops_upload: Buffer,
    ops: Buffer,
    results: Buffer,
    download: Buffer,
}

struct RunParams {
    shots_buffer_size: usize,
    ops_buffer_size: usize,
    state_vector_buffer_size: usize,
    results_buffer_size: usize,
    entries_per_shot: usize,
    entries_per_workgroup: usize,
    entries_per_thread: usize,
    batch_count: usize,
    shots_per_batch: usize,
    workgroups_per_shot: usize,
    op_count: usize,
    gate_op_count: usize,
    result_count: usize,
}

// The below structures are used to collate results across workgroups within a shot
struct QubitProbabilities {
    zero: f32,
    one: f32,
}

// Each workgroup sums the probabilities for the entries it processed for each qubit
struct WorkgroupSums {
    qubits: [QubitProbabilities; MAX_QUBIT_COUNT as usize],
}

// Once the dispatch for the workgroup processing is done, the results from all workgroups
// for all active shots are collated here for final processing in the next prepare_op step.
struct WorkgroupCollationBuffer {
    sums: [WorkgroupSums; MAX_PARTITIONED_WORKGROUPS],
}

impl GpuContext {
    pub async fn get_adapter() -> std::result::Result<Adapter, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| e.to_string())
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
            .max(run_params.results_buffer_size);

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
        let run_params: RunParams = Self::get_params(
            qubit_count,
            result_count + 1,
            op_count,
            gate_op_count,
            shots,
        )?;

        let adapter = wgpu::Instance::new(&wgpu::InstanceDescriptor::default())
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| e.to_string())?;

        Self::validate_adapter_capabilities(&adapter)?;

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
        let shader_module = device.create_shader_module(wgpu::include_wgsl!("simulator.wgsl"));

        let bind_group_layout = create_bind_group_layout(&device);

        Ok(GpuContext {
            device,
            queue,
            shader_module,
            bind_group_layout,
            resources: None,
            ops,
            qubit_count,
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
            shots_per_batch * result_count as usize * std::mem::size_of::<u32>();

        let batch_count = (shot_count as usize - 1) / shots_per_batch + 1;

        let entries_per_workgroup = entries_per_shot / workgroups_per_shot;

        let entries_per_thread = entries_per_workgroup / THREADS_PER_WORKGROUP as usize;

        Ok(RunParams {
            shots_buffer_size,
            ops_buffer_size,
            state_vector_buffer_size,
            results_buffer_size,
            entries_per_shot,
            entries_per_workgroup,
            entries_per_thread,
            batch_count,
            workgroups_per_shot,
            shots_per_batch,
            op_count: op_count as usize,
            gate_op_count: gate_op_count as usize,
            result_count: result_count as usize,
        })
    }

    fn create_buffers(&mut self) -> GpuBuffers {
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
            size: self.run_params.results_buffer_size as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        GpuBuffers {
            workgroup_collation,
            shot_state,
            state_vector,
            ops_upload,
            ops,
            results,
            download,
        }
    }

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

        // Overrides needs to be passed as an f64 due to wgpu restrictions
        let qubit_count = f64::from(self.qubit_count);
        let workgroups_per_shot = f64::from(
            u32::try_from(self.run_params.workgroups_per_shot).expect("invalid conversion"),
        );
        let result_count =
            f64::from(u32::try_from(self.run_params.result_count).expect("invalid conversion"));
        let entries_per_thread = f64::from(
            u32::try_from(self.run_params.entries_per_thread).expect("invalid conversion"),
        );

        let pipeline_prepare_op =
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("prepare_op pipeline"),
                    layout: Some(pipeline_layout),
                    module: &self.shader_module,
                    entry_point: Some("prepare_op"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[
                            ("QUBIT_COUNT", qubit_count),
                            ("WORKGROUPS_PER_SHOT", workgroups_per_shot),
                            ("RESULT_COUNT", result_count),
                            ("ENTRIES_PER_THREAD", entries_per_thread),
                        ],
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
                        constants: &[
                            ("QUBIT_COUNT", qubit_count),
                            ("WORKGROUPS_PER_SHOT", workgroups_per_shot),
                            ("RESULT_COUNT", result_count),
                            ("ENTRIES_PER_THREAD", entries_per_thread),
                        ],
                        ..Default::default()
                    },
                    cache: None,
                });

        self.resources = Some(GpuResources {
            pipeline_prepare_op,
            pipeline_execute_op,
            bind_group,
            buffers,
        });
    }

    pub async fn run(&self) -> Vec<u32> {
        let resources: &GpuResources = self.resources.as_ref().expect("Resources not initialized");

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("StateVector Command Encoder"),
            });

        encoder.copy_buffer_to_buffer(
            &resources.buffers.ops_upload,
            0,
            &resources.buffers.ops,
            0,
            resources.buffers.ops.size(),
        );
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

        // TODO: Support multiple batches of shots
        if self.run_params.batch_count != 1 {
            unimplemented!("Multiple batches of shots not yet supported");
        }

        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("StateVector Compute Pass"),
            timestamp_writes: None,
        });

        // TODO: Break into multiple dispatches if too many ops
        if self.run_params.op_count > MAX_DISPATCHES_PER_SUBMIT as usize {
            unimplemented!("Multiple submits per circuit not yet supported");
        }

        compute_pass.set_bind_group(0, &resources.bind_group, &[]);

        // Currently always 1 workgroup per shot (assume 1 thread per workgroup) for the prepare_op stage
        let prepare_workgroup_count = u32::try_from(self.run_params.shots_per_batch)
            .expect("shots_per_batch should fit in u32");
        // Workgroups for execute_op depends on qubit count
        let execute_workgroup_count =
            u32::try_from(self.run_params.workgroups_per_shot * self.run_params.shots_per_batch)
                .expect("workgroups_per_shot * shots_per_batch should fit in u32");

        // Dispatch the compute shaders for each op for this batch of shots
        for i in 0..self.run_params.gate_op_count {
            compute_pass.set_pipeline(&resources.pipeline_prepare_op);
            compute_pass.dispatch_workgroups(prepare_workgroup_count, 1, 1);

            compute_pass.set_pipeline(&resources.pipeline_execute_op);
            compute_pass.dispatch_workgroups(execute_workgroup_count, 1, 1);
        }

        drop(compute_pass);

        // Copy the results to the download buffer
        encoder.copy_buffer_to_buffer(
            &resources.buffers.results,
            0,
            &resources.buffers.download,
            0,
            resources.buffers.download.size(),
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
        let results: Vec<u32> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        resources.buffers.download.unmap();

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
        ("WorkgroupCollation", 0, false),
        ("ShotState", 1, false),
        ("Ops", 2, true),
        ("StateVector", 3, false),
        ("Results", 4, false),
    ]
    .into_iter()
    .map(|(_, binding, read_only)| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    })
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
