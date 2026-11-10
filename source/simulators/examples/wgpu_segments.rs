// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Spike (not for review): what a QIR-machine-driven GPU engine would cost.
//!
//! The classical side (control flow, sampling) stays on the CPU; the GPU holds
//! the state vectors of all shots, which advance in lockstep. All gates up to
//! the next measurement go in one submission, and each measurement costs one
//! small readback. The workload matches `bench.py`: n qubits, 8 rounds of
//! (Rx on every qubit, a CZ chain, `MResetZ` on q0, X on q1 if it gave One).
//!
//! Strategies: A = one pass per gate; B = the CZ chain as one diagonal pass,
//! and reset + conditional X fused; C = B with the Rx gates on the low 10
//! qubits applied in one pass through workgroup memory.
//!
//! Run: `cargo run --release -p qdk_simulators --example wgpu_segments -- 12,1024 20,1`

use std::time::Instant;

const ROUNDS: usize = 8;
const WG: u32 = 256;
const BLOCK_QUBITS: u32 = 10;
const SLOT: u64 = 512;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Strategy {
    PerGate,
    Fused,
    Blocked,
}

fn angle(q: u32, r: usize) -> f64 {
    0.3 + 0.01 * f64::from(q + u32::try_from(r).expect("small"))
}

fn rx(theta: f64) -> [f32; 8] {
    let (s, c) = ((theta / 2.0).sin() as f32, (theta / 2.0).cos() as f32);
    [c, 0.0, 0.0, -s, 0.0, -s, c, 0.0]
}

const HEAD: &str = "struct G { m: mat2x4<f32>, target_bit: u32, nq: u32, p0: u32, p1: u32 }
fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> { return vec2(a.x*b.x - a.y*b.y, a.x*b.y + a.y*b.x); }
";

const GATE1: &str = "@group(0) @binding(0) var<storage, read_write> s: array<vec2<f32>>;
@group(0) @binding(1) var<uniform> g: G;
@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) n: vec3<u32>) {
  let k = id.x + id.y * n.x * 256u;
  if (k >= arrayLength(&s) / 2u) { return; }
  let low = k & ((1u << g.target_bit) - 1u);
  let i0 = ((k >> g.target_bit) << (g.target_bit + 1u)) | low;
  let i1 = i0 | (1u << g.target_bit);
  let a = s[i0]; let b = s[i1];
  s[i0] = cmul(g.m[0].xy, a) + cmul(g.m[0].zw, b);
  s[i1] = cmul(g.m[1].xy, a) + cmul(g.m[1].zw, b);
}";

// One CZ between target_bit and target_bit + 1 (strategy A).
const CZ1: &str = "@group(0) @binding(0) var<storage, read_write> s: array<vec2<f32>>;
@group(0) @binding(1) var<uniform> g: G;
@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) n: vec3<u32>) {
  let k = id.x + id.y * n.x * 256u;
  if (k >= arrayLength(&s)) { return; }
  let both = (3u << g.target_bit);
  if ((k & both) == both) { s[k] = -s[k]; }
}";

// The whole CZ chain on qubits 0..nq-1 as one diagonal (strategies B, C).
const CZCHAIN: &str = "@group(0) @binding(0) var<storage, read_write> s: array<vec2<f32>>;
@group(0) @binding(1) var<uniform> g: G;
@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) n: vec3<u32>) {
  let k = id.x + id.y * n.x * 256u;
  if (k >= arrayLength(&s)) { return; }
  let i = k & ((1u << g.nq) - 1u);
  let pairs = i & (i >> 1u) & ((1u << (g.nq - 1u)) - 1u);
  if ((countOneBits(pairs) & 1u) == 1u) { s[k] = -s[k]; }
}";

// Per-shot partial sums of P(q0 = 1); 4096 amplitudes per workgroup.
const PROB: &str = "@group(0) @binding(0) var<storage, read_write> s: array<vec2<f32>>;
@group(0) @binding(1) var<uniform> g: G;
@group(0) @binding(2) var<storage, read_write> partial: array<f32>;
var<workgroup> acc: array<f32, 256>;
@compute @workgroup_size(256) fn main(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) n: vec3<u32>) {
  let w = wid.x + wid.y * n.x;
  let half = 1u << (g.nq - 1u);
  let per_shot = max(1u, half / 4096u);
  let shot = w / per_shot;
  let chunk = w % per_shot;
  var sum = 0.0;
  for (var j = 0u; j < 16u; j++) {
    let e = chunk * 4096u + j * 256u + lid.x;
    if (e < half) {
      let a = s[(shot << g.nq) + ((e << 1u) | 1u)];
      sum += dot(a, a);
    }
  }
  acc[lid.x] = sum;
  workgroupBarrier();
  for (var stride = 128u; stride > 0u; stride >>= 1u) {
    if (lid.x < stride) { acc[lid.x] += acc[lid.x + stride]; }
    workgroupBarrier();
  }
  if (lid.x == 0u) { partial[w] = acc[0]; }
}";

