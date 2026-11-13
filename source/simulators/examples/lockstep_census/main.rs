// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development branch only (not for review): the lockstep census of an
//! Adaptive Profile program (H₂'s phase estimation) on the CPU engine.
//! It answers whether shots that run different states but the same steps
//! are many enough, and their bonds close enough, for a GPU engine to run
//! them in one launch per update (lockstep).
//!
//! Two modes:
//!
//! - `--record DIR`: runs each shot of the slice alone, `--threads` at a
//!   time, through `MpsResumer` with the tuned routing that restores at each
//!   batch's end (`rb`), and writes one trace per shot to `DIR`: each
//!   batch's keys, draws and splits ([`trace`], recorded by [`probe`]).
//!   With `--fault-p P`, the resumer injects Pauli faults after each
//!   two-qubit gate (EXPERIMENTAL `PauliFaults`). With `--batch-limit B`,
//!   the probe stops applying operators from batch B on, so a shot whose
//!   bonds grow (faults) ends fast, and its trace keeps its first B
//!   batches.
//! - `--analyze DIR`: replays the traces of the shots
//!   `--first-shot..+--shots` breadth-first, batch by batch ([`cohorts`]),
//!   and prints the census, with the per-round table to `--csv FILE`.
//!   `--rounds R` keeps each trace's first R batches, to compare with
//!   traces recorded with `--batch-limit R`.
//!
//! ```text
//! cargo run --release -p qdk_simulators --example lockstep_census -- \
//!     --record DIR --bytecode H2.bytecode.json [--seed 1] [--chi 128] \
//!     [--first-shot 0] [--shots 1] [--threads 1] [--fault-p P] \
//!     [--fault-seed 1] [--deadline-seconds S] [--dump-batch B] \
//!     [--batch-limit B]
//! cargo run --release -p qdk_simulators --example lockstep_census -- \
//!     --analyze DIR [--first-shot 0] [--shots N] [--wave 108] [--csv FILE] \
//!     [--rounds R]
//! ```

// Counts and sizes become f64 for ratios; they stay far below 2^52.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::cell::RefCell;
use std::fmt::Write as _;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;

use qdk_cpu_mps::CpuMpsEngine;
use qdk_simulators::mps_simulator::{
    MpsResumer, PauliFaults, Restore, RouteTuning, Routing, reset_routing_trace, routing_trace,
};
use qdk_simulators::qir::{QirMachine, QirProgram, QirResumer, QirShotId, expect_ready};
use qdk_simulators::{MeasurementResult, OutputRecord};

#[path = "../../tests/run_shot_compatibility/bytecode_json.rs"]
mod bytecode_json;
mod cohorts;
mod probe;
mod trace;

use cohorts::{CohortCost, CohortKey, census};
use probe::{BatchRecorder, CensusProbe};
use trace::ShotTrace;

/// The A100's streaming multiprocessors: the updates of one wave, one
/// block each.
const A100_WAVE: usize = 108;

/// The per-update costs the census's prediction is evaluated at, in µs:
/// the A100's launch + sync floor, stage 2's assumed tens of µs,
/// cuTensorNet's `TensorQR` and `GateSplit` at bond 24 (A100, measured).
const UPDATE_MICROS: [(&str, f64); 5] = [
    ("floor 6.3us", 6.3),
    ("stage2 20us", 20.0),
    ("stage2 50us", 50.0),
    ("TensorQR 0.27ms", 270.0),
    ("GateSplit 4.37ms", 4370.0),
];

/// The launch + sync floor, which each draw's launch costs.
const SYNC_MICROS: f64 = 6.3;

struct Options {
    args: Vec<String>,
}

impl Options {
    fn value(&self, flag: &str) -> Option<String> {
        self.args.iter().position(|arg| arg == flag).map(|at| {
            self.args
                .get(at + 1)
                .unwrap_or_else(|| panic!("{flag} takes a value"))
                .clone()
        })
    }

