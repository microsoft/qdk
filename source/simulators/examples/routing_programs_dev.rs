// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development branch only (not for review): the resumer's `Routing::Tuned`
//! with the layout's order restored at every batch end (`rb`) against
//! `Routing::Engine`, on programs other than H₂.
//!
//! - `--part corpus`: every noiseless case of the `run_shot` corpus
//!   (`tests/run_shot_compatibility/run_shot_cases.json`), its shots in one
//!   `run` call, on the exact reference (at most 20 qubits) and the CPU
//!   engine; one `CASE` line per case and engine: whether each shot's
//!   records are the same under both routings, the largest bond and the
//!   lost weight of each.
//! - `--part programs`: the resumer kit's programs (`tests/resumers/`), the
//!   same way.
//! - `--part adversarial --qubits N --chi X --shots S`: programs of one long
//!   batch of distant two-qubit gates and no draw until every qubit is
//!   measured at the end, on the CPU engine at the capacity `X`, under both
//!   routings, and on the exact reference (`N` at most 20) as the truth:
//!   one `ADV` line per program and routing, with the capacity-bound
//!   splits, the lost weight, the routing's counters and how many shots
//!   record what the exact reference records; `rb-db250` is `rb` that also
//!   restores the order when a route leaves the drift above 0.25, and
//!   `rb-eK` restores it every K routed operators, within a batch too.
//!
//! ```text
//! cargo run --release -p qdk_simulators --example routing_programs_dev -- \
//!     --part corpus|programs|adversarial [--qubits 20] [--chi 16] [--shots 16]
//! ```

use std::num::NonZeroUsize;
use std::time::Instant;

use qdk_cpu_mps::CpuMpsEngine;
use qdk_simulators::bytecode::{AdaptiveProgram, Instruction};
use qdk_simulators::mps_simulator::{
    MpsResumer, Restore, RouteTuning, Routing, reset_routing_trace, routing_trace,
};
use qdk_simulators::qir::{QirMachine, QirProgram, QirResumer, QirShotId, expect_ready};
use serde::Deserialize;
use tensornet::MpsEngine;
use tensornet::reference::ExactReference;

#[path = "../tests/run_shot_compatibility/bytecode_json.rs"]
mod bytecode_json;

/// The bytecode's encoding, which the crate keeps to itself.
#[allow(dead_code)]
#[path = "../src/bytecode/encoding.rs"]
mod encoding;

#[allow(dead_code)]
#[path = "../tests/resumers/programs.rs"]
mod programs;

use encoding::{
    FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_SRC0_IMM, OP_QUANTUM_GATE, OPID_H, OPID_MZ, OPID_RXX,
    OPID_RY, OPID_RZZ,
};

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    shots: u32,
    noise: Option<serde_json::Value>,
    bytecode: bytecode_json::Bytecode,
}

fn rb() -> Routing {
    Routing::Tuned(RouteTuning {
        restore: Restore::BatchEnd,
        ..RouteTuning::default()
    })
}

/// What one `run` call of a program under one routing gave.
struct Outcome {
    /// The records of each shot, or the error, as text.
    shots: Result<Vec<String>, String>,
    largest_bond: usize,
    lost_weight: f64,
    seconds: f64,
    capacity_bound: u64,
    capacity_lost: f64,
    largest_need: usize,
    trace: qdk_simulators::mps_simulator::RoutingTrace,
}

fn run<E: MpsEngine>(
    engine: E,
    program: &QirProgram,
    shots: u32,
    seed: u64,
    chi: Option<NonZeroUsize>,
    routing: Routing,
) -> Outcome
where
    E::Error: std::fmt::Debug,
{
    let mut resumer = MpsResumer::new(engine, seed).with_routing(routing);
    if let Some(chi) = chi {
        resumer = resumer.with_max_bond_dimension(chi);
    }
    qdk_cpu_mps::dev_trace::reset();
    reset_routing_trace();
    let started = Instant::now();
    let run = expect_ready(resumer.run(&QirMachine::new(program), &QirShotId::first(shots)));
    let seconds = started.elapsed().as_secs_f64();
    let split = qdk_cpu_mps::dev_trace::get();
    let report = *resumer.last_run();
    Outcome {
        shots: run
            .map(|shots| shots.iter().map(|records| format!("{records:?}")).collect())
            .map_err(|error| format!("{error:?}")),
        largest_bond: report.largest_bond,
        lost_weight: 1.0 - report.least_retained_weight,
        seconds,
        capacity_bound: split.capacity_bound,
        capacity_lost: split.capacity_lost,
        largest_need: split.largest_need,
        trace: routing_trace(),
    }
}

