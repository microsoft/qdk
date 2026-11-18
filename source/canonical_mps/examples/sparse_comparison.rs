// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine against the QDK's sparse state simulator, on the same
//! circuits: prints how far they agree, then both times and their ratio.
//! It measures, and asserts nothing: the crate's tests check the engine.
//!
//! ```text
//! cargo run --release -p canonical_mps --example sparse_comparison -- \
//!     --family u3cx|ising|qft --n 20 [--depth 20] [--seed 1] [--chi …] \
//!     [--order …] [--cutoff 1e-14] [--reference sparse|dense] \
//!     [--strings 3n] [--samples 256] [--only mps|sparse]
//! cargo run --release -p canonical_mps --example sparse_comparison -- \
//!     --gates FILE | --replay SHOT.replay [--order …] [--chi …] \
//!     [--cutoff 1e-14] [--reference sparse|dense] [--strings 3n] \
//!     [--samples 256] [--only mps|sparse]
//! ```
//!
//! - `u3cx`: random `u3` on every qubit, then CNOT or CY on alternating
//!   neighbours; `ising`: a quench of the critical transverse-field Ising
//!   chain; `qft`: a random product state, then the Fourier transform.
//! - `--n` is required with a family: its number of qubits.
//! - `--gates FILE`: a unitary gate list, one gate per line: `cx a b`,
//!   `h q`, `s q`, `sadj q`, `x q`, `rz θ q` or `ry θ q`, such as a
//!   chemistry program's phase estimation.
//! - `--replay SHOT.replay`: a shot with its mid-circuit measurements,
//!   recorded by a simulator run outside this crate; its exact readout
//!   distribution is read from `SHOT.readout`. The `h2_shots` example's
//!   header gives both files' formats.
//!
//! `--chi` defaults to `2^(n/2)`, the bond of any state of `n` qubits, so
//! the engine is exact up to its cutoff (`--cutoff`, by default
//! [`DEFAULT_CUTOFF`]).
//!
//! `--reference dense` compares a unitary circuit's engine state with a
//! state vector the example computes from the gates' matrices, instead of
//! the sparse simulator's state, and also prints the sparse simulator's
//! own gap to that vector: the sparse simulator drops an amplitude whose
//! real and imaginary parts are both at most 1e-10, so its state is not
//! exact either.
//!
//! The agreement printed: for a unitary circuit, the largest gap in a
//! qubit's P(One) and in a random Pauli string's expectation between the
//! engine and the simulator's state vector, and the TVD of the whole
//! distribution from samples (`--strings`, `--samples`; 0 skips); for a
//! shot, each one's TVD on the readout to the exact distribution, and the
//! largest gap of a projection's probability to the recorded one.
//!
//! `--only mps` or `--only sparse` runs one side alone and prints its time
//! and the process's peak resident memory (Linux), so each side's memory
//! is measured apart.

#[path = "support/support.rs"]
mod support;

use canonical_mps::{CpuAlgebra, CpuMpsEngine, DEFAULT_CUTOFF};
use qdk_simulators::SparseStateSim;
use support::{
    Res,
    agreement::agreement,
    args::{flag, number, order, qubits},
    circuit::Circuit,
    dense::Dense,
    families::{brickwork, ising, qft},
    readout::{readout_distribution, tvd_to},
    recordings::{gate_list, readout_file, replay},
    runs::{peak_memory_kb, run_engine, run_sparse},
};
use tensornet::MpsEngine;

/// The distribution of `qubits` read together, outcome `x` with
/// `qubits[0]` as its most significant bit.
fn sparse_readout(sim: &mut SparseStateSim, qubits: &[usize]) -> Vec<f64> {
    let (entries, _) = sim.get_state();
    let k = qubits.len();
    let mut distribution = vec![0.0; 1 << k];
    let mut total = 0.0;
    for (index, amplitude) in entries {
        let outcome = qubits.iter().fold(0, |x, &q| {
            x << 1 | usize::from(index.bit(u64::try_from(q).expect("a small qubit")))
        });
        distribution[outcome] += amplitude.norm_sqr();
        total += amplitude.norm_sqr();
    }
    distribution.iter().map(|p| p / total).collect()
}