    fn number<T: std::str::FromStr>(&self, flag: &str, default: T) -> T {
        self.value(flag).map_or(default, |v| {
            v.parse().unwrap_or_else(|_| panic!("{flag}: a number"))
        })
    }
}

fn main() {
    let options = Options {
        args: std::env::args().skip(1).collect(),
    };
    if let Some(dir) = options.value("--record") {
        record(&options, Path::new(&dir));
    } else if let Some(dir) = options.value("--analyze") {
        analyze(&options, Path::new(&dir));
    } else {
        panic!("--record DIR or --analyze DIR");
    }
}

/// The `rb` routing: tuned, routed before fusion, restored at each batch's
/// end, with no fallback that a bond below χ triggers.
fn rb() -> Routing {
    Routing::Tuned(RouteTuning {
        after_fusion: false,
        restore: Restore::BatchEnd,
        drift_budget_permille: 1000,
        bond_guard_percent: 100,
        max_route_distance: 0,
        restore_every_routed: 0,
    })
}

fn record(options: &Options, dir: &Path) {
    let bytecode = options
        .value("--bytecode")
        .expect("--bytecode FILE: the program's bytecode JSON");
    let json = std::fs::read_to_string(bytecode).expect("the bytecode is readable");
    let bytecode: bytecode_json::Bytecode = serde_json::from_str(&json).expect("bytecode JSON");
    let program = QirProgram::new(bytecode.program()).expect("the program builds");
    let seed: u64 = options.number("--seed", 1);
    let chi = NonZeroUsize::new(options.number("--chi", 128)).expect("--chi: at least 1");
    let first: u32 = options.number("--first-shot", 0);
    let end = first + options.number::<u32>("--shots", 1);
    let threads: usize = options.number::<usize>("--threads", 1).max(1);
    let faults = options.value("--fault-p").map(|p| PauliFaults {
        probability: p.parse().expect("--fault-p: a probability"),
        seed: options.number("--fault-seed", 1),
    });
    let dump: Option<usize> = options
        .value("--dump-batch")
        .map(|b| b.parse().expect("--dump-batch: a batch index"));
    let batch_limit: Option<usize> = options
        .value("--batch-limit")
        .map(|b| b.parse().expect("--batch-limit: a batch count"));
    let deadline = options
        .value("--deadline-seconds")
        .map(|s| s.parse::<f64>().expect("--deadline-seconds: a number"));
    println!(
        "PROGRAM qubits={} two_qubit_gates={} seed={seed} chi={chi} shots={first}..{end} \
         threads={threads} faults={faults:?} deadline={deadline:?}",
        program.qubits(),
        program.two_qubit_gates().len(),
    );
    let setup = Recording {
        program: &program,
        seed,
        chi,
        faults,
        dump,
        batch_limit,
        dir,
    };
    let started = Instant::now();
    let next = AtomicU32::new(first);
    let ok = AtomicBool::new(true);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    if deadline.is_some_and(|d| started.elapsed().as_secs_f64() > d) {
                        break;
                    }
                    let shot = next.fetch_add(1, Ordering::Relaxed);
                    if shot >= end {
                        break;
                    }
                    if ShotTrace::path(dir, shot).exists() {
                        println!("SKIP shot={shot} (traced)");
                        continue;
                    }
                    if !record_shot(&setup, shot) {
                        ok.store(false, Ordering::Relaxed);
                    }
                }
            });
        }
    });
    if !ok.into_inner() {
        std::process::exit(1);
    }
}

/// What every recorded shot shares: the program, the simulation's
/// settings, and where the traces go.
struct Recording<'a> {
    program: &'a QirProgram,
    seed: u64,
    chi: NonZeroUsize,
    faults: Option<PauliFaults>,
    dump: Option<usize>,
    batch_limit: Option<usize>,
    dir: &'a Path,
}

