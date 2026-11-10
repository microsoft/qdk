// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Spike (not for review): a [`QirResumer`] whose shots' state vectors live
//! on a GPU (wgpu), driven by the real [`QirMachine`] ([`QirGpuResumer`]).
//!
//! All shots of a run (or of a chunk, so that a chunk's state fits one
//! storage binding of 128 MiB) live in one buffer and advance in lockstep:
//! each round, every running shot is resumed to its next stop; shots whose
//! batches are equal form a group, and each group's batch is translated into
//! GPU passes that run on that group's shots only (a shot-index list). The
//! passes of all groups of a round go in one submission; measurements are
//! sampled on the GPU from a per-shot generator seeded by the shot's id, so a
//! round costs at most one readback (its outcomes), whatever number of
//! measurements its batches hold.
//!
//! The translation of a batch fuses (each a [`Strategy`] switch, so that its
//! effect can be measured): diagonal gates into one pass (`fuse_diag`); gates
//! on the low qubits into one pass through workgroup memory (`block`); and
//! one-qubit gates on one qubit into one matrix (`merge_1q`). A gate moves
//! past the earlier gates it commutes with (disjoint qubits, or both
//! diagonal) to join a pass.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::time::{Duration, Instant};

use num_complex::Complex64;
use qdk_simulators::OutputRecord;
use qdk_simulators::qir::{
    OperatorMatrix, QirCall, QirMachine, QirOutcome, QirReadout, QirResumeError, QirResumer,
    QirShotContext, QirShotId, QirStop,
};

/// The largest state per binding: 128 MiB of complex f32.
const MAX_AMPLITUDES_LOG2: u32 = 24;
/// The workgroup-memory tile of the block pass: 2^10 amplitudes.
const BLOCK_QUBITS: u32 = 10;
const WG: u64 = 256;
/// The bytes of a pass's parameter slot (dynamic uniform offset).
const SLOT: u64 = 256;
const PARAMS_BYTES: u64 = 80;
const NO_RESULT: u32 = u32::MAX;

/// What fails in the resumer's own work: a call it does not support.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuError(pub String);

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which fusions the translation of a batch makes, and how many shots a
/// chunk holds at most.
#[derive(Clone, Copy, Debug)]
pub struct Strategy {
    pub fuse_diag: bool,
    pub block: bool,
    pub merge_1q: bool,
    pub max_slots: u32,
    /// Whether translated batches are kept, by their calls, for later
    /// rounds and runs.
    pub cache: bool,
    /// Whether the prefix and suffix that all groups' batches share run
    /// once on all their shots.
    pub share: bool,
    /// Whether one-qubit gates on two qubits merge into one two-qubit pass.
    pub kron: bool,
}

impl Strategy {
    pub const ALL: Self = Self {
        fuse_diag: true,
        block: true,
        merge_1q: true,
        max_slots: u32::MAX,
        cache: true,
        share: true,
        kron: true,
    };
    pub const PER_GATE: Self = Self {
        fuse_diag: false,
        block: false,
        merge_1q: false,
        max_slots: u32::MAX,
        cache: true,
        share: false,
        kron: false,
    };
    pub const FUSED: Self = Self {
        fuse_diag: true,
        block: false,
        merge_1q: true,
        max_slots: u32::MAX,
        cache: true,
        share: true,
        kron: true,
    };

    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "blocked" | "all" => Self::ALL,
            "perpass" => Self::PER_GATE,
            "fused" => Self::FUSED,
            "single" => Self {
                max_slots: 1,
                ..Self::ALL
            },
            "nocache" => Self {
                cache: false,
                ..Self::ALL
            },
            "noshare" => Self {
                share: false,
                ..Self::ALL
            },
            "nokron" => Self {
                kron: false,
                ..Self::ALL
            },
            _ => return None,
        })
    }
}

/// Where the time of the runs went, summed over runs.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    /// Resuming contexts (the machine: the program's classical part).
    pub machine: Duration,
    /// Grouping shots by batch, and translating each group's batch into
    /// passes and their parameters.
    pub translate: Duration,
    /// Growing and writing the round's buffers (params, ops, shot lists).
    pub upload: Duration,
    /// Encoding the passes, and submitting (and a chunk's set-up).
    pub encode: Duration,
    /// Waiting for the round's outcomes (GPU work and the readback).
    pub wait: Duration,
    /// Waiting for the GPU at the end of a chunk (no outcomes to read).
    pub drain: Duration,
    pub rounds: u64,
    pub groups: u64,
    pub passes: u64,
    pub readbacks: u64,
    pub chunks: u64,
}

impl Stats {
    pub fn total(&self) -> Duration {
        self.machine + self.translate + self.upload + self.encode + self.wait + self.drain
    }

    /// The stats of the work done since `earlier`.
    pub fn since(&self, earlier: &Self) -> Self {
        Self {
            machine: self.machine - earlier.machine,
            translate: self.translate - earlier.translate,
            upload: self.upload - earlier.upload,
            encode: self.encode - earlier.encode,
            wait: self.wait - earlier.wait,
            drain: self.drain - earlier.drain,
            rounds: self.rounds - earlier.rounds,
            groups: self.groups - earlier.groups,
            passes: self.passes - earlier.passes,
            readbacks: self.readbacks - earlier.readbacks,
            chunks: self.chunks - earlier.chunks,
        }
    }
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        write!(
            f,
            "machine {:.2} ms, translate {:.2} ms, upload {:.2} ms, encode {:.2} ms, wait {:.2} ms, \
             drain {:.2} ms; {} chunks, {} rounds, {} groups, {} passes, {} readbacks",
            ms(self.machine),
            ms(self.translate),
            ms(self.upload),
            ms(self.encode),
            ms(self.wait),
            ms(self.drain),
            self.chunks,
            self.rounds,
            self.groups,
            self.passes,
            self.readbacks
        )
    }
}

