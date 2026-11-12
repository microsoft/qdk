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
//! on the low qubits into one pass through workgroup memory (`block`); gates
//! on high qubits, three or more, into a gather block, a workgroup-memory
//! pass on those qubits and the lowest three (`gather`); one-qubit gates on
//! one qubit into one matrix (`merge_1q`), and on two qubits into one 4x4
//! (`kron`). Groups' common batch prefixes and suffixes run once on their
//! shots together (`share`). A gate moves past the earlier gates it
//! commutes with (disjoint qubits, or both diagonal) to join a pass.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::rc::Rc;
use std::time::{Duration, Instant};

use num_complex::Complex64;
use qdk_simulators::OutputRecord;
use qdk_simulators::noise_config::{Fault, FaultTerm, NoiseConfig, NoiseTable};
use qdk_simulators::qir::{
    OperatorMatrix, QirCall, QirGate, QirMachine, QirOutcome, QirReadout, QirResumeError,
    QirResumer, QirShotContext, QirShotId, QirStop,
};

/// The largest state per binding: 128 MiB of complex f32.
const MAX_AMPLITUDES_LOG2: u32 = 24;
/// The workgroup-memory tile of the block pass: 2^10 amplitudes.
const BLOCK_QUBITS: u32 = 10;
const WG: u64 = 256;
/// The bytes of a pass's parameter slot (dynamic uniform offset).
const SLOT: u64 = 256;
const PARAMS_BYTES: u64 = 96;
/// The most noise sites a diagonal pass holds (the kernel's private array).
const DIAG_NOISE: usize = 32;
const NO_RESULT: u32 = u32::MAX;
/// The qubits every gather block holds, so that its loads and stores are
/// runs of 8 consecutive amplitudes.
const CARRIERS: u64 = 0b111;
/// The vec4 of block code the `block` kernel holds in workgroup memory.
const BLOCK_CODE: usize = 256;

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
    /// Whether gates on high qubits run in gather blocks (with `block`).
    pub gather: bool,
    /// Whether a round that writes no results (which the machine yields only
    /// at a program's return) waits for the next resume before it is
    /// submitted, so that it is dropped when every shot ends right after it
    /// (its work is unobservable: a final `ResetAll`).
    pub skip_tail: bool,
    /// Whether noise sites join block and diagonal passes (drawn inside
    /// them), rather than each being a Pauli pass of its own.
    pub fuse_noise: bool,
    /// Whether a block's noise sites move past later ops on other qubits
    /// (Pauli faults commute with them), so that one-qubit gates still pair.
    pub sink_noise: bool,
    /// Whether block and diagonal passes draw each noise site once per
    /// workgroup into workgroup memory, and skip fault-free sites.
    pub hoist_noise: bool,
    /// Whether a hoisted block also carries its noiseless code, which a
    /// workgroup whose shot has no fault in the block runs instead.
    pub clean_variant: bool,
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
        gather: true,
        skip_tail: true,
        fuse_noise: true,
        sink_noise: true,
        hoist_noise: true,
        clean_variant: true,
    };
    pub const PER_GATE: Self = Self {
        fuse_diag: false,
        block: false,
        merge_1q: false,
        max_slots: u32::MAX,
        cache: true,
        share: false,
        kron: false,
        gather: false,
        skip_tail: false,
        fuse_noise: false,
        sink_noise: false,
        hoist_noise: false,
        clean_variant: false,
    };
    pub const FUSED: Self = Self {
        fuse_diag: true,
        block: false,
        merge_1q: true,
        max_slots: u32::MAX,
        cache: true,
        share: true,
        kron: true,
        gather: true,
        skip_tail: true,
        fuse_noise: true,
        sink_noise: true,
        hoist_noise: true,
        clean_variant: true,
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
            "nogather" => Self {
                gather: false,
                ..Self::ALL
            },
            "notail" => Self {
                skip_tail: false,
                ..Self::ALL
            },
            "sepnoise" => Self {
                fuse_noise: false,
                ..Self::ALL
            },
            "nosink" => Self {
                sink_noise: false,
                ..Self::ALL
            },
            "nohoist" => Self {
                hoist_noise: false,
                clean_variant: false,
                ..Self::ALL
            },
            "noclean" => Self {
                clean_variant: false,
                ..Self::ALL
            },
            // The first noise commit's fused passes.
            "fused0" => Self {
                sink_noise: false,
                hoist_noise: false,
                clean_variant: false,
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
    /// Rounds dropped because every shot ended right after them.
    pub skipped: u64,
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
            skipped: self.skipped - earlier.skipped,
        }
    }
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        write!(
            f,
            "machine {:.2} ms, translate {:.2} ms, upload {:.2} ms, encode {:.2} ms, wait {:.2} ms, \
             drain {:.2} ms; {} chunks, {} rounds, {} groups, {} passes, {} readbacks, \
             {} skipped",
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
            self.readbacks,
            self.skipped
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
    pauli: wgpu::ComputePipeline,
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
                // Every kernel writes the workgroup memory it reads; the
                // zeroing code naga adds otherwise costs the driver seconds
                // of compile time.
                compilation_options: wgpu::PipelineCompilationOptions {
                    zero_initialize_workgroup_memory: false,
                    ..Default::default()
                },
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
            pauli: make("pauli"),
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
            ("pauli", &self.pauli),
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
    /// The round (each shot's batch index), for noise draws.
    round: u32,
    /// The batch index of the pass's first call (or, with `site_mode` 1,
    /// that many calls before the end of each shot's batch).
    site_off: u32,
    /// Where each slot's (noise seed, batch length) start in `shots`.
    meta_off: u32,
    site_mode: u32,
}

