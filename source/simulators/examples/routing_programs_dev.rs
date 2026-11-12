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
//!   `rb-eK` restores it every K routed operators, within a batch too;
//!   `guard-a`/`guard-b`/`guard-b-unread` are `rb` with the bond guard's
//!   policies, `thr-L`
//!   `rb` with the length threshold, `rb-check` and `rb-check-apply` `rb`
//!   checking its bond bound. `--routings a,b` keeps those routings.
//! - `--part need-log --qubits N --chi X --shots S`: where the
//!   capacity-bound splits sit (see [`need_log`]).
//!
//! ```text
//! cargo run --release -p qdk_simulators --example routing_programs_dev -- \
//!     --part corpus|programs|adversarial|need-log [--qubits 20] [--chi 16] \
//!     [--shots 16] [--programs a,b] [--routings a,b]
//! ```

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::type_complexity,
    reason = "a development harness"
)]

use std::num::NonZeroUsize;
use std::time::Instant;

use qdk_cpu_mps::CpuMpsEngine;
use qdk_simulators::bytecode::{AdaptiveProgram, Instruction};
use qdk_simulators::mps_simulator::{
    BoundCheck, GuardPolicy, MpsResumer, Restore, RouteTuning, Routing, reset_routing_trace,
    routing_trace,
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

/// `Engine`, `rb`, `rb` with a drift budget of 0.25 (`rb-db250`), `rb`
/// restoring every K routed operators within a batch too (`rb-eK`), `rb`
/// with the bond guard's policies (`guard-a`: cut, `guard-b`: hand over,
/// `guard-b-unread`: hand over on the bound as walked, never re-read),
/// `rb` with the length threshold (`thr-L`: restored after L routed
/// operators with no draw between), and `rb` checking its bond bound at
/// every read (`rb-check`) and after every operator (`rb-check-apply`).
fn adversarial_routings() -> Vec<(String, Routing)> {
    let tuned = |drift_budget_permille, restore_every_routed| {
        Routing::Tuned(RouteTuning {
            restore: Restore::BatchEnd,
            drift_budget_permille,
            restore_every_routed,
            ..RouteTuning::default()
        })
    };
    let rb_with = |tuning: RouteTuning| {
        Routing::Tuned(RouteTuning {
            restore: Restore::BatchEnd,
            ..tuning
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
    for (name, policy) in [
        ("guard-a", GuardPolicy::Cut),
        ("guard-b", GuardPolicy::HandOver),
        ("guard-b-unread", GuardPolicy::HandOverUnread),
    ] {
        all.push((
            name.to_string(),
            rb_with(RouteTuning {
                guard: Some(policy),
                ..RouteTuning::default()
            }),
        ));
    }
    for length in [8, 16, 32, 64, 128, 256] {
        all.push((
            format!("thr-{length}"),
            rb_with(RouteTuning {
                cut_after_routed: length,
                ..RouteTuning::default()
            }),
        ));
    }
    for (name, check_bound) in [
        ("rb-check", BoundCheck::Reads),
        ("rb-check-apply", BoundCheck::EveryApply),
    ] {
        all.push((
            name.to_string(),
            rb_with(RouteTuning {
                check_bound,
                ..RouteTuning::default()
            }),
        ));
    }
    all
}

fn adversarial(
    n: usize,
    chi: NonZeroUsize,
    shots: u32,
    only: Option<&str>,
    routings: Option<&str>,
) {
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
            if routings.is_some_and(|only| !only.split(',').any(|o| o == routing_name)) {
                continue;
            }
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
                 drift_fb={} bond_fb={} first_fb_batch={:?} max_drift={} \
                 guard_rereads={} guard_at_risk={} guard_cuts={} guard_handovers={} \
                 threshold_cuts={} bound_checks={} bound_violations={}",
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
                t.guard_rereads,
                t.guard_at_risk,
                t.guard_cuts,
                t.guard_handovers,
                t.threshold_cuts,
                t.bound_checks,
                t.bound_violations,
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
            value("--routings").as_deref(),
        ),
        Some("need-log") => need_log::run_all(
            number("--qubits", 20),
            NonZeroUsize::new(number("--chi", 16)).expect("--chi: at least 1"),
            u32::try_from(number("--shots", 8)).expect("a shot count"),
            value("--programs").as_deref(),
            value("--routings").as_deref(),
        ),
        other => panic!("--part {other:?}: corpus, programs, adversarial or need-log"),
    }
}

/// `--part need-log`: where the capacity-bound splits sit (step 1b).
///
/// Each program runs unfused in the resumer (so that every SWAP the
/// resumer routes reaches the engine as an exact SWAP) on the CPU engine,
/// wrapped by [`Logged`], which follows the chain's placement from those
/// SWAPs, keeps the logical pairs a non-SWAP two-site gate joined so far,
/// and takes the engine's split log after each call. The CPU engine fuses
/// a pair's gates until the next operation elsewhere, so a split drained
/// during a call belongs to the placement before that call's operator.
/// One `NEED` line per program and routing: for the capacity-bound splits
/// against all splits (the engine's own route splits apart), the share at
/// a displaced cut (one whose left side holds other sites than the
/// layout's), the joined pairs that straddle the cut under the placement
/// and under the layout's order, the inverted pairs that straddle it, and
/// the capacity-bound splits per cut.
mod need_log {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::num::NonZeroUsize;
    use std::rc::Rc;

    use num_complex::Complex64;
    use qdk_cpu_mps::CpuMpsEngine;
    use qdk_cpu_mps::dev_trace::{SplitEvent, start_split_log, stop_split_log, take_split_log};
    use qdk_simulators::mps_simulator::{GateFusion, MpsResumer};
    use qdk_simulators::qir::{QirMachine, QirResumer, QirShotId, expect_ready};
    use tensornet::{MpsEngine, MpsShape, PauliSum};

    /// The splits of one class: how many, the weight their capacity
    /// dropped, and the sums the means are made of.
    #[derive(Default)]
    struct Bucket {
        count: u64,
        lost: f64,
        displaced: u64,
        lost_displaced: f64,
        straddle_now: u64,
        straddle_layout: u64,
        inverted: u64,
    }

    impl Bucket {
        fn add(&mut self, event: &SplitEvent, cut: &CutView) {
            self.count += 1;
            self.lost += event.lost;
            if cut.displaced {
                self.displaced += 1;
                self.lost_displaced += event.lost;
            }
            self.straddle_now += cut.straddle_now;
            self.straddle_layout += cut.straddle_layout;
            self.inverted += cut.inverted;
        }

        fn line(&self, name: &str) -> String {
            #[allow(clippy::cast_precision_loss)]
            let mean = |sum: u64| {
                if self.count == 0 {
                    0.0
                } else {
                    sum as f64 / self.count as f64
                }
            };
            format!(
                "{name}={} {name}_lost={:.3e} {name}_displaced={} {name}_lost_displaced={:.3e} \
                 {name}_straddle_now={:.2} {name}_straddle_layout={:.2} {name}_inverted={:.2}",
                self.count,
                self.lost,
                self.displaced,
                self.lost_displaced,
                mean(self.straddle_now),
                mean(self.straddle_layout),
                mean(self.inverted),
            )
        }
    }

    /// What one cut is, under a placement.
    struct CutView {
        displaced: bool,
        straddle_now: u64,
        straddle_layout: u64,
        inverted: u64,
    }

    #[derive(Default)]
    struct Stats {
        /// The splits outside the engine's routes, capacity-bound and all.
        capacity_bound: Bucket,
        all: Bucket,
        /// The splits of the engine's own routes (`Engine`'s distant
        /// operators), capacity-bound.
        route_capacity_bound: Bucket,
        /// The capacity-bound splits (outside routes) per cut.
        per_cut: BTreeMap<usize, u64>,
    }

    /// A state with its placement: the layout site at each chain position,
    /// and the logical pairs a non-SWAP two-site gate joined.
    struct Placed<S> {
        inner: S,
        site_at: Vec<usize>,
        joined: Vec<(usize, usize)>,
    }

    impl<S> Placed<S> {
        fn view(&self, cut: usize) -> CutView {
            let n = self.site_at.len();
            let mut position = vec![0; n];
            for (at, &site) in self.site_at.iter().enumerate() {
                position[site] = at;
            }
            let displaced = self.site_at[..=cut].iter().any(|&site| site > cut);
            let straddles = |x: usize, y: usize| (x <= cut) != (y <= cut);
            let straddle_now = self
                .joined
                .iter()
                .filter(|&&(a, b)| straddles(position[a], position[b]))
                .count() as u64;
            let straddle_layout = self
                .joined
                .iter()
                .filter(|&&(a, b)| straddles(a, b))
                .count() as u64;
            let mut inverted = 0;
            for a in 0..n {
                for b in a + 1..n {
                    if position[a] > position[b] && straddles(position[a], position[b]) {
                        inverted += 1;
                    }
                }
            }
            CutView {
                displaced,
                straddle_now,
                straddle_layout,
                inverted,
            }
        }
    }

    /// The CPU engine, logged (see the module documentation).
    struct Logged {
        inner: CpuMpsEngine,
        stats: Rc<RefCell<Stats>>,
    }

    fn is_swap(matrix: &[Complex64]) -> bool {
        let one = |at: usize| [0, 6, 9, 15].contains(&at);
        matrix.len() == 16
            && matrix
                .iter()
                .enumerate()
                .all(|(at, z)| z.im == 0.0 && z.re == if one(at) { 1.0 } else { 0.0 })
    }

    impl Logged {
        fn drain(&self, state: &Placed<<CpuMpsEngine as MpsEngine>::State>, route: bool) {
            let mut stats = self.stats.borrow_mut();
            for event in take_split_log() {
                if event.transient {
                    continue;
                }
                let cut = state.view(event.cut);
                let bound = event.bond < event.need;
                if route {
                    if bound {
                        stats.route_capacity_bound.add(&event, &cut);
                    }
                    continue;
                }
                stats.all.add(&event, &cut);
                if bound {
                    stats.capacity_bound.add(&event, &cut);
                    *stats.per_cut.entry(event.cut).or_default() += 1;
                }
            }
        }
    }

    type Inner = <CpuMpsEngine as MpsEngine>::State;
    type Error = <CpuMpsEngine as MpsEngine>::Error;

    impl MpsEngine for Logged {
        type State = Placed<Inner>;
        type Error = Error;

        fn prepare(&self, capacity: &MpsShape) -> Result<Self::State, Error> {
            let state = Placed {
                inner: self.inner.prepare(capacity)?,
                site_at: (0..capacity.site_count()).collect(),
                joined: Vec::new(),
            };
            self.drain(&state, false);
            Ok(state)
        }

        fn apply(
            &self,
            state: &mut Self::State,
            sites: &[usize],
            matrix: &[Complex64],
        ) -> Result<(), Error> {
            if let &[a, b] = sites {
                if a.abs_diff(b) > 1 {
                    self.inner.shape(&state.inner)?;
                    self.drain(state, false);
                    let applied = self.inner.apply(&mut state.inner, sites, matrix);
                    self.drain(state, true);
                    let pair = (state.site_at[a], state.site_at[b]);
                    state.joined.push(pair);
                    return applied;
                }
                let applied = self.inner.apply(&mut state.inner, sites, matrix);
                self.drain(state, false);
                if is_swap(matrix) {
                    state.site_at.swap(a, b);
                } else {
                    let pair = (state.site_at[a], state.site_at[b]);
                    state.joined.push(pair);
                }
                return applied;
            }
            let applied = self.inner.apply(&mut state.inner, sites, matrix);
            self.drain(state, false);
            applied
        }

        fn probability_of_one(&self, state: &Self::State, site: usize) -> Result<f64, Error> {
            let read = self.inner.probability_of_one(&state.inner, site);
            self.drain(state, false);
            read
        }

        fn project(&self, state: &mut Self::State, site: usize, one: bool) -> Result<f64, Error> {
            let read = self.inner.project(&mut state.inner, site, one);
            self.drain(state, false);
            read
        }

        fn fork(&self, state: &Self::State) -> Result<Self::State, Error> {
            let inner = self.inner.fork(&state.inner)?;
            self.drain(state, false);
            Ok(Placed {
                inner,
                site_at: state.site_at.clone(),
                joined: state.joined.clone(),
            })
        }

        fn sample(
            &self,
            state: &Self::State,
            sites: &[usize],
            shots: usize,
            seed: u64,
        ) -> Result<Vec<bool>, Error> {
            let read = self.inner.sample(&state.inner, sites, shots, seed);
            self.drain(state, false);
            read
        }

        fn expectation(
            &self,
            state: &Self::State,
            observable: &PauliSum,
        ) -> Result<Complex64, Error> {
            let read = self.inner.expectation(&state.inner, observable);
            self.drain(state, false);
            read
        }

        fn shape(&self, state: &Self::State) -> Result<MpsShape, Error> {
            let read = self.inner.shape(&state.inner);
            self.drain(state, false);
            read
        }

        fn retained_weight(&self, state: &Self::State) -> Result<f64, Error> {
            let read = self.inner.retained_weight(&state.inner);
            self.drain(state, false);
            read
        }
    }

    pub(super) fn run_all(
        n: usize,
        chi: NonZeroUsize,
        shots: u32,
        only: Option<&str>,
        routings: Option<&str>,
    ) {
        for (name, _, program) in super::adversarial_programs(n) {
            if only.is_some_and(|only| !only.split(',').any(|o| o == name)) {
                continue;
            }
            for (routing_name, routing) in super::adversarial_routings() {
                if routings.is_some_and(|only| !only.split(',').any(|o| o == routing_name)) {
                    continue;
                }
                let stats = Rc::new(RefCell::new(Stats::default()));
                let engine = Logged {
                    inner: CpuMpsEngine::default(),
                    stats: Rc::clone(&stats),
                };
                let mut resumer = MpsResumer::new(engine, 1)
                    .with_routing(routing)
                    .with_gate_fusion(GateFusion::Unfused)
                    .with_max_bond_dimension(chi);
                start_split_log();
                let run =
                    expect_ready(resumer.run(&QirMachine::new(&program), &QirShotId::first(shots)));
                stop_split_log();
                let lost = 1.0 - resumer.last_run().least_retained_weight;
                let stats = stats.borrow();
                let per_cut = stats
                    .per_cut
                    .iter()
                    .map(|(cut, count)| format!("{cut}:{count}"))
                    .collect::<Vec<_>>()
                    .join(",");
                println!(
                    "NEED program={name} qubits={n} chi={chi} routing={routing_name} \
                     shots={shots} ok={} lost_weight={lost:.3e} {} {} {} cap_cuts={per_cut}",
                    run.is_ok(),
                    stats.capacity_bound.line("cap"),
                    stats.all.line("all"),
                    stats.route_capacity_bound.line("route_cap"),
                );
            }
        }
    }
}