/// The device, its pipelines and their bind group layout: made once, shared
/// by every resumer made on it.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    init: wgpu::ComputePipeline,
    gate1: wgpu::ComputePipeline,
    gate2: wgpu::ComputePipeline,
    diag: wgpu::ComputePipeline,
    block: wgpu::ComputePipeline,
    prob: wgpu::ComputePipeline,
    sample: wgpu::ComputePipeline,
    collapse: wgpu::ComputePipeline,
    /// Seconds to create the pipelines, and to compile them at a first
    /// dispatch each (Mesa compiles lazily).
    pub create_seconds: f64,
    pub compile_seconds: f64,
    pub adapter_name: String,
}

impl Gpu {
    pub fn new() -> Rc<Self> {
        futures::executor::block_on(Self::make())
    }

    async fn make() -> Rc<Self> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::all();
        let instance = wgpu::Instance::new(desc);
        let adapter = instance
            .enumerate_adapters(wgpu::Backends::all())
            .await
            .into_iter()
            .find(|a| a.get_info().device_type != wgpu::DeviceType::Cpu)
            .expect("a GPU adapter (run with GALLIUM_DRIVER=d3d12)");
        let adapter_name = adapter.get_info().name;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("qir-gpu-resumer"),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .expect("device");
        assert!(u64::from(device.limits().min_uniform_buffer_offset_alignment) <= SLOT);
        let started = Instant::now();
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("resumer"),
            entries: &[
                storage(0, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: std::num::NonZeroU64::new(PARAMS_BYTES),
                    },
                    count: None,
                },
                storage(2, true),
                storage(3, true),
                storage(4, false),
                storage(5, false),
                storage(6, false),
                storage(7, false),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("resumer"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kernels"),
            source: wgpu::ShaderSource::Wgsl(include_str!("kernels.wgsl").into()),
        });
        let make = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let mut gpu = Self {
            init: make("init"),
            gate1: make("gate1"),
            gate2: make("gate2"),
            diag: make("diag"),
            block: make("block"),
            prob: make("prob"),
            sample: make("sample"),
            collapse: make("collapse"),
            create_seconds: started.elapsed().as_secs_f64(),
            compile_seconds: 0.0,
            adapter_name,
            device,
            queue,
            layout,
        };
        gpu.compile_seconds = gpu.warm_up();
        Rc::new(gpu)
    }

    /// Dispatches every pipeline once, on no shot, so that the driver
    /// compiles them now; gives the seconds it took.
    fn warm_up(&self) -> f64 {
        let started = Instant::now();
        let mut buffers = Buffers::new(&self.device, &self.layout);
        buffers.ensure(&self.device, &self.layout, 4, 4, 4, 64, 4, 4);
        let mut params = vec![0u8; usize::try_from(SLOT).expect("small")];
        let p = Params {
            n: 2,
            per_shot: 1,
            block: 2,
            ..Params::default()
        };
        p.write(&mut params, 0);
        self.queue.write_buffer(&buffers.uniforms, 0, &params);
        for (name, pipeline) in [
            ("init", &self.init),
            ("gate1", &self.gate1),
            ("gate2", &self.gate2),
            ("diag", &self.diag),
            ("block", &self.block),
            ("prob", &self.prob),
            ("sample", &self.sample),
            ("collapse", &self.collapse),
        ] {
            let t = Instant::now();
            let mut encoder = self.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, buffers.bind_group.as_ref(), &[0]);
                pass.dispatch_workgroups(1, 1, 1);
            }
            self.queue.submit([encoder.finish()]);
            wait(&self.device);
            println!("  compile {name}: {:.3} s", t.elapsed().as_secs_f64());
        }
        started.elapsed().as_secs_f64()
    }

    fn shape(&self, groups: u64) -> (u32, u32) {
        let max = u64::from(self.device.limits().max_compute_workgroups_per_dimension);
        let groups = groups.max(1);
        if groups <= max {
            (u32::try_from(groups).expect("fits"), 1)
        } else {
            (
                u32::try_from(max).expect("fits"),
                u32::try_from(groups.div_ceil(max)).expect("fits"),
            )
        }
    }
}

/// The parameters of one pass, as the kernels' `P` reads them.
#[derive(Clone, Copy, Default)]
struct Params {
    m0: [f32; 4],
    m1: [f32; 4],
    n: u32,
    q0: u32,
    q1: u32,
    shot_off: u32,
    shot_len: u32,
    op_off: u32,
    op_len: u32,
    kind: u32,
    meas: u32,
    slots: u32,
    per_shot: u32,
    block: u32,
}

impl Params {
    fn write(&self, bytes: &mut [u8], at: usize) {
        let mut words = [0u32; 20];
        for (i, x) in self.m0.iter().chain(&self.m1).enumerate() {
            words[i] = x.to_bits();
        }
        words[8..20].copy_from_slice(&[
            self.n,
            self.q0,
            self.q1,
            self.shot_off,
            self.shot_len,
            self.op_off,
            self.op_len,
            self.kind,
            self.meas,
            self.slots,
            self.per_shot,
            self.block,
        ]);
        bytes[at..at + 80].copy_from_slice(bytemuck::cast_slice(&words));
    }
}

/// The buffers of a resumer, grown as runs need, and their bind group.
struct Buffers {
    state: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    ops: wgpu::Buffer,
    shots: wgpu::Buffer,
    rng: wgpu::Buffer,
    partial: wgpu::Buffer,
    outcomes: wgpu::Buffer,
    flags: wgpu::Buffer,
    readback: wgpu::Buffer,
    bind_group: Option<wgpu::BindGroup>,
}

fn buffer(device: &wgpu::Device, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: size.max(16).next_multiple_of(16),
        usage,
        mapped_at_creation: false,
    })
}

const STORAGE: wgpu::BufferUsages = wgpu::BufferUsages::STORAGE
    .union(wgpu::BufferUsages::COPY_DST)
    .union(wgpu::BufferUsages::COPY_SRC);
const UNIFORM: wgpu::BufferUsages =
    wgpu::BufferUsages::UNIFORM.union(wgpu::BufferUsages::COPY_DST);
const READBACK: wgpu::BufferUsages =
    wgpu::BufferUsages::MAP_READ.union(wgpu::BufferUsages::COPY_DST);

