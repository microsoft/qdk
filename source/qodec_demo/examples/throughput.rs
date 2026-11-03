//! The cost of the chain against the Qodec runtime's own stack, on the
//! programs and coins of the tests: c422 (one layer) and c4c6 (two layers
//! stacked), each run three ways:
//!
//! - `stack`: the Qodec runtime's own stack, shot by shot, each top call
//!   converted from the protocol's per shot (the reference);
//! - `chain`: `static_driver() >> qodec >> device`;
//! - `profiled`: `static_driver() >> qodec >> profiler() >> device`.
//!
//! It first checks, on 8 shots, that the three give the same outcomes and
//! run as many bottom instructions; then times each, interleaved, best of
//! 5, and reports nanoseconds per bottom instruction and the ratios to the
//! stack's. Each timing includes building what runs, once per run: the
//! chain's layers (each plans its own layer and its decoder) and the stack
//! (which plans every layer and its decoder); the per-shot runtimes are
//! made inside the run in both. A sample of fewer than 1024 shots runs
//! `⌈1024 / shots⌉` times over, so that each sample is long enough to time;
//! it reports, too, the time to build the chain's layers alone, which weighs
//! most on few shots.
//!
//! `cargo run --release -p qodec_demo --example throughput [shots...]`
//! (by default 1000 and 3000 shots).

#[path = "../tests/common/mod.rs"]
mod common;

use std::time::Instant;

use executor_protocol::{Needs, ShotId, ShotOutcome, ShotSet, shot_seed, static_driver};
use qodec::Qodec;
use qodec_demo::coin::{CoinBackend, coin_device};
use qodec_demo::profiler::profiler;
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::QodecLayer;
use qodec_demo::stack::on_the_stack_over;

use common::{SEED, bottom, c4c6, c4c6_program, c422, c422_program, sorted};

const REPEATS: usize = 5;

/// The shots a sample runs at least, by running small sets several times.
const SAMPLE_SHOTS: ShotId = 1024;

/// One example: its qodec and its program.
struct Case {
    name: &'static str,
    qodec: Qodec,
    /// How many Qodec layers the chain stacks: the qodec's layers but the
    /// bottom one, the device's.
    layers: usize,
    program: Program,
}

#[derive(Clone, Copy)]
enum Variant {
    Stack,
    Chain,
    Profiled,
}

const VARIANTS: [(Variant, &str); 3] = [
    (Variant::Stack, "stack"),
    (Variant::Chain, "chain"),
    (Variant::Profiled, "profiled"),
];

/// The outcomes of the chain from the layers of `case` over coins, the
/// profiler between them when `profiled`, on `shots`, and the bottom
/// instructions the coins ran.
fn chain(case: &Case, shots: &ShotSet, profiled: bool) -> (Vec<ShotOutcome>, u64) {
    let layer = |k| {
        QodecLayer::new(&case.qodec, k)
            .expect("the qodec plans")
            .adapter()
    };
    let coins = || coin_device(bottom(&case.qodec).set.clone(), SEED);
    let program = case.program.to_static(Needs::default());
    macro_rules! go {
        ($chain:expr, |$report:pat_param| $count:expr) => {{
            let chain = $chain.expect("every boundary matches");
            let (outcomes, $report) = chain
                .run(program, shots.clone())
                .expect("the job fits the chain")
                .wait();
            (outcomes.expect("the run succeeds"), $count)
        }};
    }
    match (case.layers, profiled) {
        (1, false) => go!(static_driver() >> layer(0) >> coins(), |r| r
            .states
            .instructions()),
        (1, true) => go!(
            static_driver() >> layer(0) >> profiler() >> coins(),
            |(_, r)| r.states.instructions()
        ),
        (2, false) => go!(static_driver() >> layer(0) >> layer(1) >> coins(), |r| r
            .states
            .instructions()),
        (2, true) => go!(
            static_driver() >> layer(0) >> layer(1) >> profiler() >> coins(),
            |(_, r)| r.states.instructions()
        ),
        (layers, _) => panic!("{}: {layers} layers", case.name),
    }
}