/// One `CASE` line: `program` under both routings on engines `make` makes.
/// Whether every shot recorded the same.
fn compare<E: MpsEngine>(
    name: &str,
    engine_name: &str,
    make: &dyn Fn() -> E,
    program: &QirProgram,
    shots: u32,
) -> bool
where
    E::Error: std::fmt::Debug,
{
    let base = run(make(), program, shots, 0x5eed, None, Routing::Engine);
    let routed = run(make(), program, shots, 0x5eed, None, rb());
    let (same, differ, outcome) = match (&base.shots, &routed.shots) {
        (Ok(a), Ok(b)) => {
            let same = a.iter().zip(b).filter(|(a, b)| a == b).count();
            (same, a.len() - same, "ok".to_string())
        }
        (Err(a), Err(b)) if a == b => (0, 0, "same-error".to_string()),
        (a, b) => (
            0,
            shots as usize,
            format!(
                "MISMATCH engine_err={} rb_err={}",
                a.as_ref().err().map_or("none", |_| "yes"),
                b.as_ref().err().map_or("none", |_| "yes")
            ),
        ),
    };
    println!(
        "CASE name={name} engine={engine_name} shots={shots} outcome={outcome} same={same} \
         differ={differ} bond_engine={} bond_rb={} lost_engine={:.3e} lost_rb={:.3e} \
         route_swaps={} restore_swaps={} fallbacks={}",
        base.largest_bond,
        routed.largest_bond,
        base.lost_weight,
        routed.lost_weight,
        routed.trace.route_swaps,
        routed.trace.restore_swaps,
        routed.trace.drift_fallbacks + routed.trace.bond_fallbacks,
    );
    differ == 0 && !outcome.starts_with("MISMATCH")
}

fn corpus() {
    let corpus: Corpus = serde_json::from_str(include_str!(
        "../tests/run_shot_compatibility/run_shot_cases.json"
    ))
    .expect("the corpus is valid JSON");
    let (mut cases, mut failed) = (0, 0);
    for case in corpus.cases.iter().filter(|case| case.noise.is_none()) {
        let bytecode = case.bytecode.program();
        let qubits = bytecode.num_qubits;
        let program = QirProgram::new(bytecode).expect("the corpus builds");
        cases += 1;
        if qubits <= 20
            && !compare(
                &case.name,
                "reference",
                &ExactReference::new,
                &program,
                case.shots,
            )
        {
            failed += 1;
        }
        if !compare(
            &case.name,
            "cpu",
            &CpuMpsEngine::default,
            &program,
            case.shots,
        ) {
            failed += 1;
        }
    }
    println!("CORPUS cases={cases} failed={failed}");
}

fn kit_programs() {
    let all: [(&str, fn() -> QirProgram); 5] = [
        ("coin_then_branch", programs::coin_then_branch),
        ("coin_then_end_or_toss", programs::coin_then_end_or_toss),
        (
            "coin_then_read_one_of_two_qubits",
            programs::coin_then_read_one_of_two_qubits,
        ),
        (
            "coin_then_toss_into_one_of_two_results",
            programs::coin_then_toss_into_one_of_two_results,
        ),
        (
            "coin_then_a_batch_or_its_prefix",
            programs::coin_then_a_batch_or_its_prefix,
        ),
    ];
    for (name, make) in all {
        let program = make();
        compare(name, "reference", &ExactReference::new, &program, 64);
        compare(name, "cpu", &CpuMpsEngine::default, &program, 64);
    }
}

