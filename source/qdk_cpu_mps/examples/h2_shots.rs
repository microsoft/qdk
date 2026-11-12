// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! H₂ shots replayed on the engine, against the exact readout distribution
//! of each shot.
//!
//! A shot of a chemistry program (H₂'s phase estimation, 29 qubits after
//! the compiler's decomposition into Clifford+T) is recorded as the
//! operations a sparse state received: about 390,000 gates and 1,400
//! mid-circuit measurements, each with the outcome the shot drew. The
//! example replays each shot on the engine, projecting each measurement on
//! the recorded outcome with the engine's own probability, then reads the
//! five readout qubits' distribution through Z-string expectations and
//! prints its total variation distance (TVD) to the exact one, beside the
//! seconds the replay took.
//!
//! It also measures what the unitarity check of `apply` costs on this run:
//! it times every `apply` by kind (one site, two neighbouring sites, two
//! distant sites), then times the contract's check alone on the same gates,
//! and prints check / (apply − check) per kind, the check's cost against
//! the rest of the call.
//!
//! ```text
//! cargo run --release -p qdk_cpu_mps --example h2_shots -- \
//!     [--chi 64] [--order 26,25,…] SHOT.replay…
//! ```
//!
//! Each `SHOT.replay` has its exact distribution in `SHOT.readout`. The
//! replay format is that of [`support::replay`], the readout's that of
//! [`support::readout_file`]. `--order` puts qubit `q` on site `order[q]`.

#[path = "support/circuits.rs"]
mod support;

use qdk_cpu_mps::{CpuMpsEngine, DEFAULT_CUTOFF};
use std::{hint::black_box, time::Instant};
use support::{Op, Res, chain, flag, order, readout_distribution, readout_file, replay};
use tensornet::{MpsEngine, checks};

/// The kinds of gate whose check is timed apart: the check's work grows
/// with the matrix (`8^k` products for `k` sites), the gate's with routing.
#[derive(Clone, Copy)]
enum Kind {
    OneSite,
    Neighbours,
    Distant,
}

const KINDS: [(Kind, &str); 3] = [
    (Kind::OneSite, "one site"),
    (Kind::Neighbours, "two neighbouring sites"),
    (Kind::Distant, "two distant sites"),
];

fn kind(sites: &[usize]) -> Kind {
    match *sites {
        [_] => Kind::OneSite,
        [a, b] if a.abs_diff(b) == 1 => Kind::Neighbours,
        _ => Kind::Distant,
    }
}

/// Per kind: the `apply` calls of every shot and the seconds they took,
/// and the gates of the first shot, on which the check is timed alone.
#[derive(Default)]
struct ApplyTimes {
    calls: [usize; 3],
    seconds: [f64; 3],
    first_shot: [Vec<(Vec<usize>, Vec<support::C>)>; 3],
}

#[expect(clippy::cast_precision_loss, reason = "gate counts")]
fn main() -> Res<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let chi: usize = flag(&args, "--chi").map_or(Ok(64), |x| x.parse())?;
    let shots: Vec<&String> = args.iter().filter(|a| a.ends_with(".replay")).collect();
    let engine = CpuMpsEngine::default();
    println!("χ {chi}, cutoff {DEFAULT_CUTOFF:e}");
    println!(
        "| shot | gates | two-site | projections | seconds | TVD | \
         max projection gap | 1 − W | largest bond at rest | largest transient bond |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    let mut all = ApplyTimes::default();
    let mut site_count = 0;
    for (shot, path) in shots.iter().enumerate() {
        let circuit = replay(path)?;
        let order = order(&args, circuit.qubits)?;
        site_count = circuit.qubits;
        let reference = readout_file(&path.replace(".replay", ".readout"))?;
        let mut state = engine.prepare(&chain(circuit.qubits, chi))?;
        let mut max_projection_gap: f64 = 0.0;
        let started = Instant::now();
        for op in &circuit.ops {
            match op {
                Op::Gate { qubits, matrix, .. } => {
                    let sites: Vec<usize> = qubits.iter().map(|&q| order[q]).collect();
                    let at = kind(&sites) as usize;
                    let call = Instant::now();
                    engine.apply(&mut state, &sites, matrix)?;
                    all.seconds[at] += call.elapsed().as_secs_f64();
                    all.calls[at] += 1;
                    if shot == 0 {
                        all.first_shot[at].push((sites, matrix.clone()));
                    }
                }
                Op::Project {
                    qubit,
                    one,
                    probability,
                } => {
                    let p = engine.project(&mut state, order[*qubit], *one)?;
                    max_projection_gap = max_projection_gap.max((p - probability).abs());
                }
            }
        }
        // The engine leaves the last fused pair unsplit until a read; this
        // read splits it, so the time includes that split.
        black_box(engine.retained_weight(&state)?);
        let seconds = started.elapsed().as_secs_f64();
        let sites: Vec<usize> = circuit.readout.iter().map(|&q| order[q]).collect();
        let distribution = readout_distribution(&engine, &state, &sites)?;
        let mut tvd = 0.0;
        for (bits, exact) in &reference {
            let outcome = usize::from_str_radix(bits, 2)?;
            tvd += 0.5 * (distribution[outcome] - exact).abs();
        }
        let statistics = state.split_statistics();
        println!(
            "| {} | {} | {} | {} | {seconds:.1} | {tvd:.2e} | {max_projection_gap:.1e} | \
             {:.1e} | {} | {} |",
            path.rsplit('/').next().unwrap_or(path),
            circuit.ops.len() - circuit.projections(),
            circuit.two_qubit_gates(),
            circuit.projections(),
            1.0 - engine.retained_weight(&state)?,
            statistics.largest_bond_at_rest(),
            statistics.largest_transient_bond(),
        );
    }
    println!();
    println!("Unitarity check of `apply`, per kind of gate (ns per gate):");
    println!("| kind | gates | apply | check | check / (apply − check) |");
    println!("|---|---|---|---|---|");
    for (kind, name) in KINDS {
        let gates = &all.first_shot[kind as usize];
        if gates.is_empty() {
            continue;
        }
        let repeats = 5;
        let started = Instant::now();
        for _ in 0..repeats {
            for (sites, matrix) in gates {
                black_box(checks::apply(
                    site_count,
                    black_box(sites),
                    black_box(matrix),
                ))?;
            }
        }
        let check =
            started.elapsed().as_secs_f64() * 1e9 / (gates.len() as f64 * f64::from(repeats));
        let calls = all.calls[kind as usize];
        let apply = all.seconds[kind as usize] * 1e9 / calls as f64;
        println!(
            "| {name} | {calls} | {apply:.0} | {check:.1} | {:.2e} |",
            check / (apply - check)
        );
    }
    Ok(())
}
