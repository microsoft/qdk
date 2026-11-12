// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development branch only (not for review): H₂'s phase estimation (or any
//! Adaptive Profile bytecode) through `MpsResumer`, on the engine a flag
//! chooses: the exact reference (at most 20 qubits, so not H₂), the CPU
//! engine, or cuTensorNet on a GPU (Linux x86-64).
//!
//! Each shot of the slice `--first-shot N --shots M` runs alone (per shot),
//! on its own engine, `--threads` at a time. The shot ids are global, so
//! slices run in separate processes give each shot the records one run of
//! all of them gives: 16 processes under CUDA MPS, each a slice, run 16
//! shots at once on one A100. Each shot prints one `SHOT` line
//! (`key=value`), which `h2_summary.py` aggregates (throughput, peaks,
//! G-test, the acceptance checks).
//!
//! With `--readout TABLE`, the shot's readout is checked against the exact
//! table of the sparse simulator: an engine wrapper forks the state before
//! the readout's first draw (`--readout-first-draw`, H₂'s 1,399: its
//! readout is its last five measurements, before its ten resets), and its
//! exact joint distribution is read on that fork by conditional
//! probabilities, after the shot's time is taken. The TVD and the two
//! peaks go in the `SHOT` line.
//!
//! ```text
//! cargo run --release -p qdk_simulators --example h2_dev -- \
//!     --engine cpu|cutensornet|reference --bytecode H2.bytecode.json \
//!     [--readout TABLE] [--readout-first-draw 1399] [--seed 1] [--chi 128] \
//!     [--first-shot 0] [--shots 1] [--threads 1] \
//!     [--routing engine|meet-halfway|meet-halfway-after-fusion]
//! ```
//!
//! `--routing` (EXPERIMENTAL) chooses the resumer's `Routing` of distant
//! two-qubit gates; `engine`, the default, is the resumer's behaviour
//! without it.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use num_complex::Complex64;
use qdk_simulators::mps_simulator::{MpsResumer, Routing};
use qdk_simulators::qir::{QirMachine, QirProgram, QirResumer, QirShotId, expect_ready};
use qdk_simulators::{MeasurementResult, OutputRecord};
use tensornet::{MpsEngine, MpsShape, PauliSum};

#[path = "../tests/run_shot_compatibility/bytecode_json.rs"]
mod bytecode_json;

/// The results a readout reads.
const READOUT: usize = 5;

/// What the probe saw of a shot.
struct Seen<S> {
    draws: Cell<usize>,
    /// The draw before which the readout starts.
    readout_first_draw: Option<usize>,
    /// The state before the readout's first draw.
    readout_state: RefCell<Option<S>>,
    /// The sites of the readout's draws, in order.
    readout_sites: RefCell<Vec<usize>>,
}

/// The engine `E`, observed (see the module documentation).
struct Probe<E: MpsEngine> {
    inner: Rc<E>,
    seen: Rc<Seen<E::State>>,
}

impl<S> Seen<S> {
    /// The exact joint distribution of the readout's draws, in the order
    /// they were drawn (the first the most significant bit), read by
    /// `engine` on the state before the first of them.
    fn readout_distribution<E: MpsEngine<State = S>>(&self, engine: &E) -> Option<Vec<f64>> {
        let start = self.readout_state.borrow();
        let state = start.as_ref()?;
        let sites = self.readout_sites.borrow().clone();
        let mut rows = vec![(Vec::<(usize, bool)>::new(), 1.0)];
        for &site in &sites {
            let mut next = Vec::new();
            for (given, p) in rows {
                let p_one = if p == 0.0 {
                    0.5
                } else {
                    engine
                        .conditional_probability_of_one(state, site, &given)
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0)
                };
                for (one, q) in [(false, 1.0 - p_one), (true, p_one)] {
                    let mut given = given.clone();
                    given.push((site, one));
                    next.push((given, if p == 0.0 { 0.0 } else { p * q }));
                }
            }
            rows = next;
        }
        Some(rows.into_iter().map(|(_, p)| p).collect())
    }
}

impl<E: MpsEngine> MpsEngine for Probe<E> {
    type State = E::State;
    type Error = E::Error;

    fn prepare(&self, capacity: &MpsShape) -> Result<E::State, E::Error> {
        self.inner.prepare(capacity)
    }

    fn apply(
        &self,
        state: &mut E::State,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), E::Error> {
        self.inner.apply(state, sites, matrix)
    }

    fn probability_of_one(&self, state: &E::State, site: usize) -> Result<f64, E::Error> {
        let draw = self.seen.draws.get();
        if let Some(first) = self.seen.readout_first_draw
            && (first..first + READOUT).contains(&draw)
        {
            if draw == first {
                *self.seen.readout_state.borrow_mut() = Some(self.inner.fork(state)?);
            }
            self.seen.readout_sites.borrow_mut().push(site);
        }
        self.inner.probability_of_one(state, site)
    }