/// A gate of operation id `operation` on the constant qubits `q1` and `q2`
/// (`q2` unused by a one-qubit gate), with the constant `angle`.
fn gate(operation: u64, q1: usize, q2: usize, angle: f64) -> Instruction<u64> {
    Instruction {
        src0: angle.to_bits(),
        aux0: operation,
        aux1: q1 as u64,
        aux2: q2 as u64,
        ..programs::op(
            OP_QUANTUM_GATE,
            FLAG_SRC0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
        )
    }
}

/// splitmix64, for the programs' angles and pairs.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// `gates`, then every qubit measured into its result and recorded.
fn measured(qubits: usize, mut gates: Vec<Instruction<u64>>) -> QirProgram {
    for qubit in 0..qubits {
        gates.push(programs::measure(OPID_MZ, qubit as u64, qubit as u64));
    }
    for qubit in 0..qubits {
        gates.push(programs::record_result(qubit as u64));
    }
    gates.push(programs::ret());
    let size = u32::try_from(qubits).expect("a qubit count");
    let bytecode: AdaptiveProgram<u64> = programs::program(size, &[&gates]);
    QirProgram::new(bytecode).expect("the program builds")
}

/// The adversarial programs on `n` qubits: one batch each.
fn adversarial_programs(n: usize) -> Vec<(String, usize, QirProgram)> {
    let mut random = Random(0x0dd5_eed5);
    let ry_layer = |random: &mut Random, gates: &mut Vec<Instruction<u64>>| {
        for q in 0..n {
            gates.push(gate(OPID_RY, q, 0, 0.3 + 1.2 * random.unit()));
        }
    };
    let mut all = Vec::new();
    // Random far pairs: each layer, RZZ on random pairs at least n/4 apart,
    // one per two qubits, between RY layers; strong (angles 0.2-0.8) or weak
    // (0.02-0.12, like H₂'s many small rotations: a low need under
    // `Engine`). Batched: the last qubit is an ancilla, untouched, measured
    // and read after each layer, so that each layer is a batch of its own
    // (as H₂'s reads end its batches), on the other n - 1 qubits.
    for (kind, low, span, layer_counts) in [
        ("random-far", 0.2, 0.6, &[2, 4, 8, 16][..]),
        ("weak-far", 0.02, 0.1, &[8, 16, 32, 64][..]),
        ("batched-random-far", 0.2, 0.6, &[4, 8][..]),
        ("batched-weak-far", 0.02, 0.1, &[8, 16, 32][..]),
    ] {
        let batched = kind.starts_with("batched");
        let data = if batched { n - 1 } else { n };
        for &layers in layer_counts {
            let mut gates = Vec::new();
            let mut count = 0;
            for _ in 0..layers {
                for q in 0..data {
                    gates.push(gate(OPID_RY, q, 0, 0.3 + 1.2 * random.unit()));
                }
                for _ in 0..data / 2 {
                    let a = random.below(data);
                    let b = loop {
                        let b = random.below(data);
                        if a.abs_diff(b) >= data / 4 {
                            break b;
                        }
                    };
                    gates.push(gate(OPID_RZZ, a, b, low + span * random.unit()));
                    count += 1;
                }
                if batched {
                    gates.push(programs::measure(OPID_MZ, (n - 1) as u64, (n - 1) as u64));
                    gates.push(programs::read_result(0, (n - 1) as u64));
                }
            }
            all.push((format!("{kind}-L{layers}"), count, measured(n, gates)));
        }
    }
    // Mirror pairs: (i, n-1-i), every gate crossing the middle.
    for layers in [1, 3] {
        let mut gates = Vec::new();
        let mut count = 0;
        for _ in 0..layers {
            ry_layer(&mut random, &mut gates);
            for i in 0..n / 2 {
                gates.push(gate(OPID_RXX, i, n - 1 - i, 0.2 + 0.6 * random.unit()));
                count += 1;
            }
        }
        all.push((format!("mirror-L{layers}"), count, measured(n, gates)));
    }
    // A QFT-like ladder on an RY layer: H on i, then RZZ(π/2^(j-i)) on
    // (i, j) for every j > i: the order drifts toward the reverse.
    {
        let mut gates = Vec::new();
        let mut count = 0;
        ry_layer(&mut random, &mut gates);
        for i in 0..n {
            gates.push(gate(OPID_H, i, 0, 0.0));
            for j in i + 1..n {
                let angle = std::f64::consts::PI / f64::from(1u32 << (j - i).min(30));
                gates.push(gate(OPID_RZZ, i, j, angle));
                count += 1;
            }
        }
        all.push(("qft-ladder".to_string(), count, measured(n, gates)));
    }
    all
}

