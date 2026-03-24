// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use bytemuck::{bytes_of, cast_slice, from_bytes};
use wgpu::{
    Adapter, BindGroup, BindGroupEntry, BindGroupLayout, Buffer, BufferDescriptor, BufferUsages,
    CommandBuffer, CommandEncoder, ComputePipeline, ComputePipelineDescriptor, Device, PollType,
    Queue,
};

use crate::shader_types::{DiagnosticsData, Uniforms, WorkgroupCollationBuffer};

#[derive(Debug, Default)]
pub struct GpuResources {
    adapter: Option<Adapter>,
    device: Option<Device>,
    bind_group_layout: Option<BindGroupLayout>,
    dbg_capture: bool,
    queue: Option<Queue>,
    device_resources: GpuDeviceResources,
}

// Resources that depend on the device and need to be recreated if the device is recreated
#[derive(Debug)]
struct GpuDeviceResources {
    pub kernels: Option<GpuKernels>,
    pub bind_group: Option<BindGroup>,
    pub bound_buffers: Vec<BufferBinding>,
}
#[derive(Debug)]
pub struct GpuKernels {
    pub init_op: ComputePipeline,
    pub prepare_op: ComputePipeline,
    pub execute_op: ComputePipeline,
}

#[derive(Debug)]
struct BufferBinding {
    pub name: &'static str,
    pub is_uniform: bool,
    pub read_only: bool,
    pub usage: BufferUsages,
    pub buffer: Option<Buffer>,
}

// Keep the below in sync with the shader bindings in simulator.wgsl
const WORKGROUP_COLLATION_BUF_IDX: usize = 0;
const SHOT_STATE_BUF_IDX: usize = 1;
const OPS_BUF_IDX: usize = 2;
const STATE_VECTOR_BUF_IDX: usize = 3;
const RESULTS_BUF_IDX: usize = 4;
const DIAGNOSTICS_BUF_IDX: usize = 5;
const UNIFORM_BUF_IDX: usize = 6;
const CORRELATED_NOISE_TABLES_BUF_IDX: usize = 7;
const CORRELATED_NOISE_ENTRIES_BUF_IDX: usize = 8;

impl Default for GpuDeviceResources {
    fn default() -> Self {
        GpuDeviceResources {
            kernels: None,
            bind_group: None,
            bound_buffers: vec![
                BufferBinding {
                    name: "WorkgroupCollation",
                    is_uniform: false,
                    read_only: false,
                    usage: BufferUsages::STORAGE,
                    buffer: None,
                },
                BufferBinding {
                    name: "ShotState",
                    is_uniform: false,
                    read_only: false,
                    usage: BufferUsages::STORAGE,
                    buffer: None,
                },
                BufferBinding {
                    name: "Ops",
                    is_uniform: false,
                    read_only: true,
                    usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                    buffer: None,
                },
                BufferBinding {
                    name: "StateVector",
                    is_uniform: false,
                    read_only: false,
                    usage: BufferUsages::STORAGE,
                    buffer: None,
                },
                BufferBinding {
                    name: "Results",
                    is_uniform: false,
                    read_only: false,
                    usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                    buffer: None,
                },
                BufferBinding {
                    name: "Diagnostics",
                    is_uniform: false,
                    read_only: false,
                    usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                    buffer: None,
                },
                BufferBinding {
                    name: "Uniforms",
                    is_uniform: true,
                    read_only: false,
                    usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                    buffer: None,
                },
                BufferBinding {
                    name: "CorrelatedNoiseTables",
                    is_uniform: false,
                    read_only: true,
                    usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                    buffer: None,
                },
                BufferBinding {
                    name: "CorrelatedNoiseEntries",
                    is_uniform: false,
                    read_only: true,
                    usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                    buffer: None,
                },
            ],
        }
    }
}

impl Drop for GpuResources {
    fn drop(&mut self) {
        // If a device was capturing, stop the capture on drop
        self.stop_graphics_debugger_capture();
    }
}