// MResetZ on q0 with the sampled outcome, then renormalize (strategy A).
const COLLAPSE: &str = "@group(0) @binding(0) var<storage, read_write> s: array<vec2<f32>>;
@group(0) @binding(1) var<uniform> g: G;
@group(0) @binding(2) var<storage, read_write> flags: array<vec4<f32>>;
@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) n: vec3<u32>) {
  let k = id.x + id.y * n.x * 256u;
  if (k >= arrayLength(&s) / 2u) { return; }
  let f = flags[k >> (g.nq - 1u)];
  let i0 = k << 1u;
  let i1 = i0 | 1u;
  s[i0] = select(s[i0], s[i1], f.x > 0.5) * f.y;
  s[i1] = vec2(0.0, 0.0);
}";

// X on q1 for the shots whose outcome was One (strategy A).
const CONDX: &str = "@group(0) @binding(0) var<storage, read_write> s: array<vec2<f32>>;
@group(0) @binding(1) var<uniform> g: G;
@group(0) @binding(2) var<storage, read_write> flags: array<vec4<f32>>;
@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) n: vec3<u32>) {
  let k = id.x + id.y * n.x * 256u;
  if (k >= arrayLength(&s) / 2u) { return; }
  if (flags[k >> (g.nq - 1u)].x < 0.5) { return; }
  let i0 = ((k >> 1u) << 2u) | (k & 1u);
  let i1 = i0 | 2u;
  let a = s[i0];
  s[i0] = s[i1];
  s[i1] = a;
}";

// MResetZ on q0, renormalize and X on q1 if One, in one pass (B, C).
const QUAD: &str = "@group(0) @binding(0) var<storage, read_write> s: array<vec2<f32>>;
@group(0) @binding(1) var<uniform> g: G;
@group(0) @binding(2) var<storage, read_write> flags: array<vec4<f32>>;
@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) n: vec3<u32>) {
  let k = id.x + id.y * n.x * 256u;
  if (k >= arrayLength(&s) / 4u) { return; }
  let f = flags[k >> (g.nq - 2u)];
  let one = f.x > 0.5;
  let i = k << 2u;
  let x0 = select(s[i], s[i | 1u], one) * f.y;
  let x1 = select(s[i | 2u], s[i | 3u], one) * f.y;
  s[i] = select(x0, x1, one);
  s[i | 2u] = select(x1, x0, one);
  s[i | 1u] = vec2(0.0, 0.0);
  s[i | 3u] = vec2(0.0, 0.0);
}";

// The Rx gates on the low 10 qubits of 1024 amplitudes in workgroup memory (C).
const BLOCK: &str = "struct B { m: array<mat2x4<f32>, 10> }
@group(0) @binding(0) var<storage, read_write> s: array<vec2<f32>>;
@group(0) @binding(1) var<uniform> b: B;
var<workgroup> tile: array<vec2<f32>, 1024>;
@compute @workgroup_size(256) fn main(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) n: vec3<u32>) {
  let base = (wid.x + wid.y * n.x) * 1024u;
  for (var j = 0u; j < 4u; j++) { tile[lid.x + j * 256u] = s[base + lid.x + j * 256u]; }
  workgroupBarrier();
  for (var q = 0u; q < 10u; q++) {
    let m = b.m[q];
    for (var j = 0u; j < 2u; j++) {
      let p = lid.x + j * 256u;
      let low = p & ((1u << q) - 1u);
      let i0 = ((p >> q) << (q + 1u)) | low;
      let i1 = i0 | (1u << q);
      let x = tile[i0]; let y = tile[i1];
      tile[i0] = cmul(m[0].xy, x) + cmul(m[0].zw, y);
      tile[i1] = cmul(m[1].xy, x) + cmul(m[1].zw, y);
    }
    workgroupBarrier();
  }
  for (var j = 0u; j < 4u; j++) { s[base + lid.x + j * 256u] = tile[lid.x + j * 256u]; }
}";

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    gate1: wgpu::ComputePipeline,
    cz1: wgpu::ComputePipeline,
    czchain: wgpu::ComputePipeline,
    prob: wgpu::ComputePipeline,
    collapse: wgpu::ComputePipeline,
    condx: wgpu::ComputePipeline,
    quad: wgpu::ComputePipeline,
    block: wgpu::ComputePipeline,
}

