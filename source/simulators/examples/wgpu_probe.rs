// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Spike (not for review): which wgpu adapters this machine offers, their
//! limits, and what a classical-GPU round trip and a state-vector gate cost.
//!
//! Run: `cargo run --release -p qdk_simulators --example wgpu_probe [backend]`
//! where backend is `all` (default), `gl`, `vulkan`.

use std::time::Instant;

const ROUND_TRIPS: usize = 200;
const BANDWIDTH_BYTES: u64 = 64 << 20;
const QUBIT_COUNTS: [u32; 5] = [4, 10, 16, 20, 22];
const GATES: u32 = 200;

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let backends = match which.as_str() {
        "gl" => wgpu::Backends::GL,
        "vulkan" => wgpu::Backends::VULKAN,
        _ => wgpu::Backends::all(),
    };
    futures::executor::block_on(run(backends));
    cpu_gates();
    // GL teardown over Mesa's d3d12 driver hangs at exit; skip it.
    std::process::exit(0);
}

async fn run(backends: wgpu::Backends) {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = backends;
    let instance = wgpu::Instance::new(desc);
    let adapters = instance.enumerate_adapters(backends).await;
    println!("{} adapter(s) for {backends:?}", adapters.len());
    for adapter in adapters {
        let info = adapter.get_info();
        let l = adapter.limits();
        println!(
            "\n== {} | {:?} | {:?} | driver {} {}",
            info.name, info.backend, info.device_type, info.driver, info.driver_info
        );
        println!(
            "   max_storage_buffer_binding_size {} MiB, max_buffer_size {} MiB, \
             workgroup storage {} KiB, storage buffers/stage {}, invocations/workgroup {}",
            l.max_storage_buffer_binding_size >> 20,
            l.max_buffer_size >> 20,
            l.max_compute_workgroup_storage_size >> 10,
            l.max_storage_buffers_per_shader_stage,
            l.max_compute_invocations_per_workgroup,
        );
        let f = adapter.features();
        println!(
            "   features: SHADER_INT64 {} SHADER_F64 {} SUBGROUP {} TIMESTAMP_QUERY {} MAPPABLE_PRIMARY_BUFFERS {}",
            f.contains(wgpu::Features::SHADER_INT64),
            f.contains(wgpu::Features::SHADER_F64),
            f.contains(wgpu::Features::SUBGROUP),
            f.contains(wgpu::Features::TIMESTAMP_QUERY),
            f.contains(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS),
        );
        if info.device_type == wgpu::DeviceType::Cpu {
            println!("   (CPU device: skipping timings)");
            continue;
        }
        if let Err(e) = measure(&adapter).await {
            println!("   measurement failed: {e}");
        }
    }
}

async fn measure(adapter: &wgpu::Adapter) -> Result<(), String> {
    let t = Instant::now();
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("probe"),
            required_limits: adapter.limits(),
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?;
    println!("   device: {:.1} ms", ms(t));

    round_trip(&device, &queue);
    bandwidth(&device, &queue);
    for q in QUBIT_COUNTS {
        gates(&device, &queue, q);
    }
    Ok(())
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

fn wait(device: &wgpu::Device) {
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");
}

fn read_back(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    wait(device);
    let out = slice.get_mapped_range().to_vec();
    buffer.unmap();
    out
}

fn storage(device: &wgpu::Device, size: u64, label: &str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn readback(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

fn pipeline(device: &wgpu::Device, src: &str) -> wgpu::ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("probe"),
        source: wgpu::ShaderSource::Wgsl(src.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("probe"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

/// The cost of one decision the classical side waits for: a dispatch whose
/// 4-byte result the CPU reads before the next one (a mid-circuit
/// measurement driving a branch, one round trip per shot and measurement).
fn round_trip(device: &wgpu::Device, queue: &wgpu::Queue) {
    let src = "@group(0) @binding(0) var<storage, read_write> d: array<u32>;
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  if (id.x < arrayLength(&d)) { d[id.x] = d[id.x] + 1u; }
}";
    let p = pipeline(device, src);
    let data = storage(device, 256, "rt");
    let rb = readback(device, 4);
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &p.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: data.as_entire_binding(),
        }],
    });
    let once = || {
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&p);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        enc.copy_buffer_to_buffer(&data, 0, &rb, 0, 4);
        queue.submit([enc.finish()]);
        u32::from_le_bytes(read_back(device, &rb)[..4].try_into().expect("4 bytes"))
    };
    let t = Instant::now();
    let first = once();
    let first_ms = ms(t);
    let t = Instant::now();
    let mut last = first;
    for _ in 0..ROUND_TRIPS {
        last = once();
    }
    let per = ms(t) / ROUND_TRIPS as f64;
    assert_eq!(last, first + ROUND_TRIPS as u32, "the counter counts every dispatch");
    println!("   round trip (dispatch + 4-byte read): first {first_ms:.2} ms, then {:.1} µs each", per * 1e3);
}

