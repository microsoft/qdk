// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(unused)]

use super::shader_types::{Op, Result};

use futures::FutureExt;
use std::num::NonZeroU64;
use wgpu::{
    Adapter, BindGroup, BindGroupLayout, Buffer, BufferDescriptor, BufferUsages, ComputePipeline,
    Device, Limits, Queue, ShaderModule,
};

const DO_CAPTURE: bool = false;

pub struct GpuContext {
    device: Device,
    queue: Queue,
    shader_module: ShaderModule,
    bind_group_layout: BindGroupLayout,
    ops: Vec<Op>,
    qubit_count: i32,
    resources: Option<GpuResources>,
    entries_per_thread: i32,
    threads_per_workgroup: i32,
    workgroup_count: i32,
}

struct GpuResources {
    pipeline: ComputePipeline,
    state_vector_buffer: Buffer,
    ops_upload_buffer: Buffer,
    ops_buffer: Buffer,
    results_buffer: Buffer,
    download_buffer: Buffer,
    bind_group: BindGroup,
}

impl GpuContext {
    pub async fn new(qubit_count: i32, ops: Vec<Op>) -> Self {
        // wgpu limits buffers to 1GB, which is 2^30 bytes.
        // As we need 8 bytes (2^3) per complex number, we can only support up to 2^27 state vector entries.
        // See https://github.com/gfx-rs/wgpu/issues/2337#issuecomment-1549935712
        assert!(
            (1..=27).contains(&qubit_count),
            "Qubit count out of range: {qubit_count}",
        );

        let (entries_per_thread, threads_per_workgroup, workgroup_count) =
            Self::get_params(qubit_count);
        let mut desc = wgpu::InstanceDescriptor::default();
        let instance = wgpu::Instance::new(&desc);
        let adapter: Adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("Failed to create an adapter");

        let downlevel_capabilities = adapter.get_downlevel_capabilities();
        if !downlevel_capabilities
            .flags
            .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
        {
            panic!("Adapter does not support compute shaders");
        }

        let buffer_needed: u32 = if qubit_count < 17 {
            1024 * 1024 // Min 1MB for small circuits
        } else {
            (1u32 << qubit_count) * 8 // 8 bytes per complex number for larger circuits
        };

        let (device, queue): (Device, Queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: Limits {
                    max_compute_workgroup_size_x: 32,
                    max_compute_workgroups_per_dimension: 65535,
                    max_storage_buffer_binding_size: buffer_needed,
                    ..Default::default()
                },
                // required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("failed to create device");

        if DO_CAPTURE {
            unsafe {
                device.start_graphics_debugger_capture();
            }
        }

        // Create the shader module and bind group layout
        let shader_module = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let bind_group_layout = create_bind_group_layout(&device);

        GpuContext {
            device,
            queue,
            shader_module,
            bind_group_layout,
            resources: None,
            ops,
            qubit_count,
            entries_per_thread,
            threads_per_workgroup,
            workgroup_count,
        }
    }

    pub fn get_params(qubit_count: i32) -> (i32, i32, i32) {
        // Figure out how many threads and threadgroups to use based on the qubit count.
        const MAX_QUBITS_PER_THREAD: i32 = 10;
        const MAX_QUBITS_PER_THREADGROUP: i32 = 12;

        if qubit_count < MAX_QUBITS_PER_THREAD {
            // All qubits fit in one thread
            return (
                1 << qubit_count, // Output states to process per thread
                1,                // Threads per workgroup
                1,                // Workgroup count
            );
        } else if qubit_count <= MAX_QUBITS_PER_THREADGROUP {
            // All qubits fit in one threadgroup
            return (
                1 << MAX_QUBITS_PER_THREAD,
                1 << (qubit_count - MAX_QUBITS_PER_THREAD),
                1,
            );
        } else if qubit_count <= 30 {
            // Then add more threadgroups
            return (
                1 << MAX_QUBITS_PER_THREAD,
                1 << (MAX_QUBITS_PER_THREADGROUP - MAX_QUBITS_PER_THREAD),
                1 << (qubit_count - MAX_QUBITS_PER_THREADGROUP),
            );
        }
        panic!("Qubit count too high: {}", qubit_count);
    }

    pub fn create_resources(&mut self) {
        // Assert the the Op size is 256 bytes
        assert_eq!(
            std::mem::size_of::<Op>(),
            256,
            "Op struct must be 256 bytes for WebGPU dynamic buffer alignment"
        );
        let state_vector_entries: u64 = 2u64.pow(self.qubit_count as u32);
        let result_buffer_size_bytes: u64 =
            std::mem::size_of::<Result>() as u64 * state_vector_entries;

        let state_vector_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("StateVector Buffer"),
            size: state_vector_entries * 2 * std::mem::size_of::<f32>() as u64, // 2 floats per complex entry
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Initialize ops buffer from the circuit using bytemuck
        let (ops_upload_buffer, ops_buffer) = create_ops_buffers(&self.ops, &self.device);

        let results_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Results Buffer"),
            size: result_buffer_size_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let download_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Download buffer"),
            size: result_buffer_size_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let result_idx_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Result Index Buffer"),
            size: std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("StateVector Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: state_vector_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    // Bind a 256-byte slice; dynamic offsets will move this window
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &ops_buffer,
                        offset: 0,
                        size: Some(NonZeroU64::new(256).unwrap()),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: results_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: result_idx_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("StateVector Pipeline"),
                layout: Some(&self.device.create_pipeline_layout(
                    &wgpu::PipelineLayoutDescriptor {
                        label: Some("StateVector pipeline Layout"),
                        bind_group_layouts: &[&self.bind_group_layout],
                        push_constant_ranges: &[],
                    },
                )),
                module: &self.shader_module,
                entry_point: Some("run_statevector_ops"),
                // When creating the pipeline, override the workgroup size based on the qubit count.
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[
                        ("WORKGROUP_SIZE_X", self.threads_per_workgroup as f64),
                        ("QUBIT_COUNT", self.qubit_count as f64),
                    ],
                    ..Default::default()
                },
                cache: None,
            });

