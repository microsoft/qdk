// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Spike (not for review): a GPU resumer of the QIR machine
//! (`gpu_resumer.rs`), checked and measured. One mode per run, given as one
//! comma-separated word:
//!
//! - `conformance`: the `qir::conformance` kit on the resumer;
//! - `corpus[,shots]`: the noiseless cases of the `run_shot` compatibility
//!   corpus, against what their Python tests expect and against the CPU
//!   simulator resumer's distribution;
//! - `tests,<programs.json>`: the adaptive GPU tests' programs, against
//!   what the tests expect and the CPU's distribution;
//! - `bench,<programs.json>,<qubits>:<shots>:<strategy>,...`: the bench
//!   program, a warm-up then the best of 3, with the time's breakdown;
//! - `profile,<qubits>:<shots>,...`: the GPU time of each kind of pass;
//! - `state,<qubits>:<shots>:<gates>,...`: the state after a random batch
//!   of every gate kind, for each strategy, against an f64 reference.
//!
//! GL processes hang at exit here: every mode ends with libc's `_exit`.

#[path = "../../tests/run_shot_compatibility/bytecode_json.rs"]
mod bytecode_json;
mod gpu_resumer;
#[path = "../qir_resumers/simulator_resumer.rs"]
mod simulator_resumer;

use std::collections::BTreeMap;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use num_complex::Complex64;
use qdk_simulators::cpu_full_state_simulator::FullStateSimulator;
use qdk_simulators::qir::{
    OperatorMatrix, QirCall, QirGate, QirMachine, QirProgram, QirResumer, QirShotId, conformance,
    expect_ready,
};
use qdk_simulators::{MeasurementResult, OutputRecord, Simulator};
use serde::Deserialize;

use bytecode_json::Bytecode;
use gpu_resumer::{Gpu, QirGpuResumer, Strategy};
use simulator_resumer::QirSimulatorResumer;

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    let words: Vec<&str> = arg.split(',').collect();
    let started = Instant::now();
    let gpu = Gpu::new();
    println!(
        "adapter {}: pipelines created in {:.3} s, compiled in {:.3} s (gpu ready after {:.3} s)",
        gpu.adapter_name,
        gpu.create_seconds,
        gpu.compile_seconds,
        started.elapsed().as_secs_f64()
    );
    let ok = match words[0] {
        "conformance" => conformance_mode(&gpu),
        "corpus" => corpus_mode(
            &gpu,
            words.get(1).map_or(2000, |w| w.parse().expect("shots")),
        ),
        "tests" => tests_mode(&gpu, words[1]),
        "bench" => bench_mode(&gpu, &words[1..]),
        "profile" => profile_mode(&gpu, &words[1..]),
        "state" => state_mode(&gpu, &words[1..]),
        other => {
            println!("no mode {other}");
            false
        }
    };
    println!("{}", if ok { "PASS" } else { "FAIL" });
    std::io::stdout().flush().expect("stdout flushes");
    // Even `process::exit` hangs here, in an exit handler of the GL
    // driver: end without running them.
    unsafe extern "C" {
        fn _exit(code: i32) -> !;
    }
    // SAFETY: output is flushed; nothing else needs to run.
    unsafe { _exit(if ok { 0 } else { 1 }) }
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_default()
}