fn bandwidth(device: &wgpu::Device, queue: &wgpu::Queue) {
    let size = BANDWIDTH_BYTES.min(device.limits().max_buffer_size);
    let host = vec![7u8; usize::try_from(size).expect("fits")];
    let gpu = storage(device, size, "bw");
    let rb = readback(device, size);
    let t = Instant::now();
    queue.write_buffer(&gpu, 0, &host);
    queue.submit([]);
    wait(device);
    let up = ms(t);
    let t = Instant::now();
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_buffer_to_buffer(&gpu, 0, &rb, 0, size);
    queue.submit([enc.finish()]);
    let back = read_back(device, &rb);
    let down = ms(t);
    assert!(back.iter().all(|&b| b == 7), "the bytes come back");
    let gib = size as f64 / f64::from(1u32 << 30);
    println!(
        "   {} MiB: upload {up:.1} ms ({:.2} GiB/s), readback {down:.1} ms ({:.2} GiB/s)",
        size >> 20,
        gib / (up / 1e3),
        gib / (down / 1e3)
    );
}

/// A 2x2 gate on a 2^QUBITS state vector of complex f32, one dispatch per
/// gate, all in one submission: the device's throughput when the classical
/// side does not wait in between.
fn gates(device: &wgpu::Device, queue: &wgpu::Queue, qubits: u32) {
    let src = "struct G { m: mat2x4<f32>, target_bit: u32 }
@group(0) @binding(0) var<storage, read_write> s: array<vec2<f32>>;
@group(0) @binding(1) var<uniform> g: G;
fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> { return vec2(a.x*b.x - a.y*b.y, a.x*b.y + a.y*b.x); }
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) n: vec3<u32>) {
  let k = id.x + id.y * n.x * 64u;
  let half = arrayLength(&s) / 2u;
  if (k >= half) { return; }
  let low = k & ((1u << g.target_bit) - 1u);
  let i0 = ((k >> g.target_bit) << (g.target_bit + 1u)) | low;
  let i1 = i0 | (1u << g.target_bit);
  let a = s[i0]; let b = s[i1];
  s[i0] = cmul(g.m[0].xy, a) + cmul(g.m[0].zw, b);
  s[i1] = cmul(g.m[1].xy, a) + cmul(g.m[1].zw, b);
}";
    let p = pipeline(device, src);
    let n = 1u64 << qubits;
    let state = storage(device, n * 8, "state");
    let mut init = vec![0f32; usize::try_from(n * 2).expect("fits")];
    init[0] = 1.0;
    queue.write_buffer(&state, 0, bytemuck::cast_slice(&init));
    let align = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(48);
    let h = std::f32::consts::FRAC_1_SQRT_2;
    let mut params = vec![0u8; usize::try_from(align * u64::from(GATES)).expect("fits")];
    for i in 0..GATES {
        let at = usize::try_from(align * u64::from(i)).expect("fits");
        let m: [f32; 8] = [h, 0.0, h, 0.0, h, 0.0, -h, 0.0];
        params[at..at + 32].copy_from_slice(bytemuck::cast_slice(&m));
        params[at + 32..at + 36].copy_from_slice(&(i % qubits).to_le_bytes());
    }
    let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gates"),
        size: params.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&ubuf, 0, &params);
    let bgs: Vec<_> = (0..GATES)
        .map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &p.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: state.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &ubuf,
                            offset: align * u64::from(i),
                            size: std::num::NonZeroU64::new(48),
                        }),
                    },
                ],
            })
        })
        .collect();
    let groups = u32::try_from((n / 2).div_ceil(64)).expect("fits");
    let max = device.limits().max_compute_workgroups_per_dimension;
    let (gx, gy) = if groups <= max { (groups, 1) } else { (max, groups.div_ceil(max)) };
    let rb = readback(device, 8);
    let run = |count: u32| {
        let mut enc = device.create_command_encoder(&Default::default());
        for bg in &bgs[..count as usize] {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&p);
            pass.set_bind_group(0, bg, &[]);
            pass.dispatch_workgroups(gx, gy, 1);
        }
        enc.copy_buffer_to_buffer(&state, 0, &rb, 0, 8);
        queue.submit([enc.finish()]);
        read_back(device, &rb)
    };
    run(1);
    // Twice H on every qubit is the identity, so amplitude 0 returns to 1.
    queue.write_buffer(&state, 0, bytemuck::cast_slice(&init));
    let t = Instant::now();
    let back = run(GATES);
    let total = ms(t);
    let a0: [f32; 2] = bytemuck::pod_read_unaligned(&back[..8]);
    println!(
        "   {GATES} gates on {qubits} qubits in one submission: {total:.1} ms, {:.1} µs/gate; amplitude 0 = {:?} (expect ~[1, 0])",
        total * 1e3 / f64::from(GATES),
        a0
    );
}

/// The same H gates on one CPU core, as a reference for the GPU's figures.
fn cpu_gates() {
    println!("\n== CPU, one core");
    let h = std::f32::consts::FRAC_1_SQRT_2;
    for qubits in QUBIT_COUNTS {
        let n = 1usize << qubits;
        let mut s = vec![[0f32; 2]; n];
        s[0] = [1.0, 0.0];
        let t = Instant::now();
        for i in 0..GATES {
            let bit = 1usize << (i % qubits);
            for i0 in (0..n).filter(|k| k & bit == 0) {
                let (a, b) = (s[i0], s[i0 | bit]);
                s[i0] = [h * (a[0] + b[0]), h * (a[1] + b[1])];
                s[i0 | bit] = [h * (a[0] - b[0]), h * (a[1] - b[1])];
            }
        }
        let total = ms(t);
        println!(
            "   {GATES} gates on {qubits} qubits: {total:.1} ms, {:.1} µs/gate; amplitude 0 = {:?}",
            total * 1e3 / f64::from(GATES),
            s[0]
        );
    }
}