impl Buffers {
    fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let mut buffers = Self {
            state: buffer(device, 16, STORAGE),
            uniforms: buffer(device, SLOT, UNIFORM),
            ops: buffer(device, 16, STORAGE),
            shots: buffer(device, 16, STORAGE),
            rng: buffer(device, 16, STORAGE),
            partial: buffer(device, 16, STORAGE),
            outcomes: buffer(device, 16, STORAGE),
            flags: buffer(device, 16, STORAGE),
            readback: buffer(device, 16, READBACK),
            bind_group: None,
        };
        buffers.bind(device, layout);
        buffers
    }

    /// Grows each buffer to at least the given bytes (powers of two, so
    /// that growth is rare), rebinding when one grew.
    #[allow(clippy::too_many_arguments)]
    fn ensure(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        state: u64,
        uniforms: u64,
        ops: u64,
        shots: u64,
        slots: u64,
        partial: u64,
    ) {
        let mut grew = false;
        let mut grow = |b: &mut wgpu::Buffer, size: u64, usage| {
            if b.size() < size {
                *b = buffer(device, size.next_power_of_two(), usage);
                grew = true;
            }
        };
        grow(&mut self.state, state, STORAGE);
        grow(&mut self.uniforms, uniforms, UNIFORM);
        grow(&mut self.ops, ops, STORAGE);
        grow(&mut self.shots, shots, STORAGE);
        grow(&mut self.rng, slots * 4, STORAGE);
        grow(&mut self.flags, slots * 8, STORAGE);
        grow(&mut self.partial, partial, STORAGE);
        if grew {
            self.bind(device, layout);
        }
    }

    fn ensure_outcomes(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        bytes: u64,
    ) {
        if self.outcomes.size() < bytes {
            self.outcomes = buffer(device, bytes.next_power_of_two(), STORAGE);
            self.readback = buffer(device, bytes.next_power_of_two(), READBACK);
            self.bind(device, layout);
        }
    }

    fn bind(&mut self, device: &wgpu::Device, layout: &wgpu::BindGroupLayout) {
        fn whole(binding: u32, b: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry {
                binding,
                resource: b.as_entire_binding(),
            }
        }
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("resumer"),
            layout,
            entries: &[
                whole(0, &self.state),
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.uniforms,
                        offset: 0,
                        size: std::num::NonZeroU64::new(PARAMS_BYTES),
                    }),
                },
                whole(2, &self.ops),
                whole(3, &self.shots),
                whole(4, &self.rng),
                whole(5, &self.partial),
                whole(6, &self.outcomes),
                whole(7, &self.flags),
            ],
        }));
    }
}

fn wait(device: &wgpu::Device) {
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("the device polls");
}

// ---------------------------------------------------------------------------
// Translation of a batch into passes.
// ---------------------------------------------------------------------------

type C32 = [f32; 2];

fn c32(c: Complex64) -> C32 {
    #[allow(clippy::cast_possible_truncation)]
    [c.re as f32, c.im as f32]
}

#[derive(Clone, Debug)]
enum Op {
    G1 { q: u32, m: [Complex64; 4] },
    G2 { a: u32, b: u32, m: [Complex64; 16] },
    /// kind 0: mz; kind 1: to |0> after (mresetz, reset). `meas`: the
    /// outcome's index in the batch, or `NO_RESULT` (reset).
    Meas { q: u32, kind: u32, meas: u32 },
}

fn is_diag(op: &Op) -> bool {
    let zero = Complex64::new(0.0, 0.0);
    match op {
        Op::G1 { m, .. } => m[1] == zero && m[2] == zero,
        Op::G2 { m, .. } => m
            .iter()
            .enumerate()
            .all(|(i, v)| i / 4 == i % 4 || *v == zero),
        Op::Meas { .. } => false,
    }
}

fn qubit_mask(op: &Op) -> u64 {
    match op {
        Op::G1 { q, .. } | Op::Meas { q, .. } => 1 << q,
        Op::G2 { a, b, .. } => (1 << a) | (1 << b),
    }
}

fn mul2(a: &[Complex64; 4], b: &[Complex64; 4]) -> [Complex64; 4] {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
    ]
}

/// `a ⊗ b`, `a` on the more significant index bit.
fn kron(a: &[Complex64; 4], b: &[Complex64; 4]) -> [Complex64; 16] {
    let mut m = [Complex64::new(0.0, 0.0); 16];
    for r in 0..4 {
        for c in 0..4 {
            m[4 * r + c] = a[2 * (r / 2) + c / 2] * b[2 * (r % 2) + c % 2];
        }
    }
    m
}

fn mul4(a: &[Complex64; 16], b: &[Complex64; 16]) -> [Complex64; 16] {
    let mut m = [Complex64::new(0.0, 0.0); 16];
    for r in 0..4 {
        for c in 0..4 {
            m[4 * r + c] = (0..4).map(|k| a[4 * r + k] * b[4 * k + c]).sum();
        }
    }
    m
}

#[derive(Clone, Debug)]
enum SegKind {
    Block(Vec<Op>),
    Diag(Vec<Op>),
    Gate1 { q: u32, m: [Complex64; 4] },
    Gate2 { a: u32, b: u32, m: [Complex64; 16] },
    Meas { q: u32, kind: u32, meas: u32 },
}

#[derive(Clone, Debug)]
struct Segment {
    kind: SegKind,
    /// Qubits that non-diagonal ops (and measurements) of it touch.
    nondiag: u64,
    /// Qubits that diagonal ops of it touch.
    diag: u64,
}

/// A batch translated: its passes, in order, and the outcome index of each
/// call that writes a result, in the batch's order (`None`: answered on the
/// CPU, a loss check of a simulator without loss).
#[derive(Clone, Debug)]
struct Plan {
    segments: Vec<Segment>,
    results: Vec<(u32, Option<u32>)>,
    measurements: u32,
}