impl Params {
    fn write(&self, bytes: &mut [u8], at: usize) {
        let mut words = [0u32; 24];
        for (i, x) in self.m0.iter().chain(&self.m1).enumerate() {
            words[i] = x.to_bits();
        }
        words[8..24].copy_from_slice(&[
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
            self.round,
            self.site_off,
            self.meta_off,
            self.site_mode,
        ]);
        bytes[at..at + 96].copy_from_slice(bytemuck::cast_slice(&words));
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
const UNIFORM: wgpu::BufferUsages = wgpu::BufferUsages::UNIFORM.union(wgpu::BufferUsages::COPY_DST);
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
    G1 {
        q: u32,
        m: [Complex64; 4],
    },
    G2 {
        a: u32,
        b: u32,
        m: [Complex64; 16],
    },
    /// kind 0: mz; kind 1: to |0> after (mresetz, reset). `meas`: the
    /// outcome's index in the batch, or `NO_RESULT` (reset).
    Meas {
        q: u32,
        kind: u32,
        meas: u32,
    },
    /// A noise site: a fault drawn from the table at `ops[off..][..len]`
    /// (per shot, from its seed, the round and the call's batch index
    /// `site`), applied to `targets`.
    Noise {
        targets: Vec<u32>,
        site: u32,
        off: u32,
        len: u32,
    },
}

fn is_noise(op: &Op) -> bool {
    matches!(op, Op::Noise { .. })
}

/// A noise table on the GPU: `len` entries at `ops[off..]`, each
/// `(thr_hi, thr_lo, xbits, zbits)`: the cumulative probability as a 32-bit
/// threshold split in 16-bit halves (`thr_hi` 65536: always), and the
/// fault's X and Z bits over the call's targets (bit i: target i).
#[derive(Clone, Copy, Debug)]
struct Table {
    off: u32,
    len: u32,
    loss: bool,
}

/// The noise a resumer applies: the QDK's noise config, as GPU tables.
#[derive(Clone, Debug, Default)]
pub struct NoiseTables {
    code: Vec<[f32; 4]>,
    gates: Vec<(QirGate, Table)>,
    mz: Option<Table>,
    mresetz: Option<Table>,
    mov: Option<Table>,
    intrinsics: HashMap<u32, Table>,
    /// Idle noise is configured: refused (not simulated).
    idle: bool,
}

impl NoiseTables {
    pub fn new(config: &NoiseConfig<f64, f64>) -> Self {
        let mut tables = Self {
            idle: config.idle.s_probability > 0.0,
            ..Self::default()
        };
        let gates: [(QirGate, &NoiseTable<f64>); 20] = [
            (QirGate::X, &config.x),
            (QirGate::Y, &config.y),
            (QirGate::Z, &config.z),
            (QirGate::H, &config.h),
            (QirGate::S, &config.s),
            (QirGate::SAdj, &config.s_adj),
            (QirGate::T, &config.t),
            (QirGate::TAdj, &config.t_adj),
            (QirGate::Sx, &config.sx),
            (QirGate::SxAdj, &config.sx_adj),
            (QirGate::Rx, &config.rx),
            (QirGate::Ry, &config.ry),
            (QirGate::Rz, &config.rz),
            (QirGate::Cx, &config.cx),
            (QirGate::Cy, &config.cy),
            (QirGate::Cz, &config.cz),
            (QirGate::Rxx, &config.rxx),
            (QirGate::Ryy, &config.ryy),
            (QirGate::Rzz, &config.rzz),
            (QirGate::Swap, &config.swap),
        ];
        for (gate, table) in gates {
            if let Some(t) = tables.add(table) {
                tables.gates.push((gate, t));
            }
        }
        tables.mz = tables.add(&config.mz);
        tables.mresetz = tables.add(&config.mresetz);
        tables.mov = tables.add(&config.mov);
        let mut ids: Vec<_> = config.intrinsics.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            if let Some(t) = tables.add(&config.intrinsics[&id]) {
                tables.intrinsics.insert(id, t);
            }
        }
        tables
    }

