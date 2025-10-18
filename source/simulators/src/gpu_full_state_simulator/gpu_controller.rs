// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(unused)]

use super::shader_types::{Op, Result};

use futures::FutureExt;
use rand::distributions::uniform;
use std::{cmp::max, cmp::min, num::NonZeroU64};
use wgpu::{
    Adapter, BindGroup, BindGroupLayout, Buffer, BufferDescriptor, BufferUsages, ComputePipeline,
    Device, Limits, Queue, RequestAdapterError, ShaderModule, wgc::pipeline,
};

const THREADS_PER_WORKGROUP: u32 = 32; // 32 gives good occupancy across various GPUs
const MAX_SHOTS_PER_BATCH: u32 = 65535; // To align with max workgroups per dimension WebGPU default
const MIN_QUBIT_COUNT: u32 = 10; // Round up circuit qubits if smaller to enable to optimizations re unrolling, etc.
const MAX_QUBIT_COUNT: u32 = 22; // Limit so we can fit each shot in one workgroup, and to avoid issues with f32 precision on larger state vectors
const MAX_BUFFER_SIZE: usize = 1 << 30; // 1 GB limit due to some wgpu restrictions
const MAX_WORKGROUP_STORAGE_SIZE: usize = 16384; // 16 KB default for WebGPU

const MAX_CIRCUIT_OPS: usize = MAX_BUFFER_SIZE / std::mem::size_of::<Op>();
const SIZEOF_SHOTDATA: usize = 400; // Size of ShotData struct on the GPU in bytes
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
    uniform_params: Buffer,
    shot_state: Buffer,
    state_vector: Buffer,
    ops_upload: Buffer,
    ops: Buffer,
    results: Buffer,
    download: Buffer,
}

#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Copy, Clone, Debug)]
struct UniformParams {
    start_shot_id: u32,
    rng_seed: u32,
}

struct RunParams {
    shots_buffer_size: usize,
    ops_buffer_size: usize,
    state_vector_buffer_size: usize,
    results_buffer_size: usize,
    entries_processed_per_thread: usize,
    batch_count: usize,
    shots_per_batch: usize,
    op_count: usize,
}

struct QubitProbabilities {
    zero: f32,
    one: f32,
}

impl GpuContext {
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
                "Required buffer size of {max_required_buffer_size} exceeds maximum GPU buffer size of {max_storage_buffer_size}",
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
        let op_count: u32 = ops.len().try_into().map_err(|_| "Too many operations")?;
        let qubit_count = qubit_count.max(MIN_QUBIT_COUNT);
        if qubit_count > MAX_QUBIT_COUNT {
            return Err(format!(
                "Qubit count {qubit_count} exceeds maximum supported qubit count of {MAX_QUBIT_COUNT}"
            ));
        }

        let run_params: RunParams = Self::get_params(qubit_count, result_count, op_count, shots)?;

