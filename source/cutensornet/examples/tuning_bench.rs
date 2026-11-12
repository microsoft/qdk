// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the tuning bench of the GPU speed
//! plan (§1½). It replays the engine calls of the first draws of an H₂ shot,
//! recorded on the CPU engine by `qdk_simulators`' `h2_dev --record-calls`,
//! on `CuTensorNetEngine` with one change at a time, and times each:
//!
//! - T1: the distant trace (routing left to cuTensorNet) against the
//!   adjacent one (`--routing tuned --restore batch`);
//! - T2: the SVD algorithm, GESVD (the engine's), GESVDJ and GESVDR;
//! - T3a: the hyper-samples of a read, 8 (the engine's), 4 and 1;
//! - T3b: P(One) read from the canonical centre's tensor;
//! - T4: pooled device buffers; the compute's prepare and compute phases
//!   are timed apart in every variant.
//!
//! Every projection replays the recorded outcome, so every variant walks
//! the same states. Before printing a time, a variant's answers (each
//! P(One), each projection's probability, each retained weight) must agree
//! with GESVD's on the same trace within 1e-9, and its bonds must match;
//! a variant that disagrees prints its deviation and no time, and one the
//! library refuses prints the error. The deviation from the CPU engine's
//! recorded answers is reported, not checked.
//!
//! ```text
//! cargo run --release -p qdk_cutensornet --example tuning_bench -- \
//!     --trace-adjacent ADJACENT.calls --trace-distant DISTANT.calls \
//!     [--engine cutensornet|cpu] [--draws N]
//! ```
//!
//! `--engine cpu` replays the two traces on the CPU engine only, a check of
//! the replay (it reproduces the recorded answers) that runs anywhere.

use num_complex::Complex64;
use std::fmt::Debug;
use std::time::Instant;
use tensornet::{MpsEngine, MpsShape};

/// The agreement every variant must reach with the baseline.
const TOLERANCE: f64 = 1e-9;

/// One recorded call (the format is in `h2_dev.rs`).
#[derive(Clone)]
enum Call {
    Prepare(MpsShape),
    Apply(usize, Vec<usize>),
    Read(usize, f64),
    Project(usize, bool, f64),
    Shape(Vec<usize>),
    Weight(f64),
}

/// A recorded trace: its matrices and its calls.
struct Trace {
    name: String,
    header: String,
    matrices: Vec<Vec<Complex64>>,
    calls: Vec<Call>,
    /// For each call, the site of the next read after it, if any.
    next_read: Vec<Option<usize>>,
}

impl Trace {
    /// Reads the trace at `path`, keeping its first `draws` projections.
    fn read(name: &str, path: &str, draws: usize) -> Self {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("the trace {path} is readable: {error}"));
        let mut header = String::new();
        let mut matrices = Vec::new();
        let mut calls = Vec::new();
        let mut projections = 0;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix('#') {
                header = rest.trim().to_string();
                continue;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            let int = |at: usize| -> usize { fields[at].parse().expect("an integer") };
            let real = |at: usize| -> f64 { fields[at].parse().expect("a number") };
            match fields.first().copied() {
                Some("P") => {
                    let sites = int(1);
                    let bonds: Vec<usize> = (2..fields.len()).map(int).collect();
                    assert_eq!(bonds.len() + 1, sites, "one bond per cut");
                    let chain = (0..sites)
                        .map(|site| {
                            let left = site.checked_sub(1).map(|cut| bonds[cut]);
                            let right = bonds.get(site).copied();
                            left.into_iter().chain([2]).chain(right).collect()
                        })
                        .collect();
                    calls.push(Call::Prepare(MpsShape::new(chain).expect("a chain")));
                }
                Some("M") => {
                    assert_eq!(int(1), matrices.len(), "matrices are numbered in order");
                    let values = (2..fields.len())
                        .step_by(2)
                        .map(|at| Complex64::new(real(at), real(at + 1)))
                        .collect();
                    matrices.push(values);
                }
                Some("A") => calls.push(Call::Apply(int(1), (2..fields.len()).map(int).collect())),
                Some("R") => calls.push(Call::Read(int(1), real(2))),
                Some("J") => {
                    calls.push(Call::Project(int(1), fields[2] == "1", real(3)));
                    projections += 1;
                    if projections == draws {
                        break;
                    }
                }
                Some("S") => calls.push(Call::Shape((1..fields.len()).map(int).collect())),
                Some("W") => calls.push(Call::Weight(real(1))),
                Some("U") | None => break,
                Some(other) => panic!("{path}: unknown call {other}"),
            }
        }
        let mut next_read = vec![None; calls.len()];
        let mut next = None;
        for (at, call) in calls.iter().enumerate().rev() {
            next_read[at] = next;
            if let Call::Read(site, _) = call {
                next = Some(*site);
            }
        }
        Self {
            name: name.to_string(),
            header,
            matrices,
            calls,
            next_read,
        }
    }

    /// The trace up to its `draws`-th projection, named `name`.
    #[cfg_attr(
        not(all(target_os = "linux", target_arch = "x86_64")),
        expect(dead_code, reason = "the warm-up is on cuTensorNet only")
    )]
    fn first_draws(&self, name: &str, draws: usize) -> Self {
        let mut projections = 0;
        let end = self
            .calls
            .iter()
            .position(|call| {
                projections += usize::from(matches!(call, Call::Project(..)));
                projections == draws
            })
            .map_or(self.calls.len(), |at| at + 1);
        Self {
            name: name.to_string(),
            header: self.header.clone(),
            matrices: self.matrices.clone(),
            calls: self.calls[..end].to_vec(),
            next_read: self.next_read[..end].to_vec(),
        }
    }
}

