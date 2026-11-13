// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development branch only (not for review): H₂'s phase estimation (or any
//! Adaptive Profile bytecode) through `MpsResumer`, on the engine a flag
//! chooses: the exact reference (at most 20 qubits, so not H₂), the CPU
//! engine, or cuTensorNet on a GPU (Linux x86-64): its state API
//! (`cutensornet`), or the CPU engine's algorithm on its tensor API
//! (`cuda-mps`).
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
//!     --engine cpu|cutensornet|cuda-mps|reference \
//!     --bytecode H2.bytecode.json \
//!     [--readout TABLE] [--readout-first-draw 1399] [--seed 1] [--chi 128] \
//!     [--first-shot 0] [--shots 1] [--threads 1] [--group 1] \
//!     [--routing engine|meet-halfway|meet-halfway-after-fusion] \
//!     [--fusion pairs|unfused|seq|seq3] [--apply engine|mpo] \
//!     [--reset split|merge]
//! ```
//!
//! Development only: `--fusion seq|seq3` fuses each run of gates in order
//! into blocks of at most 2 or 3 sites (the resumer's dev in-order fusion,
//! with `--routing engine` only); `--apply mpo` makes the CPU engine apply
//! every operator on non-adjacent sites as an MPO
//! (`CpuMpsEngine::with_dev_mpo_far`); `--reset merge` keeps a grouped
//! call's shots together at a run of resets that settles every site
//! (`DevResetHandling::Merge`; it acts only with `--group`).
//!
//! `--group K` (development only) runs each K consecutive shots of the slice
//! in one `run` call, so that they share a state until a draw splits them:
//! one `CALL` line per call (time, forks, the TVD of each state that reached
//! the readout) and one `GSHOT` line per shot (its records). `--census`
//! (development only) adds one `CENSUS` line per call: its resets, the
//! draws that split it, and the resets `--reset merge` merged.
//!
//! `--routing` (EXPERIMENTAL) chooses the resumer's `Routing` of distant
//! two-qubit gates; `engine`, the default, is the resumer's behaviour
//! without it.
//!
//! `--record-calls FILE [--record-draws 300]` (development only, one shot)
//! writes the engine calls of the shot's state until its N-th draw, with
//! their answers, for `qdk_cutensornet`'s `tuning_bench` to replay. One
//! line per call:
//!
//! ```text
//! # calls v1 key=value...      the run that recorded it
//! P n b0 b1 ... b(n-2)         prepare: n qubits, the capacity's bonds
//! M id re im re im ...         a matrix, first used next (row-major)
//! A id site...                 apply matrix id
//! R site p                     probability_of_one, its answer
//! J site 0|1 p                 project, its answer
//! S b0 b1 ...                  shape, the realized bonds
//! W w                          retained_weight, its answer
//! U name                       a call the replay does not support; the
//!                              trace stops there
//! ```

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use num_complex::Complex64;
use qdk_simulators::mps_simulator::{
    DevResetHandling, GateFusion, MpsResumer, Restore, RouteTuning, Routing, reset_routing_trace,
    routing_trace,
};
use qdk_simulators::qir::{QirMachine, QirProgram, QirResumer, QirShotId, expect_ready};
use qdk_simulators::{MeasurementResult, OutputRecord};
use tensornet::{MpsEngine, MpsShape, PauliSum};

#[path = "../tests/run_shot_compatibility/bytecode_json.rs"]
mod bytecode_json;

/// The results a readout reads.
const READOUT: usize = 5;

/// Development only: a state of the probe, with the draws it has seen
/// (copied on a fork) and the readout it reached, if any.
struct Tagged<S> {
    inner: S,
    draws: usize,
    readout: Cell<Option<usize>>,
}

/// A state before a readout's first draw, and the readout's sites.
struct Readout<S> {
    state: S,
    sites: Vec<Option<usize>>,
}

/// What the probe saw of a shot (or of a call's shots, with `--group`).
struct Seen<S> {
    /// The draws of all states.
    draws: Cell<usize>,
    /// The forks.
    forks: Cell<usize>,
    /// Development only: the CPU engine's capacity-bound splits seen so
    /// far, and when the first and last of them happened (draw, routed
    /// batch, whether routing had fallen back then).
    capacity_bound: Cell<u64>,
    first_capacity_bound: Cell<Option<(usize, u64, bool)>>,
    last_capacity_bound: Cell<Option<(usize, u64, bool)>>,
    /// The draw before which the readout starts.
    readout_first_draw: Option<usize>,
    /// Each state that reached the readout's first draw, before it (one
    /// per shot alone; one per state a grouped call's forks left).
    readouts: RefCell<Vec<Readout<S>>>,
    /// Development only: the calls recorded for `--record-calls`.
    recorder: RefCell<Option<Recorder>>,
}