fn conformance_mode(gpu: &Rc<Gpu>) -> bool {
    let mut ok = true;
    let t = Instant::now();
    let checked = catch_unwind(AssertUnwindSafe(|| {
        conformance::check(|| QirGpuResumer::new(gpu.clone(), Strategy::ALL));
    }));
    println!(
        "conformance::check: {} ({:.2} s)",
        checked
            .as_ref()
            .map_or_else(|e| panic_text(e.as_ref()), |()| "passed".into()),
        t.elapsed().as_secs_f64()
    );
    ok &= checked.is_ok();
    let t = Instant::now();
    let checked = catch_unwind(AssertUnwindSafe(|| {
        conformance::check_with_failing_shots(|failing| {
            QirGpuResumer::new(gpu.clone(), Strategy::ALL).failing(failing)
        });
    }));
    println!(
        "conformance::check_with_failing_shots: {} ({:.2} s)",
        checked
            .as_ref()
            .map_or_else(|e| panic_text(e.as_ref()), |()| "passed".into()),
        t.elapsed().as_secs_f64()
    );
    ok &= checked.is_ok();
    for (name, strategy) in [
        ("perpass", Strategy::PER_GATE),
        ("single", Strategy::named("single").expect("a strategy")),
    ] {
        let checked = catch_unwind(AssertUnwindSafe(|| {
            conformance::check(|| QirGpuResumer::new(gpu.clone(), strategy));
        }));
        println!(
            "conformance::check ({name}): {}",
            checked
                .as_ref()
                .map_or_else(|e| panic_text(e.as_ref()), |()| "passed".into()),
        );
        ok &= checked.is_ok();
    }
    ok
}

/// The results of `records` as the Python tests write them.
fn results(records: &[OutputRecord]) -> String {
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

/// Shots of `machine`'s program on the CPU simulator resumer (f64), each
/// seeded from its id (seeds unlike the GPU's).
fn cpu_shots(machine: &QirMachine<'_>, shots: u32) -> Vec<Vec<OutputRecord>> {
    let full_state = |program: &QirProgram, shot: QirShotId| {
        FullStateSimulator::new(
            program.qubits() as usize,
            program.results() as usize,
            shot.0.wrapping_mul(2_654_435_761).wrapping_add(17),
            Arc::default(),
        )
    };
    expect_ready(QirSimulatorResumer::new(full_state).run(machine, &QirShotId::first(shots)))
        .expect("the CPU runs the program")
}

fn counts(shots: &[Vec<OutputRecord>]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for records in shots {
        *counts.entry(format!("{records:?}")).or_default() += 1;
    }
    counts
}

/// The total-variation distance of two samples' distributions, and the
/// largest z score of one outcome's frequency difference.
fn compare(a: &[Vec<OutputRecord>], b: &[Vec<OutputRecord>]) -> (f64, f64) {
    let (ca, cb) = (counts(a), counts(b));
    #[allow(clippy::cast_precision_loss)]
    let (na, nb) = (a.len() as f64, b.len() as f64);
    let mut tvd = 0.0;
    let mut z_max: f64 = 0.0;
    let keys: std::collections::BTreeSet<&String> = ca.keys().chain(cb.keys()).collect();
    for key in keys {
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = (
            ca.get(key).copied().unwrap_or(0) as f64,
            cb.get(key).copied().unwrap_or(0) as f64,
        );
        let (pa, pb) = (x / na, y / nb);
        tvd += 0.5 * (pa - pb).abs();
        let pooled = (x + y) / (na + nb);
        let var = pooled * (1.0 - pooled) * (1.0 / na + 1.0 / nb);
        if var > 0.0 {
            z_max = z_max.max((pa - pb).abs() / var.sqrt());
        }
    }
    (tvd, z_max)
}

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    shots: u32,
    noise: Option<serde_json::Value>,
    expect: Expect,
    bytecode: Bytecode,
}

#[derive(Deserialize)]
struct Expect {
    every_shot: Option<String>,
    every_shot_records: Option<serde_json::Value>,
    distribution: Option<Distribution>,
    differential_only: Option<String>,
}

#[derive(Deserialize)]
struct Distribution {
    support: Vec<String>,
    greater_than: BTreeMap<String, usize>,
}

