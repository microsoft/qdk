//! Plays a program of any qodec through the protocol's chain, to explore
//! it.
//!
//! The chain is `static_driver() >> layer 0 >> … >> profiler() >> coins`:
//! the qodec's layers as adapters, a profiler counting what reaches the
//! device, and biased coins of the qodec's bottom set as the device (each
//! readout 1 with chance `p`, 0.01 unless given, drawn from the run's
//! seed: at 0 a noiseless device, above it a noisy one). It prints each
//! shot's results with their conditions, how often each shot's results
//! occur, and the bottom instructions the profiler counted.
//!
//! ```text
//! cargo run --release -p qodec_demo --example play -- <qodec> [<program> [shots] [p] [seed]]
//! ```
//!
//! Given a qodec alone, it lists the qodec's top instructions. The program
//! is in the Qodec REPL's syntax: statements separated by `;`, top batches
//! by `|`. For example, from the repository's root:
//!
//! ```text
//! cargo run --release -p qodec_demo --example play -- \
//!     source/qodec_lowerer/examples/steane/steane.qodec.yaml \
//!     "prepare_z 0; idle_ft 0 | measure_z 0" 16
//! ```

use std::collections::BTreeMap;
use std::process::ExitCode;

use executor_protocol::{
    Needs, ShotConditions, ShotId, ShotOutcome, ShotSet, Value, static_driver,
};
use qodec::Qodec;
use qodec_demo::boundary::Boundary;
use qodec_demo::coin::biased_coin_device;
use qodec_demo::profiler::{Profile, profiler};
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::QodecLayer;

/// The shots whose results are printed one by one.
const SHOTS_SHOWN: usize = 8;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: play <qodec> [<program> [shots] [p] [seed]]");
        return ExitCode::FAILURE;
    };
    let qodec = match Qodec::load(path) {
        Ok(qodec) => qodec,
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let Some(program) = args.get(1) else {
        list_top_instructions(&qodec);
        return ExitCode::SUCCESS;
    };
    let shots: ShotId = args
        .get(2)
        .map_or(16, |s| s.parse().expect("shots: a number"));
    let p: f64 = args
        .get(3)
        .map_or(0.01, |s| s.parse().expect("p: a number"));
    if !(0.0..=1.0).contains(&p) {
        eprintln!("p is a chance: between 0 and 1");
        return ExitCode::FAILURE;
    }
    let seed: u64 = args
        .get(4)
        .map_or(0x5EED, |s| s.parse().expect("seed: a number"));

    let batches: Vec<&str> = program.split('|').map(str::trim).collect();
    let program = Program::parse(&qodec, &batches);
    let shots = ShotSet::new((0..shots).collect()).expect("distinct shots");
    let Some((outcomes, profile)) = run(&qodec, &program, &shots, p, seed) else {
        return ExitCode::FAILURE;
    };
    print_results(&outcomes);
    print_profile(&qodec, &profile);
    ExitCode::SUCCESS
}

/// The top instructions of `qodec`, with their operands.
fn list_top_instructions(qodec: &Qodec) {
    let top = Boundary::new(&qodec.layers()[0].instruction_set)
        .expect("the operands express the top set");
    println!(
        "{} layers; the top one's instructions, with their operands:",
        qodec.layers().len()
    );
    for mnemonic in top.set.mnemonics() {
        let operands = top.set.operands(mnemonic).expect("a mnemonic of the set");
        println!("  {mnemonic} {operands:?}");
    }
}

/// The outcomes of `program` on `shots` through the chain of `qodec`'s
/// layers, on coins that read 1 with chance `p`, and the profile of what
/// reached them.
fn run(
    qodec: &Qodec,
    program: &Program,
    shots: &ShotSet,
    p: f64,
    seed: u64,
) -> Option<(Vec<ShotOutcome>, Profile)> {
    let layer = |k| {
        QodecLayer::new(qodec, k)
            .expect("the qodec plans")
            .adapter()
    };
    let bottom = qodec.layers().last().expect("a layer");
    let bottom =
        Boundary::new(&bottom.instruction_set).expect("the operands express the bottom set");
    let coins = || biased_coin_device(bottom.set.clone(), seed, p);
    let program = program.to_static(Needs::default());
    macro_rules! go {
        ($chain:expr) => {{
            let chain = $chain.expect("every boundary matches");
            let (outcomes, (profile, _device)) = chain
                .run(program, shots.clone())
                .expect("the job fits the chain")
                .wait();
            Some((outcomes.expect("the run succeeds"), profile))
        }};
    }
    // The bottom layer is the device's set; each layer above it is an
    // adapter. A chain's type grows with its parts, so each depth is spelt.
    match qodec.layers().len() - 1 {
        1 => go!(static_driver() >> layer(0) >> profiler() >> coins()),
        2 => go!(static_driver() >> layer(0) >> layer(1) >> profiler() >> coins()),
        3 => go!(static_driver() >> layer(0) >> layer(1) >> layer(2) >> profiler() >> coins()),
        layers => {
            eprintln!("{layers} layers above the device: play stacks 1 to 3");
            None
        }
    }
}

/// Each shown shot's results, then how often each shot's results occur.
fn print_results(outcomes: &[ShotOutcome]) {
    let mut by_shot: BTreeMap<ShotId, Vec<&ShotOutcome>> = BTreeMap::new();
    for outcome in outcomes {
        by_shot.entry(outcome.shot).or_default().push(outcome);
    }
    let line = |results: &[&ShotOutcome]| {
        let mut results = results.to_vec();
        results.sort_by_key(|o| o.tag);
        (results.iter())
            .map(|o| format!("{}={}{}", o.tag, value(o.value), conditions(o.conditions)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    println!("results (tag=value, conditions in brackets):");
    for (shot, results) in by_shot.iter().take(SHOTS_SHOWN) {
        println!("  shot {shot}: {}", line(results));
    }
    if by_shot.len() > SHOTS_SHOWN {
        println!("  … {} more shots", by_shot.len() - SHOTS_SHOWN);
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for results in by_shot.values() {
        *counts.entry(line(results)).or_default() += 1;
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    println!("how often ({} shots):", by_shot.len());
    for (results, count) in counts {
        println!("  {count:>6}  {results}");
    }
}

/// The bottom instructions the profiler counted, by mnemonic.
fn print_profile(qodec: &Qodec, profile: &Profile) {
    let bottom = qodec.layers().last().expect("a layer");
    let bottom =
        Boundary::new(&bottom.instruction_set).expect("the operands express the bottom set");
    println!(
        "profile: {} bottom batches (one per shot each), {} instructions",
        profile.batches,
        profile.instructions()
    );
    for (opcode, count) in &profile.counts {
        let mnemonic = bottom.set.mnemonic(*opcode).unwrap_or("?");
        println!("  {count:>8}  {mnemonic}");
    }
}

fn value(value: Value) -> &'static str {
    match value {
        Value::Zero => "0",
        Value::One => "1",
        _ => "missing",
    }
}

fn conditions(conditions: ShotConditions) -> String {
    let names = [
        (ShotConditions::REJECTED, "rejected"),
        (ShotConditions::INCONSISTENT, "inconsistent"),
        (ShotConditions::UNRESOLVED, "unresolved"),
    ];
    let set: Vec<_> = (names.iter())
        .filter(|(flag, _)| conditions.contains(*flag))
        .map(|(_, name)| *name)
        .collect();
    if set.is_empty() {
        String::new()
    } else {
        format!("[{}]", set.join(","))
    }
}