    fn project(&self, state: &mut E::State, site: usize, one: bool) -> Result<f64, E::Error> {
        self.seen.draws.set(self.seen.draws.get() + 1);
        self.inner.project(state, site, one)
    }

    fn fork(&self, state: &E::State) -> Result<E::State, E::Error> {
        self.inner.fork(state)
    }

    fn sample(
        &self,
        state: &E::State,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, E::Error> {
        self.inner.sample(state, sites, shots, seed)
    }

    fn expectation(&self, state: &E::State, observable: &PauliSum) -> Result<Complex64, E::Error> {
        self.inner.expectation(state, observable)
    }

    fn shape(&self, state: &E::State) -> Result<MpsShape, E::Error> {
        self.inner.shape(state)
    }

    fn retained_weight(&self, state: &E::State) -> Result<f64, E::Error> {
        self.inner.retained_weight(state)
    }
}

/// The exact table: record-order bits (result order) to probability.
fn table(path: &str) -> BTreeMap<String, f64> {
    std::fs::read_to_string(path)
        .expect("the readout table is readable")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            (
                fields[0].to_string(),
                fields[2].parse().expect("a probability"),
            )
        })
        .collect()
}

fn bits(records: &[OutputRecord]) -> String {
    records
        .iter()
        .filter_map(|record| match record {
            OutputRecord::Result(MeasurementResult::Zero) => Some('0'),
            OutputRecord::Result(MeasurementResult::One) => Some('1'),
            OutputRecord::Result(MeasurementResult::Loss) => Some('L'),
            _ => None,
        })
        .collect()
}

fn unix_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after 1970")
        .as_secs_f64()
}

/// The options.
struct Options {
    engine: String,
    bytecode: String,
    readout: Option<BTreeMap<String, f64>>,
    readout_first_draw: usize,
    seed: u64,
    chi: NonZeroUsize,
    first_shot: u32,
    shots: u32,
    threads: usize,
    routing: (String, Routing),
}

impl Options {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let value = |flag: &str| {
            args.iter().position(|arg| arg == flag).map(|at| {
                args.get(at + 1)
                    .unwrap_or_else(|| panic!("{flag} takes a value"))
                    .clone()
            })
        };
        let number = |flag: &str, default: u64| {
            value(flag).map_or(default, |v| {
                v.parse().unwrap_or_else(|_| panic!("{flag}: a number"))
            })
        };
        let as_usize = |n: u64| usize::try_from(n).expect("fits a usize");
        Self {
            engine: value("--engine").unwrap_or_else(|| "cpu".to_string()),
            bytecode: value("--bytecode").expect("--bytecode FILE: the program's bytecode JSON"),
            readout: value("--readout").map(|path| table(&path)),
            readout_first_draw: as_usize(number("--readout-first-draw", 1399)),
            seed: number("--seed", 1),
            chi: NonZeroUsize::new(as_usize(number("--chi", 128))).expect("--chi: at least 1"),
            first_shot: u32::try_from(number("--first-shot", 0)).expect("a shot id"),
            shots: u32::try_from(number("--shots", 1)).expect("a shot count"),
            threads: as_usize(number("--threads", 1)).max(1),
            routing: {
                let name = value("--routing").unwrap_or_else(|| "engine".to_string());
                let routing = match name.as_str() {
                    "engine" => Routing::Engine,
                    "meet-halfway" => Routing::MeetHalfway,
                    "meet-halfway-after-fusion" => Routing::MeetHalfwayAfterFusion,
                    other => panic!(
                        "--routing {other}: engine, meet-halfway or meet-halfway-after-fusion"
                    ),
                };
                (name, routing)
            },
        }
    }
}