fn translate(batch: &[QirCall], n: u32, strategy: Strategy) -> Result<Plan, GpuError> {
    let block_qubits = BLOCK_QUBITS.min(n);
    let low = (1u64 << block_qubits) - 1;
    let mut segments: Vec<Segment> = Vec::new();
    let mut results = Vec::new();
    let mut measurements = 0;
    for call in batch {
        let op = match call {
            QirCall::Gate {
                gate,
                qubits: (q, second),
                angle,
            } => match (gate.matrix_at(*angle), second) {
                (OperatorMatrix::One(m), None) => Op::G1 { q: *q, m },
                (OperatorMatrix::Two(m), Some(b)) if b != q => Op::G2 { a: *q, b: *b, m },
                _ => return Err(GpuError(format!("unsupported gate call {call}"))),
            },
            QirCall::Measure { qubit, result } | QirCall::MeasureReset { qubit, result } => {
                let kind = u32::from(matches!(call, QirCall::MeasureReset { .. }));
                results.push((*result, Some(measurements)));
                measurements += 1;
                Op::Meas {
                    q: *qubit,
                    kind,
                    meas: measurements - 1,
                }
            }
            QirCall::Reset { qubit } => Op::Meas {
                q: *qubit,
                kind: 1,
                meas: NO_RESULT,
            },
            QirCall::Move { .. } => continue,
            // No loss without noise: a loss check reads "not lost".
            QirCall::PeekLoss { result, .. } => {
                results.push((*result, None));
                continue;
            }
            QirCall::CorrelatedNoise { .. } | QirCall::ReadoutNoise { .. } => {
                return Err(GpuError(format!(
                    "the GPU resumer spike supports no noise: {call}"
                )));
            }
        };
        place(&mut segments, op, low, strategy);
    }
    Ok(Plan {
        segments,
        results,
        measurements,
    })
}

/// Puts `op` in the earliest segment it may join, after the last segment
/// it does not commute with; or in a new segment at the end.
fn place(segments: &mut Vec<Segment>, op: Op, low: u64, strategy: Strategy) {
    let mask = qubit_mask(&op);
    let diag = is_diag(&op);
    let conflicts = |s: &Segment| {
        if diag {
            s.nondiag & mask != 0
        } else {
            (s.nondiag | s.diag) & mask != 0
        }
    };
    let start = segments.iter().rposition(conflicts).unwrap_or(0);
    let is_gate = !matches!(op, Op::Meas { .. });
    for segment in &mut segments[start..] {
        let joined = match (&mut segment.kind, &op) {
            (SegKind::Block(ops), _) if strategy.block && is_gate && mask & !low == 0 => {
                ops.push(op.clone());
                true
            }
            (SegKind::Diag(ops), _) if strategy.fuse_diag && diag => {
                ops.push(op.clone());
                true
            }
            (SegKind::Gate1 { q, m }, Op::G1 { q: oq, m: om }) if strategy.merge_1q && q == oq => {
                *m = mul2(om, m);
                true
            }
            // Two one-qubit gates on different qubits: one two-qubit pass.
            (SegKind::Gate1 { q, m }, Op::G1 { q: oq, m: om }) if strategy.kron => {
                segment.kind = SegKind::Gate2 {
                    a: *q,
                    b: *oq,
                    m: kron(m, om),
                };
                true
            }
            (SegKind::Gate2 { a, b, m }, Op::G1 { q: oq, m: om })
                if strategy.kron && (oq == a || oq == b) =>
            {
                let identity = [
                    Complex64::new(1.0, 0.0),
                    Complex64::new(0.0, 0.0),
                    Complex64::new(0.0, 0.0),
                    Complex64::new(1.0, 0.0),
                ];
                let g = if oq == a {
                    kron(om, &identity)
                } else {
                    kron(&identity, om)
                };
                *m = mul4(&g, m);
                true
            }
            _ => false,
        };
        if joined {
            if diag {
                segment.diag |= mask;
            } else {
                segment.nondiag |= mask;
            }
            return;
        }
    }
    let kind = match op {
        _ if strategy.block && is_gate && mask & !low == 0 => SegKind::Block(vec![op]),
        _ if strategy.fuse_diag && diag => SegKind::Diag(vec![op]),
        Op::G1 { q, m } => SegKind::Gate1 { q, m },
        Op::G2 { a, b, m } => SegKind::Gate2 { a, b, m },
        Op::Meas { q, kind, meas } => SegKind::Meas { q, kind, meas },
    };
    segments.push(Segment {
        kind,
        nondiag: if diag { 0 } else { mask },
        diag: if diag { mask } else { 0 },
    });
}

/// Appends the diagonal term of `op` (3 vec4) to `ops`.
fn push_diag_term(ops: &mut Vec<[f32; 4]>, op: &Op) {
    match op {
        Op::G1 { q, m } => {
            #[allow(clippy::cast_precision_loss)]
            ops.push([1.0, *q as f32, 0.0, 0.0]);
            let (d0, d1) = (c32(m[0]), c32(m[3]));
            ops.push([d0[0], d0[1], d1[0], d1[1]]);
            ops.push([0.0; 4]);
        }
        Op::G2 { a, b, m } => {
            #[allow(clippy::cast_precision_loss)]
            ops.push([2.0, *a as f32, *b as f32, 0.0]);
            let d: Vec<C32> = [0, 5, 10, 15].iter().map(|&i| c32(m[i])).collect();
            ops.push([d[0][0], d[0][1], d[1][0], d[1][1]]);
            ops.push([d[2][0], d[2][1], d[3][0], d[3][1]]);
        }
        Op::Meas { .. } => unreachable!("a measurement is no diagonal term"),
    }
}

fn push_matrix4(ops: &mut Vec<[f32; 4]>, m: &[Complex64; 16]) {
    for pair in m.chunks(2) {
        let (x, y) = (c32(pair[0]), c32(pair[1]));
        ops.push([x[0], x[1], y[0], y[1]]);
    }
}