impl GpuResources {
    // NOTE: After migrationing to wgpu v28 wasm32 and native will align on enumerate_adapters behavior
    // and also become async. See https://github.com/gfx-rs/wgpu/releases/tag/v28.0.0
    #[cfg(target_arch = "wasm32")]
    pub fn try_get_adapter() -> std::result::Result<Adapter, String> {
        Err("wasm32 is not supported currently".to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn try_get_adapter() -> std::result::Result<Adapter, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        let adapters = instance.enumerate_adapters(wgpu::Backends::PRIMARY);

        let score_adapter = |adapter: &Adapter| -> (u32, u32, u32, u32) {
            let info = adapter.get_info();
            let device_score = match info.device_type {
                wgpu::DeviceType::DiscreteGpu => 8,
                wgpu::DeviceType::IntegratedGpu => 4,
                _ => 0,
            };
            let backend_score = match info.backend {
                wgpu::Backend::Vulkan | wgpu::Backend::Metal => 2,
                wgpu::Backend::Dx12 => 1,
                _ => 0,
            };
            let limits = adapter.limits();
            (
                device_score,
                backend_score,
                limits.max_compute_workgroup_storage_size,
                limits.max_storage_buffer_binding_size,
            )
        };

        // Filter to discrete or integrated GPUs that support Vulkan, Metal, or DX12
        // Then sort prefering discrete over integrated, Vulkan/Metal over DX12, and most workgroup memory
        // On Windows we want to prefer Vulkan if possible. It seems to have less issues than DX12.
        // Note that requesting a high-performance adapter via the wgpu API just prefers discrete GPUs also.
        let adapter = adapters
            .into_iter()
            .filter(|a| {
                let score = score_adapter(a);
                // Require a storage buffer of at least 1GB to ensure we can hold large state vectors of up to 27 qubits
                // Note: 1GB is the limit in wgpu currently. See https://github.com/gfx-rs/wgpu/issues/2337#issuecomment-1549935712
                score.0 > 0 /* discrete or integrated */ &&
                score.1 > 0 /* supported backend */ &&
                score.2 >= (1u32 << 14) /* at least 16KB compute workgroup storage (which implies compute capabilities also) */ &&
                score.3 >= (1u32 << 30) /* at least 1GB storage buffers */
            })
            .max_by_key(score_adapter)
            .ok_or_else(|| "No suitable GPU adapter found".to_string())?;

        Ok(adapter)
    }

    fn stop_graphics_debugger_capture(&self) {
        if let Some(device) = &self.device
            && self.dbg_capture
        {
            unsafe {
                device.stop_graphics_debugger_capture();
            }
        }
    }

    pub async fn create_device(&mut self, dbg_capture: bool) -> Result<(), String> {
        // If we already had a prior device and it was capturing, stop any existing capture on it.
        self.stop_graphics_debugger_capture();

        // Per the WebGPU spec, creating a device multiple times from the same adapter is disallowed,
        // so recreate the adapter as well if creating a device and queue.
        let adapter = Self::try_get_adapter()?;

        let adapter_limits = adapter.limits();
        let required_limits = wgpu::Limits {
            max_storage_buffer_binding_size: 1u32 << 30, // 1GB
            ..adapter_limits
        };

        let (device, queue): (Device, Queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("QDK GPU simulator"),
                // Note that mappable primary buffers are a native-only feature, but using here to
                // workaround the Xcode bug described in https://github.com/gfx-rs/wgpu/issues/8111.
                // This will need to be revisited for web support.
                required_features: wgpu::Features::MAPPABLE_PRIMARY_BUFFERS, // TODO: Can remove this?
                required_limits,
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;

        if dbg_capture {
            unsafe {
                device.start_graphics_debugger_capture();
            }
        }
        // Drop any resources created with a prior device, since the new device will be used now
        self.device_resources = GpuDeviceResources::default();

        // Create the fixed sized buffers
        self.device_resources.bound_buffers[UNIFORM_BUF_IDX].buffer = Some(create_dst_buffer(
            &device,
            std::mem::size_of::<Uniforms>(),
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            "Uniform Buffer",
        ));

        self.device_resources.bound_buffers[WORKGROUP_COLLATION_BUF_IDX].buffer =
            Some(create_dst_buffer(
                &device,
                std::mem::size_of::<WorkgroupCollationBuffer>(),
                BufferUsages::STORAGE,
                "Workgroup Collation Buffer",
            ));

        self.adapter = Some(adapter);
        self.device = Some(device);
        self.dbg_capture = dbg_capture;
        self.queue = Some(queue);
        self.create_bind_group_layout()
    }

    pub async fn ensure_device(&mut self, dbg_capture: bool) -> Result<(), String> {
        if self.device.is_none() {
            self.create_device(dbg_capture).await?;
        }
        Ok(())
    }

    fn create_bind_group_layout(&mut self) -> Result<(), String> {
        let device = self.device.as_ref().ok_or("GPU device not initialized")?;

        #[allow(clippy::cast_possible_truncation)]
        let entries = self
            .device_resources
            .bound_buffers
            .iter()
            .enumerate()
            .map(|(binding, buffer_bindings)| wgpu::BindGroupLayoutEntry {
                binding: binding as u32,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: if buffer_bindings.is_uniform {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage {
                            read_only: buffer_bindings.read_only,
                        }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect::<Vec<_>>();

        self.bind_group_layout = Some(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Simulator bind group layout"),
                entries: &entries,
            },
        ));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_shaders(
        &mut self,
        qubit_count: i32,
        result_count: i32,
        workgroups_per_shot: i32,
        entries_per_thread: i32,
        threads_per_workgroup: i32,
        max_qubit_count: i32,
        max_qubits_per_workgroup: i32,
    ) -> Result<(), String> {
        let adapter = self.adapter.as_ref().ok_or("GPU adapter not initialized")?;
        let device = self.device.as_ref().ok_or("GPU device not initialized")?;
        let bind_group_layout = self
            .bind_group_layout
            .as_ref()
            .ok_or("Bind group layout not initialized")?; // This is created with the device, so should exist here

        // Create the shader module and bind group layout
        let raw_shader_src = include_str!("simulator.wgsl");
        let mut shader_src = raw_shader_src
            .replace("{{QUBIT_COUNT}}", &qubit_count.to_string())
            .replace("{{RESULT_COUNT}}", &(result_count + 1).to_string()) // +1 for result code per shot
            .replace("{{WORKGROUPS_PER_SHOT}}", &workgroups_per_shot.to_string())
            .replace("{{ENTRIES_PER_THREAD}}", &entries_per_thread.to_string())
            .replace(
                "{{THREADS_PER_WORKGROUP}}",
                &threads_per_workgroup.to_string(),
            )
            .replace("{{MAX_QUBIT_COUNT}}", &max_qubit_count.to_string())
            .replace(
                "{{MAX_QUBITS_PER_WORKGROUP}}",
                &max_qubits_per_workgroup.to_string(),
            );

        // Strip out DX12-incompatible code sections if needed
        if adapter.get_info().backend == wgpu::Backend::Dx12 {
            shader_src = strip_dx12_sections(&shader_src);
        }

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Simulator Shader Module"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GPU simulator pipeline layout"),
            bind_group_layouts: &[bind_group_layout],
            push_constant_ranges: &[],
        });

        let get_kernel = |name: &str| -> ComputePipeline {
            device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some(&format!("GPU kernel - {name}")),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some(name),
                compilation_options: Default::default(),
                cache: None,
            })
        };