/// The corpus's records, as `OutputRecord`'s Debug text, for comparison.
fn corpus_records(value: &serde_json::Value) -> Vec<OutputRecord> {
    value
        .as_array()
        .expect("a list of records")
        .iter()
        .map(|record| {
            let (kind, v) = record
                .as_object()
                .and_then(|o| o.iter().next())
                .expect("a record is an object of one key");
            match kind.as_str() {
                "Result" => OutputRecord::Result(match v.as_str() {
                    Some("Zero") => MeasurementResult::Zero,
                    Some("One") => MeasurementResult::One,
                    _ => MeasurementResult::Loss,
                }),
                "Bool" => OutputRecord::Bool(v.as_bool().expect("a bool")),
                "Int" => OutputRecord::Int(v.as_i64().expect("an int")),
                _ => OutputRecord::Double(v.as_f64().expect("a double")),
            }
        })
        .collect()
}

impl Case {
    /// How the GPU's `shots` miss the case's expectation, with its
    /// distribution's counts scaled to the shots run.
    fn misses(&self, shots: &[Vec<OutputRecord>]) -> Vec<String> {
        let mut misses = Vec::new();
        let expect = &self.expect;
        if let Some(expected) = &expect.every_shot {
            for (shot, records) in shots.iter().enumerate() {
                if results(records) != *expected {
                    misses.push(format!("shot {shot}: {records:?}, not {expected}"));
                }
            }
        } else if let Some(expected) = &expect.every_shot_records {
            let expected = format!("{:?}", corpus_records(expected));
            for (shot, records) in shots.iter().enumerate() {
                if format!("{records:?}") != expected {
                    misses.push(format!("shot {shot}: {records:?}, not {expected}"));
                }
            }
        } else if let Some(distribution) = &expect.distribution {
            let mut counts = BTreeMap::<String, usize>::new();
            for (shot, records) in shots.iter().enumerate() {
                let results = results(records);
                if distribution.support.contains(&results) {
                    *counts.entry(results).or_default() += 1;
                } else {
                    misses.push(format!("shot {shot}: {records:?}, out of the support"));
                }
            }
            for (results, least) in &distribution.greater_than {
                let least = least * shots.len() / self.shots as usize;
                let count = counts.get(results).copied().unwrap_or_default();
                if count <= least {
                    misses.push(format!("{results} in {count} shots, not > {least}"));
                }
            }
        } else {
            assert!(expect.differential_only.is_some(), "an expectation");
            for (shot, records) in shots.iter().enumerate() {
                let results = results(records);
                if results.len() < 4 || results.contains('L') {
                    misses.push(format!("shot {shot}: {records:?}"));
                }
            }
        }
        misses
    }
}

fn corpus_mode(gpu: &Rc<Gpu>, at_least: u32) -> bool {
    let corpus: Corpus = serde_json::from_str(include_str!(
        "../../tests/run_shot_compatibility/run_shot_cases.json"
    ))
    .expect("the corpus parses");
    let mut resumer = QirGpuResumer::new(gpu.clone(), Strategy::ALL);
    let (mut run, mut passed, mut skipped, mut failed) = (0, 0, Vec::new(), Vec::new());
    let mut tvd_max: (f64, String) = (0.0, String::new());
    let mut z_max: (f64, String) = (0.0, String::new());
    let t = Instant::now();
    for case in &corpus.cases {
        if case.noise.is_some() {
            skipped.push(format!("{}: noise", case.name));
            continue;
        }
        let program = QirProgram::new(case.bytecode.program()).expect("the case builds");
        let machine = QirMachine::new(&program);
        let shots = at_least.max(case.shots);
        run += 1;
        let gpu_shots = match expect_ready(resumer.run(&machine, &QirShotId::first(shots))) {
            Ok(records) => records,
            Err(error) => {
                failed.push(format!("{}: the GPU run fails: {error}", case.name));
                continue;
            }
        };
        let misses = case.misses(&gpu_shots);
        let cpu = cpu_shots(&machine, shots);
        let (tvd, z) = compare(&gpu_shots, &cpu);
        if tvd > tvd_max.0 {
            tvd_max = (tvd, case.name.clone());
        }
        if z > z_max.0 {
            z_max = (z, case.name.clone());
        }
        println!(
            "{}: {} qubits, {shots} shots, misses {}, tvd {tvd:.4}, z {z:.2}",
            case.name,
            program.qubits(),
            misses.len()
        );
        if !misses.is_empty() || z > 5.0 {
            failed.push(format!(
                "{}: misses {:?}, tvd {tvd:.4}, z {z:.2}",
                case.name,
                misses.iter().take(3).collect::<Vec<_>>()
            ));
        } else {
            passed += 1;
        }
    }
    println!(
        "corpus: {run} cases run, {passed} passed, {} failed, {} skipped ({:.1} s)",
        failed.len(),
        skipped.len(),
        t.elapsed().as_secs_f64()
    );
    println!(
        "largest tvd {:.4} ({}); largest z {:.2} ({})",
        tvd_max.0, tvd_max.1, z_max.0, z_max.1
    );
    for s in &skipped {
        println!("skipped: {s}");
    }
    for f in &failed {
        println!("FAILED: {f}");
    }
    println!("stats: {}", resumer.stats);
    failed.is_empty()
}