/// Runs shot `shot` on an engine `make` makes, and prints its `SHOT` line.
/// Whether it succeeded.
fn run_shot<E: MpsEngine>(
    options: &Options,
    program: &QirProgram,
    make: &(dyn Fn() -> E + Sync),
    shot: u32,
) -> bool
where
    E::Error: std::fmt::Debug,
{
    let engine = Rc::new(make());
    let seen = Rc::new(Seen {
        draws: Cell::new(0),
        readout_first_draw: options.readout.as_ref().map(|_| options.readout_first_draw),
        readout_state: RefCell::new(None),
        readout_sites: RefCell::new(Vec::new()),
    });
    let probe = Probe {
        inner: Rc::clone(&engine),
        seen: Rc::clone(&seen),
    };
    let mut resumer = MpsResumer::new(probe, options.seed)
        .with_max_bond_dimension(options.chi)
        .with_routing(options.routing.1);
    let (start, started) = (unix_seconds(), Instant::now());
    let run = expect_ready(resumer.run(&QirMachine::new(program), &[QirShotId(shot)]));
    let seconds = started.elapsed().as_secs_f64();
    let end = unix_seconds();
    let records = match run {
        Ok(records) => bits(&records[0]),
        Err(error) => {
            println!("FAILED shot={shot} seconds={seconds:.1} error={error:?}");
            return false;
        }
    };
    let report = *resumer.last_run();
    let mut line = format!(
        "SHOT shot={shot} engine={} routing={} chi={} seed={} start_unix={start:.3} end_unix={end:.3} \
         seconds={seconds:.1} records={records} draws={} largest_bond={} lost_weight={:.3e}",
        options.engine,
        options.routing.0,
        options.chi,
        options.seed,
        seen.draws.get(),
        report.largest_bond,
        1.0 - report.least_retained_weight,
    );
    if let Some(table) = &options.readout {
        match seen.readout_distribution(&*engine) {
            Some(distribution) if distribution.len() == 1 << READOUT => {
                let row = |index: usize| format!("{index:0READOUT$b}");
                let tvd = 0.5
                    * distribution
                        .iter()
                        .enumerate()
                        .map(|(index, p)| {
                            (p - table.get(&row(index)).copied().unwrap_or(0.0)).abs()
                        })
                        .sum::<f64>();
                let mut order: Vec<usize> = (0..distribution.len()).collect();
                order.sort_by(|a, b| distribution[*b].total_cmp(&distribution[*a]));
                // Chemistry order: the record order reversed.
                let peaks: Vec<String> = order
                    .iter()
                    .take(2)
                    .map(|&i| row(i).chars().rev().collect())
                    .collect();
                write!(line, " tvd={tvd:.3e} peaks={}", peaks.join(","))
                    .expect("a String takes any text");
            }
            _ => line += " tvd=none peaks=none",
        }
    }
    // The readout's fork is released before the engine.
    seen.readout_state.borrow_mut().take();
    drop(resumer);
    println!("{line}");
    true
}

/// Runs the slice's shots, `options.threads` at a time. Whether all
/// succeeded.
fn run_slice<E: MpsEngine>(
    options: &Options,
    program: &QirProgram,
    make: &(dyn Fn() -> E + Sync),
) -> bool
where
    E::Error: std::fmt::Debug,
{
    let ok = AtomicBool::new(true);
    let next = std::sync::atomic::AtomicU32::new(options.first_shot);
    let end = options.first_shot + options.shots;
    std::thread::scope(|scope| {
        for _ in 0..options.threads {
            scope.spawn(|| {
                loop {
                    let shot = next.fetch_add(1, Ordering::Relaxed);
                    if shot >= end {
                        break;
                    }
                    if !run_shot(options, program, make, shot) {
                        ok.store(false, Ordering::Relaxed);
                    }
                }
            });
        }
    });
    ok.into_inner()
}

fn main() {
    let options = Options::parse();
    let json = std::fs::read_to_string(&options.bytecode).expect("the bytecode is readable");
    let bytecode: bytecode_json::Bytecode = serde_json::from_str(&json).expect("bytecode JSON");
    let program = QirProgram::new(bytecode.program()).expect("the program builds");
    println!(
        "PROGRAM qubits={} two_qubit_gates={} measurements={} engine={} routing={} chi={} seed={} \
         first_shot={} shots={} threads={} readout={}",
        program.qubits(),
        program.two_qubit_gates().len(),
        program.measured_qubits().len(),
        options.engine,
        options.routing.0,
        options.chi,
        options.seed,
        options.first_shot,
        options.shots,
        options.threads,
        options.readout.is_some(),
    );
    let ok = match options.engine.as_str() {
        "reference" => run_slice(
            &options,
            &program,
            &tensornet::reference::ExactReference::new,
        ),
        "cpu" => run_slice(&options, &program, &qdk_cpu_mps::CpuMpsEngine::default),
        "cutensornet" => run_on_cutensornet(&options, &program),
        other => panic!("--engine {other}: reference, cpu or cutensornet"),
    };
    if !ok {
        std::process::exit(1);
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_on_cutensornet(options: &Options, program: &QirProgram) -> bool {
    let availability = qdk_cutensornet::discover().expect("cuTensorNet and the CUDA Runtime load");
    println!("AVAILABILITY {:?}", availability.report());
    let make = || {
        availability
            .mps_engine()
            .expect("an engine on CUDA device 0")
    };
    run_slice(options, program, &make)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn run_on_cutensornet(_options: &Options, _program: &QirProgram) -> bool {
    let error = qdk_cutensornet::discover().expect_err("off Linux x86-64");
    panic!("cuTensorNet runs on Linux x86-64 only: {error}");
}
