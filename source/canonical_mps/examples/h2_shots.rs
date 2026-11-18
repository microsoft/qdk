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
//! seconds the replay took. It measures, and asserts nothing: the crate's
//! tests check the engine.
//!
//! ```text
//! cargo run --release -p canonical_mps --example h2_shots -- \
//!     [--chi 64] [--order 26,25,…] SHOT.replay…
//! ```
//!
//! `--order` puts qubit `q` on site `order[q]`.
//!
//! The shots are recorded by a run of the program on a sparse state
//! simulator outside this crate, which writes two files per shot:
//!
//! - `SHOT.replay`, the operations in order, one per line (`#` starts a
//!   comment): `<Gate> <q> [<angle>]` for a one-qubit gate (`X`, `Y`, `Z`,
//!   `H`, `S`, `SAdj`, `T`, `TAdj`, or `Rx`, `Ry`, `Rz` with an angle in
//!   radians); `Cx|Cy|Cz <a> <b>` for a controlled gate, control `a`;
//!   `P <q> <b> <p>` for the projection of `q` on `|b⟩`, the outcome the
//!   shot drew, which had probability `p`; and last, `READOUT <q0> <q1> …`,
//!   the qubits read out, in record order;
//! - `SHOT.readout`, the exact distribution of those qubits before they
//!   are measured: one line per outcome, `<bits in record order>
//!   <bits in reverse order> <probability>` (`#` starts a comment).
//!
//! Qubits are numbered from 0 and start in `|0⟩`; numbers are decimal.

#[path = "support/support.rs"]
mod support;

use canonical_mps::{CpuAlgebra, CpuMpsEngine, DEFAULT_CUTOFF};
use support::{
    Res,
    args::{flag, order},
    readout::{readout_distribution, tvd_to},
    recordings::{readout_file, replay},
    runs::run_engine,
};
use tensornet::MpsEngine;

fn main() -> Res<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let chi: usize = flag(&args, "--chi").map_or(Ok(64), |x| x.parse())?;
    let shots: Vec<&String> = args.iter().filter(|a| a.ends_with(".replay")).collect();
    let engine = CpuMpsEngine::new(CpuAlgebra);
    println!("χ {chi}, cutoff {DEFAULT_CUTOFF:e}");
    println!(
        "| shot | gates | two-site | projections | seconds | TVD | \
         max projection gap | 1 − W | largest bond at rest | largest transient bond |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for path in shots {
        let circuit = replay(path)?;
        let order = order(&args, circuit.qubits)?;
        let recorded = readout_file(&path.replace(".replay", ".readout"))?;
        let run = run_engine(&engine, &circuit, &order, chi)?;
        let sites: Vec<usize> = circuit.readout.iter().map(|&q| order[q]).collect();
        let tvd = tvd_to(
            &readout_distribution(&engine, &run.state, &sites)?,
            &recorded,
        )?;
        let statistics = run.state.statistics();
        println!(
            "| {} | {} | {} | {} | {:.1} | {tvd:.2e} | {:.1e} | {:.1e} | {} | {} |",
            path.rsplit('/').next().unwrap_or(path),
            circuit.ops.len() - circuit.projections(),
            circuit.two_qubit_gates(),
            circuit.projections(),
            run.seconds,
            run.max_projection_gap,
            1.0 - engine.retained_weight(&run.state)?,
            statistics.largest_bond_at_rest(),
            statistics.largest_transient_bond(),
        );
    }
    Ok(())
}