/// Appends a block op (9 vec4) to `ops`.
fn push_block_op(ops: &mut Vec<[f32; 4]>, op: &Op) {
    let start = ops.len();
    if is_diag(op) {
        push_diag_term(ops, op);
    } else {
        match op {
            Op::G1 { q, m } => {
                #[allow(clippy::cast_precision_loss)]
                ops.push([11.0, *q as f32, 0.0, 0.0]);
                let r: Vec<C32> = m.iter().map(|&c| c32(c)).collect();
                ops.push([r[0][0], r[0][1], r[1][0], r[1][1]]);
                ops.push([r[2][0], r[2][1], r[3][0], r[3][1]]);
            }
            Op::G2 { a, b, m } => {
                #[allow(clippy::cast_precision_loss)]
                ops.push([12.0, *a as f32, *b as f32, 0.0]);
                push_matrix4(ops, m);
            }
            Op::Meas { .. } => unreachable!("a measurement is in no block"),
        }
    }
    ops.resize(start + 9, [0.0; 4]);
}

/// The lengths of the prefix and of the suffix that all `batches` share,
/// not overlapping in any.
fn common_ends(batches: &[Vec<QirCall>]) -> (usize, usize) {
    let shortest = batches.iter().map(Vec::len).min().unwrap_or(0);
    let first = &batches[0];
    let pre = (0..shortest)
        .take_while(|&i| batches.iter().all(|b| b[i] == first[i]))
        .count();
    let suf = (0..shortest - pre)
        .take_while(|&i| {
            let call = &first[first.len() - 1 - i];
            batches.iter().all(|b| b[b.len() - 1 - i] == *call)
        })
        .count();
    (pre, suf)
}

/// The passes of a round being laid out: their parameters, op lists and
/// dispatches.
struct Layout<'a, 'g> {
    gpu: &'g Gpu,
    n: u32,
    slots: u32,
    per_shot: u32,
    params: &'a mut Vec<u8>,
    ops: &'a mut Vec<[f32; 4]>,
    passes: &'a mut Vec<(&'g wgpu::ComputePipeline, u32, u64)>,
}

impl Layout<'_, '_> {
    /// Lays out `plan` on the shots `shots[shot_off..][..shot_len]`, its
    /// outcomes at `meas_off` and after.
    fn emit(&mut self, plan: &Plan, shot_off: u32, shot_len: u32, meas_off: u32) {
        let (gpu, n) = (self.gpu, self.n);
        let base = Params {
            n,
            shot_off,
            shot_len,
            slots: self.slots,
            per_shot: self.per_shot,
            ..Params::default()
        };
        let len = u64::from(shot_len);
        for segment in &plan.segments {
            let mut p = base;
            let at = self.params.len();
            self.params
                .resize(at + usize::try_from(SLOT).expect("small"), 0);
            let slot = u32::try_from(at as u64 / SLOT).expect("fits");
            let op_off = u32::try_from(self.ops.len()).expect("fits");
            match &segment.kind {
                SegKind::Gate1 { q, m } => {
                    let r: Vec<C32> = m.iter().map(|&c| c32(c)).collect();
                    p.m0 = [r[0][0], r[0][1], r[1][0], r[1][1]];
                    p.m1 = [r[2][0], r[2][1], r[3][0], r[3][1]];
                    p.q0 = *q;
                    self.passes
                        .push((&gpu.gate1, slot, (len << (n - 1)).div_ceil(WG)));
                }
                SegKind::Gate2 { a, b, m } => {
                    push_matrix4(self.ops, m);
                    p.q0 = *a;
                    p.q1 = *b;
                    p.op_off = op_off;
                    self.passes
                        .push((&gpu.gate2, slot, (len << (n - 2)).div_ceil(WG)));
                }
                SegKind::Diag(terms) => {
                    for term in terms {
                        push_diag_term(self.ops, term);
                    }
                    p.op_off = op_off;
                    p.op_len = u32::try_from(terms.len()).expect("fits");
                    self.passes.push((&gpu.diag, slot, (len << n).div_ceil(WG)));
                }
                SegKind::Block(block_ops) => {
                    for op in block_ops {
                        push_block_op(self.ops, op);
                    }
                    let b = BLOCK_QUBITS.min(n);
                    p.op_off = op_off;
                    p.op_len = u32::try_from(block_ops.len()).expect("fits");
                    p.block = b;
                    self.passes.push((&gpu.block, slot, len << (n - b)));
                }
                SegKind::Meas { q, kind, meas } => {
                    p.q0 = *q;
                    p.kind = *kind;
                    p.meas = if *meas == NO_RESULT {
                        NO_RESULT
                    } else {
                        meas + meas_off
                    };
                    self.passes
                        .push((&gpu.prob, slot, len * u64::from(self.per_shot)));
                    self.passes.push((&gpu.sample, slot, len.div_ceil(WG)));
                    self.passes
                        .push((&gpu.collapse, slot, (len << (n - 1)).div_ceil(WG)));
                }
            }
            p.write(self.params, at);
        }
    }
}

// ---------------------------------------------------------------------------
// The resumer.
// ---------------------------------------------------------------------------

/// Runs shots of a [`QirMachine`]'s program on the GPU, all shots of a chunk
/// in lockstep, grouped by their next batch (see the module's doc).
pub struct QirGpuResumer {
    gpu: Rc<Gpu>,
    strategy: Strategy,
    seed: u64,
    /// Shots whose own work fails at their first batch, for the conformance
    /// kit's rule on the resumer's own failures.
    failing: Vec<QirShotId>,
    buffers: Option<Buffers>,
    /// Translated batches, by their calls, reused across rounds and runs.
    cache: RefCell<HashMap<BatchKey, Rc<Plan>>>,
    pub stats: Stats,
}

/// A batch, as a hashable key: its calls' Debug text.
type BatchKey = String;

impl QirGpuResumer {
    pub fn new(gpu: Rc<Gpu>, strategy: Strategy) -> Self {
        Self {
            gpu,
            strategy,
            seed: 0x5EED_0F_5107,
            failing: Vec::new(),
            buffers: None,
            cache: RefCell::new(HashMap::new()),
            stats: Stats::default(),
        }
    }