struct Outcome {
    seconds: f64,
    /// P(q0 = 1) per round and shot, as the GPU computed it.
    p1: Vec<Vec<f64>>,
    /// The sampled outcomes per round and shot.
    ones: Vec<Vec<bool>>,
}

fn main() {
    let specs: Vec<(u32, u32)> = std::env::args()
        .skip(1)
        .map(|a| {
            let (n, s) = a.split_once(',').expect("n,shots");
            (n.parse().expect("n"), s.parse().expect("shots"))
        })
        .collect();
    let gpu = futures::executor::block_on(Gpu::new());
    check_against_reference(&gpu);
    for (n, shots) in specs {
        for strategy in [Strategy::PerGate, Strategy::Fused, Strategy::Blocked] {
            let _warm = gpu.run(strategy, n, shots, None);
            let best = (0..3)
                .map(|_| gpu.run(strategy, n, shots, None).seconds)
                .fold(f64::INFINITY, f64::min);
            println!(
                "segments {strategy:?} n={n} R={ROUNDS} shots={shots} best={best:.4}s per_shot={:.3}ms",
                best / f64::from(shots) * 1e3
            );
        }
    }
    // GL teardown over Mesa's d3d12 driver hangs at exit; skip it.
    std::process::exit(0);
}

/// Every strategy must agree with an f64 CPU simulation that is fed the same
/// outcomes, round by round, before any timing is trusted.
fn check_against_reference(gpu: &Gpu) {
    let (n, shots) = (12, 4);
    for strategy in [Strategy::PerGate, Strategy::Fused, Strategy::Blocked] {
        let out = gpu.run(strategy, n, shots, Some(7));
        let cpu = reference(n, &out.ones);
        let mut worst = 0f64;
        for (g, c) in out.p1.iter().zip(&cpu) {
            for (a, b) in g.iter().zip(c) {
                worst = worst.max((a - b).abs());
            }
        }
        assert!(worst < 1e-4, "{strategy:?} disagrees with the CPU: {worst}");
        println!("check {strategy:?} n={n} shots={shots}: max |dP| = {worst:.2e}");
    }
}

fn reference(n: u32, ones: &[Vec<bool>]) -> Vec<Vec<f64>> {
    let shots = ones[0].len();
    let dim = 1usize << n;
    let mut all = vec![vec![0f64; shots]; ROUNDS];
    for shot in 0..shots {
        let mut s = vec![[0f64; 2]; dim];
        s[0] = [1.0, 0.0];
        for r in 0..ROUNDS {
            for q in 0..n {
                let (sn, c) = (angle(q, r) / 2.0).sin_cos();
                let bit = 1usize << q;
                for i0 in (0..dim).filter(|k| k & bit == 0) {
                    let (a, b) = (s[i0], s[i0 | bit]);
                    s[i0] = [c * a[0] + sn * b[1], c * a[1] - sn * b[0]];
                    s[i0 | bit] = [sn * a[1] + c * b[0], -sn * a[0] + c * b[1]];
                }
            }
            for q in 0..n - 1 {
                let both = 3usize << q;
                for (i, a) in s.iter_mut().enumerate() {
                    if i & both == both {
                        *a = [-a[0], -a[1]];
                    }
                }
            }
            let p1: f64 = s.iter().skip(1).step_by(2).map(|a| a[0] * a[0] + a[1] * a[1]).sum();
            all[r][shot] = p1;
            let one = ones[r][shot];
            let inv = 1.0 / (if one { p1 } else { 1.0 - p1 }).sqrt();
            for i in (0..dim).step_by(2) {
                let keep = if one { s[i + 1] } else { s[i] };
                s[i] = [keep[0] * inv, keep[1] * inv];
                s[i + 1] = [0.0, 0.0];
            }
            if one {
                for i in (0..dim).filter(|k| k & 2 == 0) {
                    s.swap(i, i | 2);
                }
            }
        }
    }
    all
}