/// The time to build the chain's layers of `case`, best of `REPEATS`.
fn build_time(case: &Case) -> f64 {
    (0..REPEATS)
        .map(|_| {
            let start = Instant::now();
            for k in 0..case.layers {
                let layer = QodecLayer::new(&case.qodec, k).expect("the qodec plans");
                std::hint::black_box(layer);
            }
            start.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

/// The outcomes of `variant` on `shots`, and the bottom instructions run.
fn run(case: &Case, variant: Variant, shots: &ShotSet) -> (Vec<ShotOutcome>, u64) {
    match variant {
        Variant::Stack => {
            let bottom = bottom(&case.qodec);
            let stack = on_the_stack_over(&case.qodec, &case.program, shots, |shot| {
                CoinBackend::new(shot_seed(SEED, shot), bottom.clone())
            })
            .expect("the stack runs");
            let count = (stack.backends.iter())
                .map(|(_, coins)| coins.ran().instructions())
                .sum();
            (stack.outcomes, count)
        }
        Variant::Chain => chain(case, shots, false),
        Variant::Profiled => chain(case, shots, true),
    }
}

fn main() {
    let counts: Vec<ShotId> = std::env::args()
        .skip(1)
        .map(|s| s.parse().expect("a number of shots"))
        .collect();
    let counts = if counts.is_empty() {
        vec![1000, 3000]
    } else {
        counts
    };
    let cases = [
        {
            let qodec = c422();
            let program = c422_program(&qodec);
            Case {
                name: "c422",
                qodec,
                layers: 1,
                program,
            }
        },
        {
            let qodec = c4c6();
            let program = c4c6_program(&qodec);
            Case {
                name: "c4c6",
                qodec,
                layers: 2,
                program,
            }
        },
    ];

    let few = ShotSet::range(0..=7).expect("not empty");
    for case in &cases {
        let (expected, n) = run(case, Variant::Stack, &few);
        assert!(!expected.is_empty(), "{}: the stack reads", case.name);
        let expected = sorted(expected);
        for (variant, name) in VARIANTS {
            let (outcomes, m) = run(case, variant, &few);
            assert_eq!(
                sorted(outcomes),
                expected,
                "{}: {name}: outcomes differ",
                case.name
            );
            assert_eq!(m, n, "{}: {name}: bottom instructions differ", case.name);
        }
    }

    for case in &cases {
        println!(
            "{}: building the chain's layers takes {:.2} ms",
            case.name,
            build_time(case) * 1e3
        );
    }

    for shots in counts {
        let all = ShotSet::range(0..=shots - 1).expect("not empty");
        let times = SAMPLE_SHOTS.div_ceil(shots);
        // Interleaved: each round runs every variant once, so that a drift
        // of the machine hits all alike.
        let mut best = vec![[f64::INFINITY; VARIANTS.len()]; cases.len()];
        for _ in 0..REPEATS {
            for (case, row) in cases.iter().zip(&mut best) {
                for ((variant, _), best) in VARIANTS.iter().zip(row.iter_mut()) {
                    let start = Instant::now();
                    let n: u64 = (0..times).map(|_| run(case, *variant, &all).1).sum();
                    #[expect(clippy::cast_precision_loss, reason = "a count of instructions")]
                    let per = start.elapsed().as_secs_f64() * 1e9 / n as f64;
                    *best = best.min(per);
                }
            }
        }
        println!(
            "{shots} shots, {times} run(s) a sample, best of {REPEATS} interleaved; \
             ns per bottom instruction"
        );
        println!(
            "{:<8} {:>8} {:>8} {:>9} {:>8} {:>11}",
            "example", "stack", "chain", "profiled", "chain/s", "profiled/s"
        );
        for (case, [stack, chain, profiled]) in cases.iter().zip(&best) {
            println!(
                "{:<8} {stack:>8.1} {chain:>8.1} {profiled:>9.1} {:>8.2} {:>11.2}",
                case.name,
                chain / stack,
                profiled / stack
            );
        }
    }
}