    pub fn failing(mut self, failing: &[QirShotId]) -> Self {
        self.failing = failing.to_vec();
        self
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    fn plan(&self, batch: &[QirCall], n: u32) -> Result<Rc<Plan>, GpuError> {
        if !self.strategy.cache {
            return Ok(Rc::new(translate(batch, n, self.strategy)?));
        }
        let key = format!("{n}:{batch:?}");
        if let Some(plan) = self.cache.borrow().get(&key) {
            return Ok(plan.clone());
        }
        let plan = Rc::new(translate(batch, n, self.strategy)?);
        let mut cache = self.cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(key, plan.clone());
        Ok(plan)
    }
}

impl QirGpuResumer {
    /// Times each kind of pass on `slots` shots of `n` qubits: dispatches
    /// it `reps` times in one submission, and gives the milliseconds per
    /// dispatch (GPU time and its encoding, without any round trip).
    pub fn profile(&mut self, n: u32, slots: u32, reps: u32) -> Vec<(String, f64)> {
        let gpu = self.gpu.clone();
        let (device, queue) = (&gpu.device, &gpu.queue);
        let per_shot = ((1u32 << (n - 1)) / 4096).max(1);
        let buffers = self
            .buffers
            .get_or_insert_with(|| Buffers::new(device, &gpu.layout));
        buffers.ensure(
            device,
            &gpu.layout,
            (u64::from(slots) << n) * 8,
            SLOT * 16,
            1 << 16,
            u64::from(slots) * 4,
            u64::from(slots),
            u64::from(slots) * u64::from(per_shot) * 8,
        );
        buffers.ensure_outcomes(device, &gpu.layout, u64::from(slots) * 4);
        let shot_list: Vec<u32> = (0..slots).collect();
        queue.write_buffer(&buffers.shots, 0, bytemuck::cast_slice(&shot_list));
        let rx = |q: u32, t: f64| {
            let (c, s) = ((t / 2.0).cos(), (t / 2.0).sin());
            Op::G1 {
                q,
                m: [
                    Complex64::new(c, 0.0),
                    Complex64::new(0.0, -s),
                    Complex64::new(0.0, -s),
                    Complex64::new(c, 0.0),
                ],
            }
        };
        let one = Complex64::new(1.0, 0.0);
        let cz = |a: u32, b: u32| {
            let mut m = [Complex64::new(0.0, 0.0); 16];
            for i in 0..4 {
                m[5 * i] = if i == 3 { -one } else { one };
            }
            Op::G2 { a, b, m }
        };
        let high = n - 1;
        let block_qubits = BLOCK_QUBITS.min(n);
        let rx_m = |q, t| match rx(q, t) {
            Op::G1 { m, .. } => m,
            _ => unreachable!(),
        };
        let kinds: Vec<(&str, SegKind)> = vec![
            ("gate1 (high qubit)", SegKind::Gate1 { q: high, m: rx_m(high, 0.3) }),
            ("gate1 (qubit 0)", SegKind::Gate1 { q: 0, m: rx_m(0, 0.3) }),
            (
                "gate2 (two high qubits)",
                SegKind::Gate2 {
                    a: high,
                    b: high - 1,
                    m: kron(&rx_m(high, 0.3), &rx_m(high - 1, 0.4)),
                },
            ),
            (
                "diag (n-1 CZ terms)",
                SegKind::Diag((0..n - 1).map(|q| cz(q, q + 1)).collect()),
            ),
            (
                "block (Rx and CZ on the low qubits)",
                SegKind::Block(
                    (0..block_qubits)
                        .map(|q| rx(q, 0.3))
                        .chain((0..block_qubits - 1).map(|q| cz(q, q + 1)))
                        .collect(),
                ),
            ),
            (
                "mresetz (prob, sample, collapse)",
                SegKind::Meas {
                    q: 0,
                    kind: 1,
                    meas: 0,
                },
            ),
        ];
        let mut times = Vec::new();
        for (name, kind) in kinds {
            let plan = Plan {
                segments: vec![Segment {
                    kind,
                    nondiag: 0,
                    diag: 0,
                }],
                results: Vec::new(),
                measurements: 1,
            };
            let mut params = Vec::new();
            let mut ops = Vec::new();
            let mut passes = Vec::new();
            Layout {
                gpu: &gpu,
                n,
                slots,
                per_shot,
                params: &mut params,
                ops: &mut ops,
                passes: &mut passes,
            }
            .emit(&plan, 0, slots, 0);
            queue.write_buffer(&buffers.uniforms, 0, &params);
            queue.write_buffer(&buffers.ops, 0, bytemuck::cast_slice(&ops));
            let mut best = f64::INFINITY;
            for _ in 0..3 {
                let t = Instant::now();
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_compute_pass(&Default::default());
                    for _ in 0..reps {
                        for (pipeline, slot, groups) in &passes {
                            pass.set_pipeline(pipeline);
                            #[allow(clippy::cast_possible_truncation)]
                            pass.set_bind_group(
                                0,
                                buffers.bind_group.as_ref(),
                                &[(u64::from(*slot) * SLOT) as u32],
                            );
                            let (x, y) = gpu.shape(*groups);
                            pass.dispatch_workgroups(x, y, 1);
                        }
                    }
                }
                queue.submit([encoder.finish()]);
                wait(device);
                best = best.min(t.elapsed().as_secs_f64() * 1e3 / f64::from(reps));
            }
            times.push((name.to_string(), best));
        }
        times
    }
}

/// A shot of a chunk.
struct GpuShot {
    id: QirShotId,
    slot: u32,
    context: QirShotContext,
    outcomes: Vec<QirOutcome>,
    records: Option<Vec<OutputRecord>>,
    group: Option<usize>,
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

impl QirResumer for QirGpuResumer {
    type Error = GpuError;

