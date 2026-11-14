// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): runs an MPS state-preparation
//! circuit of the study behind `mps_prep_check` (in `qdk_cutensornet`'s
//! examples, whose `inputs.rs` this reads) on the sparse simulator,
//! `SparseStateSim`, to measure how far it goes: its support (the number of
//! stored amplitudes) after each site and at its largest, its peak memory
//! and its time. At the end it reads ⟨H⟩, every qubit's P(One) and the
//! checks' Pauli strings from the state and compares them with the
//! references, as `mps_prep_check` does.
//!
//! The program qubits are the simulator's qubits, in the circuit's own
//! labels (the blocked Jordan-Wigner layout, then the bond register): the
//! sparse simulator has no chain, so the layout does not matter to it.
//! `cx`, `cz`, `ry` and `rz` go to the simulator's own gates; a dense `u1`
//! or `u2` gate to `SparseStateSim::apply`, which takes any unitary.
//!
//! ```text
//! cargo run --release -p qdk_simulators --no-default-features \
//!     --example mps_prep_sparse_dev -- --case DIR [--stop-after-seconds S]
//! ```
//!
//! Every line it prints starts with `SPARSE`; a `progress` line each minute
//! shows how far a run got if it is killed. Exit status: 0 the readings
//! agree, 2 they do not, 3 stopped at the time limit, 1 an error.

#[allow(dead_code, reason = "the MPS harness's inputs; this reads some")]
#[path = "../../cutensornet/examples/mps_prep_check/inputs.rs"]
mod inputs;

use inputs::{Checks, Circuit, Hamiltonian, Kind, Op, Res};
use ndarray::Array2;
use num_complex::Complex64;
use num_traits::ToPrimitive;
use qdk_simulators::SparseStateSim;
use rustc_hash::FxHashMap;
use std::time::Instant;
use tensornet::Pauli;

/// How closely the readings must agree with the references: the simulator
/// drops amplitudes below 1e-10, and the references are exact to rounding.
const TOLERANCE: f64 = 1e-6;

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("SPARSE error {error}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Res<i32> {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|at| args.get(at + 1).cloned())
    };
    let case = flag("--case").ok_or("--case DIR")?;
    let stop_after: f64 = flag("--stop-after-seconds").map_or(Ok(3600.0), |s| s.parse())?;

    let circuit = Circuit::read(&format!("{case}/circuit"))?;
    let hamiltonian = Hamiltonian::read(&format!("{case}/ham"))?;
    let checks = Checks::read(&format!("{case}/checks"))?;
    let (gates, two_qubit) = circuit.gate_counts();
    println!(
        "SPARSE case dir={case} qubits={} state_qubits={} register={} gates={gates} \
         two_qubit={two_qubit} stop_after_seconds={stop_after}",
        circuit.qubits,
        circuit.state_qubits,
        circuit.register.len()
    );
    check_bit_order()?;

    let mut sim = SparseStateSim::new(None);
    for q in 0..circuit.qubits {
        let id = sim.allocate();
        if id != q {
            return Err(format!("qubit {q} allocated as {id}").into());
        }
    }

    let start = Instant::now();
    let mut last_progress = Instant::now();
    let mut applied = 0usize;
    let mut peak_support = 1usize;
    let mut last_mark = "start".to_string();
    for op in &circuit.ops {
        match op {
            Op::Mark(text) => {
                let support = sim.dev_support_len(true);
                peak_support = peak_support.max(support);
                status("mark", &last_mark, applied, start, support, peak_support);
                last_mark.clone_from(text);
                last_mark = last_mark.replace(' ', "_");
            }
            Op::Gate {
                qubits,
                matrix,
                kind,
            } => {
                apply(&mut sim, qubits, matrix, *kind);
                applied += 1;
                let support = sim.dev_support_len(false);
                peak_support = peak_support.max(support);
                if applied.is_multiple_of(64) && last_progress.elapsed().as_secs_f64() > 60.0 {
                    // A run killed for memory leaves its last progress line.
                    status(
                        "progress",
                        &last_mark,
                        applied,
                        start,
                        support,
                        peak_support,
                    );
                    last_progress = Instant::now();
                }
                if applied.is_multiple_of(64) && start.elapsed().as_secs_f64() > stop_after {
                    let support = sim.dev_support_len(true);
                    status("stopped", &last_mark, applied, start, support, peak_support);
                    println!("SPARSE exit verdict=Stopped");
                    return Ok(3);
                }
            }
        }
    }
    let support = sim.dev_support_len(true);
    peak_support = peak_support.max(support);
    status(
        "prepared",
        &last_mark,
        applied,
        start,
        support,
        peak_support,
    );
    read_and_compare(sim, &circuit, &hamiltonian, &checks, start)
}