/// Runs shot `shot` alone, as `setup` says, and writes its trace; whether
/// it succeeded.
fn record_shot(setup: &Recording, shot: u32) -> bool {
    let &Recording {
        program,
        seed,
        chi,
        faults,
        dump,
        batch_limit,
        dir,
    } = setup;
    qdk_cpu_mps::dev_trace::reset();
    qdk_cpu_mps::dev_trace::start_log();
    reset_routing_trace();
    let recorder = Rc::new(RefCell::new(BatchRecorder::new(shot, dump, batch_limit)));
    let probe = CensusProbe {
        inner: CpuMpsEngine::default(),
        recorder: Rc::clone(&recorder),
    };
    let mut resumer = MpsResumer::new(probe, seed)
        .with_max_bond_dimension(chi)
        .with_routing(rb());
    if let Some(faults) = faults {
        resumer = resumer.with_pauli_faults(faults);
    }
    let started = Instant::now();
    let run = expect_ready(resumer.run(&QirMachine::new(program), &[QirShotId(shot)]));
    let seconds = started.elapsed().as_secs_f64();
    let report = *resumer.last_run();
    drop(resumer);
    let routing = routing_trace();
    let splits = qdk_cpu_mps::dev_trace::get();
    let batches = Rc::into_inner(recorder)
        .expect("the resumer, and its probe, are dropped")
        .into_inner()
        .finish(routing.batches);
    qdk_cpu_mps::dev_trace::stop_log();
    let records = match run {
        Ok(records) => bits(&records[0]),
        Err(error) => {
            println!("FAILED shot={shot} seconds={seconds:.1} error={error:?}");
            return false;
        }
    };
    let trace = ShotTrace {
        shot,
        seconds,
        records,
        batches,
    };
    let logged: usize = trace.batches.iter().map(|b| b.splits.len()).sum();
    let draws: usize = trace.batches.iter().map(|b| b.draws.len()).sum();
    trace.write(dir).expect("the trace is written");
    println!(
        "SHOT shot={shot} seconds={seconds:.1} records={} batches={} draws={draws} \
         splits={} logged_splits={logged} largest_bond={} need_max={} route_swaps={} \
         restore_swaps={} drift_fb={} bond_fb={}",
        trace.records,
        routing.batches,
        splits.splits,
        report.largest_bond,
        splits.largest_need,
        routing.route_swaps,
        routing.restore_swaps,
        routing.drift_fallbacks,
        routing.bond_fallbacks,
    );
    true
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

fn analyze(options: &Options, dir: &Path) {
    let first: u32 = options.number("--first-shot", 0);
    let count: u32 = options.number("--shots", 64);
    let wave: usize = options.number("--wave", A100_WAVE);
    let rounds: Option<usize> = options
        .value("--rounds")
        .map(|r| r.parse().expect("--rounds: a batch count"));
    let traces: Vec<ShotTrace> = (first..first + count)
        .map(|shot| {
            let mut trace = ShotTrace::read(&ShotTrace::path(dir, shot))
                .unwrap_or_else(|error| panic!("shot {shot}'s trace: {error}"));
            if let Some(rounds) = rounds {
                trace.batches.truncate(rounds);
            }
            trace
        })
        .collect();
    let census = census(&traces, wave);
    if let Some(csv) = options.value("--csv") {
        std::fs::write(PathBuf::from(csv), census.rounds_csv()).expect("the CSV is written");
    }
    let n = census.shots as f64;
    let seconds: f64 = traces.iter().map(|t| t.seconds).sum::<f64>() / n;
    let batches: Vec<usize> = traces.iter().map(|t| t.batches.len()).collect();
    let distinct_records = {
        let mut records: Vec<&str> = traces.iter().map(|t| t.records.as_str()).collect();
        records.sort_unstable();
        records.dedup();
        records.len()
    };
    let mut out = String::new();
    let line = &mut out;
    let _ = writeln!(
        line,
        "CENSUS dir={} shots={first}..{} wave={wave} rounds={} batches_min={} batches_max={} \
         distinct_records={distinct_records} cpu_seconds_per_shot={seconds:.1}",
        dir.display(),
        first + count,
        census.rounds.len(),
        batches.iter().min().unwrap_or(&0),
        batches.iter().max().unwrap_or(&0),
    );
    let groups: Vec<usize> = census.rounds.iter().map(|r| r.groups).collect();
    let mean = |v: &[usize]| v.iter().sum::<usize>() as f64 / v.len().max(1) as f64;
    let exact: Vec<usize> = census.rounds.iter().map(|r| r.cohorts[0]).collect();
    let shape: Vec<usize> = census.rounds.iter().map(|r| r.cohorts[1]).collect();
    let _ = writeln!(
        line,
        "ROUNDS groups_mean={:.1} groups_max={} exact_cohorts_mean={:.1} exact_cohorts_max={} \
         shape_cohorts_mean={:.1} shape_cohorts_max={} first_round_all_groups_split={}",
        mean(&groups),
        groups.iter().max().unwrap_or(&0),
        mean(&exact),
        exact.iter().max().unwrap_or(&0),
        mean(&shape),
        shape.iter().max().unwrap_or(&0),
        census
            .rounds
            .iter()
            .position(|r| r.groups == r.live)
            .map_or("never".to_string(), |r| r.to_string()),
    );
    let _ = writeln!(
        line,
        "PER_SHOT splits={:.0} draws={:.0} | GROUPED splits={:.0} draws={:.0} \
         (state sharing alone: {:.3}x fewer updates)",
        census.shot_splits as f64 / n,
        census.shot_draws as f64 / n,
        census.group_splits as f64 / n,
        census.group_draws as f64 / n,
        census.shot_splits as f64 / census.group_splits as f64,
    );
    for (key, cost) in CohortKey::ALL.into_iter().zip(&census.cost) {
        cohort_lines(line, key.name(), cost, &census, n);
    }
    print!("{out}");
}

fn cohort_lines(out: &mut String, name: &str, cost: &CohortCost, census: &cohorts::Census, n: f64) {
    let k = cost.updates as f64 / cost.launches as f64;
    let shots_per_launch = cost.shot_updates as f64 / cost.launches as f64;
    let eta = cost.cubes / cost.padded_cubes;
    let _ = writeln!(
        out,
        "{name} launches_per_shot={:.1} waves_per_shot={:.1} draw_launches_per_shot={:.1} \
         K={k:.2} K_by_updates={:.2} shots_per_launch={shots_per_launch:.2} eta={eta:.3} \
         K_eta={:.2} largest_K={} \
         cut_mismatch_launches={} count_mismatch_cohorts={} b_cubed_per_update={:.0}",
        cost.launches as f64 / n,
        cost.waves as f64 / n,
        cost.draw_launches as f64 / n,
        cost.updates_k / cost.updates as f64,
        k * eta,
        cost.largest_k,
        cost.cut_mismatches,
        cost.count_mismatches,
        cost.cubes / cost.updates as f64,
    );
    let total: f64 = cost.eta_bins.iter().sum();
    let bins: Vec<String> = cost
        .eta_bins
        .iter()
        .enumerate()
        .map(|(bin, cubes)| format!("{:.1}:{:.3}", bin as f64 / 10.0, cubes / total))
        .collect();
    let _ = writeln!(out, "{name}_ETA_BY_B3 {}", bins.join(" "));
    // Seconds per shot: each launch (in waves) at the per-update cost,
    // plus one sync per draw launch; and the same divided by η, as if
    // every update were compute-bound at the cohort's largest bond.
    for (label, micros) in UPDATE_MICROS {
        let updates = cost.waves as f64 * micros / n / 1e6;
        let draws = cost.draw_launches as f64 * SYNC_MICROS / n / 1e6;
        let per_shot_alone =
            (census.shot_splits as f64 * micros + census.shot_draws as f64 * SYNC_MICROS) / n / 1e6;
        let _ = writeln!(
            out,
            "{name}_PREDICT t_upd={label} seconds_per_shot={:.3} \
             padded_compute_bound={:.3} per_shot_alone={per_shot_alone:.2}",
            updates + draws,
            updates / eta + draws,
        );
    }
}