#[derive(Deserialize)]
struct Programs {
    programs: Vec<Named>,
}

#[derive(Deserialize)]
struct Named {
    name: String,
    bytecode: Bytecode,
}

fn load_programs(path: &str) -> Vec<(String, QirProgram)> {
    let text = std::fs::read_to_string(path).expect("the programs file reads");
    let programs: Programs = serde_json::from_str(&text).expect("the programs parse");
    programs
        .programs
        .into_iter()
        .map(|p| {
            let program = QirProgram::new(p.bytecode.program()).expect("the program builds");
            (p.name, program)
        })
        .collect()
}

fn count_results(shots: &[Vec<OutputRecord>]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for records in shots {
        *counts.entry(results(records)).or_default() += 1;
    }
    counts
}

#[allow(clippy::too_many_lines)]
fn tests_mode(gpu: &Rc<Gpu>, path: &str) -> bool {
    let programs = load_programs(path);
    let mut ok = true;
    for (name, program) in &programs {
        let machine = QirMachine::new(program);
        let shots: u32 = match name.as_str() {
            "TeleportedTEcho" => 8,
            "TeleportedTDistribution" => 256,
            "TeleportedTNoiseReporting" => 64,
            "CONDITIONAL_LOOP_QIR" => 5000,
            n if n.starts_with("bench_") => {
                if program.qubits() <= 12 {
                    2000
                } else {
                    continue;
                }
            }
            _ => 10_000,
        };
        let mut resumer = QirGpuResumer::new(gpu.clone(), Strategy::ALL);
        let t = Instant::now();
        let gpu_shots = match expect_ready(resumer.run(&machine, &QirShotId::first(shots))) {
            Ok(records) => records,
            Err(error) => {
                println!("{name}: FAILED: the GPU run fails: {error}");
                ok = false;
                continue;
            }
        };
        let gpu_seconds = t.elapsed().as_secs_f64();
        let counts = count_results(&gpu_shots);
        let get = |k: &str| counts.get(k).copied().unwrap_or(0);
        let all = |want: &dyn Fn(&str) -> bool| counts.keys().all(|k| want(k));
        let check: Result<(), String> = match name.as_str() {
            "TeleportedTEcho" => {
                if get(&"0".repeat(20)) == 8 {
                    Ok(())
                } else {
                    Err(format!("{counts:?}"))
                }
            }
            "TeleportedTDistribution" => {
                let target = [2, 6, 17];
                let expected = (std::f64::consts::PI / 8.0).sin().powi(2);
                let shape = all(&|k: &str| {
                    k.len() == 20
                        && k.chars()
                            .enumerate()
                            .all(|(i, b)| b == '0' || target.contains(&i))
                });
                let probabilities: Vec<f64> = target
                    .iter()
                    .map(|&bit| {
                        let ones: usize = counts
                            .iter()
                            .filter(|(k, _)| k.as_bytes()[bit] == b'1')
                            .map(|(_, c)| c)
                            .sum();
                        #[allow(clippy::cast_precision_loss)]
                        let p = ones as f64 / f64::from(shots);
                        p
                    })
                    .collect();
                if shape && probabilities.iter().all(|p| (p - expected).abs() < 0.07) {
                    Ok(())
                } else {
                    Err(format!(
                        "shape {shape}, P(1) {probabilities:?} vs {expected:.4}"
                    ))
                }
            }
            "TeleportedTNoiseReporting" => {
                // Noiseless: no loss, and the 20 data bits all 0 (the first
                // three are the teleports' random input results).
                if all(&|k: &str| k.len() == 23 && !k.contains('L') && k[3..] == "0".repeat(20)) {
                    Ok(())
                } else {
                    Err(format!("{counts:?}"))
                }
            }
            "MEASURE_AND_CORRECT_QIR" => {
                if get("0") > 4000 && get("1") > 4000 && get("0") + get("1") == 10_000 {
                    Ok(())
                } else {
                    Err(format!("{counts:?}"))
                }
            }
            "CONDITIONAL_LOOP_QIR" => {
                if get("1") == 5000 {
                    Ok(())
                } else {
                    Err(format!("{counts:?}"))
                }
            }
            "LOOP_WITH_PHI_QIR" => {
                if get("00000") > 4000
                    && get("11111") > 4000
                    && get("00000") + get("11111") == 10_000
                {
                    Ok(())
                } else {
                    Err(format!("{counts:?}"))
                }
            }
            "BOOLEAN_COMPUTATION_QIR" => {
                let (c0, c1) = (get("0"), get("1"));
                if 1500 < c1 && c1 < 3500 && 6500 < c0 && c0 < 8500 && c0 + c1 == 10_000 {
                    Ok(())
                } else {
                    Err(format!("{counts:?}"))
                }
            }
            "TELEPORT_CHAIN_QIR" => {
                if get("00") > 4000 && get("11") > 4000 && get("00") + get("11") == 10_000 {
                    Ok(())
                } else {
                    Err(format!("{counts:?}"))
                }
            }
            "DYNAMIC_ROTATION_ANGLE_QIR" => {
                let (c0, c1) = (get("0"), get("1"));
                if c1 > 1400 && c0 > 8400 && c0 + c1 == 10_000 {
                    Ok(())
                } else {
                    Err(format!("{counts:?}"))
                }
            }
            _ => Ok(()),
        };
        let cpu = cpu_shots(&machine, shots);
        let (tvd, z) = compare(&gpu_shots, &cpu);
        let distinct = counts.len();
        let line = format!(
            "{name}: {} qubits, {shots} shots in {gpu_seconds:.3} s, {distinct} distinct results, \
             tvd vs CPU {tvd:.4}, z {z:.2}",
            program.qubits()
        );
        match check {
            Ok(()) if z <= 5.0 => println!("{line}: ok"),
            Ok(()) => {
                println!("{line}: FAILED: z > 5 against the CPU");
                ok = false;
            }
            Err(why) => {
                println!("{line}: FAILED: {why}");
                ok = false;
            }
        }
        println!("  stats: {}", resumer.stats);
    }
    ok
}