/// Development only: the calls of one state, until `limit` draws (see the
/// module documentation for the format).
struct Recorder {
    lines: Vec<String>,
    matrices: BTreeMap<Vec<u64>, usize>,
    draws: usize,
    limit: usize,
    /// Why recording stopped, if it did.
    stopped: Option<String>,
}

impl Recorder {
    fn new(limit: usize) -> Self {
        Self {
            lines: Vec::new(),
            matrices: BTreeMap::new(),
            draws: 0,
            limit,
            stopped: None,
        }
    }

    fn active(&self) -> bool {
        self.stopped.is_none()
    }

    fn push(&mut self, line: String) {
        if self.active() {
            self.lines.push(line);
        }
    }

    fn unsupported(&mut self, call: &str) {
        if self.active() {
            self.lines.push(format!("U {call}"));
            self.stopped = Some(format!("unsupported {call}"));
        }
    }

    fn matrix(&mut self, matrix: &[Complex64]) -> usize {
        let key: Vec<u64> = matrix
            .iter()
            .flat_map(|z| [z.re.to_bits(), z.im.to_bits()])
            .collect();
        if let Some(&id) = self.matrices.get(&key) {
            return id;
        }
        let id = self.matrices.len();
        self.matrices.insert(key, id);
        let mut line = format!("M {id}");
        for z in matrix {
            write!(line, " {} {}", z.re, z.im).expect("a String takes any text");
        }
        self.push(line);
        id
    }

