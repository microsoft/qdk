// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development branch only (not for review): the polyene iterative QPE
//! programs of the chemistry case (a development case, and a candidate
//! relevant case pending the chemistry team) run whole through
//! `MpsResumer`, with a census of what the grouped run did per round.
//!
//! ```text
//! cargo run --release -p qdk_simulators --example polyene_shots_dev -- \
//!     --bytecode ppp8_m6.bytecode.json [--params ppp8.params.json] \
//!     [--engine cpu|reference|cuda-mps|cutensornet] [--chi 64] [--shots 8] [--first-shot 0] \
//!     [--seed 1] [--mode grouped|per-shot] [--reset split|merge|fresh] \
//!     [--fusion unfused|pairs|seq|seq3] [--apply engine|mpo] \
//!     [--layout program|identity|interleave-centre] [--segments] \
//!     [--variant base|seq3-mpo] [--order resumer|identity|interleave-centre] \
//!     [--qr-moves] [--svd-algo gesvd|gesvdj]
//! ```
//!
//! `--variant` and `--order` (development only) are the flags the polyene
//! and H₂ harnesses share: `--variant base` is `--fusion unfused --apply
//! engine` (the defaults), `seq3-mpo` is `--fusion seq3 --apply mpo`;
//! `--order resumer` is `--layout program`, the others the layouts of the
//! same name. Each replaces the flags it names.
//!
//! `--layout identity` places qubit `q` on site `q` instead of the
//! resumer's order for the program; `interleave-centre` places the modes
//! α₀ β₀ α₁ β₁ … (qubit 1 + i is αᵢ, 1 + N + i is βᵢ) with the ancilla
//! (qubit 0) in the middle; the `LAYOUT` line gives the order used.
//!
//! `--fusion seq|seq3` fuses each run of gates in order, one block open at
//! a time, into blocks of at most 2 or 3 sites (the resumer's dev in-order
//! fusion); `--apply mpo` makes the CPU engine apply every operator on
//! non-adjacent sites (every 3-site block) as an MPO
//! (`CpuMpsEngine::with_dev_mpo_far`). `seq3` needs `--apply mpo`.
//!
//! `--engine cuda-mps|cutensornet` runs `CudaMpsEngine` or
//! `CuTensorNetEngine` on a GPU (Linux x86-64); `--apply mpo` gives the
//! first `with_dev_mpo_far` and the second `with_dev_wide_operators`
//! (three-site operators passed to cuTensorNet as one MPO); `--qr-moves`
//! (`with_dev_qr_moves`) and `--svd-algo` (`with_dev_svd_algo`) set
//! `cuda-mps`'s centre moves and SVD algorithm. `--engine reference` runs
//! the exact state vector (`ExactReference`),
//! whose bond reads are replaced by a constant (its Schmidt ranks cost
//! seconds at 17 qubits and change no draw). `--segments` prints one line
//! per segment (what one group ran of one batch). `--print-reads` prints
//! one `READ` line per P(One) the engine answers, in the run's order.
//!
//! Lines: `PROGRAM`, then one `SHOT` per shot (its bits in record order and
//! k = Σ r[j]·2^j), one `ROUND` per round (results drawn before / 2), and
//! `SUMMARY`.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::time::Instant;

use num_complex::Complex64;
use qdk_simulators::mps_simulator::{
    DevResetHandling, GateFusion, MpsResumer, Routing, SegmentCensus, start_census, take_census,
};
use qdk_simulators::qir::{QirMachine, QirProgram, QirResumer, QirShotId, expect_ready};
use qdk_simulators::{MeasurementResult, OutputRecord};
use tensornet::{MpsEngine, MpsShape, PauliSum};

#[path = "../tests/run_shot_compatibility/bytecode_json.rs"]
mod bytecode_json;

// The driver's on/off flags, one bool each.
#[allow(clippy::struct_excessive_bools)]
struct Options {
    bytecode: String,
    params: Option<String>,
    engine: String,
    chi: NonZeroUsize,
    shots: u64,
    first_shot: u64,
    seed: u64,
    per_shot: bool,
    reset: DevResetHandling,
    fusion: GateFusion,
    in_order: Option<usize>,
    mpo: bool,
    layout: String,
    segments: bool,
    print_reads: bool,
    /// `cuda-mps`'s centre moves by QR (`--qr-moves`) and its SVD
    /// algorithm (`--svd-algo`, its default if `None`).
    qr_moves: bool,
    svd_algo: Option<String>,
}