/// `words`: the programs file, then configurations `qubits:shots:strategy`.
fn bench_mode(gpu: &Rc<Gpu>, words: &[&str]) -> bool {
    let programs = load_programs(words[0]);
    for config in &words[1..] {
        let parts: Vec<&str> = config.split(':').collect();
        let n: u32 = parts[0].parse().expect("qubits");
        let shots: u32 = parts[1].parse().expect("shots");
        let strategy_name = parts.get(2).copied().unwrap_or("blocked");
        bench_one(gpu, &programs, n, shots, strategy_name);
    }
    true
}

fn bench_one(
    gpu: &Rc<Gpu>,
    programs: &[(String, QirProgram)],
    n: u32,
    shots: u32,
    strategy_name: &str,
) {
    let strategy = Strategy::named(strategy_name).expect("a strategy");
    let name = format!("bench_{n}");
    let (_, program) = programs
        .iter()
        .find(|(p, _)| *p == name)
        .expect("the bench program");
    let machine = QirMachine::new(program);
    let ids = QirShotId::first(shots);
    let mut resumer = QirGpuResumer::new(gpu.clone(), strategy);
    let mut best = f64::INFINITY;
    let mut best_stats = None;
    let mut first = 0.0;
    let mut all = Vec::new();
    for run in 0..4 {
        let before = resumer.stats.clone();
        let t = Instant::now();
        let records = expect_ready(resumer.run(&machine, &ids)).expect("the bench runs");
        let seconds = t.elapsed().as_secs_f64();
        assert_eq!(records.len(), shots as usize);
        let stats = resumer.stats.since(&before);
        println!("run {run}: {seconds:.4} s; {stats}");
        all.push(seconds);
        if run == 0 {
            first = seconds;
        } else if seconds < best {
            best = seconds;
            best_stats = Some(stats);
        }
    }
    let stats = best_stats.expect("three runs");
    #[allow(clippy::cast_precision_loss)]
    let per_round = stats.total().as_secs_f64() / stats.rounds as f64;
    #[allow(clippy::cast_precision_loss)]
    let per_pass = stats.total().as_secs_f64() / stats.passes as f64;
    println!(
        "BENCH n={n} shots={shots} strategy={strategy_name}: first {first:.4} s, best {best:.4} s, \
         {:.3} ms per round, {:.3} ms per pass; best: {stats}",
        per_round * 1e3,
        per_pass * 1e3,
    );
}