    async fn run(
        &mut self,
        machine: &QirMachine<'_>,
        shots: &[QirShotId],
    ) -> Result<Vec<Vec<OutputRecord>>, QirResumeError<GpuError>> {
        let n = machine.program().qubits().max(2);
        if n > MAX_AMPLITUDES_LOG2 {
            return match shots.first() {
                None => Ok(Vec::new()),
                Some(&shot) => Err(QirResumeError::Resumer {
                    shot,
                    error: GpuError(format!(
                        "{n} qubits: the spike holds at most {MAX_AMPLITUDES_LOG2} (one binding)"
                    )),
                }),
            };
        }
        let max_slots = (1usize << (MAX_AMPLITUDES_LOG2 - n))
            .min(usize::try_from(self.strategy.max_slots).unwrap_or(usize::MAX))
            .max(1);
        let mut records = Vec::with_capacity(shots.len());
        for chunk in shots.chunks(max_slots) {
            records.extend(self.run_chunk(machine, chunk, n)?);
        }
        Ok(records)
    }
}

impl QirGpuResumer {
    #[allow(clippy::too_many_lines)]
    fn run_chunk(
        &mut self,
        machine: &QirMachine<'_>,
        chunk: &[QirShotId],
        n: u32,
    ) -> Result<Vec<Vec<OutputRecord>>, QirResumeError<GpuError>> {
        let gpu = self.gpu.clone();
        let (device, queue) = (&gpu.device, &gpu.queue);
        let slots = u32::try_from(chunk.len()).expect("a chunk fits u32");
        let per_shot = ((1u32 << (n - 1)) / 4096).max(1);
        let started = Instant::now();
        let buffers = self
            .buffers
            .get_or_insert_with(|| Buffers::new(device, &gpu.layout));
        buffers.ensure(
            device,
            &gpu.layout,
            (u64::from(slots) << n) * 8,
            SLOT,
            16,
            16,
            u64::from(slots),
            u64::from(slots) * u64::from(per_shot) * 8,
        );
        let seeds: Vec<u32> = chunk
            .iter()
            .map(|id| {
                #[allow(clippy::cast_possible_truncation)]
                let seed = splitmix(self.seed ^ splitmix(u64::from(id.0))) as u32;
                seed
            })
            .collect();
        queue.write_buffer(&buffers.rng, 0, bytemuck::cast_slice(&seeds));
        let mut params = vec![0u8; usize::try_from(SLOT).expect("small")];
        Params {
            n,
            slots,
            ..Params::default()
        }
        .write(&mut params, 0);
        queue.write_buffer(&buffers.uniforms, 0, &params);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&gpu.init);
            pass.set_bind_group(0, buffers.bind_group.as_ref(), &[0]);
            let (x, y) = gpu.shape((u64::from(slots) << n).div_ceil(WG));
            pass.dispatch_workgroups(x, y, 1);
        }
        queue.submit([encoder.finish()]);
        self.stats.encode += started.elapsed();
        self.stats.chunks += 1;