/// `Engine`, `rb`, `rb` with a drift budget of 0.25 (`rb-db250`), and
/// `rb` restoring every K routed operators within a batch too (`rb-eK`).
fn adversarial_routings() -> Vec<(String, Routing)> {
    let tuned = |drift_budget_permille, restore_every_routed| {
        Routing::Tuned(RouteTuning {
            restore: Restore::BatchEnd,
            drift_budget_permille,
            restore_every_routed,
            ..RouteTuning::default()
        })
    };
    let mut all = vec![
        ("engine".to_string(), Routing::Engine),
        ("rb".to_string(), rb()),
        ("rb-db250".to_string(), tuned(250, 0)),
    ];
    for every in [4, 8, 16, 32] {
        all.push((format!("rb-e{every}"), tuned(1000, every)));
    }
    all
}

fn adversarial(n: usize, chi: NonZeroUsize, shots: u32, only: Option<&str>) {
    for (name, count, program) in adversarial_programs(n) {
        if only.is_some_and(|only| !only.split(',').any(|o| o == name)) {
            continue;
        }
        let truth = (n <= 20).then(|| {
            run(
                ExactReference::new(),
                &program,
                shots,
                1,
                None,
                Routing::Engine,
            )
        });
        for (routing_name, routing) in adversarial_routings() {
            let outcome = run(
                CpuMpsEngine::default(),
                &program,
                shots,
                1,
                Some(chi),
                routing,
            );
            let as_exact = match (&truth, &outcome.shots) {
                (Some(Outcome { shots: Ok(t), .. }), Ok(o)) => {
                    t.iter().zip(o).filter(|(a, b)| a == b).count().to_string()
                }
                _ => "n/a".to_string(),
            };
            let t = outcome.trace;
            println!(
                "ADV program={name} qubits={n} two_qubit_gates={count} chi={chi} \
                 routing={routing_name} shots={shots} seconds={:.2} ok={} as_exact={as_exact} \
                 largest_bond={} lost_weight={:.3e} cap_bound={} cap_lost_sum={:.3e} \
                 need_max={} routed={} route_swaps={} restore_swaps={} restores={} \
                 drift_fb={} bond_fb={} first_fb_batch={:?} max_drift={}",
                outcome.seconds,
                outcome.shots.is_ok(),
                outcome.largest_bond,
                outcome.lost_weight,
                outcome.capacity_bound,
                outcome.capacity_lost,
                outcome.largest_need,
                t.routed,
                t.route_swaps,
                t.restore_swaps,
                t.restores,
                t.drift_fallbacks,
                t.bond_fallbacks,
                t.first_fallback_batch,
                t.max_drift_permille,
            );
        }
        if let Some(truth) = truth {
            println!(
                "ADV program={name} qubits={n} routing=exact shots={shots} seconds={:.2} ok={}",
                truth.seconds,
                truth.shots.is_ok(),
            );
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let number =
        |flag: &str, default: usize| value(flag).map_or(default, |v| v.parse().expect("a number"));
    match value("--part").as_deref() {
        Some("corpus") => corpus(),
        Some("programs") => kit_programs(),
        Some("adversarial") => adversarial(
            number("--qubits", 20),
            NonZeroUsize::new(number("--chi", 16)).expect("--chi: at least 1"),
            u32::try_from(number("--shots", 16)).expect("a shot count"),
            value("--programs").as_deref(),
        ),
        other => panic!("--part {other:?}: corpus, programs or adversarial"),
    }
}