/// `profile,n:shots,...`: milliseconds per pass of each kind.
fn profile_mode(gpu: &Rc<Gpu>, words: &[&str]) -> bool {
    let mut resumer = QirGpuResumer::new(gpu.clone(), Strategy::ALL);
    for word in words {
        let (n, shots) = word.split_once(':').expect("n:shots");
        let (n, shots): (u32, u32) = (n.parse().expect("n"), shots.parse().expect("shots"));
        for (name, ms) in resumer.profile(n, shots, 20) {
            #[allow(clippy::cast_precision_loss)]
            let gib_s = (f64::from(shots) * (1u64 << n) as f64 * 8.0 * 2.0)
                / (ms * 1e-3)
                / f64::from(1u32 << 30);
            println!(
                "PROFILE n={n} shots={shots} {name}: {ms:.3} ms per pass \
                 ({gib_s:.1} GiB/s if it reads and writes the state once)"
            );
        }
    }
    true
}

/// `state,n:slots:gates,...`: the state after a random batch of every gate
/// kind, on random qubits (low, high, and across), for each strategy, against
/// an f64 CPU reference; passes when every fidelity is above 1 - 1e-5.
fn state_mode(gpu: &Rc<Gpu>, words: &[&str]) -> bool {
    let gates = [
        QirGate::X,
        QirGate::Y,
        QirGate::Z,
        QirGate::H,
        QirGate::S,
        QirGate::SAdj,
        QirGate::T,
        QirGate::TAdj,
        QirGate::Sx,
        QirGate::SxAdj,
        QirGate::Rx,
        QirGate::Ry,
        QirGate::Rz,
        QirGate::Cx,
        QirGate::Cy,
        QirGate::Cz,
        QirGate::Rxx,
        QirGate::Ryy,
        QirGate::Rzz,
        QirGate::Swap,
    ];
    let mut ok = true;
    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for word in words {
        let mut it = word.split(':').map(|w| w.parse::<u32>().expect("number"));
        let (n, slots, count) = (
            it.next().expect("n"),
            it.next().expect("slots"),
            it.next().expect("gates"),
        );
        // H on every qubit first, so that every gate acts on a superposition.
        let mut batch: Vec<QirCall> = (0..n)
            .map(|q| QirCall::Gate {
                gate: QirGate::H,
                qubits: (q, None),
                angle: None,
            })
            .collect();
        let qubit = |r: u64| u32::try_from(r % u64::from(n)).expect("small");
        for _ in 0..count {
            let gate = gates[usize::try_from(next() % gates.len() as u64).expect("small")];
            #[allow(clippy::cast_precision_loss)]
            let angle = matches!(
                gate,
                QirGate::Rx
                    | QirGate::Ry
                    | QirGate::Rz
                    | QirGate::Rxx
                    | QirGate::Ryy
                    | QirGate::Rzz
            )
            .then(|| (next() % 10_000) as f64 / 10_000.0 * 6.0 - 3.0);
            let a = qubit(next());
            let two = matches!(gate.matrix_at(angle), OperatorMatrix::Two(_));
            let second = two.then(|| {
                let mut b = qubit(next());
                while b == a {
                    b = qubit(next());
                }
                b
            });
            batch.push(QirCall::Gate {
                gate,
                qubits: (a, second),
                angle,
            });
        }
        // The f64 reference; a two-qubit matrix's row is (bit a, bit b).
        let mut psi = vec![Complex64::new(0.0, 0.0); 1 << n];
        psi[0] = Complex64::new(1.0, 0.0);
        for call in &batch {
            let QirCall::Gate {
                gate,
                qubits: (a, b),
                angle,
            } = call
            else {
                unreachable!()
            };
            match (gate.matrix_at(*angle), b) {
                (OperatorMatrix::One(m), None) => {
                    let bit = 1usize << a;
                    for k in 0..psi.len() {
                        if k & bit == 0 {
                            let (x, y) = (psi[k], psi[k | bit]);
                            psi[k] = m[0] * x + m[1] * y;
                            psi[k | bit] = m[2] * x + m[3] * y;
                        }
                    }
                }
                (OperatorMatrix::Two(m), Some(b)) => {
                    let (ba, bb) = (1usize << a, 1usize << b);
                    for k in 0..psi.len() {
                        if k & (ba | bb) == 0 {
                            let idx = [k, k | bb, k | ba, k | ba | bb];
                            let v = idx.map(|i| psi[i]);
                            for r in 0..4 {
                                psi[idx[r]] = (0..4).map(|c| m[4 * r + c] * v[c]).sum();
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
        for name in [
            "blocked", "nogather", "noshare", "nokron", "fused", "perpass",
        ] {
            let mut resumer =
                QirGpuResumer::new(gpu.clone(), Strategy::named(name).expect("a strategy"));
            let state = match resumer.state_after(n, slots, &batch) {
                Ok(state) => state,
                Err(error) => {
                    println!("STATE n={n} {name}: FAILED: {error}");
                    ok = false;
                    continue;
                }
            };
            let got: Vec<Complex64> = state
                .iter()
                .map(|&[re, im]| Complex64::new(f64::from(re), f64::from(im)))
                .collect();
            let overlap: Complex64 = psi.iter().zip(&got).map(|(r, g)| r.conj() * g).sum();
            let norm: f64 = got.iter().map(Complex64::norm_sqr).sum();
            let fidelity = overlap.norm_sqr() / norm;
            let phase = overlap / overlap.norm();
            let max_diff = psi
                .iter()
                .zip(&got)
                .map(|(r, g)| (r * phase - g).norm())
                .fold(0.0f64, f64::max);
            let pass = 1.0 - fidelity < 1e-5;
            ok &= pass;
            println!(
                "STATE n={n} slots={slots} gates={} strategy={name}: 1-fidelity {:.2e}, \
                 norm {norm:.6}, max |diff| {max_diff:.2e}: {}",
                batch.len(),
                1.0 - fidelity,
                if pass { "ok" } else { "FAIL" }
            );
        }
    }
    ok
}