        self.resources = Some(GpuResources {
            pipeline,
            state_vector_buffer,
            ops_upload_buffer,
            ops_buffer,
            results_buffer,
            download_buffer,
            bind_group,
        });
    }

    pub async fn run(&self) -> Vec<Result> {
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
            &resources.state_vector_buffer,
            0,
            state_init_buffer.size(),
        );

        encoder.copy_buffer_to_buffer(
            &resources.ops_upload_buffer,
            0,
            &resources.ops_buffer,
            0,
            resources.ops_buffer.size(),
        );

        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("StateVector Compute Pass"),
            timestamp_writes: None,
        });

        compute_pass.set_pipeline(&resources.pipeline);

        let op_count = u32::try_from(self.ops.len()).expect("Too many ops");
        assert!(self.workgroup_count > 0);
        let workgroup_count: u32 =
            u32::try_from(self.workgroup_count).expect("Too many workgroups");
        for i in 0..op_count {
            let op_offset: u32 = i * 256; // Each op is 256 bytes (aligned)
            compute_pass.set_bind_group(0, &resources.bind_group, &[op_offset]);
            compute_pass.dispatch_workgroups(workgroup_count, 1, 1);
        }

        drop(compute_pass);

        // Copy the results to the download buffer
        encoder.copy_buffer_to_buffer(
            &resources.results_buffer,
            0,
            &resources.download_buffer,
            0,
            resources.download_buffer.size(),
        );

        let command_buffer = encoder.finish();
        self.queue.submit([command_buffer]);

        // Fetching the actual results is a real pain. For details, see:
        // https://github.com/gfx-rs/wgpu/blob/v26/examples/features/src/repeated_compute/mod.rs#L74

        // Cross-platform readback: async map + native poll
        let buffer_slice = resources.download_buffer.slice(..);

        let (sender, receiver) = futures::channel::oneshot::channel();

        buffer_slice.map_async(wgpu::MapMode::Read, move |_| {
            sender.send(()).unwrap();
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
        resources.download_buffer.unmap();

        if DO_CAPTURE {
            unsafe {
                self.device.stop_graphics_debugger_capture();
            }
        }

        results
    }
}

fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("StateVector bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                // StateVector buffer
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // Ops buffer
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: true,
                    // Specify the per-op slice size so dynamic offsets are allowed
                    min_binding_size: Some(NonZeroU64::new(256).unwrap()),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // Result buffer
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                // Result index buffer
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    bind_group_layout
}

pub fn create_ops_buffers(ops: &Vec<Op>, device: &Device) -> (Buffer, Buffer) {
    let buffer_size: u64 = (ops.len() * std::mem::size_of::<Op>()) as u64;

    let ops_upload_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Ops Upload Buffer"),
        size: buffer_size,
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
        size: buffer_size,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    (ops_upload_buffer, ops_buffer)
}