/// What a replay answered, and how long it took.
struct Outcome {
    /// Each P(One), projection probability and retained weight, in order.
    values: Vec<f64>,
    /// The bonds of each shape read.
    bonds: Vec<Vec<usize>>,
    seconds: f64,
    draws: usize,
    applies: usize,
    /// The largest deviation from the recorded (CPU engine) answers.
    recorded_deviation: f64,
}

/// A step a variant takes before the compute that follows the last change
/// of the state: T3b sets the canonical centre to the site read next.
type BeforeCompute<'a, S> = &'a dyn Fn(&S, usize) -> Result<(), String>;

/// Replays `trace` on `engine`, forcing the recorded outcomes.
fn replay<E: MpsEngine>(
    engine: &E,
    trace: &Trace,
    before_compute: Option<BeforeCompute<'_, E::State>>,
) -> Result<Outcome, String>
where
    E::Error: Debug,
{
    let failed = |call: &'static str| move |error: E::Error| format!("{call}: {error:?}");
    let mut state = None;
    let mut outcome = Outcome {
        values: Vec::new(),
        bonds: Vec::new(),
        seconds: 0.0,
        draws: 0,
        applies: 0,
        recorded_deviation: 0.0,
    };
    let started = Instant::now();
    for (at, call) in trace.calls.iter().enumerate() {
        if let Call::Prepare(capacity) = call {
            state = Some(engine.prepare(capacity).map_err(failed("prepare"))?);
            continue;
        }
        let state = state.as_mut().ok_or("the trace starts with a prepare")?;
        let mut answer = |value: f64, recorded: f64| {
            outcome.values.push(value);
            outcome.recorded_deviation = outcome.recorded_deviation.max((value - recorded).abs());
        };
        let changes = match call {
            Call::Prepare(_) => unreachable!("handled above"),
            Call::Apply(matrix, sites) => {
                engine
                    .apply(state, sites, &trace.matrices[*matrix])
                    .map_err(failed("apply"))?;
                outcome.applies += 1;
                true
            }
            Call::Read(site, recorded) => {
                let p = engine
                    .probability_of_one(state, *site)
                    .map_err(failed("probability_of_one"))?;
                answer(p, *recorded);
                false
            }
            Call::Project(site, one, recorded) => {
                let p = engine
                    .project(state, *site, *one)
                    .map_err(failed("project"))?;
                answer(p, *recorded);
                outcome.draws += 1;
                true
            }
            Call::Shape(_) => {
                let shape = engine.shape(state).map_err(failed("shape"))?;
                outcome.bonds.push(
                    (0..shape.site_count().saturating_sub(1))
                        .filter_map(|cut| shape.bond_dim(cut))
                        .collect(),
                );
                false
            }
            Call::Weight(recorded) => {
                let weight = engine
                    .retained_weight(state)
                    .map_err(failed("retained_weight"))?;
                answer(weight, *recorded);
                false
            }
        };
        let next_changes = matches!(
            trace.calls.get(at + 1),
            Some(Call::Apply(..) | Call::Project(..))
        );
        if changes
            && !next_changes
            && let (Some(before_compute), Some(site)) = (before_compute, trace.next_read[at])
        {
            before_compute(state, site)?;
        }
    }
    outcome.seconds = started.elapsed().as_secs_f64();
    Ok(outcome)
}

/// The recorded shapes' bonds, in order, from the trace.
fn recorded_bonds(trace: &Trace) -> Vec<Vec<usize>> {
    trace
        .calls
        .iter()
        .filter_map(|call| match call {
            Call::Shape(bonds) => Some(bonds.clone()),
            _ => None,
        })
        .collect()
}

/// The largest difference of two answer series, by index (infinite if
/// their lengths differ).
fn deviation(values: &[f64], baseline: &[f64]) -> f64 {
    if values.len() != baseline.len() {
        return f64::INFINITY;
    }
    values
        .iter()
        .zip(baseline)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max)
}