impl Gpu {
    async fn new() -> Self {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::all();
        let instance = wgpu::Instance::new(desc);
        let adapter = instance
            .enumerate_adapters(wgpu::Backends::all())
            .await
            .into_iter()
            .find(|a| a.get_info().device_type != wgpu::DeviceType::Cpu)
            .expect("a GPU adapter");
        println!("adapter: {:?}", adapter.get_info().name);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("segments"),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .expect("device");
        let t = Instant::now();
        let make = |body: &str| pipeline(&device, &format!("{HEAD}{body}"));
        let gpu = Self {
            gate1: make(GATE1),
            cz1: make(CZ1),
            czchain: make(CZCHAIN),
            prob: make(PROB),
            collapse: make(COLLAPSE),
            condx: make(CONDX),
            quad: make(QUAD),
            block: make(BLOCK),
            device,
            queue,
        };
        println!("pipelines created in {:.1} ms", ms(t));
        gpu
    }

    #[allow(clippy::too_many_lines)]
    fn run(&self, strategy: Strategy, n: u32, shots: u32, record: Option<u64>) -> Outcome {
        let d = &self.device;
        let amps = u64::from(shots) << n;
        let state = storage(d, amps * 8);
        let mut init = vec![0f32; usize::try_from(amps * 2).expect("fits")];
        for shot in 0..shots {
            init[usize::try_from(u64::from(shot) << (n + 1)).expect("fits")] = 1.0;
        }
        let per_shot = ((1u32 << (n - 1)) / 4096).max(1);
        let partial = storage(d, u64::from(shots * per_shot) * 4);
        let rb = readback(d, u64::from(shots * per_shot) * 4);
        let flags = storage(d, u64::from(shots) * 16);

        // Uniform slots: [0] = n only; then one per (round, qubit) Rx; then one
        // per qubit for single CZs; then one per round for the block's 10 Rx.
        let gate_slot = |r: usize, q: u32| 1 + r as u64 * u64::from(n) + u64::from(q);
        let cz_slot = |q: u32| 1 + (ROUNDS as u64) * u64::from(n) + u64::from(q);
        let block_slot = |r: usize| 1 + (ROUNDS as u64 + 1) * u64::from(n) + r as u64;
        let slots = block_slot(ROUNDS);
        let mut bytes = vec![0u8; usize::try_from(slots * SLOT).expect("fits")];
        let put = |bytes: &mut [u8], slot: u64, m: [f32; 8], target: u32| {
            let at = usize::try_from(slot * SLOT).expect("fits");
            bytes[at..at + 32].copy_from_slice(bytemuck::cast_slice(&m));
            bytes[at + 32..at + 36].copy_from_slice(&target.to_le_bytes());
            bytes[at + 36..at + 40].copy_from_slice(&n.to_le_bytes());
        };
        put(&mut bytes, 0, [0.0; 8], 0);
        for r in 0..ROUNDS {
            for q in 0..n {
                put(&mut bytes, gate_slot(r, q), rx(angle(q, r)), q);
            }
            let at = usize::try_from(block_slot(r) * SLOT).expect("fits");
            for q in 0..BLOCK_QUBITS {
                let o = at + 32 * q as usize;
                bytes[o..o + 32].copy_from_slice(bytemuck::cast_slice(&rx(angle(q, r))));
            }
        }
        for q in 0..n - 1 {
            put(&mut bytes, cz_slot(q), [0.0; 8], q);
        }
        let uniforms = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&uniforms, 0, &bytes);
        let bind = |p: &wgpu::ComputePipeline, slot: u64, size: u64, extra: Option<&wgpu::Buffer>| {
            let mut entries = vec![
                wgpu::BindGroupEntry { binding: 0, resource: state.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniforms,
                        offset: slot * SLOT,
                        size: std::num::NonZeroU64::new(size),
                    }),
                },
            ];
            if let Some(b) = extra {
                entries.push(wgpu::BindGroupEntry { binding: 2, resource: b.as_entire_binding() });
            }
            d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &p.get_bind_group_layout(0),
                entries: &entries,
            })
        };
        let gate_bgs: Vec<Vec<_>> = (0..ROUNDS)
            .map(|r| (0..n).map(|q| bind(&self.gate1, gate_slot(r, q), 48, None)).collect())
            .collect();
        let cz_bgs: Vec<_> = (0..n - 1).map(|q| bind(&self.cz1, cz_slot(q), 48, None)).collect();
        let block_bgs: Vec<_> = (0..ROUNDS).map(|r| bind(&self.block, block_slot(r), 320, None)).collect();
        let chain_bg = bind(&self.czchain, 0, 48, None);
        let prob_bg = bind(&self.prob, 0, 48, Some(&partial));
        let collapse_bg = bind(&self.collapse, 0, 48, Some(&flags));
        let condx_bg = bind(&self.condx, 0, 48, Some(&flags));
        let quad_bg = bind(&self.quad, 0, 48, Some(&flags));

        let mut rng: Vec<u64> = (0..shots)
            .map(|s| (u64::from(s) + record.unwrap_or(1)).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
            .collect();
        let mut p1s = Vec::new();
        let mut ones_all = Vec::new();
        let max = d.limits().max_compute_workgroups_per_dimension;
        let shape = |groups: u64| {
            let groups = u32::try_from(groups).expect("fits");
            if groups <= max { (groups, 1) } else { (max, groups.div_ceil(max)) }
        };
        let threads = |count: u64| shape(count.div_ceil(u64::from(WG)));

        self.queue.write_buffer(&state, 0, bytemuck::cast_slice(&init));
        self.queue.submit([]);
        wait(d);
        let started = Instant::now();
        for r in 0..=ROUNDS {
            let mut enc = d.create_command_encoder(&Default::default());
            let mut pass_with = |p: &wgpu::ComputePipeline, bg: &wgpu::BindGroup, dims: (u32, u32)| {
                let mut pass = enc.begin_compute_pass(&Default::default());
                pass.set_pipeline(p);
                pass.set_bind_group(0, bg, &[]);
                pass.dispatch_workgroups(dims.0, dims.1, 1);
            };
            if r > 0 {
                if strategy == Strategy::PerGate {
                    pass_with(&self.collapse, &collapse_bg, threads(amps / 2));
                    pass_with(&self.condx, &condx_bg, threads(amps / 2));
                } else {
                    pass_with(&self.quad, &quad_bg, threads(amps / 4));
                }
            }
            if r < ROUNDS {
                let first_gate = if strategy == Strategy::Blocked {
                    pass_with(&self.block, &block_bgs[r], shape(amps / 1024));
                    BLOCK_QUBITS
                } else {
                    0
                };
                for q in first_gate..n {
                    pass_with(&self.gate1, &gate_bgs[r][q as usize], threads(amps / 2));
                }
                if strategy == Strategy::PerGate {
                    for bg in &cz_bgs {
                        pass_with(&self.cz1, bg, threads(amps));
                    }
                } else {
                    pass_with(&self.czchain, &chain_bg, threads(amps));
                }
                pass_with(&self.prob, &prob_bg, shape(u64::from(shots * per_shot)));
                enc.copy_buffer_to_buffer(&partial, 0, &rb, 0, u64::from(shots * per_shot) * 4);
            }
            self.queue.submit([enc.finish()]);
            if r == ROUNDS {
                wait(d);
                break;
            }
            let back = read_back(d, &rb);
            let partials: Vec<f32> = bytemuck::pod_collect_to_vec(&back);
            let mut p1 = Vec::new();
            let mut ones = Vec::new();
            let mut f = vec![0f32; shots as usize * 4];
            for shot in 0..shots as usize {
                let p: f64 = partials[shot * per_shot as usize..(shot + 1) * per_shot as usize]
                    .iter()
                    .map(|&x| f64::from(x))
                    .sum();
                let x = &mut rng[shot];
                *x ^= *x << 13;
                *x ^= *x >> 7;
                *x ^= *x << 17;
                let one = ((*x >> 11) as f64 / (1u64 << 53) as f64) < p;
                f[shot * 4] = if one { 1.0 } else { 0.0 };
                f[shot * 4 + 1] = (1.0 / (if one { p } else { 1.0 - p }).sqrt()) as f32;
                p1.push(p);
                ones.push(one);
            }
            self.queue.write_buffer(&flags, 0, bytemuck::cast_slice(&f));
            p1s.push(p1);
            ones_all.push(ones);
        }
        Outcome { seconds: started.elapsed().as_secs_f64(), p1: p1s, ones: ones_all }
    }
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

fn wait(device: &wgpu::Device) {
    device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
}

fn read_back(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    wait(device);
    let out = slice.get_mapped_range().to_vec();
    buffer.unmap();
    out
}

fn storage(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn readback(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

fn pipeline(device: &wgpu::Device, src: &str) -> wgpu::ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(src.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}