    fn bonds(shape: &MpsShape) -> String {
        (0..shape.site_count().saturating_sub(1))
            .filter_map(|cut| shape.bond_dim(cut))
            .map(|bond| bond.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }
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
    fn readout_distribution<E: MpsEngine<State = S>>(
        &self,
        engine: &E,
        index: usize,
    ) -> Option<Vec<f64>> {
        let readouts = self.readouts.borrow();
        let readout = readouts.get(index)?;
        let state = &readout.state;
        let sites: Vec<usize> = readout
            .sites
            .iter()
            .map(|site| site.expect("read"))
            .collect();
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

impl<E: MpsEngine> Probe<E> {
    /// Development only: records a call with `--record-calls`.
    fn record(&self, record: impl FnOnce(&mut Recorder)) {
        if let Some(recorder) = self.seen.recorder.borrow_mut().as_mut()
            && recorder.active()
        {
            record(recorder);
        }
    }
}

impl<E: MpsEngine> MpsEngine for Probe<E> {
    type State = Tagged<E::State>;
    type Error = E::Error;

    fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, E::Error> {
        self.record(|recorder| {
            if recorder.lines.is_empty() {
                let line = format!("P {} {}", capacity.site_count(), Recorder::bonds(capacity));
                recorder.push(line);
            } else {
                recorder.unsupported("prepare");
            }
        });
        Ok(Tagged {
            inner: self.inner.prepare(capacity)?,
            draws: 0,
            readout: Cell::new(None),
        })
    }

    fn apply(
        &self,
        state: &mut Self::State,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), E::Error> {
        let applied = self.inner.apply(&mut state.inner, sites, matrix);
        if applied.is_ok() {
            self.record(|recorder| {
                let id = recorder.matrix(matrix);
                let sites: Vec<String> = sites.iter().map(ToString::to_string).collect();
                recorder.push(format!("A {id} {}", sites.join(" ")));
            });
        }
        let bound = qdk_cpu_mps::dev_trace::get().capacity_bound;
        if bound > self.seen.capacity_bound.get() {
            self.seen.capacity_bound.set(bound);
            let routing = routing_trace();
            let when = (
                self.seen.draws.get(),
                routing.batches,
                routing.first_fallback_batch.is_some(),
            );
            if self.seen.first_capacity_bound.get().is_none() {
                self.seen.first_capacity_bound.set(Some(when));
            }
            self.seen.last_capacity_bound.set(Some(when));
        }
        applied
    }

    fn probability_of_one(&self, state: &Self::State, site: usize) -> Result<f64, E::Error> {
        let draw = state.draws;
        if let Some(first) = self.seen.readout_first_draw
            && (first..first + READOUT).contains(&draw)
        {
            if draw == first && state.readout.get().is_none() {
                let mut readouts = self.seen.readouts.borrow_mut();
                readouts.push(Readout {
                    state: self.inner.fork(&state.inner)?,
                    sites: vec![None; READOUT],
                });
                state.readout.set(Some(readouts.len() - 1));
            }
            if let Some(index) = state.readout.get() {
                let mut readouts = self.seen.readouts.borrow_mut();
                readouts[index].sites[draw - first].get_or_insert(site);
            }
        }
        let p = self.inner.probability_of_one(&state.inner, site)?;
        self.record(|recorder| recorder.push(format!("R {site} {p}")));
        Ok(p)
    }

    fn project(&self, state: &mut Self::State, site: usize, one: bool) -> Result<f64, E::Error> {
        self.seen.draws.set(self.seen.draws.get() + 1);
        state.draws += 1;
        let p = self.inner.project(&mut state.inner, site, one)?;
        self.record(|recorder| {
            recorder.push(format!("J {site} {} {p}", u8::from(one)));
            recorder.draws += 1;
            if recorder.draws >= recorder.limit {
                recorder.stopped = Some("limit".to_string());
            }
        });
        Ok(p)
    }

    fn fork(&self, state: &Self::State) -> Result<Self::State, E::Error> {
        self.record(|recorder| recorder.unsupported("fork"));
        self.seen.forks.set(self.seen.forks.get() + 1);
        Ok(Tagged {
            inner: self.inner.fork(&state.inner)?,
            draws: state.draws,
            readout: Cell::new(state.readout.get()),
        })
    }

    fn sample(
        &self,
        state: &Self::State,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, E::Error> {
        self.record(|recorder| recorder.unsupported("sample"));
        self.inner.sample(&state.inner, sites, shots, seed)
    }

    fn expectation(
        &self,
        state: &Self::State,
        observable: &PauliSum,
    ) -> Result<Complex64, E::Error> {
        self.record(|recorder| recorder.unsupported("expectation"));
        self.inner.expectation(&state.inner, observable)
    }

    fn shape(&self, state: &Self::State) -> Result<MpsShape, E::Error> {
        let shape = self.inner.shape(&state.inner)?;
        self.record(|recorder| recorder.push(format!("S {}", Recorder::bonds(&shape))));
        Ok(shape)
    }

    fn retained_weight(&self, state: &Self::State) -> Result<f64, E::Error> {
        let weight = self.inner.retained_weight(&state.inner)?;
        self.record(|recorder| recorder.push(format!("W {weight}")));
        Ok(weight)
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
    /// Development only: the shots of one `run` call (1: each shot alone).
    group: u32,
    census: bool,
    routing: (String, Routing),
    /// Development only: `--record-calls FILE` and `--record-draws N`.
    record_calls: Option<(String, usize)>,
    /// Development only: the gate fusion, the in-order fusion's largest
    /// block (replacing it), the CPU engine's MPO path, the reset handling,
    /// and their name.
    fusion: GateFusion,
    in_order: Option<usize>,
    mpo: bool,
    reset: DevResetHandling,
    variant: String,
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
        let fusion = value("--fusion").unwrap_or_else(|| "pairs".to_string());
        let apply = value("--apply").unwrap_or_else(|| "engine".to_string());
        let reset = value("--reset").unwrap_or_else(|| "split".to_string());
        assert!(fusion != "seq3" || apply == "mpo", "seq3 needs --apply mpo");
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
            census: args.iter().any(|arg| arg == "--census"),
            group: u32::try_from(number("--group", 1))
                .expect("a shot count")
                .max(1),
            routing: {
                let name = value("--routing").unwrap_or_else(|| "engine".to_string());
                let routing = match name.as_str() {
                    "engine" => Routing::Engine,
                    "meet-halfway" => Routing::MeetHalfway,
                    "meet-halfway-after-fusion" => Routing::MeetHalfwayAfterFusion,
                    "tuned" => {
                        let small = |flag: &str, default: u64| {
                            u16::try_from(number(flag, default)).expect("a small number")
                        };
                        Routing::Tuned(RouteTuning {
                            after_fusion: args.iter().any(|arg| arg == "--after-fusion"),
                            restore: match value("--restore").as_deref() {
                                None | Some("never") => Restore::Never,
                                Some("batch") => Restore::BatchEnd,
                                Some("draws") => Restore::Draws,
                                Some(other) => panic!("--restore {other}: never, batch or draws"),
                            },
                            drift_budget_permille: small("--drift-budget", 1000),
                            bond_guard_percent: small("--bond-guard", 100),
                            max_route_distance: small("--max-route-distance", 0),
                            restore_every_routed: small("--restore-every", 0),
                        })
                    }
                    other => panic!(
                        "--routing {other}: engine, meet-halfway, meet-halfway-after-fusion or tuned"
                    ),
                };
                let name = match routing {
                    Routing::Tuned(t) => format!(
                        "tuned:af={},restore={:?},db={},bg={},md={},re={}",
                        u8::from(t.after_fusion),
                        t.restore,
                        t.drift_budget_permille,
                        t.bond_guard_percent,
                        t.max_route_distance,
                        t.restore_every_routed
                    ),
                    _ => name,
                };
                (name, routing)
            },
            record_calls: value("--record-calls")
                .map(|path| (path, as_usize(number("--record-draws", 300)))),
            fusion: match fusion.as_str() {
                "unfused" => GateFusion::Unfused,
                _ => GateFusion::Pairs,
            },
            in_order: match fusion.as_str() {
                "pairs" | "unfused" => None,
                "seq" => Some(2),
                "seq3" => Some(3),
                other => panic!("--fusion {other}: pairs, unfused, seq or seq3"),
            },
            mpo: match apply.as_str() {
                "engine" => false,
                "mpo" => true,
                other => panic!("--apply {other}: engine or mpo"),
            },
            reset: match reset.as_str() {
                "split" => DevResetHandling::Split,
                "merge" => DevResetHandling::Merge,
                other => panic!("--reset {other}: split or merge"),
            },
            variant: format!("{fusion}/{apply}/{reset}"),
        }
    }

    /// The resumer these options configure, on `engine`.
    fn resumer<E: MpsEngine>(&self, engine: E) -> MpsResumer<E> {
        let mut resumer = MpsResumer::new(engine, self.seed)
            .with_max_bond_dimension(self.chi)
            .with_routing(self.routing.1)
            .with_gate_fusion(self.fusion)
            .with_dev_reset_handling(self.reset);
        if let Some(max_sites) = self.in_order {
            resumer = resumer.with_dev_in_order_fusion(max_sites);
        }
        resumer
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
    qdk_cpu_mps::dev_trace::reset();
    reset_routing_trace();
    let seen = Rc::new(Seen {
        draws: Cell::new(0),
        forks: Cell::new(0),
        capacity_bound: Cell::new(0),
        first_capacity_bound: Cell::new(None),
        last_capacity_bound: Cell::new(None),
        readout_first_draw: options.readout.as_ref().map(|_| options.readout_first_draw),
        readouts: RefCell::new(Vec::new()),
        recorder: RefCell::new(
            options
                .record_calls
                .as_ref()
                .map(|&(_, limit)| Recorder::new(limit)),
        ),
    });
    let probe = Probe {
        inner: Rc::clone(&engine),
        seen: Rc::clone(&seen),
    };
    let mut resumer = options.resumer(probe);
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
    let (splits, routing) = (qdk_cpu_mps::dev_trace::get(), routing_trace());
    let mut line = format!(
        "SHOT shot={shot} engine={} routing={} variant={} chi={} seed={} start_unix={start:.3} end_unix={end:.3} \
         seconds={seconds:.1} records={records} draws={} largest_bond={} lost_weight={:.3e}",
        options.engine,
        options.routing.0,
        options.variant,
        options.chi,
        options.seed,
        seen.draws.get(),
        report.largest_bond,
        1.0 - report.least_retained_weight,
    );
    let when = |w: Option<(usize, u64, bool)>| {
        w.map_or("none".to_string(), |(draw, batch, fell)| {
            format!("d{draw}/b{batch}/{}", if fell { "after" } else { "before" })
        })
    };
    write!(
        line,
        " splits={} cap_bound={} cap_lost_sum={:.3e} cap_lost_max={:.3e} need_max={} \
         cap_need={}/{} first_cap={} last_cap={} batches={} routed={} route_swaps={} \
         restores={} restore_swaps={} drift_fb={} bond_fb={} first_fb={} max_drift={}",
        splits.splits,
        splits.capacity_bound,
        splits.capacity_lost,
        splits.capacity_lost_max,
        splits.largest_need,
        splits.capacity_bound_need,
        splits.capacity_bound_cap,
        when(seen.first_capacity_bound.get()),
        when(seen.last_capacity_bound.get()),
        routing.batches,
        routing.routed,
        routing.route_swaps,
        routing.restores,
        routing.restore_swaps,
        routing.drift_fallbacks,
        routing.bond_fallbacks,
        routing
            .first_fallback_batch
            .map_or("none".to_string(), |b| b.to_string()),
        routing.max_drift_permille,
    )
    .expect("a String takes any text");
    if let Some(table) = &options.readout {
        match seen.readout_distribution(&*engine, 0) {
            Some(distribution) if distribution.len() == 1 << READOUT => {
                let (tvd, peaks) = tvd_and_peaks(&distribution, table);
                write!(line, " tvd={tvd:.3e} peaks={peaks}").expect("a String takes any text");
            }
            _ => line += " tvd=none peaks=none",
        }
    }
    if let Some(recorder) = seen.recorder.borrow_mut().take() {
        let draws = write_calls(options, shot, &recorder);
        write!(line, " recorded={draws}").expect("a String takes any text");
    }
    // The readout's fork is released before the engine.
    seen.readouts.borrow_mut().clear();
    drop(resumer);
    println!("{line}");
    true
}

/// Development only: writes the calls `recorder` recorded to the
/// `--record-calls` file; the draws recorded.
fn write_calls(options: &Options, shot: u32, recorder: &Recorder) -> usize {
    let (path, _) = options
        .record_calls
        .as_ref()
        .expect("a recorder is made for --record-calls");
    let mut text = format!(
        "# calls v1 engine={} routing={} chi={} seed={} shot={shot} draws={} stopped={}",
        options.engine,
        options.routing.0,
        options.chi,
        options.seed,
        recorder.draws,
        recorder.stopped.as_deref().unwrap_or("end"),
    );
    for recorded in &recorder.lines {
        text.push('\n');
        text += recorded;
    }
    text.push('\n');
    std::fs::write(path, text).expect("the calls file is writable");
    recorder.draws
}

/// The TVD of a readout's `distribution` from the exact `table`, and its
/// two peaks, in chemistry order.
fn tvd_and_peaks(distribution: &[f64], table: &BTreeMap<String, f64>) -> (f64, String) {
    let row = |index: usize| format!("{index:0READOUT$b}");
    let tvd = 0.5
        * distribution
            .iter()
            .enumerate()
            .map(|(index, p)| (p - table.get(&row(index)).copied().unwrap_or(0.0)).abs())
            .sum::<f64>();
    let mut order: Vec<usize> = (0..distribution.len()).collect();
    order.sort_by(|a, b| distribution[*b].total_cmp(&distribution[*a]));
    // Chemistry order: the record order reversed.
    let peaks: Vec<String> = order
        .iter()
        .take(2)
        .map(|&i| row(i).chars().rev().collect())
        .collect();
    (tvd, peaks.join(","))
}

/// Development only: runs the shots `first..first + count` in ONE `run`
/// call (so they share a state until a draw splits them, and the resumer
/// forks), and prints a `CALL` line and one `GSHOT` line per shot. Whether
/// it succeeded.
fn run_call<E: MpsEngine>(
    options: &Options,
    program: &QirProgram,
    make: &(dyn Fn() -> E + Sync),
    first: u32,
    count: u32,
) -> bool
where
    E::Error: std::fmt::Debug,
{
    let engine = Rc::new(make());
    qdk_cpu_mps::dev_trace::reset();
    reset_routing_trace();
    let seen = Rc::new(Seen {
        draws: Cell::new(0),
        forks: Cell::new(0),
        capacity_bound: Cell::new(0),
        first_capacity_bound: Cell::new(None),
        last_capacity_bound: Cell::new(None),
        readout_first_draw: options.readout.as_ref().map(|_| options.readout_first_draw),
        readouts: RefCell::new(Vec::new()),
        recorder: RefCell::new(
            options
                .record_calls
                .as_ref()
                .map(|&(_, limit)| Recorder::new(limit)),
        ),
    });
    let probe = Probe {
        inner: Rc::clone(&engine),
        seen: Rc::clone(&seen),
    };
    let mut resumer = options.resumer(probe);
    let shots: Vec<QirShotId> = (first..first + count).map(QirShotId).collect();
    if options.census {
        qdk_simulators::mps_simulator::start_census();
    }
    let started = Instant::now();
    let run = expect_ready(resumer.run(&QirMachine::new(program), &shots));
    let seconds = started.elapsed().as_secs_f64();
    if options.census {
        print_census(first);
    }
    let records = match run {
        Ok(records) => records,
        Err(error) => {
            println!("FAILED call={first} seconds={seconds:.1} error={error:?}");
            return false;
        }
    };
    let report = *resumer.last_run();
    let (splits, routing) = (qdk_cpu_mps::dev_trace::get(), routing_trace());
    let mut tvds = Vec::new();
    let mut peaks = Vec::new();
    if let Some(table) = &options.readout {
        for index in 0..seen.readouts.borrow().len() {
            if let Some(distribution) = seen.readout_distribution(&*engine, index) {
                let (tvd, peak) = tvd_and_peaks(&distribution, table);
                tvds.push(format!("{tvd:.3e}"));
                peaks.push(peak);
            }
        }
    }
    let max_tvd = tvds
        .iter()
        .map(|t| t.parse::<f64>().expect("a number"))
        .fold(0.0, f64::max);
    println!(
        "CALL first={first} shots={count} engine={} routing={} variant={} chi={} seed={} seconds={seconds:.1} \
         draws={} forks={} readouts={} max_tvd={max_tvd:.3e} tvds={} peaks={} largest_bond={} \
         lost_weight={:.3e} splits={} cap_bound={} need_max={} batches={} route_swaps={} \
         restore_swaps={} drift_fb={} bond_fb={}",
        options.engine,
        options.routing.0,
        options.variant,
        options.chi,
        options.seed,
        seen.draws.get(),
        seen.forks.get(),
        tvds.len(),
        tvds.join(","),
        peaks.join(";"),
        report.largest_bond,
        1.0 - report.least_retained_weight,
        splits.splits,
        splits.capacity_bound,
        splits.largest_need,
        routing.batches,
        routing.route_swaps,
        routing.restore_swaps,
        routing.drift_fallbacks,
        routing.bond_fallbacks,
    );
    for (shot, records) in (first..).zip(&records) {
        println!(
            "GSHOT shot={shot} call={first} routing={} seed={} records={}",
            options.routing.0,
            options.seed,
            bits(records)
        );
    }
    seen.readouts.borrow_mut().clear();
    drop(resumer);
    true
}

/// Prints the `CENSUS` line of the call that started at shot `first`.
fn print_census(first: u32) {
    let segments = qdk_simulators::mps_simulator::take_census();
    let sum = |field: fn(&qdk_simulators::mps_simulator::SegmentCensus) -> u64| {
        segments.iter().map(field).sum::<u64>()
    };
    println!(
        "CENSUS call={first} segments={} resets={} measure_splits={} reset_splits={} \
         reset_merges={} forks={}",
        segments.len(),
        sum(|s| s.resets),
        sum(|s| s.measure_splits),
        sum(|s| s.reset_splits),
        sum(|s| s.reset_merges),
        sum(|s| s.forks),
    );
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
                    let shot = next.fetch_add(options.group, Ordering::Relaxed);
                    if shot >= end {
                        break;
                    }
                    let done = if options.group > 1 {
                        run_call(options, program, make, shot, options.group.min(end - shot))
                    } else {
                        run_shot(options, program, make, shot)
                    };
                    if !done {
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
    assert!(
        options.record_calls.is_none() || (options.shots == 1 && options.group == 1),
        "--record-calls records one shot alone: --shots 1 --group 1"
    );
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
        "cpu" => run_slice(&options, &program, &|| {
            qdk_cpu_mps::CpuMpsEngine::default().with_dev_mpo_far(options.mpo)
        }),
        "cutensornet" => run_on_cutensornet(&options, &program),
        "cuda-mps" => run_on_cuda_mps(&options, &program),
        other => panic!("--engine {other}: reference, cpu, cutensornet or cuda-mps"),
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
        qdk_cutensornet::CuTensorNetEngine::new(&availability).expect("an engine on CUDA device 0")
    };
    run_slice(options, program, &make)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn run_on_cutensornet(_options: &Options, _program: &QirProgram) -> bool {
    let error = qdk_cutensornet::discover().expect_err("off Linux x86-64");
    panic!("cuTensorNet runs on Linux x86-64 only: {error}");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_on_cuda_mps(options: &Options, program: &QirProgram) -> bool {
    let availability = qdk_cutensornet::discover().expect("cuTensorNet and the CUDA Runtime load");
    println!("AVAILABILITY {:?}", availability.report());
    let make = || {
        qdk_cutensornet::CudaMpsEngine::new(&availability).expect("the tensor API on CUDA device 0")
    };
    run_slice(options, program, &make)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn run_on_cuda_mps(_options: &Options, _program: &QirProgram) -> bool {
    panic!("cuda-mps runs on Linux x86-64 only");
}