/// Prints a variant's line: its time if it agrees with `baseline`, its
/// deviation otherwise. Returns its seconds if it agreed.
fn report(
    variant: &str,
    trace: &Trace,
    outcome: &Result<Outcome, String>,
    baseline: Option<&Outcome>,
    phases: &str,
) -> Option<f64> {
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            println!(
                "VARIANT name={variant} trace={} status=refused error={error:?}",
                trace.name
            );
            return None;
        }
    };
    let (dev, bonds_match) = baseline.map_or((0.0, true), |baseline| {
        (
            deviation(&outcome.values, &baseline.values),
            outcome.bonds == baseline.bonds,
        )
    });
    let recorded_bonds_match = outcome.bonds == recorded_bonds(trace);
    let common = format!(
        "name={variant} trace={} draws={} applies={} dev_baseline={dev:.3e} bonds_match={bonds_match} \
         dev_recorded={:.3e} recorded_bonds_match={recorded_bonds_match}",
        trace.name, outcome.draws, outcome.applies, outcome.recorded_deviation
    );
    if dev > TOLERANCE || !bonds_match {
        println!("VARIANT status=disagrees {common}");
        return None;
    }
    let count = |n: usize| f64::from(u32::try_from(n.max(1)).unwrap_or(u32::MAX));
    let (draws, applies) = (count(outcome.draws), count(outcome.applies));
    println!(
        "VARIANT status=ok {common} seconds={:.3} s_per_draw={:.4} ms_per_apply={:.4} {phases}",
        outcome.seconds,
        outcome.seconds / draws,
        1e3 * outcome.seconds / applies,
    );
    Some(outcome.seconds)
}

struct Options {
    adjacent: Trace,
    distant: Trace,
    engine: String,
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
        let draws = value("--draws").map_or(usize::MAX, |n| n.parse().expect("--draws N"));
        let trace = |flag: &str, name: &str| {
            let path = value(flag).unwrap_or_else(|| panic!("{flag} FILE: a recorded trace"));
            Trace::read(name, &path, draws)
        };
        Self {
            adjacent: trace("--trace-adjacent", "adjacent"),
            distant: trace("--trace-distant", "distant"),
            engine: value("--engine").unwrap_or_else(|| "cutensornet".to_string()),
        }
    }
}

/// T1, by index: the distant trace's answers against the adjacent
/// baseline's (the same draws, the qubits on other sites).
fn report_t1(distant: Option<&Outcome>, adjacent: Option<&Outcome>) {
    if let (Some(distant), Some(adjacent)) = (distant, adjacent) {
        let reads = |outcome: &Outcome| outcome.values.len();
        println!(
            "T1 dev_by_index={:.3e} answers={}/{} seconds_distant={:.3} seconds_adjacent={:.3} \
             adjacent_speedup={:.2}",
            deviation(&distant.values, &adjacent.values),
            reads(distant),
            reads(adjacent),
            distant.seconds,
            adjacent.seconds,
            distant.seconds / adjacent.seconds,
        );
    }
}

fn main() {
    let options = Options::parse();
    for trace in [&options.adjacent, &options.distant] {
        println!(
            "TRACE name={} calls={} matrices={} recorded_by=[{}]",
            trace.name,
            trace.calls.len(),
            trace.matrices.len(),
            trace.header
        );
    }
    let ok = match options.engine.as_str() {
        "cpu" => run_on_cpu(&options),
        "cutensornet" => run_on_cutensornet(&options),
        other => panic!("--engine {other}: cutensornet or cpu"),
    };
    if !ok {
        std::process::exit(1);
    }
}