#[expect(clippy::too_many_lines, reason = "one run per mode, in sequence")]
fn main() -> Res<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let number = |name: &str, default: usize| number(&args, name, default);
    let (label, circuit): (String, Circuit) = if let Some(path) = flag(&args, "--gates") {
        (
            path.rsplit('/').next().unwrap_or(&path).into(),
            gate_list(&path)?,
        )
    } else if let Some(path) = flag(&args, "--replay") {
        (
            path.rsplit('/').next().unwrap_or(&path).into(),
            replay(&path)?,
        )
    } else {
        let family = flag(&args, "--family").ok_or("--family, --gates or --replay")?;
        let (n, depth) = (qubits(&args)?, number("--depth", 20)?);
        let seed = u64::try_from(number("--seed", 1)?)?;
        match family.as_str() {
            "u3cx" => (
                format!("u3cx n {n} depth {depth} seed {seed}"),
                brickwork(n, depth, seed),
            ),
            "ising" => (format!("ising n {n} depth {depth}"), ising(n, depth)),
            "qft" => (format!("qft n {n} seed {seed}"), qft(n, seed)),
            other => return Err(format!("unknown family {other}").into()),
        }
    };
    if !circuit.is_native() {
        return Err("the circuit has a gate the sparse simulator does not run".into());
    }
    let n = circuit.qubits;
    let chi = number("--chi", 1 << (n / 2))?;
    let order = order(&args, n)?;
    let cutoff = flag(&args, "--cutoff").map_or(Ok(DEFAULT_CUTOFF), |x| x.parse())?;
    let engine = CpuMpsEngine::new(CpuAlgebra).with_cutoff(cutoff);
    let dense_reference = match flag(&args, "--reference").as_deref() {
        None | Some("sparse") => false,
        Some("dense") => true,
        Some(other) => return Err(format!("--reference sparse or dense, not {other}").into()),
    };
    let only = flag(&args, "--only");
    match only.as_deref() {
        Some("mps") => {
            let run = run_engine(&engine, &circuit, &order, chi)?;
            println!(
                "| {label} | mps | χ {chi} | {:.3} s | largest bond at rest {} | \
                 largest transient bond {} | peak {} MB |",
                run.seconds,
                run.state.statistics().largest_bond_at_rest(),
                run.state.statistics().largest_transient_bond(),
                peak_memory_kb() / 1024
            );
            return Ok(());
        }
        Some("sparse") => {
            let run = run_sparse(&circuit)?;
            println!(
                "| {label} | sparse | {:.3} s | peak {} MB |",
                run.seconds,
                peak_memory_kb() / 1024
            );
            return Ok(());
        }
        Some(other) => return Err(format!("--only mps or sparse, not {other}").into()),
        None => {}
    }
    let mut sparse = run_sparse(&circuit)?;
    let mps = run_engine(&engine, &circuit, &order, chi)?;
    let ratio = sparse.seconds / mps.seconds;
    let statistics = mps.state.statistics();
    let (at_rest, transient) = (
        statistics.largest_bond_at_rest(),
        statistics.largest_transient_bond(),
    );
    let lost = 1.0 - engine.retained_weight(&mps.state)?;
    if circuit.projections() == 0 {
        let exact = if dense_reference {
            let dense = Dense::run(&circuit);
            let of_sparse = Dense::of_sparse(&mut sparse.sim);
            let gap = (0..n)
                .map(|q| (of_sparse.probability_of_one(q) - dense.probability_of_one(q)).abs())
                .fold(0.0, f64::max);
            println!("sparse simulator against the dense state: max ΔP(One) {gap:.1e}");
            dense
        } else {
            Dense::of_sparse(&mut sparse.sim)
        };
        let strings = number("--strings", 3 * n)?;
        let samples = number("--samples", 256)?;
        let agreement = agreement(&engine, &mps.state, &order, &exact, strings, samples)?;
        println!(
            "| case | χ | cutoff | reference | sparse s | MPS s | sparse / MPS | \
             largest bond at rest | largest transient bond | 1 − W | max ΔP(One) | max Δ⟨P⟩ | \
             sampled TVD |"
        );
        println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
        let reference = if dense_reference { "dense" } else { "sparse" };
        println!(
            "| {label} | {chi} | {cutoff:.0e} | {reference} | {:.3} | {:.3} | {ratio:.2} | \
             {at_rest} | {transient} | {lost:.1e} | {:.1e} | {:.1e} | {:.1e} |",
            sparse.seconds,
            mps.seconds,
            agreement.max_probability_gap,
            agreement.max_pauli_gap,
            agreement.sampled_tvd
        );
    } else {
        let reference = readout_file(
            &flag(&args, "--replay")
                .expect("a replay")
                .replace(".replay", ".readout"),
        )?;
        let sites: Vec<usize> = circuit.readout.iter().map(|&q| order[q]).collect();
        let engine_tvd = tvd_to(
            &readout_distribution(&engine, &mps.state, &sites)?,
            &reference,
        )?;
        let sparse_tvd = tvd_to(
            &sparse_readout(&mut sparse.sim, &circuit.readout),
            &reference,
        )?;
        println!(
            "| case | χ | sparse s | MPS s | sparse / MPS | largest bond at rest | \
             largest transient bond | 1 − W | sparse TVD | MPS TVD | sparse gap | MPS gap |"
        );
        println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
        println!(
            "| {label} | {chi} | {:.1} | {:.1} | {ratio:.2} | {at_rest} | {transient} | \
             {lost:.1e} | {sparse_tvd:.1e} | {engine_tvd:.1e} | {:.1e} | {:.1e} |",
            sparse.seconds, mps.seconds, sparse.max_projection_gap, mps.max_projection_gap
        );
    }
    Ok(())
}