/// Reads the prepared state, compares it with the references and returns the
/// exit status.
fn read_and_compare(
    mut sim: SparseStateSim,
    circuit: &Circuit,
    hamiltonian: &Hamiltonian,
    checks: &Checks,
    start: Instant,
) -> Res<i32> {
    let gate_seconds = start.elapsed().as_secs_f64();
    let read_start = Instant::now();
    let (entries, count) = sim.get_state();
    drop(sim);
    let state: FxHashMap<u64, Complex64> = entries
        .into_iter()
        .map(|(index, amplitude)| (index.to_u64().expect("at most 64 qubits"), amplitude))
        .collect();
    if count != circuit.qubits {
        return Err(format!("{count} qubits in the state").into());
    }
    let verdict = compare(&state, circuit, hamiltonian, checks);
    println!(
        "SPARSE done seconds={:.3} gate_seconds={gate_seconds:.3} read_seconds={:.3} \
         peak_rss_mb={:.0}",
        start.elapsed().as_secs_f64(),
        read_start.elapsed().as_secs_f64(),
        peak_rss_mb()
    );
    println!(
        "SPARSE exit verdict={}",
        if verdict { "Agrees" } else { "Disagrees" }
    );
    Ok(if verdict { 0 } else { 2 })
}

/// Prints one `SPARSE <what>` line: where the run is, the support now (the
/// stored amplitudes) and the largest so far.
fn status(what: &str, after: &str, applied: usize, start: Instant, support: usize, peak: usize) {
    println!(
        "SPARSE {what} after={after} gates_applied={applied} seconds={:.3} support={support} \
         peak_support={peak} peak_rss_mb={:.0}",
        start.elapsed().as_secs_f64(),
        peak_rss_mb()
    );
}

fn apply(sim: &mut SparseStateSim, qubits: &[usize], matrix: &[Complex64], kind: Kind) {
    match kind {
        Kind::Cx => sim.mcx(&[qubits[0]], qubits[1]),
        Kind::Cz => sim.mcz(&[qubits[0]], qubits[1]),
        Kind::Ry(theta) => sim.ry(theta, qubits[0]),
        Kind::Rz(theta) => sim.rz(theta, qubits[0]),
        Kind::Dense => {
            let size = 1 << qubits.len();
            let unitary =
                Array2::from_shape_vec((size, size), matrix.to_vec()).expect("a square matrix");
            sim.apply(&unitary, qubits, None);
        }
    }
}

/// Checks that bit `q` of a state's index is qubit `q`, and that `apply`
/// takes its first target as the most significant bit of the matrix's
/// index, as the circuit files do.
fn check_bit_order() -> Res<()> {
    let mut sim = SparseStateSim::new(None);
    for _ in 0..3 {
        let _ = sim.allocate();
    }
    sim.x(1);
    // On |q1 q2⟩ = |1 0⟩ (index 2 with q1 the most significant), a matrix
    // that maps index 2 to index 3 sets q2.
    let mut swap23 = Array2::<Complex64>::eye(4);
    swap23[[2, 2]] = Complex64::new(0.0, 0.0);
    swap23[[3, 3]] = Complex64::new(0.0, 0.0);
    swap23[[2, 3]] = Complex64::new(1.0, 0.0);
    swap23[[3, 2]] = Complex64::new(1.0, 0.0);
    sim.apply(&swap23, &[1, 2], None);
    let (state, _) = sim.get_state();
    let index = state[0].0.to_u64().expect("small");
    if state.len() != 1 || index != 0b110 {
        return Err(format!("bit order: index {index:b}, expected 110").into());
    }
    Ok(())
}