        self.device_resources.kernels = Some(GpuKernels {
            init_op: get_kernel("initialize"),
            prepare_op: get_kernel("prepare_op"),
            execute_op: get_kernel("execute"),
        });

        Ok(())
    }

    pub fn get_kernels(&self) -> Result<&GpuKernels, String> {
        self.device_resources
            .kernels
            .as_ref()
            .ok_or("GPU kernels not initialized".to_string())
    }

    fn ensure_buffer(device: &Device, buf_binding: &mut BufferBinding, min_size: usize) {
        if buf_binding.buffer.is_none() {
            let new_buffer =
                create_dst_buffer(device, min_size, buf_binding.usage, buf_binding.name);
            buf_binding.buffer = Some(new_buffer);
        }
    }

    pub fn get_bind_group(&mut self) -> Result<BindGroup, String> {
        if let Some(ref bind_group) = self.device_resources.bind_group {
            // Already created. BindGroup largely wraps ref-counted handles, so cloning is cheap.
            return Ok(bind_group.clone());
        }

        let device = self.device.as_ref().ok_or("GPU device not initialized")?;
        let bind_group_layout = self.bind_group_layout.as_ref().ok_or("Missing layout")?;
        let bound_buffers = &mut self.device_resources.bound_buffers;

        // Even if correlated noise is not used, the buffers still need to exist anyway to be bound
        // NoiseTableMetadata is 16 bytes, NoiseTableEntry is 16 bytes
        Self::ensure_buffer(
            device,
            &mut bound_buffers[CORRELATED_NOISE_TABLES_BUF_IDX],
            16,
        );
        Self::ensure_buffer(
            device,
            &mut bound_buffers[CORRELATED_NOISE_ENTRIES_BUF_IDX],
            16,
        );

        // Ensure all buffers are created
        if bound_buffers.iter().any(|entry| entry.buffer.is_none()) {
            return Err(
                "All buffers to bind must be created before creating the bind group".to_string(),
            );
        }

        #[allow(clippy::cast_possible_truncation)]
        let entries: Vec<BindGroupEntry> = bound_buffers
            .iter()
            .enumerate()
            .map(|(idx, buffer_bindings)| BindGroupEntry {
                binding: idx as u32,
                resource: buffer_bindings
                    .buffer
                    .as_ref()
                    .expect("Buffer should exist")
                    .as_entire_binding(),
            })
            .collect();

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Simulator Bind Group"),
            layout: bind_group_layout,
            entries: &entries,
        });
        self.device_resources.bind_group = Some(bind_group.clone());
        Ok(bind_group)
    }

    pub fn upload_uniform(&self, uniforms: &Uniforms) -> Result<(), String> {
        let queue = self.queue.as_ref().ok_or("GPU queue not initialized")?;
        let uniform_buffer = &self.try_get_buffer(UNIFORM_BUF_IDX)?;
        queue.write_buffer(uniform_buffer, 0, bytes_of(uniforms));
        Ok(())
    }

    pub fn get_encoder(&self, label: &str) -> Result<CommandEncoder, String> {
        let device = self.device.as_ref().ok_or("GPU device not initialized")?;
        Ok(device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) }))
    }

    pub fn submit_command_buffer(&self, buffer: CommandBuffer) -> Result<(), String> {
        let queue = self.queue.as_ref().ok_or("GPU queue not initialized")?;
        queue.submit([buffer]);
        Ok(())
    }

    pub fn upload_ops_data(&mut self, ops: &[u8]) -> Result<(), String> {
        self.upload_data(ops, OPS_BUF_IDX)
    }

    pub fn upload_noise_metadata(&mut self, metadata: &[u8]) -> Result<(), String> {
        self.upload_data(metadata, CORRELATED_NOISE_TABLES_BUF_IDX)
    }

    pub fn upload_noise_entries(&mut self, entries: &[u8]) -> Result<(), String> {
        self.upload_data(entries, CORRELATED_NOISE_ENTRIES_BUF_IDX)
    }

    pub fn free_noise_buffers(&mut self) -> Result<(), String> {
        self.device_resources.bound_buffers[CORRELATED_NOISE_TABLES_BUF_IDX].buffer = None;
        self.device_resources.bound_buffers[CORRELATED_NOISE_ENTRIES_BUF_IDX].buffer = None;
        self.device_resources.bind_group = None; // Invalidate bind group to recreate later
        Ok(())
    }

    fn try_get_buffer(&self, buf_idx: usize) -> Result<&Buffer, String> {
        self.device_resources.bound_buffers[buf_idx]
            .buffer
            .as_ref()
            .ok_or_else(|| format!("Buffer at index {buf_idx} not initialized"))
    }

    // Verify that the per-run buffers are created and of sufficient size, recreating them if needed
    pub fn ensure_run_buffers(
        &mut self,
        shot_state_buffer_size: usize,
        state_vector_buffer_size: usize,
        results_buffer_size: usize,
        diagnostics_buffer_size: usize,
    ) -> Result<(), String> {
        let device = self.device.as_ref().ok_or("GPU device not initialized")?;

        let mut check_buffer = |idx: usize, required_size: usize| {
            let buf_binding = &mut self.device_resources.bound_buffers[idx];
            if let Some(ref buffer) = buf_binding.buffer
                && buffer.size() == required_size as u64
            {
                // Buffer is already the correct size, no need to recreate
            } else {
                let new_buffer =
                    create_dst_buffer(device, required_size, buf_binding.usage, buf_binding.name);
                buf_binding.buffer = Some(new_buffer);
                self.device_resources.bind_group = None; // Invalidate bind group to recreate later
            }
        };

        for (idx, size) in [
            (SHOT_STATE_BUF_IDX, shot_state_buffer_size),
            (STATE_VECTOR_BUF_IDX, state_vector_buffer_size),
            (RESULTS_BUF_IDX, results_buffer_size),
            (DIAGNOSTICS_BUF_IDX, diagnostics_buffer_size),
        ] {
            check_buffer(idx, size);
        }
        Ok(())
    }

    fn upload_data(&mut self, data: &[u8], buf_idx: usize) -> Result<(), String> {
        let device = self.device.as_ref().ok_or("GPU device not initialized")?;
        let queue = self.queue.as_ref().ok_or("GPU queue not initialized")?;

        let dst_buffer = &mut self.device_resources.bound_buffers[buf_idx];

        if let Some(ref buffer) = dst_buffer.buffer
            && buffer.size() == data.len() as u64
        {
            // Buffer is already the correct size, no need to recreate
            copy_data_to_gpu(device, queue, data, buffer)?;
        } else {
            let new_buffer =
                create_dst_buffer(device, data.len(), dst_buffer.usage, dst_buffer.name);
            copy_data_to_gpu(device, queue, data, &new_buffer)?;
            dst_buffer.buffer = Some(new_buffer);
            self.device_resources.bind_group = None; // Invalidate bind group to recreate later
        }
        Ok(())
    }

    pub async fn download_batch_results(&self) -> Result<(Vec<u32>, Box<DiagnosticsData>), String> {
        let device = self.device.as_ref().ok_or("GPU device not initialized")?;
        let results = self.try_get_buffer(RESULTS_BUF_IDX)?;
        let diagnostics = self.try_get_buffer(DIAGNOSTICS_BUF_IDX)?;

        let download = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Download buffer"),
            size: results.size() + diagnostics.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self.get_encoder("Download Command Encoder")?;

        // Copy the results and diagnostics to the download buffer
        encoder.copy_buffer_to_buffer(results, 0, &download, 0, results.size());
        encoder.copy_buffer_to_buffer(
            diagnostics,
            0,
            &download,
            results.size(),
            diagnostics.size(),
        );

        let command_buffer = encoder.finish();
        self.submit_command_buffer(command_buffer)?;

        // Now map and convert the data
        // Fetching the actual results is a real pain. For details, see:
        // https://github.com/gfx-rs/wgpu/blob/v26/examples/features/src/repeated_compute/mod.rs#L74

        // Cross-platform readback: async map + native poll
        let buffer_slice = download.slice(..);

        let (sender, receiver) = futures::channel::oneshot::channel();

        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            if let Err(ref e) = result {
                // NOTE: Should we just panic here? Or maybe let the receiver handle it?
                eprintln!("Buffer mapping failed: {e:?}");
            }
            let _ = sender.send(result);
        });

        // Block until all pending GPU work is complete
        device
            .poll(PollType::wait_indefinitely())
            .expect("GPU poll failed");

        // Await the mapping completion (which is a Result of a Result - check both)
        let map_result = receiver.await.expect("Failed to receive map completion");
        map_result.expect("Buffer mapping failed");

        // Read, copy out, and unmap.
        let data = buffer_slice.get_mapped_range();

        // Fetch results and diagnostics from the download buffer to return
        #[allow(clippy::cast_possible_truncation)]
        let results_bytes = results.size() as usize;
        let results_data = &data[..results_bytes];

        let diagnositcs_data = &data[results_bytes..];
        // Keep this on the heap to avoid large stack frames in debug builds.
        let diagnostics = Box::new(*from_bytes::<DiagnosticsData>(diagnositcs_data));
        let batch_results: Vec<u32> = cast_slice(results_data).to_vec();

        drop(data);
        download.unmap();

        Ok((batch_results, diagnostics))
    }
}

