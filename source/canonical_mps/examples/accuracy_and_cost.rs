// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine on circuits larger than the tests run: prints how far its
//! state is from the exact one, the time it took and its bonds. It
//! measures, and asserts nothing: the crate's tests check the engine.
//!
//! ```text
//! cargo run --release -p canonical_mps --example accuracy_and_cost -- \
//!     --family brickwork|ising|qft --n 20 [--depth 20] [--seed 1] [--chi …] \
//!     [--order …] [--cutoff 1e-14] [--strings 3n] [--samples 256] \
//!     [--engine-only]
//! cargo run --release -p canonical_mps --example accuracy_and_cost -- \
//!     --gates FILE [--order …] [--chi …] [--cutoff 1e-14] [--strings 3n] \
//!     [--samples 256] [--engine-only]
//! ```
//!
//! - `brickwork`: random `u3` on every qubit, then CNOT or CY on alternating
//!   neighbours; `ising`: a quench of the critical transverse-field Ising
//!   chain; `qft`: a random product state, then the Fourier transform.
//! - `--n` is required with a family: its number of qubits.
//! - `--gates FILE`: a unitary gate list, one gate per line: `cx a b`,
//!   `h q`, `s q`, `sadj q`, `x q`, `rz θ q` or `ry θ q`, such as a
//!   chemistry program's phase estimation.
//!
//! `--chi` defaults to `2^(n/2)`, the bond of any state of `n` qubits, so
//! the engine is exact up to its cutoff (`--cutoff`, by default
//! [`DEFAULT_CUTOFF`]). `--order` puts qubit `q` on site `order[q]`.
//!
//! The exact state is a state vector the example computes from the gates'
//! matrices, which holds 2ⁿ amplitudes (16·2ⁿ bytes). The accuracy
//! printed: the largest gap in a qubit's P(One) and in a random Pauli
//! string's expectation between the engine and the state vector, and the
//! TVD of the whole distribution from samples (`--strings`, `--samples`;
//! 0 skips).
//!
//! `--engine-only` runs the engine alone, with no state vector, and prints
//! its time and the process's peak resident memory (Linux), so that the
//! engine's memory is measured apart, and so that a circuit too wide for a
//! state vector runs.
//!
//! Recorded shots, with their mid-circuit measurements, are the `h2_shots`
//! example's.

#[path = "support/support.rs"]
mod support;

use canonical_mps::{CpuAlgebra, CpuMpsEngine, DEFAULT_CUTOFF};
use support::{
    Res,
    agreement::agreement,
    args::{flag, number, order, qubits},
    circuit::Circuit,
    families::{brickwork, ising, qft},
    recordings::gate_list,
    runs::{peak_memory_kb, run_engine},
    state_vector::StateVector,
};
use tensornet::MpsEngine;

fn main() -> Res<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let number = |name: &str, default: usize| number(&args, name, default);
    let (label, circuit): (String, Circuit) = if let Some(path) = flag(&args, "--gates") {
        (
            path.rsplit('/').next().unwrap_or(&path).into(),
            gate_list(&path)?,
        )
    } else {
        let family = flag(&args, "--family").ok_or("--family or --gates")?;
        let (n, depth) = (qubits(&args)?, number("--depth", 20)?);
        let seed = u64::try_from(number("--seed", 1)?)?;
        match family.as_str() {
            "brickwork" => (
                format!("brickwork n {n} depth {depth} seed {seed}"),
                brickwork(n, depth, seed),
            ),
            "ising" => (format!("ising n {n} depth {depth}"), ising(n, depth)),
            "qft" => (format!("qft n {n} seed {seed}"), qft(n, seed)),
            other => return Err(format!("unknown family {other}").into()),
        }
    };
    let n = circuit.qubits;
    let chi = number("--chi", 1 << (n / 2))?;
    let order = order(&args, n)?;
    let cutoff = flag(&args, "--cutoff").map_or(Ok(DEFAULT_CUTOFF), |x| x.parse())?;
    let engine = CpuMpsEngine::new(CpuAlgebra).with_cutoff(cutoff);
    let run = run_engine(&engine, &circuit, &order, chi)?;
    let statistics = run.state.statistics();
    let (at_rest, transient) = (
        statistics.largest_bond_at_rest(),
        statistics.largest_transient_bond(),
    );
    let lost = 1.0 - engine.retained_weight(&run.state)?;
    if args.iter().any(|a| a == "--engine-only") {
        println!(
            "| case | χ | cutoff | MPS s | largest bond at rest | largest transient bond | \
             1 − W | peak MB |"
        );
        println!("|---|---|---|---|---|---|---|---|");
        println!(
            "| {label} | {chi} | {cutoff:.0e} | {:.3} | {at_rest} | {transient} | {lost:.1e} | \
             {} |",
            run.seconds,
            peak_memory_kb() / 1024
        );
        return Ok(());
    }
    let exact = StateVector::run(&circuit);
    let strings = number("--strings", 3 * n)?;
    let samples = number("--samples", 256)?;
    let agreement = agreement(&engine, &run.state, &order, &exact, strings, samples)?;
    println!(
        "| case | χ | cutoff | MPS s | largest bond at rest | largest transient bond | 1 − W | \
         max ΔP(One) | max Δ⟨P⟩ | sampled TVD |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    println!(
        "| {label} | {chi} | {cutoff:.0e} | {:.3} | {at_rest} | {transient} | {lost:.1e} | \
         {:.1e} | {:.1e} | {:.1e} |",
        run.seconds, agreement.max_probability_gap, agreement.max_pauli_gap, agreement.sampled_tvd
    );
    Ok(())
}