        let mut shots: Vec<GpuShot> = chunk
            .iter()
            .enumerate()
            .map(|(slot, &id)| GpuShot {
                id,
                slot: u32::try_from(slot).expect("fits"),
                context: machine.start_shot(),
                outcomes: Vec::new(),
                records: None,
                group: None,
            })
            .collect();
        // The first failure in the chunk's order: its place, and the error.
        let mut failure: Option<(usize, QirResumeError<GpuError>)> = None;
        let mut batches: Vec<Vec<QirCall>> = Vec::new();
        let mut params: Vec<u8> = Vec::new();
        let mut ops: Vec<[f32; 4]> = Vec::new();
        let mut shot_list: Vec<u32> = Vec::new();
        loop {
            // Resume every running shot to its next stop, grouping them.
            let t = Instant::now();
            batches.clear();
            let running = failure.as_ref().map_or(shots.len(), |(place, _)| *place);
            for (place, shot) in shots[..running].iter_mut().enumerate() {
                if shot.records.is_some() {
                    continue;
                }
                shot.group = None;
                match machine.resume(&mut shot.context, &shot.outcomes) {
                    Ok(QirStop::Ended(ended)) => shot.records = Some(ended.to_vec()),
                    Ok(QirStop::Batch(batch)) => {
                        if self.failing.contains(&shot.id) {
                            let error = GpuError(format!("{} fails on purpose", shot.id));
                            failure = Some((
                                place,
                                QirResumeError::Resumer {
                                    shot: shot.id,
                                    error,
                                },
                            ));
                            break;
                        }
                        let group = batches
                            .iter()
                            .position(|equal| equal[..] == *batch)
                            .unwrap_or_else(|| {
                                batches.push(batch.to_vec());
                                batches.len() - 1
                            });
                        shot.group = Some(group);
                    }
                    Err(trap) => {
                        failure = Some((
                            place,
                            QirResumeError::Trapped {
                                shot: shot.id,
                                trap,
                            },
                        ));
                        break;
                    }
                }
            }
            self.stats.machine += t.elapsed();
            let running = failure.as_ref().map_or(shots.len(), |(place, _)| *place);
            if batches.is_empty() {
                break;
            }

            // Translate the round's batches, and lay out its passes: the
            // calls that open and close every group's batch (a common prefix
            // and suffix) run once on the union of the groups' shots; the
            // rest of each group's batch on its own shots.
            let t = Instant::now();
            let (pre, suf) = if batches.len() == 1 {
                (batches[0].len(), 0)
            } else if self.strategy.share {
                common_ends(&batches)
            } else {
                (0, 0)
            };
            let piece = |this: &Self, batch: &[QirCall]| this.plan(batch, n);
            let shared_pre = piece(self, &batches[0][..pre]);
            let shared_suf = piece(self, &batches[0][batches[0].len() - suf..]);
            let mut middles = Vec::with_capacity(batches.len());
            for batch in &batches {
                middles.push(piece(self, &batch[pre..batch.len() - suf]));
            }
            // A piece that fails fails every shot that runs it: the first in
            // order is reported.
            for (group, middle) in middles.iter().enumerate() {
                let error = match (&shared_pre, middle, &shared_suf) {
                    (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => e.clone(),
                    _ => continue,
                };
                let place = shots[..running]
                    .iter()
                    .position(|shot| shot.group == Some(group))
                    .expect("a group has a shot");
                if failure.as_ref().is_none_or(|(at, _)| place < *at) {
                    failure = Some((
                        place,
                        QirResumeError::Resumer {
                            shot: shots[place].id,
                            error,
                        },
                    ));
                }
            }
            let running = failure.as_ref().map_or(shots.len(), |(place, _)| *place);
            params.clear();
            ops.clear();
            shot_list.clear();
            // (pipeline, params slot, workgroups) per pass.
            let mut passes: Vec<(&wgpu::ComputePipeline, u32, u64)> = Vec::new();
            let mut results: Vec<Vec<(u32, Option<u32>)>> = vec![Vec::new(); batches.len()];
            let mut total_measurements = 0;
            if let (Ok(shared_pre), Ok(shared_suf)) = (&shared_pre, &shared_suf) {
                let mut layout = Layout {
                    gpu: &gpu,
                    n,
                    slots,
                    per_shot,
                    params: &mut params,
                    ops: &mut ops,
                    passes: &mut passes,
                };
                // The union of the groups' shots, for the shared pieces.
                let union_off = u32::try_from(shot_list.len()).expect("fits");
                shot_list.extend(
                    shots[..running]
                        .iter()
                        .filter(|shot| {
                            shot.group.is_some_and(|group| middles[group].is_ok())
                        })
                        .map(|shot| shot.slot),
                );
                let union_len = u32::try_from(shot_list.len()).expect("fits") - union_off;
                if union_len > 0 {
                    layout.emit(shared_pre, union_off, union_len, 0);
                }
                let mut most_middle = 0;
                for (group, middle) in middles.iter().enumerate() {
                    let Ok(middle) = middle else { continue };
                    let shot_off = u32::try_from(shot_list.len()).expect("fits");
                    shot_list.extend(
                        shots[..running]
                            .iter()
                            .filter(|shot| shot.group == Some(group))
                            .map(|shot| shot.slot),
                    );
                    let shot_len = u32::try_from(shot_list.len()).expect("fits") - shot_off;
                    if shot_len == 0 {
                        continue;
                    }
                    self.stats.groups += 1;
                    most_middle = most_middle.max(middle.measurements);
                    if !middle.segments.is_empty() {
                        layout.emit(middle, shot_off, shot_len, shared_pre.measurements);
                    }
                }
                let suf_off = shared_pre.measurements + most_middle;
                if union_len > 0 {
                    layout.emit(shared_suf, union_off, union_len, suf_off);
                }
                total_measurements = suf_off + shared_suf.measurements;
                for (group, middle) in middles.iter().enumerate() {
                    let Ok(middle) = middle else { continue };
                    let shift = |offset: u32| {
                        move |&(result, meas): &(u32, Option<u32>)| {
                            (result, meas.map(|m| m + offset))
                        }
                    };
                    results[group] = shared_pre
                        .results
                        .iter()
                        .map(shift(0))
                        .chain(middle.results.iter().map(shift(shared_pre.measurements)))
                        .chain(shared_suf.results.iter().map(shift(suf_off)))
                        .collect();
                }
            }
            let most_measurements = total_measurements;
            self.stats.translate += t.elapsed();

            // Write the round's buffers, encode its passes and submit.
            let t = Instant::now();
            let buffers = self.buffers.as_mut().expect("made above");
            buffers.ensure(
                device,
                &gpu.layout,
                0,
                params.len() as u64,
                (ops.len() * 16) as u64,
                (shot_list.len() * 4) as u64,
                0,
                0,
            );
            let outcome_bytes = u64::from(most_measurements) * u64::from(slots) * 4;
            buffers.ensure_outcomes(device, &gpu.layout, outcome_bytes);
            if !params.is_empty() {
                queue.write_buffer(&buffers.uniforms, 0, &params);
            }
            if !ops.is_empty() {
                queue.write_buffer(&buffers.ops, 0, bytemuck::cast_slice(&ops));
            }
            if !shot_list.is_empty() {
                queue.write_buffer(&buffers.shots, 0, bytemuck::cast_slice(&shot_list));
            }
            self.stats.upload += t.elapsed();
            let t = Instant::now();
            let mut encoder = device.create_command_encoder(&Default::default());
            if !passes.is_empty() {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                for (pipeline, slot, groups) in &passes {
                    pass.set_pipeline(pipeline);
                    #[allow(clippy::cast_possible_truncation)]
                    pass.set_bind_group(
                        0,
                        buffers.bind_group.as_ref(),
                        &[(u64::from(*slot) * SLOT) as u32],
                    );
                    let (x, y) = gpu.shape(*groups);
                    pass.dispatch_workgroups(x, y, 1);
                }
            }
            if outcome_bytes > 0 {
                encoder.copy_buffer_to_buffer(
                    &buffers.outcomes,
                    0,
                    &buffers.readback,
                    0,
                    outcome_bytes,
                );
            }
            queue.submit([encoder.finish()]);
            self.stats.encode += t.elapsed();
            self.stats.rounds += 1;
            self.stats.passes += passes.len() as u64;

            // Read the round's outcomes back, and answer each shot.
            let t = Instant::now();
            let values: Vec<u32> = if outcome_bytes > 0 {
                self.stats.readbacks += 1;
                let slice = buffers.readback.slice(..outcome_bytes);
                slice.map_async(wgpu::MapMode::Read, |r| r.expect("the readback maps"));
                wait(device);
                let values = bytemuck::cast_slice(&slice.get_mapped_range()).to_vec();
                buffers.readback.unmap();
                values
            } else {
                Vec::new()
            };
            self.stats.wait += t.elapsed();
            for shot in &mut shots[..running] {
                let Some(group) = shot.group else { continue };
                shot.outcomes.clear();
                for &(result, meas) in &results[group] {
                    let value = match meas {
                        None => QirReadout::Zero,
                        Some(meas) => {
                            match values[(meas * slots + shot.slot) as usize] {
                                0 => QirReadout::Zero,
                                _ => QirReadout::One,
                            }
                        }
                    };
                    shot.outcomes.push(QirOutcome { result, value });
                }
            }
        }
        let t = Instant::now();
        wait(device);
        self.stats.drain += t.elapsed();
        match failure {
            Some((_, error)) => Err(error),
            None => Ok(shots
                .into_iter()
                .map(|shot| shot.records.expect("every shot ended"))
                .collect()),
        }
    }
}