/// Strips out sections of code delimited by DX12-start-strip and DX12-end-strip comments
fn strip_dx12_sections(source: &str) -> String {
    let mut result = String::new();
    let mut in_strip_section = false;

    for line in source.lines() {
        if line.trim() == "// DX12-start-strip" {
            in_strip_section = true;
            continue;
        }
        if line.trim() == "// DX12-end-strip" {
            in_strip_section = false;
            continue;
        }
        if !in_strip_section {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

fn create_dst_buffer(
    device: &Device,
    buffer_size: usize,
    usage: BufferUsages,
    label: &str,
) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size: buffer_size as u64,
        usage,
        mapped_at_creation: false,
    })
}

fn copy_data_to_gpu(
    device: &Device,
    queue: &Queue,
    data: &[u8],
    target: &Buffer,
) -> Result<(), String> {
    let upload_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Tmp Upload Buffer"),
        size: data.len() as u64,
        usage: BufferUsages::COPY_SRC | BufferUsages::MAP_WRITE,
        mapped_at_creation: true,
    });

    upload_buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(data);
    upload_buffer.unmap();

    // Copy from the upload buffer to the GPU buffer
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Buffer Upload Copy Encoder"),
    });
    encoder.copy_buffer_to_buffer(&upload_buffer, 0, target, 0, data.len() as u64);
    queue.submit([encoder.finish()]);

    Ok(())
}