/// ⟨ψ|P|ψ⟩ for a Pauli string, the state as a map from index to amplitude.
fn pauli_expectation(state: &FxHashMap<u64, Complex64>, factors: &[(usize, Pauli)]) -> f64 {
    let mut flip = 0u64;
    for &(q, pauli) in factors {
        if matches!(pauli, Pauli::X | Pauli::Y) {
            flip |= 1 << q;
        }
    }
    let mut total = Complex64::new(0.0, 0.0);
    for (&index, &amplitude) in state {
        let Some(&partner) = state.get(&(index ^ flip)) else {
            continue;
        };
        // P|x⟩ = phase(x)|x ⊕ flip⟩.
        let mut phase = Complex64::new(1.0, 0.0);
        for &(q, pauli) in factors {
            let one = (index >> q) & 1 == 1;
            phase *= match pauli {
                Pauli::Y if one => Complex64::new(0.0, -1.0),
                Pauli::Y => Complex64::new(0.0, 1.0),
                Pauli::Z if one => Complex64::new(-1.0, 0.0),
                Pauli::X | Pauli::Z => Complex64::new(1.0, 0.0),
            };
        }
        total += partner.conj() * phase * amplitude;
    }
    total.re
}

/// Reads the state's ⟨H⟩, P(One) of each qubit and the checks' Pauli
/// strings, prints them against the references, and says whether they
/// agree within `TOLERANCE`.
fn compare(
    state: &FxHashMap<u64, Complex64>,
    circuit: &Circuit,
    hamiltonian: &Hamiltonian,
    checks: &Checks,
) -> bool {
    let norm: f64 = state.values().map(Complex64::norm_sqr).sum();
    let register_mask: u64 = circuit.register.iter().map(|q| 1u64 << q).sum();
    let half = u32::try_from(circuit.state_qubits / 4).expect("at most 64 qubits");
    let alpha_mask: u64 = (0..circuit.state_qubits / 2).map(|q| 1u64 << q).sum();
    let beta_mask: u64 = (circuit.state_qubits / 2..circuit.state_qubits)
        .map(|q| 1u64 << q)
        .sum();
    let mut off_register = 0usize;
    let mut in_sector = 0usize;
    let mut register_weight = 0.0;
    for (&index, amplitude) in state {
        if index & register_mask != 0 {
            off_register += 1;
            register_weight += amplitude.norm_sqr();
        } else if (index & alpha_mask).count_ones() == half
            && (index & beta_mask).count_ones() == half
        {
            in_sector += 1;
        }
    }
    println!(
        "SPARSE support total={} register_zero_in_sector={in_sector} register_nonzero={off_register} \
         register_nonzero_weight={register_weight:.3e} norm={norm:.15}",
        state.len()
    );

    let mut energy = hamiltonian.constant;
    for (coefficient, factors) in &hamiltonian.terms {
        energy += coefficient * pauli_expectation(state, factors) / norm;
    }
    let mut largest = 0.0f64;
    let mut energy_gap = f64::NAN;
    if let Some(reference) = checks.energy {
        energy_gap = (energy - reference).abs();
    }
    for &(q, reference) in &checks.probabilities_of_one {
        let p: f64 = state
            .iter()
            .filter(|(index, _)| *index >> q & 1 == 1)
            .map(|(_, a)| a.norm_sqr())
            .sum::<f64>()
            / norm;
        largest = largest.max((p - reference).abs());
    }
    for (reference, factors) in &checks.pauli_strings {
        largest = largest.max((pauli_expectation(state, factors) / norm - reference).abs());
    }
    let agrees = energy_gap <= TOLERANCE && largest <= TOLERANCE;
    println!(
        "SPARSE check energy={energy:.12} reference={:.12} energy_gap={energy_gap:.3e} \
         largest={largest:.3e} tolerance={TOLERANCE:.0e}",
        checks.energy.unwrap_or(f64::NAN)
    );
    agrees
}

/// The process's peak resident memory, in MB (Linux; 0 elsewhere).
fn peak_rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmHWM:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|kb| kb.parse::<f64>().ok())
        })
        .map_or(0.0, |kb| kb / 1024.0)
}
