// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's routing on circuits whose gates act on distant sites: the
//! time, the weight the truncations kept, the largest bonds at rest and
//! during routes, and, for up to 22 qubits, how far the result is from the
//! exact state. It measures, and asserts nothing: the crate's tests check
//! the engine.
//!
//! ```text
//! cargo run --release -p canonical_mps --example routing -- \
//!     --family qft|random|mirror|qpe|u3cx|ising --n 20 [--depth 2] \
//!     [--seed 1] [--chi 64] [--reference]
//! cargo run --release -p canonical_mps --example routing -- \
//!     --gates FILE [--chi 64] [--order …] [--reference]
//! ```
//!
//! - `random`: `--depth` layers of a random unitary on every qubit, then
//!   random two-qubit unitaries on randomly paired qubits;
//! - `mirror`: `random`, then its inverse, so the exact state is `|0…0⟩`;
//!   prints 1 − P(0…0), the weight the run lost or misplaced;
//! - `qpe`: phase estimation of an Ising Trotter step, with `--depth`
//!   phase qubits whose controlled gates reach every system qubit;
//! - `qft`, `u3cx`, `ising`, `--n` and `--gates`: as in the
//!   `sparse_comparison` example.
//!
//! `--reference` builds the exact state vector (at most 22 qubits) and
//! prints the largest gap in a qubit's P(One) and in a random Pauli
//! string's expectation, the TVD of the whole distribution estimated from
//! 256 samples, and P(One) of qubit 0 from both.

#[path = "support/support.rs"]
mod support;

use canonical_mps::{CanonicalMpsState, CpuAlgebra, CpuMpsEngine};
use support::{
    Res,
    agreement::agreement,
    args::{flag, number, order, qubits},
    circuit::Circuit,
    dense::Dense,
    families::{brickwork, ising, mirror, qft, qpe, random},
    recordings::gate_list,
    runs::run_engine,
};
use tensornet::MpsEngine;

/// P(0…0), by a chain of conditional probabilities on a fork.
fn probability_of_zeros(
    engine: &CpuMpsEngine,
    state: &CanonicalMpsState<CpuAlgebra>,
    order: &[usize],
) -> Res<f64> {
    let mut fork = engine.fork(state)?;
    let mut p = 1.0;
    for &site in order {
        let zero = 1.0 - engine.probability_of_one(&fork, site)?;
        p *= zero;
        if zero <= 1e-15 {
            break;
        }
        engine.project(&mut fork, site, false)?;
    }
    Ok(p)
}

fn main() -> Res<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let number = |name: &str, default: usize| number(&args, name, default);
    let family = flag(&args, "--family");
    let (label, circuit): (String, Circuit) = if let Some(path) = flag(&args, "--gates") {
        (
            path.rsplit('/').next().unwrap_or(&path).into(),
            gate_list(&path)?,
        )
    } else {
        let family = family.as_deref().ok_or("--family or --gates")?;
        let (n, depth) = (qubits(&args)?, number("--depth", 2)?);
        let seed = u64::try_from(number("--seed", 1)?)?;
        let (circuit, uses) = match family {
            "qft" => (qft(n, seed), format!("seed {seed}")),
            "random" => (random(n, depth, seed), format!("depth {depth} seed {seed}")),
            "mirror" => (mirror(n, depth, seed), format!("depth {depth} seed {seed}")),
            "qpe" => (qpe(n, depth)?, format!("depth {depth}")),
            "u3cx" => (
                brickwork(n, depth, seed),
                format!("depth {depth} seed {seed}"),
            ),
            "ising" => (ising(n, depth), format!("depth {depth}")),
            other => return Err(format!("unknown family {other}").into()),
        };
        (format!("{family} n {n} {uses}"), circuit)
    };
    let n = circuit.qubits;
    let chi = number("--chi", 64)?;
    let order = order(&args, n)?;
    let engine = CpuMpsEngine::new(CpuAlgebra);
    let run = run_engine(&engine, &circuit, &order, chi)?;
    let statistics = run.state.split_statistics();
    println!(
        "| case | χ | gates | two-site | seconds | 1 − W | largest bond at rest | \
         largest transient bond | splits | swaps |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    println!(
        "| {label} | {chi} | {} | {} | {:.3} | {:.2e} | {} | {} | {} | {} |",
        circuit.ops.len(),
        circuit.two_qubit_gates(),
        run.seconds,
        1.0 - engine.retained_weight(&run.state)?,
        statistics.largest_bond_at_rest(),
        statistics.largest_transient_bond(),
        statistics.splits(),
        statistics.swaps()
    );
    if family.as_deref() == Some("mirror") {
        let zeros = probability_of_zeros(&engine, &run.state, &order)?;
        println!("1 − P(0…0): {:.3e}", 1.0 - zeros);
    }
    if args.iter().any(|a| a == "--reference") {
        if n > 22 {
            return Err("--reference needs at most 22 qubits".into());
        }
        let exact = Dense::run(&circuit);
        let agreement = agreement(&engine, &run.state, &order, &exact, 3 * n, 256)?;
        println!(
            "max ΔP(One) {:.2e}, max Δ⟨P⟩ {:.2e}, sampled TVD {:.2e}, \
             P(One) of qubit 0: {:.5} (exact {:.5})",
            agreement.max_probability_gap,
            agreement.max_pauli_gap,
            agreement.sampled_tvd,
            engine.probability_of_one(&run.state, order[0])?,
            exact.probability_of_one(0)
        );
    }
    Ok(())
}