/// The two traces on the CPU engine: the replay's check.
fn run_on_cpu(options: &Options) -> bool {
    let engine = qdk_cpu_mps::CpuMpsEngine::default();
    let distant = replay(&engine, &options.distant, None);
    let adjacent = replay(&engine, &options.adjacent, None);
    report("distant-cpu", &options.distant, &distant, None, "");
    report("adjacent-cpu", &options.adjacent, &adjacent, None, "");
    report_t1(distant.as_ref().ok(), adjacent.as_ref().ok());
    let reproduced = |outcome: &Result<Outcome, String>, trace: &Trace| {
        outcome.as_ref().is_ok_and(|outcome| {
            outcome.recorded_deviation <= TOLERANCE && outcome.bonds == recorded_bonds(trace)
        })
    };
    let ok = reproduced(&distant, &options.distant) && reproduced(&adjacent, &options.adjacent);
    println!("TUNING done engine=cpu replay_reproduces_recording={ok}");
    ok
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_on_cutensornet(options: &Options) -> bool {
    use qdk_cutensornet::{CuTensorNetEngine, DevTuning};

    let availability = qdk_cutensornet::discover().expect("cuTensorNet and the CUDA Runtime load");
    println!("AVAILABILITY {:?}", availability.report());
    let run = |trace: &Trace, tuning: DevTuning| -> (Result<Outcome, String>, String) {
        let engine = match CuTensorNetEngine::new(&availability) {
            Ok(engine) => engine.with_dev_tuning(tuning),
            Err(error) => return (Err(format!("engine: {error}")), String::new()),
        };
        engine.dev_reset_times();
        let centre = |state: &_, site| {
            engine
                .dev_set_canonical_centre(state, site)
                .map_err(|error| format!("canonical centre: {error}"))
        };
        let before_compute: Option<BeforeCompute<'_, _>> =
            tuning.canonical_centre_reads.then_some(&centre);
        let outcome = replay(&engine, trace, before_compute);
        (outcome, phases(&engine.dev_times()))
    };
    let base = DevTuning::default();

    // A warm-up: the device's and the library's first-use costs.
    let (outcome, _) = run(&options.adjacent.first_draws("warm-up", 5), base);
    println!("WARMUP ok={}", outcome.is_ok());

    let (baseline, baseline_phases) = run(&options.adjacent, base);
    let baseline_seconds = report(
        "adjacent-gesvd",
        &options.adjacent,
        &baseline,
        None,
        &baseline_phases,
    );
    let Ok(baseline) = baseline else {
        println!("TUNING done engine=cutensornet baseline=refused");
        return false;
    };
    let (distant, distant_phases) = run(&options.distant, base);
    report(
        "distant-gesvd",
        &options.distant,
        &distant,
        None,
        &distant_phases,
    );
    report_t1(distant.as_ref().ok(), Some(&baseline));

    let (mut agreed, mut other) = (0, 0);
    for (name, tuning) in variants(base) {
        let (outcome, phases) = run(&options.adjacent, tuning);
        match report(name, &options.adjacent, &outcome, Some(&baseline), &phases) {
            Some(seconds) => {
                agreed += 1;
                if let Some(baseline_seconds) = baseline_seconds {
                    println!(
                        "RATIO name={name} speedup_vs_adjacent_gesvd={:.3}",
                        baseline_seconds / seconds
                    );
                }
            }
            None => other += 1,
        }
    }
    println!("TUNING done engine=cutensornet agreed={agreed} disagreed_or_refused={other}");
    true
}

/// The variants compared with the adjacent trace's GESVD baseline `base`.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn variants(base: qdk_cutensornet::DevTuning) -> [(&'static str, qdk_cutensornet::DevTuning); 7] {
    use qdk_cutensornet::{DevTuning, SvdAlgorithm};
    [
        (
            "adjacent-gesvdj",
            DevTuning {
                svd_algorithm: SvdAlgorithm::Gesvdj,
                ..base
            },
        ),
        (
            "adjacent-gesvdr",
            DevTuning {
                svd_algorithm: SvdAlgorithm::Gesvdr,
                ..base
            },
        ),
        (
            "adjacent-hyper4",
            DevTuning {
                expectation_hyper_samples: 4,
                ..base
            },
        ),
        (
            "adjacent-hyper1",
            DevTuning {
                expectation_hyper_samples: 1,
                ..base
            },
        ),
        (
            "adjacent-canonical",
            DevTuning {
                canonical_centre_reads: true,
                ..base
            },
        ),
        (
            "adjacent-pooled",
            DevTuning {
                pooled_buffers: true,
                ..base
            },
        ),
        (
            "adjacent-all",
            DevTuning {
                svd_algorithm: SvdAlgorithm::Gesvdj,
                expectation_hyper_samples: 1,
                pooled_buffers: true,
                canonical_centre_reads: true,
            },
        ),
    ]
}

/// The engine's phase clocks, as `key=value` fields.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn phases(times: &qdk_cutensornet::DevTimes) -> String {
    let clock = |name: &str, clock: &qdk_cutensornet::PhaseClock| {
        format!("{name}={:.3}s/{}", clock.seconds, clock.count)
    };
    [
        clock("prepare", &times.state_prepare),
        clock("compute", &times.state_compute),
        clock("capture", &times.capture),
        clock("expectation", &times.expectation),
        clock("canonical_read", &times.canonical_read),
        clock("apply", &times.apply),
        clock("restart", &times.restart),
        format!(
            "allocations={} pool_hits={} canonical_fallbacks={}",
            times.allocations, times.pool_hits, times.canonical_fallbacks
        ),
    ]
    .join(" ")
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn run_on_cutensornet(_options: &Options) -> bool {
    let error = qdk_cutensornet::discover().expect_err("off Linux x86-64");
    panic!("cuTensorNet runs on Linux x86-64 only: {error}");
}