impl Options {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let flag = |name: &str| {
            args.iter()
                .position(|arg| arg == name)
                .and_then(|at| args.get(at + 1).cloned())
        };
        let number = |name: &str, default: u64| {
            flag(name).map_or(default, |value| value.parse().expect("a number"))
        };
        let reset = match flag("--reset").as_deref().unwrap_or("split") {
            "split" => DevResetHandling::Split,
            "merge" => DevResetHandling::Merge,
            "fresh" => DevResetHandling::Fresh,
            other => panic!("--reset {other}: split, merge or fresh"),
        };
        let (fusion_name, apply_name) = match flag("--variant").as_deref() {
            None => (
                flag("--fusion").unwrap_or_else(|| "unfused".to_string()),
                flag("--apply").unwrap_or_else(|| "engine".to_string()),
            ),
            Some(_) if flag("--fusion").is_some() || flag("--apply").is_some() => {
                panic!("--variant replaces --fusion and --apply: give one or the other")
            }
            Some("base") => ("unfused".to_string(), "engine".to_string()),
            Some("seq3-mpo") => ("seq3".to_string(), "mpo".to_string()),
            Some(other) => panic!("--variant {other}: base or seq3-mpo"),
        };
        let (fusion, in_order) = match fusion_name.as_str() {
            "unfused" => (GateFusion::Unfused, None),
            "pairs" => (GateFusion::Pairs, None),
            "seq" => (GateFusion::Unfused, Some(2)),
            "seq3" => (GateFusion::Unfused, Some(3)),
            other => panic!("--fusion {other}: unfused, pairs, seq or seq3"),
        };
        let mpo = match apply_name.as_str() {
            "engine" => false,
            "mpo" => true,
            other => panic!("--apply {other}: engine or mpo"),
        };
        assert!(in_order != Some(3) || mpo, "seq3 needs --apply mpo");
        let per_shot = match flag("--mode").as_deref().unwrap_or("grouped") {
            "grouped" => false,
            "per-shot" => true,
            other => panic!("--mode {other}: grouped or per-shot"),
        };
        let layout = match (flag("--order").as_deref(), flag("--layout")) {
            (Some(_), Some(_)) => panic!("--order replaces --layout: give one or the other"),
            (Some("resumer"), None) => "program".to_string(),
            (Some(order), None) => order.to_string(),
            (None, layout) => layout.unwrap_or_else(|| "program".to_string()),
        };
        assert!(
            ["program", "identity", "interleave-centre"].contains(&layout.as_str()),
            "--layout {layout}: program, identity or interleave-centre"
        );
        Self {
            bytecode: flag("--bytecode").expect("--bytecode <path>"),
            params: flag("--params"),
            engine: flag("--engine").unwrap_or_else(|| "cpu".to_string()),
            chi: NonZeroUsize::new(usize::try_from(number("--chi", 64)).expect("fits"))
                .expect("χ > 0"),
            shots: number("--shots", 1),
            first_shot: number("--first-shot", 0),
            seed: number("--seed", 1),
            per_shot,
            reset,
            fusion,
            in_order,
            mpo,
            layout,
            segments: args.iter().any(|arg| arg == "--segments"),
            print_reads: args.iter().any(|arg| arg == "--print-reads"),
            qr_moves: args.iter().any(|arg| arg == "--qr-moves"),
            svd_algo: flag("--svd-algo"),
        }
    }
}

/// The exact reference with a constant bond read: its Schmidt ranks take
/// seconds at 17 qubits, and the run uses the bond only for its report.
struct NoShape<E>(E);

struct WithSites<S> {
    inner: S,
    sites: usize,
}

impl<E: MpsEngine> MpsEngine for NoShape<E> {
    type State = WithSites<E::State>;
    type Error = E::Error;

    fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, E::Error> {
        Ok(WithSites {
            inner: self.0.prepare(capacity)?,
            sites: capacity.site_count(),
        })
    }
    fn apply(
        &self,
        state: &mut Self::State,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), E::Error> {
        self.0.apply(&mut state.inner, sites, matrix)
    }
    fn probability_of_one(&self, state: &Self::State, site: usize) -> Result<f64, E::Error> {
        self.0.probability_of_one(&state.inner, site)
    }
    fn project(&self, state: &mut Self::State, site: usize, one: bool) -> Result<f64, E::Error> {
        self.0.project(&mut state.inner, site, one)
    }
    fn fork(&self, state: &Self::State) -> Result<Self::State, E::Error> {
        Ok(WithSites {
            inner: self.0.fork(&state.inner)?,
            sites: state.sites,
        })
    }
    fn sample(
        &self,
        state: &Self::State,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, E::Error> {
        self.0.sample(&state.inner, sites, shots, seed)
    }
    fn expectation(
        &self,
        state: &Self::State,
        observable: &PauliSum,
    ) -> Result<Complex64, E::Error> {
        self.0.expectation(&state.inner, observable)
    }
    fn shape(&self, state: &Self::State) -> Result<MpsShape, E::Error> {
        Ok(MpsShape::qubit_chain(state.sites, 1).expect("a chain of bond 1"))
    }
    fn retained_weight(&self, state: &Self::State) -> Result<f64, E::Error> {
        self.0.retained_weight(&state.inner)
    }
}

/// The site of each qubit in the `interleave-centre` order: α₀ β₀ α₁ β₁ …
/// with the ancilla (qubit 0) in the middle, where qubit 1 + i is αᵢ and
/// qubit 1 + N + i is βᵢ (the params' mode m on qubit 1 + m).
fn interleave_centre(qubits: usize) -> Vec<usize> {
    let n = (qubits - 1) / 2;
    assert_eq!(2 * n + 1, qubits, "an ancilla and 2N modes");
    let mut order: Vec<usize> = (0..n).flat_map(|i| [1 + i, 1 + n + i]).collect();
    order.insert(order.len() / 2, 0);
    let mut sites = vec![0; qubits];
    for (site, &qubit) in order.iter().enumerate() {
        sites[qubit] = site;
    }
    sites
}

/// Development only: an engine printing each P(One) it answers
/// (`--print-reads`), in the order the run reads them, to compare two
/// engines read by read.
struct PrintReads<E>(E);

impl<E: MpsEngine> MpsEngine for PrintReads<E> {
    type State = E::State;
    type Error = E::Error;

    fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, E::Error> {
        self.0.prepare(capacity)
    }
    fn apply(
        &self,
        state: &mut Self::State,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), E::Error> {
        self.0.apply(state, sites, matrix)
    }
    fn probability_of_one(&self, state: &Self::State, site: usize) -> Result<f64, E::Error> {
        let p = self.0.probability_of_one(state, site)?;
        println!("READ site={site} p={p:.9}");
        Ok(p)
    }
    fn project(&self, state: &mut Self::State, site: usize, one: bool) -> Result<f64, E::Error> {
        self.0.project(state, site, one)
    }
    fn fork(&self, state: &Self::State) -> Result<Self::State, E::Error> {
        self.0.fork(state)
    }
    fn sample(
        &self,
        state: &Self::State,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, E::Error> {
        self.0.sample(state, sites, shots, seed)
    }
    fn expectation(
        &self,
        state: &Self::State,
        observable: &PauliSum,
    ) -> Result<Complex64, E::Error> {
        self.0.expectation(state, observable)
    }
    fn shape(&self, state: &Self::State) -> Result<MpsShape, E::Error> {
        self.0.shape(state)
    }
    fn retained_weight(&self, state: &Self::State) -> Result<f64, E::Error> {
        self.0.retained_weight(state)
    }
}

fn bits(records: &[OutputRecord]) -> Vec<bool> {
    records
        .iter()
        .filter_map(|record| match record {
            OutputRecord::Result(MeasurementResult::Zero) => Some(false),
            OutputRecord::Result(MeasurementResult::One) => Some(true),
            OutputRecord::Result(MeasurementResult::Loss) => panic!("no loss in this program"),
            _ => None,
        })
        .collect()
}

fn phase_index(bits: &[bool]) -> usize {
    bits.iter()
        .enumerate()
        .map(|(j, &one)| usize::from(one) << j)
        .sum()
}

fn bin_energies(path: &str, m: usize) -> Vec<f64> {
    let json = std::fs::read_to_string(path).expect("the params are readable");
    let params: serde_json::Value = serde_json::from_str(&json).expect("params JSON");
    params["programs"][m.to_string()]["bin_energy_Ha"]
        .as_array()
        .expect("bin energies")
        .iter()
        .map(|value| value.as_f64().expect("an energy"))
        .collect()
}

fn main() {
    let options = Options::parse();
    let json = std::fs::read_to_string(&options.bytecode).expect("the bytecode is readable");
    let bytecode: bytecode_json::Bytecode = serde_json::from_str(&json).expect("bytecode JSON");
    let program = QirProgram::new(bytecode.program()).expect("the program builds");
    println!(
        "PROGRAM qubits={} two_qubit_gates={} engine={} chi={} seed={} first_shot={} shots={} \
         mode={} reset={:?} fusion={:?} in_order={:?} mpo={} layout={} qr_moves={} svd_algo={}",
        program.qubits(),
        program.two_qubit_gates().len(),
        options.engine,
        options.chi,
        options.seed,
        options.first_shot,
        options.shots,
        if options.per_shot {
            "per-shot"
        } else {
            "grouped"
        },
        options.reset,
        options.fusion,
        options.in_order,
        options.mpo,
        options.layout,
        options.qr_moves,
        options.svd_algo.as_deref().unwrap_or("default"),
    );
    assert!(
        options.engine == "cuda-mps" || !options.qr_moves && options.svd_algo.is_none(),
        "--qr-moves and --svd-algo: --engine cuda-mps only"
    );
    let cpu = || qdk_cpu_mps::CpuMpsEngine::default().with_dev_mpo_far(options.mpo);
    let reference = || NoShape(tensornet::reference::ExactReference::new());
    match (options.engine.as_str(), options.print_reads) {
        ("cpu", false) => run(&options, &program, cpu()),
        ("cpu", true) => run(&options, &program, PrintReads(cpu())),
        ("reference", false) => run(&options, &program, reference()),
        ("reference", true) => run(&options, &program, PrintReads(reference())),
        ("cuda-mps" | "cutensornet", false) => run_on_gpu(&options, &program),
        (other, _) => panic!(
            "--engine {other}: cpu, reference, cuda-mps or cutensornet \
             (--print-reads on cpu or reference only)"
        ),
    }
}

/// The run on `CudaMpsEngine` (`cuda-mps`) or `CuTensorNetEngine`
/// (`cutensornet`), `--apply mpo` choosing their development flags.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_on_gpu(options: &Options, program: &QirProgram) {
    let availability = qdk_cutensornet::discover().expect("cuTensorNet and the CUDA Runtime load");
    println!("AVAILABILITY {:?}", availability.report());
    if options.engine == "cuda-mps" {
        let mut engine = qdk_cutensornet::CudaMpsEngine::new(&availability)
            .expect("the tensor API on CUDA device 0")
            .with_dev_mpo_far(options.mpo)
            .with_dev_qr_moves(options.qr_moves);
        if let Some(name) = &options.svd_algo {
            let algo = qdk_cutensornet::dev_gate_split::TensorSvdAlgo::from_name(name)
                .unwrap_or_else(|| panic!("--svd-algo {name}: gesvd or gesvdj"));
            engine = engine
                .with_dev_svd_algo(algo)
                .unwrap_or_else(|error| panic!("--svd-algo {name}: {error}"));
        }
        run(options, program, engine);
    } else {
        let engine = qdk_cutensornet::CuTensorNetEngine::new(&availability)
            .expect("an engine on CUDA device 0")
            .with_dev_wide_operators(options.mpo);
        run(options, program, engine);
    }
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn run_on_gpu(options: &Options, _program: &QirProgram) {
    panic!("--engine {}: runs on Linux x86-64 only", options.engine);
}

#[allow(clippy::too_many_lines)] // a development driver, printed in one place
fn run<E: MpsEngine>(options: &Options, program: &QirProgram, engine: E)
where
    E::Error: std::fmt::Debug,
{
    let mut resumer = MpsResumer::new(engine, options.seed)
        .with_max_bond_dimension(options.chi)
        .with_gate_fusion(options.fusion)
        .with_routing(Routing::Engine)
        .with_dev_reset_handling(options.reset);
    match options.layout.as_str() {
        "identity" => resumer = resumer.with_dev_identity_layout(),
        "interleave-centre" => {
            resumer = resumer.with_dev_layout(interleave_centre(program.qubits() as usize));
        }
        _ => {}
    }
    if let Some(max_sites) = options.in_order {
        resumer = resumer.with_dev_in_order_fusion(max_sites);
    }
    let machine = QirMachine::new(program);
    let shots: Vec<QirShotId> = (options.first_shot..options.first_shot + options.shots)
        .map(|id| QirShotId(u32::try_from(id).expect("a shot id fits a u32")))
        .collect();
    let calls: Vec<&[QirShotId]> = if options.per_shot {
        shots.chunks(1).collect()
    } else {
        vec![&shots[..]]
    };
    start_census();
    let started = Instant::now();
    let (mut largest_bond, mut least_weight) = (0, 1.0_f64);
    let mut histogram: BTreeMap<usize, u64> = BTreeMap::new();
    let mut m = 0;
    for call in calls {
        let call_started = Instant::now();
        let records = match expect_ready(resumer.run(&machine, call)) {
            Ok(records) => records,
            Err(error) => {
                println!(
                    "FAILED seconds={:.1} error={error:?}",
                    started.elapsed().as_secs_f64()
                );
                std::process::exit(1);
            }
        };
        let report = resumer.last_run();
        largest_bond = largest_bond.max(report.largest_bond);
        least_weight = least_weight.min(report.least_retained_weight);
        for (shot, records) in call.iter().zip(&records) {
            let bits = bits(records);
            m = bits.len();
            let k = phase_index(&bits);
            *histogram.entry(k).or_default() += 1;
            let text: String = bits
                .iter()
                .map(|&one| if one { '1' } else { '0' })
                .collect();
            println!(
                "SHOT id={} bits={text} k={k} call_seconds={:.1} weight={:.6}",
                shot.0,
                call_started.elapsed().as_secs_f64(),
                report.least_retained_weight,
            );
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    let segments = take_census();
    if options.segments {
        for s in &segments {
            println!(
                "SEGMENT rb={} batch={} start={}/{} shots={}->{} had_state={} applies={} \
                 two={} meas={} resets={} msplit={} rsplit={} merge={} fresh={} apply_ms={:.1} \
                 draw_ms={:.1} fork_ms={:.1} host_ms={:.1} h2={:016x} ended={} w={:?}",
                s.results_before,
                s.batch,
                s.start,
                s.batch_steps,
                s.shots,
                s.shots_at_end,
                s.had_state,
                s.applies,
                s.two_site,
                s.measures,
                s.resets,
                s.measure_splits,
                s.reset_splits,
                s.reset_merges,
                s.fresh_windows,
                ms(s.apply_ns),
                ms(s.draw_ns),
                ms(s.fork_ns),
                ms(s.host_ns),
                s.hash_two_site,
                s.ended,
                s.retained_weight,
            );
        }
    }
    print_rounds(&segments, m);
    let total = |field: fn(&SegmentCensus) -> u64| segments.iter().map(field).sum::<u64>();
    let states = 1 + total(|s| s.forks);
    let controlled_steps: u64 = controlled_steps(&segments, m).values().sum();
    let hist: Vec<String> = histogram.iter().map(|(k, n)| format!("{k}:{n}")).collect();
    let energy = options.params.as_ref().map(|path| {
        let energies = bin_energies(path, m);
        let n: u64 = histogram.values().sum();
        #[expect(clippy::cast_precision_loss, reason = "counts of shots")]
        let mean = histogram
            .iter()
            .map(|(&k, &count)| energies[k] * count as f64)
            .sum::<f64>()
            / n as f64;
        let (&mode, _) = histogram
            .iter()
            .max_by_key(|&(_, &count)| count)
            .expect("a shot");
        format!(
            " mode_k={mode} mode_energy={:.6} mean_energy={mean:.6}",
            energies[mode]
        )
    });
    println!(
        "SUMMARY seconds={seconds:.1} shots={} states={states} controlled_steps={controlled_steps} \
         applies={} apply_s={:.1} draw_s={:.1} fork_s={:.1} fresh_s={:.1} bond_s={:.1} host_s={:.1} \
         prob_reads={} proj_reads={} shape_reads={} weight_reads={} largest_bond={largest_bond} \
         least_weight={least_weight:.6}{} hist={}",
        options.shots,
        total(|s| s.applies),
        secs(total(|s| s.apply_ns)),
        secs(total(|s| s.draw_ns)),
        secs(total(|s| s.fork_ns)),
        secs(total(|s| s.fresh_ns)),
        secs(total(|s| s.bond_ns)),
        secs(total(|s| s.host_ns)),
        total(|s| s.probability_reads),
        total(|s| s.project_reads),
        total(|s| s.shape_reads),
        total(|s| s.weight_reads),
        energy.unwrap_or_default(),
        hist.join(","),
    );
}

#[expect(clippy::cast_precision_loss, reason = "durations for a report")]
fn ms(ns: u64) -> f64 {
    ns as f64 / 1e6
}

#[expect(clippy::cast_precision_loss, reason = "durations for a report")]
fn secs(ns: u64) -> f64 {
    ns as f64 / 1e9
}

/// Whether a segment holds a round's controlled steps: it measures the
/// round's phase result (its first draw) after many two-site operators.
fn is_controlled(segment: &SegmentCensus) -> bool {
    segment.results_before.is_multiple_of(2) && segment.measures == 1 && segment.two_site > 100
}

/// The controlled steps each round's states ran: a round r of M runs
/// 2^(M-1-r) steps, once per state that ran its controlled segment.
fn controlled_steps(segments: &[SegmentCensus], m: usize) -> BTreeMap<usize, u64> {
    let mut steps = BTreeMap::new();
    for segment in segments.iter().filter(|s| is_controlled(s)) {
        let round = segment.results_before / 2;
        *steps.entry(round).or_default() += 1u64 << (m - 1 - round);
    }
    steps
}

fn print_rounds(segments: &[SegmentCensus], m: usize) {
    let steps = controlled_steps(segments, m);
    let rounds: BTreeSet<usize> = segments.iter().map(|s| s.results_before / 2).collect();
    for round in rounds {
        let of_round: Vec<&SegmentCensus> = segments
            .iter()
            .filter(|s| s.results_before / 2 == round)
            .collect();
        let sum = |field: fn(&SegmentCensus) -> u64| of_round.iter().map(|s| field(s)).sum::<u64>();
        let controlled: Vec<&&SegmentCensus> =
            of_round.iter().filter(|s| is_controlled(s)).collect();
        let full: BTreeSet<u64> = controlled.iter().map(|s| s.hash_all).collect();
        let two: BTreeSet<u64> = controlled.iter().map(|s| s.hash_two_site).collect();
        let max_cohort = {
            let mut cohorts: BTreeMap<u64, u64> = BTreeMap::new();
            for s in &controlled {
                *cohorts.entry(s.hash_two_site).or_default() += 1;
            }
            cohorts.values().copied().max().unwrap_or(0)
        };
        println!(
            "ROUND r={round} segments={} controlled_states={} controlled_steps={} \
             lockstep_full={} lockstep_two_site={} max_cohort={max_cohort} applies={} \
             apply_s={:.1} draw_s={:.1} fork_s={:.1} fresh_s={:.1} bond_s={:.1} host_s={:.1} \
             measures={} resets={} msplits={} rsplits={} merges={} fresh={} forks={} \
             prob_reads={} proj_reads={} shape_reads={} weight_reads={}",
            of_round.len(),
            controlled.len(),
            steps.get(&round).copied().unwrap_or(0),
            full.len(),
            two.len(),
            sum(|s| s.applies),
            secs(sum(|s| s.apply_ns)),
            secs(sum(|s| s.draw_ns)),
            secs(sum(|s| s.fork_ns)),
            secs(sum(|s| s.fresh_ns)),
            secs(sum(|s| s.bond_ns)),
            secs(sum(|s| s.host_ns)),
            sum(|s| s.measures),
            sum(|s| s.resets),
            sum(|s| s.measure_splits),
            sum(|s| s.reset_splits),
            sum(|s| s.reset_merges),
            sum(|s| s.fresh_windows),
            sum(|s| s.forks),
            sum(|s| s.probability_reads),
            sum(|s| s.project_reads),
            sum(|s| s.shape_reads),
            sum(|s| s.weight_reads),
        );
    }
}