        let adapter = wgpu::Instance::new(&wgpu::InstanceDescriptor::default())
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| e.to_string())?;

        Self::validate_adapter_capabilities(&adapter)?;

        let (device, queue): (Device, Queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
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
        shot_count: u32,
    ) -> std::result::Result<RunParams, String> {
        let state_vector_entry_size = std::mem::size_of::<f32>() * 2; // complex f32
        let op_size = std::mem::size_of::<Op>();

        let state_vector_entries_per_shot: usize = 1 << qubit_count;
        let state_vector_size_per_shot = state_vector_entries_per_shot * state_vector_entry_size;
        let max_state_vectors = MAX_BUFFER_SIZE / state_vector_size_per_shot;

        // How many of the structures would fit
        let max_shots_in_buffer = min(MAX_SHOT_ENTRIES, max_state_vectors);
        // How many would we allow based on the max shots per batch
        let max_shots_per_batch = min(max_shots_in_buffer, MAX_SHOTS_PER_BATCH as usize);
        // Do that many, of however many shots if less
        let shots_per_batch = min(shot_count as usize, max_shots_per_batch);

        let shots_buffer_size = shots_per_batch * SIZEOF_SHOTDATA;
        let ops_buffer_size = op_count as usize * op_size;
        let state_vector_buffer_size = shots_per_batch * state_vector_size_per_shot;
        // Each result is an i32
        let results_buffer_size =
            shots_per_batch * result_count as usize * std::mem::size_of::<i32>();

        let mut batch_count = shot_count as usize / shots_per_batch;
        if (shot_count as usize % shots_per_batch) > 0 {
            // Need an extra batch for the remainder of shots
            // TODO: Remember to handle this smaller final batch in the run logic
            batch_count += 1;
        }

        // NOTE: There was always be min 10 qubits, so min 1024 entries
        let entries_processed_per_thread =
            state_vector_entries_per_shot / THREADS_PER_WORKGROUP as usize;

        Ok(RunParams {
            shots_buffer_size,
            ops_buffer_size,
            state_vector_buffer_size,
            results_buffer_size,
            entries_processed_per_thread,
            batch_count,
            shots_per_batch,
            op_count: op_count as usize,
        })
    }

    fn create_buffers(&mut self) -> GpuBuffers {
        let uniform_params = self.device.create_buffer(&wgpu::wgt::BufferDescriptor {
            label: Some("Uniform Params Buffer"),
            size: std::mem::size_of::<UniformParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shot_state = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shot State Buffer"),
            size: self.run_params.shots_buffer_size as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let state_vector = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("StateVector Buffer"),
            size: self.run_params.state_vector_buffer_size as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
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
            uniform_params,
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

        // TODO Fix this
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("StateVector Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.state_vector.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    // Bind a 256-byte slice; dynamic offsets will move this window
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffers.ops,
                        offset: 0,
                        size: Some(NonZeroU64::new(256).expect("Failed to create NonZeroU64")),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
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

        let pipeline_prepare_op =
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("prepare_op pipeline"),
                    layout: Some(pipeline_layout),
                    module: &self.shader_module,
                    entry_point: Some("prepare_op"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[("QUBIT_COUNT", f64::from(self.qubit_count))],
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
                        constants: &[("QUBIT_COUNT", f64::from(self.qubit_count))],
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

    pub async fn run(&self) -> Vec<Result> {
        // TODO: Need to update this for the prepare, execute loop over all ops and batching of shots
        let resources: &GpuResources = self.resources.as_ref().expect("Resources not initialized");

        // Initialize the first entry of the state vector to |0> (the rest are already zeroed)
        let state_init_buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("State init buffer"),
            size: std::mem::size_of::<f32>() as u64 * 2,
            usage: BufferUsages::MAP_WRITE | BufferUsages::COPY_SRC,
            mapped_at_creation: true,
        });

        // Upload the ops data and unmap
        let entry_0: [f32; 2] = [1.0, 0.0]; // Initial state |0>
        state_init_buffer
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(&entry_0));
        state_init_buffer.unmap();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("StateVector Command Encoder"),
            });

        // Copy the upload buffers into the state vector and ops buffers on the GPU
        encoder.copy_buffer_to_buffer(
            &state_init_buffer,
            0,
            &resources.buffers.state_vector,
            0,
            state_init_buffer.size(),
        );

        encoder.copy_buffer_to_buffer(
            &resources.buffers.ops_upload,
            0,
            &resources.buffers.ops,
            0,
            resources.buffers.ops.size(),
        );

        // TODO: This should be done per-batch with updated uniform params
        let uniform_params_data = UniformParams {
            start_shot_id: 0,
            rng_seed: 0xdead_beef,
        };
        self.queue.write_buffer(
            &resources.buffers.uniform_params,
            0,
            bytemuck::bytes_of(&uniform_params_data),
        );

        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("StateVector Compute Pass"),
            timestamp_writes: None,
        });

        // TODO: Do the prepare_op and execute_op loop here. Use MAX_DISPATCHES_PER_SUBMIT
        // Remember to handle the final batch if smaller than shots_per_batch
        compute_pass.set_pipeline(&resources.pipeline_execute_op);

        let op_count = u32::try_from(self.ops.len()).expect("Too many ops");
        // assert!(self.workgroup_count > 0);
        let workgroup_count: u32 = 1;
        // u32::try_from(self.workgroup_count).expect("Too many workgroups");
        for i in 0..op_count {
            let op_offset: u32 = i * 256; // Each op is 256 bytes (aligned)
            compute_pass.set_bind_group(0, &resources.bind_group, &[op_offset]);
            compute_pass.dispatch_workgroups(workgroup_count, 1, 1);
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
        let results: Vec<Result> = bytemuck::cast_slice(&data).to_vec();
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
        ("Params", 0, true),
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