    pub fn is_noiseless(&self) -> bool {
        self.code.is_empty() && !self.idle
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    fn add(&mut self, table: &NoiseTable<f64>) -> Option<Table> {
        if !table.has_pauli_noise() {
            return None;
        }
        let off = u32::try_from(self.code.len()).expect("fits");
        let mut loss = false;
        let mut cumulative = 0.0f64;
        for (&string, &probability) in table.pauli_strings.iter().zip(&table.probabilities) {
            cumulative += probability;
            let fault = Fault::from((string, table.qubits));
            let (mut x, mut z) = (0u32, 0u32);
            for (i, term) in fault.0.iter().enumerate() {
                match term {
                    FaultTerm::I => {}
                    FaultTerm::X => x |= 1 << i,
                    FaultTerm::Z => z |= 1 << i,
                    FaultTerm::Y => {
                        x |= 1 << i;
                        z |= 1 << i;
                    }
                    FaultTerm::Loss => loss |= probability > 0.0,
                }
            }
            let threshold = (cumulative * 4_294_967_296.0).floor().min(4_294_967_296.0) as u64;
            self.code.push([
                (threshold >> 16) as f32,
                (threshold & 0xffff) as f32,
                x as f32,
                z as f32,
            ]);
        }
        Some(Table {
            off,
            len: u32::try_from(self.code.len()).expect("fits") - off,
            loss,
        })
    }

    fn gate(&self, gate: QirGate) -> Option<Table> {
        self.gates.iter().find(|(g, _)| *g == gate).map(|(_, t)| *t)
    }
}

/// How a result-writing call of a batch is answered.
#[derive(Clone, Copy, Debug)]
enum Answer {
    /// The outcome at this index of the round's measurements.
    Meas(u32),
    /// A loss check: "not lost" (no loss is simulated).
    NotLost,
    /// Readout noise on the result's current value, drawn on the CPU from
    /// the shot's seed, the round and the call's batch index `site`.
    Readout {
        zero_as_one: f64,
        one_as_zero: f64,
        site: u32,
    },
}

fn is_diag(op: &Op) -> bool {
    let zero = Complex64::new(0.0, 0.0);
    match op {
        Op::G1 { m, .. } => m[1] == zero && m[2] == zero,
        Op::G2 { m, .. } => m
            .iter()
            .enumerate()
            .all(|(i, v)| i / 4 == i % 4 || *v == zero),
        Op::Meas { .. } | Op::Noise { .. } => false,
    }
}

fn qubit_mask(op: &Op) -> u64 {
    match op {
        Op::G1 { q, .. } | Op::Meas { q, .. } => 1 << q,
        Op::G2 { a, b, .. } => (1 << a) | (1 << b),
        Op::Noise { targets, .. } => targets.iter().fold(0, |m, t| m | 1 << t),
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
    /// Gates on the qubits of `set` (at most `BLOCK_QUBITS`), in workgroup
    /// memory: the low qubits, or a gather block (the lowest `CARRIERS`
    /// qubits, for coalesced access, and the gates' qubits).
    Block {
        set: u64,
        ops: Vec<Op>,
    },
    Diag(Vec<Op>),
    Gate1 {
        q: u32,
        m: [Complex64; 4],
    },
    Gate2 {
        a: u32,
        b: u32,
        m: [Complex64; 16],
    },
    Meas {
        q: u32,
        kind: u32,
        meas: u32,
    },
    /// A noise site in a pass of its own (an `Op::Noise`).
    Pauli(Op),
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
    results: Vec<(u32, Answer)>,
    measurements: u32,
}

#[allow(clippy::too_many_lines)]
fn translate(
    batch: &[QirCall],
    n: u32,
    strategy: Strategy,
    noise: &NoiseTables,
) -> Result<Plan, GpuError> {
    if noise.idle {
        return Err(GpuError(
            "the GPU resumer spike simulates no idle noise".to_string(),
        ));
    }
    let block_qubits = BLOCK_QUBITS.min(n);
    let low = (1u64 << block_qubits) - 1;
    let mut segments: Vec<Segment> = Vec::new();
    let mut results = Vec::new();
    let mut measurements = 0;
    for (site, call) in batch.iter().enumerate() {
        let site = u32::try_from(site).expect("fits");
        let (table, targets): (Option<Table>, Vec<u32>) = match call {
            QirCall::Gate {
                gate,
                qubits: (q, second),
                ..
            } => (noise.gate(*gate), std::iter::once(*q).chain(*second).collect()),
            QirCall::Measure { qubit, .. } => (noise.mz, vec![*qubit]),
            QirCall::MeasureReset { qubit, .. } | QirCall::Reset { qubit } => {
                (noise.mresetz, vec![*qubit])
            }
            QirCall::Move { qubit } => (noise.mov, vec![*qubit]),
            QirCall::CorrelatedNoise { qubits, table } => (
                noise.intrinsics.get(table).copied(),
                qubits.iter().copied().collect(),
            ),
            QirCall::PeekLoss { .. } | QirCall::ReadoutNoise { .. } => (None, Vec::new()),
        };
        if let Some(table) = table {
            if table.loss {
                return Err(GpuError(format!(
                    "the GPU resumer spike simulates no qubit loss: {call}'s noise table has loss"
                )));
            }
            if targets.len() > 24 {
                return Err(GpuError(format!("{call}: too many noise targets")));
            }
        }
        let noise_op = table.map(|t| Op::Noise {
            targets,
            site,
            off: t.off,
            len: t.len,
        });
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
                results.push((*result, Answer::Meas(measurements)));
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
            QirCall::Move { .. } | QirCall::CorrelatedNoise { .. } => {
                if let Some(noise_op) = noise_op {
                    place(&mut segments, noise_op, low, strategy);
                }
                continue;
            }
            // No loss is simulated: a loss check reads "not lost".
            QirCall::PeekLoss { result, .. } => {
                results.push((*result, Answer::NotLost));
                continue;
            }
            QirCall::ReadoutNoise {
                zero_as_one,
                one_as_zero,
                result,
            } => {
                results.push((
                    *result,
                    Answer::Readout {
                        zero_as_one: *zero_as_one,
                        one_as_zero: *one_as_zero,
                        site,
                    },
                ));
                continue;
            }
        };
        place(&mut segments, op, low, strategy);
        if let Some(noise_op) = noise_op {
            place(&mut segments, noise_op, low, strategy);
        }
    }
    if strategy.gather {
        segments = drop_small_gathers(segments, low, strategy);
    }
    if std::env::var("QDK_PLAN_DUMP").is_ok() {
        let summary: Vec<String> = segments
            .iter()
            .map(|seg| match &seg.kind {
                SegKind::Block { set, ops } => format!(
                    "Block({set:#x}: {} ops, {} noise)",
                    ops.len(),
                    ops.iter().filter(|op| is_noise(op)).count()
                ),
                SegKind::Diag(ops) => format!(
                    "Diag({} ops, {} noise)",
                    ops.len(),
                    ops.iter().filter(|op| is_noise(op)).count()
                ),
                SegKind::Gate1 { q, .. } => format!("Gate1({q})"),
                SegKind::Gate2 { a, b, .. } => format!("Gate2({a},{b})"),
                SegKind::Pauli(_) => "Pauli".to_string(),
                SegKind::Meas { q, .. } => format!("Meas({q})"),
            })
            .collect();
        eprintln!("PLAN {} calls: {}", batch.len(), summary.join(" | "));
    }
    Ok(Plan {
        segments,
        results,
        measurements,
    })
}

/// Replaces each gather block of fewer than `MIN_GATHER` non-diagonal gates by plain
/// passes: a gather pass costs about two plain ones (it reads scattered runs).
fn drop_small_gathers(segments: Vec<Segment>, low: u64, strategy: Strategy) -> Vec<Segment> {
    const MIN_GATHER: usize = 3;
    let small = |s: &Segment| {
        matches!(&s.kind, SegKind::Block { set, ops }
            if *set != low
                && ops.iter().filter(|op| !is_diag(op) && !is_noise(op)).count() < MIN_GATHER)
    };
    if !segments.iter().any(small) {
        return segments;
    }
    let plain = Strategy {
        gather: false,
        ..strategy
    };
    let mut out: Vec<Segment> = Vec::with_capacity(segments.len());
    for segment in segments {
        if small(&segment) {
            let SegKind::Block { ops, .. } = segment.kind else {
                unreachable!("checked")
            };
            for op in ops {
                place(&mut out, op, low, plain);
            }
        } else {
            out.push(segment);
        }
    }
    out
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
    let noise = is_noise(&op);
    let is_gate = match op {
        Op::Meas { .. } => false,
        Op::Noise { .. } => strategy.fuse_noise,
        _ => true,
    };
    // Noise joins a diagonal pass (drawn there, its X part moving the
    // pass's later phases), up to the kernel's count.
    let diag_noise = |ops: &Vec<Op>| ops.iter().filter(|op| is_noise(op)).count() < DIAG_NOISE;
    for segment in &mut segments[start..] {
        let joined = match (&mut segment.kind, &op) {
            (SegKind::Block { set, ops }, _)
                if strategy.block
                    && is_gate
                    && (*set | mask).count_ones() <= low.count_ones()
                    && (*set == low || strategy.gather)
                    // A diagonal gate grows no gather block: a diag pass
                    // takes it, with the rest of a CZ chain.
                    && (*set == low || !diag || mask & !*set == 0) =>
            {
                *set |= mask;
                ops.push(op.clone());
                true
            }
            (SegKind::Diag(ops), _)
                if strategy.fuse_diag
                    && (diag || (noise && strategy.fuse_noise && diag_noise(ops))) =>
            {
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
        _ if strategy.block && is_gate && mask & !low == 0 => SegKind::Block {
            set: low,
            ops: vec![op],
        },
        _ if strategy.block
            && strategy.gather
            && is_gate
            && !diag
            && (mask | CARRIERS).count_ones() <= low.count_ones() =>
        {
            SegKind::Block {
                set: mask | CARRIERS,
                ops: vec![op],
            }
        }
        _ if strategy.fuse_diag && (diag || (noise && strategy.fuse_noise)) => {
            SegKind::Diag(vec![op])
        }
        Op::G1 { q, m } => SegKind::Gate1 { q, m },
        Op::G2 { a, b, m } => SegKind::Gate2 { a, b, m },
        Op::Meas { q, kind, meas } => SegKind::Meas { q, kind, meas },
        op @ Op::Noise { .. } => SegKind::Pauli(op),
    };
    segments.push(Segment {
        kind,
        nondiag: if diag { 0 } else { mask },
        diag: if diag { mask } else { 0 },
    });
}

/// The phase of a product of diagonal ops, as the terms `(d, m, c)` the
/// kernels sum, `c * popcount(k & (k >> d) & m)` turns on basis state `k`:
/// one term per distance `d` between paired qubits (0 for single qubits)
/// and coefficient, dropping the product's global phase.
fn phase_terms<'o>(diag_ops: impl IntoIterator<Item = &'o Op>) -> Vec<[f32; 4]> {
    let turns = |c: Complex64| c.arg() / std::f64::consts::TAU;
    let mut single = [0.0f64; 32];
    let mut pairs: BTreeMap<(u32, u32), f64> = BTreeMap::new();
    for op in diag_ops {
        match op {
            Op::G1 { q, m } => single[*q as usize] += turns(m[3]) - turns(m[0]),
            Op::G2 { a, b, m } => {
                let (p00, p01, p10, p11) = (turns(m[0]), turns(m[5]), turns(m[10]), turns(m[15]));
                single[*a as usize] += p10 - p00;
                single[*b as usize] += p01 - p00;
                *pairs.entry(((*a).min(*b), (*a).max(*b))).or_default() += p11 - p10 - p01 + p00;
            }
            Op::Meas { .. } | Op::Noise { .. } => unreachable!("not diagonal"),
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    let reduced = |c: f64| {
        let c = c.rem_euclid(1.0);
        (c > 1e-12 && c < 1.0 - 1e-12).then_some(c as f32)
    };
    let mut terms: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for (q, &c) in single.iter().enumerate() {
        if let Some(c) = reduced(c) {
            *terms.entry((0, c.to_bits())).or_default() |= 1 << q;
        }
    }
    for (&(lo, hi), &c) in &pairs {
        if let Some(c) = reduced(c) {
            *terms.entry((hi - lo, c.to_bits())).or_default() |= 1 << lo;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    terms
        .into_iter()
        .map(|((d, c), mask)| [d as f32, mask as f32, f32::from_bits(c), 0.0])
        .collect()
}

/// A noise site's code, as every kernel reads it: a header `(marker, site,
/// table offset, table length)`, then `(count, t0, t1, t2)`, `(t3, ..)`: its
/// targets (global qubits, or block-local ones in a block).
#[allow(clippy::cast_precision_loss)]
fn noise_code(op: &Op, marker: f32) -> Vec<[f32; 4]> {
    let Op::Noise {
        targets,
        site,
        off,
        len,
    } = op
    else {
        unreachable!("a noise site")
    };
    let mut code = vec![[marker, *site as f32, *off as f32, *len as f32]];
    let words: Vec<f32> = std::iter::once(targets.len() as f32)
        .chain(targets.iter().map(|&t| t as f32))
        .collect();
    for chunk in words.chunks(4) {
        let mut v = [0.0; 4];
        v[..chunk.len()].copy_from_slice(chunk);
        code.push(v);
    }
    code
}

/// A diagonal pass's code: the phase terms of each run of diagonal ops,
/// and a noise site (marker -1) between runs; and the count of terms.
fn diag_code(diag_ops: &[Op]) -> (Vec<[f32; 4]>, u32) {
    let mut code = Vec::new();
    let mut terms = 0;
    let mut at = 0;
    while at < diag_ops.len() {
        if is_noise(&diag_ops[at]) {
            code.extend(noise_code(&diag_ops[at], -1.0));
            at += 1;
            continue;
        }
        let run = diag_ops[at..].iter().take_while(|op| !is_noise(op)).count();
        let run_terms = phase_terms(&diag_ops[at..at + run]);
        terms += u32::try_from(run_terms.len()).expect("fits");
        code.extend(run_terms);
        at += run;
    }
    (code, terms)
}

/// Whether `QDK_NOISE_DROP` (a timing experiment: `diag`, `block` or
/// `both`) drops the noise of this kind of pass.
fn drop_noise(kind: &str) -> bool {
    std::env::var("QDK_NOISE_DROP").is_ok_and(|v| v == kind || v == "both")
}

/// `block_ops` with each noise site moved down to just before the first
/// later op on one of its qubits (or to the end). Exact up to a global
/// phase: a Pauli fault commutes with ops on other qubits, and with other
/// faults up to a sign.
fn sink_noise(block_ops: &[Op]) -> Vec<Op> {
    let mut out = Vec::with_capacity(block_ops.len());
    let mut pending: Vec<Op> = Vec::new();
    for op in block_ops {
        if is_noise(op) {
            pending.push(op.clone());
            continue;
        }
        let mask = qubit_mask(op);
        let (hit, keep): (Vec<Op>, Vec<Op>) = pending
            .drain(..)
            .partition(|noise| qubit_mask(noise) & mask != 0);
        out.extend(hit);
        pending = keep;
        out.push(op.clone());
    }
    out.extend(pending);
    out
}

fn push_matrix4(ops: &mut Vec<[f32; 4]>, m: &[Complex64; 16]) {
    for pair in m.chunks(2) {
        let (x, y) = (c32(pair[0]), c32(pair[1]));
        ops.push([x[0], x[1], y[0], y[1]]);
    }
}

/// The block ops' code (as the `block` kernel reads it), in pieces of at
/// most `BLOCK_CODE` vec4: each piece's ops and code. Consecutive diagonal
/// ops become one op of phase terms. With `fast_runs` (hoisted draws), a run
/// of diagonal ops and noise sites is preceded by a kind-16 op: the run's
/// phase without its noise, which the kernel runs instead of the run when
/// none of the run's sites has a fault (and skips otherwise).
fn block_code(
    block_ops: &[Op],
    kron_in_block: bool,
    fast_runs: bool,
) -> Vec<(u32, Vec<[f32; 4]>)> {
    // Units of (op count, code), each kept in one piece.
    let mut encoded: Vec<(u32, Vec<[f32; 4]>)> = Vec::new();
    let mut at = 0;
    while at < block_ops.len() {
        let op = &block_ops[at];
        if fast_runs && (is_diag(op) || is_noise(op)) {
            let run = block_ops[at..]
                .iter()
                .take_while(|op| is_diag(op) || is_noise(op))
                .count();
            let ops = &block_ops[at..at + run];
            if ops.iter().any(is_noise) && ops.iter().any(is_diag) {
                let inner = block_units(ops, kron_in_block);
                let terms = phase_terms(ops.iter().filter(|op| is_diag(op)));
                let r: u32 = inner.iter().map(|(count, _)| count).sum();
                #[allow(clippy::cast_precision_loss)]
                let mut code = vec![[16.0, r as f32, 0.0, terms.len() as f32]];
                code.extend(terms);
                for (_, unit) in &inner {
                    code.extend(unit);
                }
                if code.len() <= BLOCK_CODE {
                    encoded.push((1 + r, code));
                } else {
                    encoded.extend(inner);
                }
                at += run;
                continue;
            }
        }
        let run = if is_noise(op) {
            1
        } else if is_diag(op) {
            block_ops[at..].iter().take_while(|op| is_diag(op)).count()
        } else {
            // A one-qubit gate and the next (a kron pair), or one gate.
            match (op, block_ops.get(at + 1)) {
                (Op::G1 { q, .. }, Some(next @ Op::G1 { q: nq, .. }))
                    if kron_in_block && q != nq && !is_diag(next) =>
                {
                    2
                }
                _ => 1,
            }
        };
        encoded.extend(block_units(&block_ops[at..at + run], kron_in_block));
        at += run;
    }
    let mut pieces: Vec<(u32, Vec<[f32; 4]>)> = Vec::new();
    for (count, code) in encoded {
        assert!(
            code.len() <= BLOCK_CODE,
            "a block op fits the kernel's code"
        );
        match pieces.last_mut() {
            Some((ops, piece)) if piece.len() + code.len() <= BLOCK_CODE => {
                *ops += count;
                piece.extend(code);
            }
            _ => pieces.push((count, code)),
        }
    }
    pieces
}

/// The block ops' code, one unit per kernel op (count 1 each).
fn block_units(block_ops: &[Op], kron_in_block: bool) -> Vec<(u32, Vec<[f32; 4]>)> {
    let mut encoded: Vec<(u32, Vec<[f32; 4]>)> = Vec::new();
    let mut at = 0;
    while at < block_ops.len() {
        let op = &block_ops[at];
        if is_noise(op) {
            encoded.push((1, noise_code(op, 14.0)));
            at += 1;
            continue;
        }
        if is_diag(op) {
            let run = block_ops[at..].iter().take_while(|op| is_diag(op)).count();
            let terms = phase_terms(&block_ops[at..at + run]);
            at += run;
            if terms.is_empty() {
                continue;
            }
            for terms in terms.chunks(BLOCK_CODE - 1) {
                #[allow(clippy::cast_precision_loss)]
                let mut code = vec![[13.0, 0.0, 0.0, terms.len() as f32]];
                code.extend(terms);
                encoded.push((1, code));
            }
            continue;
        }
        at += 1;
        // Two one-qubit gates on two qubits: one 4x4 sweep, not two.
        let pair = match (op, block_ops.get(at)) {
            (Op::G1 { q, m }, Some(next @ Op::G1 { q: nq, m: nm }))
                if kron_in_block && q != nq && !is_diag(next) =>
            {
                Some(Op::G2 {
                    a: *q,
                    b: *nq,
                    m: kron(m, nm),
                })
            }
            _ => None,
        };
        let op = match &pair {
            Some(pair) => {
                at += 1;
                pair
            }
            None => op,
        };
        #[allow(clippy::cast_precision_loss)]
        encoded.push((
            1,
            match op {
                Op::G1 { q, m } => {
                    let r: Vec<C32> = m.iter().map(|&c| c32(c)).collect();
                    vec![
                        [11.0, *q as f32, 0.0, 0.0],
                        [r[0][0], r[0][1], r[1][0], r[1][1]],
                        [r[2][0], r[2][1], r[3][0], r[3][1]],
                    ]
                }
                Op::G2 { a, b, m } => {
                    let mut code = vec![[12.0, *a as f32, *b as f32, 0.0]];
                    push_matrix4(&mut code, m);
                    code
                }
                Op::Meas { .. } | Op::Noise { .. } => {
                    unreachable!("not in a block's matrices")
                }
            },
        ));
    }
    encoded
}

/// The lengths of the prefix and of the suffix that all `batches` share,
/// not overlapping in any.
pub fn common_ends(batches: &[Vec<QirCall>]) -> (usize, usize) {
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
    /// Whether pairs of one-qubit gates in a block run as one 4x4 sweep.
    kron: bool,
    /// Whether a block's noise sites move after the gates on other qubits.
    sink: bool,
    /// Whether block and diagonal passes draw each noise site once per
    /// workgroup (not once per thread), skipping fault-free sites.
    hoist: bool,
    clean: bool,
    /// The round, and where the slots' (noise seed, batch length) are in
    /// `shots`, for noise draws.
    round: u32,
    meta_off: u32,
}

impl Layout<'_, '_> {
    /// Lays out `plan` on the shots `shots[shot_off..][..shot_len]`, its
    /// outcomes at `meas_off` and after.
    /// Its first call is at batch index `site_off` (or, with `site_mode`
    /// 1, `site_off` calls before the end of each shot's batch).
    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        plan: &Plan,
        shot_off: u32,
        shot_len: u32,
        meas_off: u32,
        site_off: u32,
        site_mode: u32,
    ) {
        let (gpu, n) = (self.gpu, self.n);
        let base = Params {
            n,
            shot_off,
            shot_len,
            slots: self.slots,
            per_shot: self.per_shot,
            round: self.round,
            site_off,
            meta_off: self.meta_off,
            site_mode,
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
                SegKind::Diag(diag_ops) => {
                    // Timing experiment only (wrong results): drop the
                    // noise of diagonal passes.
                    let kept: Vec<Op>;
                    let diag_ops = if drop_noise("diag") {
                        kept = diag_ops.iter().filter(|op| !is_noise(op)).cloned().collect();
                        &kept
                    } else {
                        diag_ops
                    };
                    let (code, terms) = diag_code(diag_ops);
                    if code.is_empty() {
                        // A global phase: nothing to do.
                        self.params.truncate(at);
                        continue;
                    }
                    p.op_off = op_off;
                    p.op_len = u32::try_from(code.len()).expect("fits");
                    p.q0 = terms;
                    // The hoisted draws: one workgroup per shot's slice, and
                    // the code fits the workgroup's fault array. The code is
                    // then preceded by the pass's phase without its noise
                    // (q0 terms), which shots without a fault run alone.
                    let hoisted =
                        self.hoist && terms != p.op_len && n >= 8 && code.len() <= 256;
                    p.meas = u32::from(hoisted);
                    if hoisted {
                        let merged = phase_terms(diag_ops.iter().filter(|op| is_diag(op)));
                        p.q0 = u32::try_from(merged.len()).expect("fits");
                        p.op_len += p.q0;
                        self.ops.extend(merged);
                    }
                    self.ops.extend(code);
                    self.passes.push((&gpu.diag, slot, (len << n).div_ceil(WG)));
                }
                SegKind::Pauli(op) => {
                    p.op_off = op_off;
                    self.ops.extend(noise_code(op, -1.0));
                    self.passes
                        .push((&gpu.pauli, slot, (len << (n - 1)).div_ceil(WG)));
                }
                SegKind::Block {
                    set,
                    ops: block_ops,
                } => {
                    // Fill the set up with the lowest qubits: longer runs of
                    // consecutive amplitudes and full workgroups.
                    let mut set = *set;
                    for q in 0..n {
                        if set.count_ones() >= BLOCK_QUBITS.min(n) {
                            break;
                        }
                        set |= 1 << q;
                    }
                    let b = set.count_ones();
                    // The set's qubits in ascending order, 5 bits each.
                    let (mut q0, mut q1) = (0u32, 0u32);
                    let mut local = [0u32; 64];
                    for (i, q) in (0..64u32).filter(|q| set >> q & 1 == 1).enumerate() {
                        let i = u32::try_from(i).expect("small");
                        local[q as usize] = i;
                        if i < 6 {
                            q0 |= q << (5 * i);
                        } else {
                            q1 |= q << (5 * (i - 6));
                        }
                    }
                    p.q0 = q0;
                    p.q1 = q1;
                    let at_local = |q: &u32| local[*q as usize];
                    let local_ops: Vec<Op> = block_ops
                        .iter()
                        .map(|op| match op {
                            Op::G1 { q, m } => Op::G1 {
                                q: at_local(q),
                                m: *m,
                            },
                            Op::G2 { a, b, m } => Op::G2 {
                                a: at_local(a),
                                b: at_local(b),
                                m: *m,
                            },
                            Op::Noise {
                                targets,
                                site,
                                off,
                                len,
                            } => Op::Noise {
                                targets: targets.iter().map(at_local).collect(),
                                site: *site,
                                off: *off,
                                len: *len,
                            },
                            Op::Meas { .. } => unreachable!("a measurement is in no block"),
                        })
                        .collect();
                    // Timing experiment only (wrong results): drop the
                    // noise of blocks.
                    let local_ops: Vec<Op> = if drop_noise("block") {
                        local_ops.into_iter().filter(|op| !is_noise(op)).collect()
                    } else {
                        local_ops
                    };
                    let local_ops = if self.sink {
                        sink_noise(&local_ops)
                    } else {
                        local_ops
                    };
                    let noisy = local_ops.iter().any(is_noise);
                    let mut pieces = block_code(&local_ops, self.kron, self.hoist && noisy);
                    // The fault-free variant: with hoisted draws, the block's
                    // code without its noise follows the noisy code, and a
                    // workgroup whose shot has no fault at any site runs it
                    // instead (per_shot: its op count + 1, m0.x: its offset).
                    let mut clean_variant = None;
                    if self.hoist && noisy && self.clean && pieces.len() == 1 {
                        let clean_ops: Vec<Op> =
                            local_ops.iter().filter(|op| !is_noise(op)).cloned().collect();
                        let clean = block_code(&clean_ops, self.kron, false);
                        let (count, code) = match clean.as_slice() {
                            [] => (0, Vec::new()),
                            [(count, code)] => (*count, code.clone()),
                            _ => (0, vec![[0.0; 4]; BLOCK_CODE + 1]),
                        };
                        if pieces[0].1.len() + code.len() <= BLOCK_CODE {
                            #[allow(clippy::cast_precision_loss)]
                            let offset = pieces[0].1.len() as f32;
                            pieces[0].1.extend(code);
                            clean_variant = Some((count + 1, offset));
                        }
                    }
                    if pieces.is_empty() {
                        // Only a global phase: nothing to do.
                        self.params.truncate(at);
                        continue;
                    }
                    for (i, (count, code)) in pieces.into_iter().enumerate() {
                        let mut p = p;
                        let (at, slot) = if i == 0 {
                            (at, slot)
                        } else {
                            let at = self.params.len();
                            self.params
                                .resize(at + usize::try_from(SLOT).expect("small"), 0);
                            (at, u32::try_from(at as u64 / SLOT).expect("fits"))
                        };
                        p.op_off = u32::try_from(self.ops.len()).expect("fits");
                        p.op_len = count;
                        p.kind = u32::try_from(code.len()).expect("fits");
                        p.block = b;
                        p.meas = u32::from(self.hoist && noisy);
                        (p.per_shot, p.m0[0]) = clean_variant.unwrap_or((0, 0.0));
                        self.ops.extend(code);
                        p.write(self.params, at);
                        self.passes.push((&gpu.block, slot, len << (n - b)));
                    }
                    continue;
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
    noise: Rc<NoiseTables>,
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
            noise: Rc::new(NoiseTables::default()),
            stats: Stats::default(),
        }
    }

    /// Applies the QDK noise `config` (Pauli faults after each gate,
    /// measurement, reset and move, and correlated-noise tables); a call
    /// whose table has loss fails.
    pub fn with_noise(mut self, config: &NoiseConfig<f64, f64>) -> Self {
        self.noise = Rc::new(NoiseTables::new(config));
        self.cache.borrow_mut().clear();
        self
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
            return Ok(Rc::new(translate(batch, n, self.strategy, &self.noise)?));
        }
        let key = format!("{n}:{batch:?}");
        if let Some(plan) = self.cache.borrow().get(&key) {
            return Ok(plan.clone());
        }
        let plan = Rc::new(translate(batch, n, self.strategy, &self.noise)?);
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
            (
                "gate1 (high qubit)",
                SegKind::Gate1 {
                    q: high,
                    m: rx_m(high, 0.3),
                },
            ),
            (
                "gate1 (qubit 0)",
                SegKind::Gate1 {
                    q: 0,
                    m: rx_m(0, 0.3),
                },
            ),
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
                SegKind::Block {
                    set: (1 << block_qubits) - 1,
                    ops: (0..block_qubits)
                        .map(|q| rx(q, 0.3))
                        .chain((0..block_qubits - 1).map(|q| cz(q, q + 1)))
                        .collect(),
                },
            ),
            (
                "block (only Rx on the low qubits)",
                SegKind::Block {
                    set: (1 << block_qubits) - 1,
                    ops: (0..block_qubits).map(|q| rx(q, 0.3)).collect(),
                },
            ),
            (
                "block (only CZ on the low qubits)",
                SegKind::Block {
                    set: (1 << block_qubits) - 1,
                    ops: (0..block_qubits - 1).map(|q| cz(q, q + 1)).collect(),
                },
            ),
            (
                "block (one Rx)",
                SegKind::Block {
                    set: (1 << block_qubits) - 1,
                    ops: vec![rx(3, 0.3)],
                },
            ),
            (
                "gather block (Rx on 7 high qubits)",
                SegKind::Block {
                    set: CARRIERS | (0x7f << (n - 7)),
                    ops: (n - 7..n).map(|q| rx(q, 0.3)).collect(),
                },
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
                kron: self.strategy.kron,
                sink: self.strategy.sink_noise,
                hoist: self.strategy.hoist_noise,
                clean: self.strategy.clean_variant,
                round: 0,
                meta_off: 0,
            }
            .emit(&plan, 0, slots, 0, 0, 0);
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

impl QirGpuResumer {
    /// The state of the last of `slots` shots of `n` qubits after `batch`
    /// (gates only) from |0...0>, up to a global phase: a check that each
    /// strategy's passes compute the same state.
    pub fn state_after(
        &mut self,
        n: u32,
        slots: u32,
        batch: &[QirCall],
    ) -> Result<Vec<[f32; 2]>, GpuError> {
        let gpu = self.gpu.clone();
        let (device, queue) = (&gpu.device, &gpu.queue);
        let per_shot = ((1u32 << (n - 1)) / 4096).max(1);
        let plan = translate(batch, n, self.strategy, &self.noise)?;
        if plan.measurements > 0 {
            return Err(GpuError("state_after takes gates only".into()));
        }
        let mut params = vec![0u8; usize::try_from(SLOT).expect("small")];
        Params {
            n,
            slots,
            ..Params::default()
        }
        .write(&mut params, 0);
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
            kron: self.strategy.kron,
            sink: self.strategy.sink_noise,
            hoist: self.strategy.hoist_noise,
            clean: self.strategy.clean_variant,
            round: 0,
            meta_off: 0,
        }
        .emit(&plan, 0, slots, 0, 0, 0);
        let buffers = self
            .buffers
            .get_or_insert_with(|| Buffers::new(device, &gpu.layout));
        buffers.ensure(
            device,
            &gpu.layout,
            (u64::from(slots) << n) * 8,
            params.len() as u64,
            (ops.len() as u64 * 16).max(16),
            u64::from(slots) * 4,
            u64::from(slots),
            u64::from(slots) * u64::from(per_shot) * 8,
        );
        buffers.ensure_outcomes(device, &gpu.layout, u64::from(slots) * 4);
        let shot_list: Vec<u32> = (0..slots).collect();
        queue.write_buffer(&buffers.shots, 0, bytemuck::cast_slice(&shot_list));
        queue.write_buffer(&buffers.uniforms, 0, &params);
        if !ops.is_empty() {
            queue.write_buffer(&buffers.ops, 0, bytemuck::cast_slice(&ops));
        }
        let bytes = 8u64 << n;
        let readback = buffer(device, bytes, READBACK);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&gpu.init);
            pass.set_bind_group(0, buffers.bind_group.as_ref(), &[0]);
            let (x, y) = gpu.shape((u64::from(slots) << n).div_ceil(WG));
            pass.dispatch_workgroups(x, y, 1);
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
        encoder.copy_buffer_to_buffer(
            &buffers.state,
            u64::from(slots - 1) * bytes,
            &readback,
            0,
            bytes,
        );
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..bytes);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("the readback maps"));
        wait(device);
        let state = bytemuck::cast_slice::<u8, [f32; 2]>(&slice.get_mapped_range()).to_vec();
        readback.unmap();
        Ok(state)
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
    /// The seed of its noise draws (from its id).
    noise_seed: u32,
    /// Each result's value so far (`true`: One), for readout noise.
    register: Vec<bool>,
}

/// A uniform draw in [0, 1) from a shot's noise seed, the round and a
/// batch index.
#[allow(clippy::cast_precision_loss)]
fn cpu_draw(seed: u32, round: u32, site: u32) -> f64 {
    let x = splitmix(
        (u64::from(seed) << 32) ^ splitmix((u64::from(round) << 32) | u64::from(site)) ^ 0xC0FFEE,
    );
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// Answers each grouped shot's batch: its result-writing calls' outcomes,
/// read from `values` (measurement-major, `slots` per measurement), in the
/// batch's order (readout noise reads the value its result holds then).
fn answer(
    shots: &mut [GpuShot],
    results: &[Vec<(u32, Answer)>],
    values: &[u32],
    slots: u32,
    round: u32,
) {
    for shot in shots {
        let Some(group) = shot.group else { continue };
        shot.outcomes.clear();
        for &(result, answer) in &results[group] {
            let one = match answer {
                Answer::NotLost => false,
                Answer::Meas(meas) => values[(meas * slots + shot.slot) as usize] != 0,
                Answer::Readout {
                    zero_as_one,
                    one_as_zero,
                    site,
                } => {
                    let held = shot.register.get(result as usize).copied().unwrap_or(false);
                    let u = cpu_draw(shot.noise_seed, round, site);
                    if held { u >= one_as_zero } else { u < zero_as_one }
                }
            };
            if let Some(slot) = shot.register.get_mut(result as usize) {
                *slot = one;
            }
            let value = if one { QirReadout::One } else { QirReadout::Zero };
            shot.outcomes.push(QirOutcome { result, value });
        }
    }
}

/// Resumes every running shot (those before the chunk's first failure) to
/// its next stop, grouping the shots whose batches are equal. Leaves the
/// round's distinct batches in `batches`, and records a new first failure.
fn resume_round(
    machine: &QirMachine<'_>,
    shots: &mut [GpuShot],
    failing: &[QirShotId],
    batches: &mut Vec<Vec<QirCall>>,
    failure: &mut Option<(usize, QirResumeError<GpuError>)>,
) {
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
                if failing.contains(&shot.id) {
                    let error = GpuError(format!("{} fails on purpose", shot.id));
                    *failure = Some((
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
                *failure = Some((
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
                #[allow(clippy::cast_possible_truncation)]
                noise_seed: splitmix(self.seed ^ 0x6E01_5E00 ^ splitmix(u64::from(id.0))) as u32,
                register: vec![false; machine.program().results() as usize],
            })
            .collect();
        let noisy = !self.noise.is_noiseless();
        // The first failure in the chunk's order: its place, and the error.
        let mut failure: Option<(usize, QirResumeError<GpuError>)> = None;
        let mut batches: Vec<Vec<QirCall>> = Vec::new();
        let mut params: Vec<u8> = Vec::new();
        let mut ops: Vec<[f32; 4]> = Vec::new();
        let mut shot_list: Vec<u32> = Vec::new();
        // Whether the shots were resumed at the end of the last round (whose
        // submission waited to see whether any shot goes on).
        let mut resumed_ahead = false;
        // Every running shot is resumed once per resume_round, so the count
        // of resumes is each shot's batch index.
        let mut resumes: u32 = 0;
        loop {
            // Resume every running shot to its next stop, grouping them.
            if !resumed_ahead {
                let t = Instant::now();
                resume_round(
                    machine,
                    &mut shots,
                    &self.failing,
                    &mut batches,
                    &mut failure,
                );
                resumes += 1;
                self.stats.machine += t.elapsed();
            }
            let round = resumes - 1;
            resumed_ahead = false;
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
            ops.extend_from_slice(&self.noise.code);
            shot_list.clear();
            if noisy {
                // Each slot's noise seed and batch length, for the draws.
                shot_list.resize(2 * slots as usize, 0);
                for shot in &shots {
                    let len = shot.group.map_or(0, |group| batches[group].len());
                    shot_list[2 * shot.slot as usize] = shot.noise_seed;
                    shot_list[2 * shot.slot as usize + 1] = u32::try_from(len).expect("fits");
                }
            }
            // (pipeline, params slot, workgroups) per pass.
            let mut passes: Vec<(&wgpu::ComputePipeline, u32, u64)> = Vec::new();
            let mut results: Vec<Vec<(u32, Answer)>> = vec![Vec::new(); batches.len()];
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
                    kron: self.strategy.kron,
                    sink: self.strategy.sink_noise,
                    hoist: self.strategy.hoist_noise,
                    clean: self.strategy.clean_variant,
                    round,
                    meta_off: 0,
                };
                // The union of the groups' shots, for the shared pieces.
                let union_off = u32::try_from(shot_list.len()).expect("fits");
                shot_list.extend(
                    shots[..running]
                        .iter()
                        .filter(|shot| shot.group.is_some_and(|group| middles[group].is_ok()))
                        .map(|shot| shot.slot),
                );
                let union_len = u32::try_from(shot_list.len()).expect("fits") - union_off;
                if union_len > 0 {
                    layout.emit(shared_pre, union_off, union_len, 0, 0, 0);
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
                        layout.emit(
                            middle,
                            shot_off,
                            shot_len,
                            shared_pre.measurements,
                            u32::try_from(pre).expect("fits"),
                            0,
                        );
                    }
                }
                let suf_off = shared_pre.measurements + most_middle;
                if union_len > 0 {
                    layout.emit(
                        shared_suf,
                        union_off,
                        union_len,
                        suf_off,
                        u32::try_from(suf).expect("fits"),
                        1,
                    );
                }
                total_measurements = suf_off + shared_suf.measurements;
                for (group, middle) in middles.iter().enumerate() {
                    let Ok(middle) = middle else { continue };
                    let shift = |offset: u32, sites: usize| {
                        let sites = u32::try_from(sites).expect("fits");
                        move |&(result, answer): &(u32, Answer)| {
                            let answer = match answer {
                                Answer::Meas(m) => Answer::Meas(m + offset),
                                Answer::NotLost => Answer::NotLost,
                                Answer::Readout {
                                    zero_as_one,
                                    one_as_zero,
                                    site,
                                } => Answer::Readout {
                                    zero_as_one,
                                    one_as_zero,
                                    site: site + sites,
                                },
                            };
                            (result, answer)
                        }
                    };
                    let len = batches[group].len();
                    results[group] = shared_pre
                        .results
                        .iter()
                        .map(shift(0, 0))
                        .chain(
                            middle
                                .results
                                .iter()
                                .map(shift(shared_pre.measurements, pre)),
                        )
                        .chain(shared_suf.results.iter().map(shift(suf_off, len - suf)))
                        .collect();
                }
            }
            let most_measurements = total_measurements;
            self.stats.translate += t.elapsed();

            // A round that writes no results: the machine yields such a batch
            // only at the program's return, so resume its shots now, before
            // submitting it; when every shot ends, its work is unobservable
            // and is dropped. (Were a shot to go on, the round is submitted
            // as usual, and the next round uses the stops reached.)
            if self.strategy.skip_tail && results.iter().all(Vec::is_empty) {
                answer(&mut shots[..running], &results, &[], slots, round);
                let t = Instant::now();
                resume_round(
                    machine,
                    &mut shots,
                    &self.failing,
                    &mut batches,
                    &mut failure,
                );
                resumes += 1;
                self.stats.machine += t.elapsed();
                if batches.is_empty() {
                    self.stats.skipped += 1;
                    break;
                }
                resumed_ahead = true;
            }

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
            if !resumed_ahead {
                answer(&mut shots[..running], &results, &values, slots, round);
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
