// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): an MPS state-preparation circuit with
//! the structure of qdk-chemistry's `matrix_product_state` state
//! preparation (a bond register, one isometry per orbital site in chain
//! order, then a layer of CZ gates for the fermionic signs), run on an MPS
//! engine, the CPU engine or `CudaMpsEngine` (`cuda-mps`, on the GPU), and
//! checked against references computed from the target MPS.
//!
//! The case folder (`--case`) holds the circuit, the Hamiltonian, the
//! checks and the target MPS, in the formats `inputs.rs` states. The binary
//! places the program qubits on the engine's chain (`--placement`):
//! `blocked` (program qubit `q` on site `q`: the blocked Jordan-Wigner
//! layout, α modes, β modes, then the bond register, as the chemistry
//! code emits it), `interleaved` (α and β of each orbital on neighbouring
//! sites, `α_o` on `2o` and `β_o` on `2o + 1`, the register after them), or
//! `following` (interleaved, with the register beside the orbital whose
//! isometry is applied: the binary moves it along by adjacent SWAPs between
//! sites, and the run ends in the interleaved placement). It
//! applies every gate through the engine contract, and reads ⟨H⟩, every
//! qubit's P(One) and the checks' Pauli strings, and how far each is from
//! its reference.
//!
//! Ported from the development line's driver of the same name to
//! `canonical_mps::CpuMpsEngine` (`cpu`) and `qdk_cutensornet::CudaMpsEngine`
//! (`cuda-mps`) with their base options only: the development line's
//! fidelity read and `--dump-amplitudes` (through `qdk_cpu_mps::dev_tensors`,
//! not on this stack), its `--target` file and its `CudaMpsEngine` counters
//! and QR-move switch are dropped; `--target` and `--dump-amplitudes` are
//! refused.
//!
//! Every line it prints starts with `MPSPREP`.
//!
//! ```text
//! cargo run --release -p qdk_cutensornet --example mps_prep_check -- \
//!     --engine cpu|cuda-mps --case DIR \
//!     [--placement blocked|interleaved|following] \
//!     [--chi-cap 1024] \
//!     [--stop-after-seconds S]
//! ```
//!
//! `--stop-after-seconds` stops applying gates once that much time has
//! passed (checked every 256 gates), prints what the run reached, and
//! exits with status 3.

mod inputs;

use inputs::{Checks, Circuit, Hamiltonian, Op, Res};
use num_complex::Complex64;
use std::time::Instant;
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

/// How closely the readings must agree with the target's. A state whose
/// splits discarded the weight `1 − W` is at distance about `√(1 − W)`
/// from the exact one, so a reading of an observable `O` moves by at most
/// `2‖O‖√(1 − W)` (`‖O‖` 1 for a Pauli string, at most the sum of the
/// coefficients' magnitudes for H); the floor covers rounding.
const TOLERANCE_FLOOR: f64 = 1e-8;

struct Options {
    engine: String,
    case: String,
    placement: String,
    chi_cap: usize,
    stop_after_seconds: Option<f64>,
}

impl Options {
    fn parse() -> Res<Self> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let flag = |name: &str| {
            args.iter()
                .position(|arg| arg == name)
                .and_then(|at| args.get(at + 1).cloned())
        };
        let placement = flag("--placement").unwrap_or_else(|| "interleaved".to_string());
        if !["blocked", "interleaved", "following"].contains(&placement.as_str()) {
            return Err(
                format!("--placement {placement}: blocked, interleaved or following").into(),
            );
        }
        let case = flag("--case").ok_or("--case DIR")?;
        if let Some(dropped) = args
            .iter()
            .find(|arg| ["--target", "--dump-amplitudes"].contains(&arg.as_str()))
        {
            return Err(format!("{dropped}: not on this stack (base options only)").into());
        }
        Ok(Self {
            engine: flag("--engine").ok_or("--engine cpu|cuda-mps")?,
            case,
            placement,
            chi_cap: flag("--chi-cap").map_or(Ok(1024), |value| value.parse())?,
            stop_after_seconds: flag("--stop-after-seconds")
                .map(|value| value.parse())
                .transpose()?,
        })
    }
}

/// The case, its qubits placed on the engine's chain.
struct Case {
    circuit: Circuit,
    hamiltonian: PauliSum,
    checks: Checks,
    /// The sum of the magnitudes of H's coefficients, the identity's
    /// excluded: a bound on ‖H − c·1‖.
    hamiltonian_norm_bound: f64,
    /// The engine site of each program qubit at the start.
    start: Vec<usize>,
    /// The engine site of each program qubit at the end, where the
    /// readings are made.
    position: Vec<usize>,
    /// Whether the bond register follows the active orbital.
    following: bool,
}

/// The engine site of each program qubit at the end of the run, for
/// `placement` (`following` ends as `interleaved`).
fn positions(placement: &str, circuit: &Circuit) -> Vec<usize> {
    let n = circuit.state_qubits / 2;
    (0..circuit.qubits)
        .map(|q| match placement {
            "interleaved" | "following" if q < n => 2 * q,
            "interleaved" | "following" if q < 2 * n => 2 * (q - n) + 1,
            _ => q,
        })
        .collect()
}

/// The engine site of each program qubit at the start of the run. With
/// `following`, the register starts right after orbital 0's pair, and the
/// run moves it past each later orbital's pair (`follow`) before that
/// orbital's isometry, so that every isometry acts on neighbouring sites
/// and the run ends in the interleaved placement.
fn start_positions(placement: &str, circuit: &Circuit) -> Vec<usize> {
    let n = circuit.state_qubits / 2;
    let b = circuit.qubits - 2 * n;
    if placement != "following" {
        return positions(placement, circuit);
    }
    (0..circuit.qubits)
        .map(|q| match q {
            0 => 0,
            q if q == n => 1,
            q if q < n => 2 * q + b,
            q if q < 2 * n => 2 * (q - n) + 1 + b,
            q => 2 + (q - 2 * n),
        })
        .collect()
}

/// Moves the register, at the sites just before orbital `o`'s pair, past
/// that pair by adjacent SWAPs, updating `at_site`; returns the SWAP count.
fn follow<E: MpsEngine>(
    engine: &E,
    state: &mut E::State,
    at_site: &mut [usize],
    o: usize,
    n: usize,
) -> Res<usize>
where
    E::Error: 'static,
{
    let mut swap = vec![Complex64::new(0.0, 0.0); 16];
    for at in [0, 6, 9, 15] {
        swap[at] = Complex64::new(1.0, 0.0);
    }
    let b = at_site.len() - 2 * n;
    let mut swaps = 0;
    for qubit in [o, n + o] {
        let from = at_site[qubit];
        for site in (from - b..from).rev() {
            engine.apply(state, &[site, site + 1], &swap)?;
            for place in at_site.iter_mut() {
                if *place == site {
                    *place = site + 1;
                } else if *place == site + 1 {
                    *place = site;
                }
            }
            swaps += 1;
        }
    }
    Ok(swaps)
}

fn place(position: &[usize], factors: &[(usize, Pauli)]) -> Vec<(usize, Pauli)> {
    factors
        .iter()
        .map(|&(qubit, pauli)| (position[qubit], pauli))
        .collect()
}

fn pauli_string(factors: Vec<(usize, Pauli)>) -> Res<PauliSum> {
    Ok(PauliSum::new(
        Complex64::new(0.0, 0.0),
        vec![PauliTerm::new(Complex64::new(1.0, 0.0), factors)?],
    )?)
}

fn read_case(options: &Options) -> Res<Case> {
    let dir = &options.case;
    let circuit = Circuit::read(&format!("{dir}/circuit"))?;
    let hamiltonian = Hamiltonian::read(&format!("{dir}/ham"))?;
    let mut checks = Checks::read(&format!("{dir}/checks"))?;
    let position = positions(&options.placement, &circuit);
    let start = start_positions(&options.placement, &circuit);
    let hamiltonian_norm_bound = hamiltonian.terms.iter().map(|(c, _)| c.abs()).sum();
    let terms = hamiltonian
        .terms
        .iter()
        .map(|(coefficient, factors)| {
            PauliTerm::new(Complex64::new(*coefficient, 0.0), place(&position, factors))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let hamiltonian = PauliSum::new(Complex64::new(hamiltonian.constant, 0.0), terms)?;
    for (qubit, _) in &mut checks.probabilities_of_one {
        *qubit = position[*qubit];
    }
    for (_, factors) in &mut checks.pauli_strings {
        *factors = place(&position, factors);
    }
    Ok(Case {
        circuit,
        hamiltonian,
        checks,
        hamiltonian_norm_bound,
        start,
        position,
        following: options.placement == "following",
    })
}

fn main() {
    match run_case() {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            println!("MPSPREP error {error}");
            std::process::exit(1);
        }
    }
}

fn run_case() -> Res<i32> {
    let options = Options::parse()?;
    let case = read_case(&options)?;
    let (gates, two_qubit) = case.circuit.gate_counts();
    println!(
        "MPSPREP case dir={} qubits={} state_qubits={} register={} placement={} engine={} \
         chi_cap={} gates={gates} two_qubit={two_qubit} hamiltonian_terms={} checks={}",
        options.case,
        case.circuit.qubits,
        case.circuit.state_qubits,
        case.circuit.register.len(),
        options.placement,
        options.engine,
        options.chi_cap,
        case.hamiltonian.terms().len(),
        case.checks.probabilities_of_one.len() + case.checks.pauli_strings.len(),
    );
    let verdict = match options.engine.as_str() {
        "cpu" => {
            let engine = canonical_mps::CpuMpsEngine::new(canonical_mps::CpuAlgebra);
            run(
                &engine,
                &case,
                &options,
                &statistics::<canonical_mps::CpuAlgebra>,
            )?
            .0
        }
        "cuda-mps" => run_on_cuda_mps(&case, &options)?,
        other => return Err(format!("--engine {other}: cpu or cuda-mps").into()),
    };
    println!("MPSPREP exit verdict={verdict:?}");
    Ok(match verdict {
        Verdict::Agrees => 0,
        Verdict::Disagrees => 2,
        Verdict::Stopped => 3,
    })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_on_cuda_mps(case: &Case, options: &Options) -> Res<Verdict> {
    let availability = qdk_cutensornet::discover()?;
    println!("MPSPREP availability {:?}", availability.report());
    let engine =
        qdk_cutensornet::CudaMpsEngine::new(qdk_cutensornet::CudaAlgebra::new(&availability)?);
    Ok(run(
        &engine,
        case,
        options,
        &statistics::<qdk_cutensornet::CudaAlgebra>,
    )?
    .0)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn run_on_cuda_mps(_case: &Case, _options: &Options) -> Res<Verdict> {
    Err("cuda-mps runs on Linux x86-64 only".into())
}

/// The engine's split statistics so far, for a mark's line.
fn statistics<A: canonical_mps::algebra::SiteAlgebra>(
    state: &canonical_mps::CanonicalMpsState<A>,
) -> String {
    let statistics = state.split_statistics();
    format!(
        "splits={} swaps={} largest_bond_at_rest={} largest_transient_bond={}",
        statistics.splits(),
        statistics.swaps(),
        statistics.largest_bond_at_rest(),
        statistics.largest_transient_bond(),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// Every reading within the tolerance of its reference.
    Agrees,
    /// A reading beyond it.
    Disagrees,
    /// `--stop-after-seconds` stopped the run before its last gate.
    Stopped,
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

fn bonds(shape: &MpsShape) -> Vec<usize> {
    (0..shape.site_count().saturating_sub(1))
        .filter_map(|cut| shape.bond_dim(cut))
        .collect()
}

/// Applies the circuit, printing a line at each mark (`progress` adds the
/// engine's counters), then reads and checks the end state.
fn run<E: MpsEngine>(
    engine: &E,
    case: &Case,
    options: &Options,
    progress: &dyn Fn(&E::State) -> String,
) -> Res<(Verdict, E::State)> {
    let circuit = &case.circuit;
    let started = Instant::now();
    let mut state = engine.prepare(&MpsShape::qubit_chain(circuit.qubits, options.chi_cap)?)?;
    let mut applied = 0_usize;
    let mut placement_swaps = 0_usize;
    let mut at_site = case.start.clone();
    let n = circuit.state_qubits / 2;
    let mut sites = Vec::new();
    let mut last_mark = (String::from("start"), 0.0_f64);
    let report = |state: &E::State, label: &str, applied: usize| -> Res<()> {
        let weight = engine.retained_weight(state)?;
        let bonds = bonds(&engine.shape(state)?);
        println!(
            "MPSPREP mark {label} gates_applied={applied} seconds={:.3} one_minus_w={:.3e} \
             largest_bond={} peak_rss_mb={:.0} {}",
            started.elapsed().as_secs_f64(),
            1.0 - weight,
            bonds.iter().max().unwrap_or(&1),
            peak_rss_mb(),
            progress(state),
        );
        Ok(())
    };
    for op in &circuit.ops {
        match op {
            Op::Mark(label) => {
                if case.following
                    && let Some(o) = label
                        .strip_prefix("site ")
                        .and_then(|o| o.parse::<usize>().ok())
                    && o > 0
                {
                    placement_swaps += follow(engine, &mut state, &mut at_site, o, n)?;
                }
                let seconds = started.elapsed().as_secs_f64();
                report(&state, label, applied)?;
                println!(
                    "MPSPREP span from={} to={label} seconds={:.3}",
                    last_mark.0,
                    seconds - last_mark.1
                );
                last_mark = (label.clone(), seconds);
            }
            Op::Gate { qubits, matrix, .. } => {
                sites.clear();
                sites.extend(qubits.iter().map(|&q| at_site[q]));
                engine.apply(&mut state, &sites, matrix)?;
                applied += 1;
                if applied.is_multiple_of(256)
                    && let Some(limit) = options.stop_after_seconds
                    && started.elapsed().as_secs_f64() > limit
                {
                    report(&state, "stopped", applied)?;
                    println!(
                        "MPSPREP stopped after {applied} of {} gates, at {:.1} s",
                        circuit.gate_counts().0,
                        started.elapsed().as_secs_f64()
                    );
                    return Ok((Verdict::Stopped, state));
                }
            }
        }
    }
    if at_site != case.position {
        return Err("the run did not end in the placement of the readings".into());
    }
    let gate_seconds = started.elapsed().as_secs_f64();
    let one_minus_w = 1.0 - engine.retained_weight(&state)?;
    let bonds = bonds(&engine.shape(&state)?);
    println!(
        "MPSPREP prepared seconds={gate_seconds:.3} one_minus_w={one_minus_w:.3e} \
         largest_bond={} bonds={bonds:?} placement_swaps={placement_swaps} peak_rss_mb={:.0}",
        bonds.iter().max().unwrap_or(&1),
        peak_rss_mb(),
    );
    let verdict = check(engine, &state, case, one_minus_w)?;
    println!(
        "MPSPREP done seconds={:.3} gate_seconds={gate_seconds:.3} peak_rss_mb={:.0}",
        started.elapsed().as_secs_f64(),
        peak_rss_mb(),
    );
    Ok((verdict, state))
}

/// Reads ⟨H⟩, every qubit's P(One) and the checks' Pauli strings, and
/// prints the largest gap of each kind from the references.
fn check<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    case: &Case,
    one_minus_w: f64,
) -> Res<Verdict> {
    let started = Instant::now();
    let checks = &case.checks;
    let energy = engine.expectation(state, &case.hamiltonian)?.re;
    let energy_gap = checks.energy.map_or(0.0, |e| (energy - e).abs());
    let register: Vec<usize> = case
        .circuit
        .register
        .iter()
        .map(|&q| case.position[q])
        .collect();
    let mut register_one = 0.0_f64;
    let mut probability_gap = (0.0_f64, 0);
    for &(qubit, expected) in &checks.probabilities_of_one {
        let value = engine.probability_of_one(state, qubit)?;
        if register.contains(&qubit) {
            register_one = register_one.max(value);
        }
        let gap = (value - expected).abs();
        if gap > probability_gap.0 {
            probability_gap = (gap, qubit);
        }
    }
    let mut pauli_gap = (0.0_f64, 0);
    for (at, (expected, factors)) in checks.pauli_strings.iter().enumerate() {
        let value = engine
            .expectation(state, &pauli_string(factors.clone())?)?
            .re;
        let gap = (value - expected).abs();
        if gap > pauli_gap.0 {
            pauli_gap = (gap, at);
        }
    }
    let distance = 2.0 * one_minus_w.max(0.0).sqrt();
    let tolerance = TOLERANCE_FLOOR + distance;
    let energy_tolerance = TOLERANCE_FLOOR + distance * case.hamiltonian_norm_bound;
    let largest = probability_gap.0.max(pauli_gap.0);
    let verdict = if largest <= tolerance && energy_gap <= energy_tolerance {
        Verdict::Agrees
    } else {
        Verdict::Disagrees
    };
    println!(
        "MPSPREP check energy={energy:.12} reference={:.12} energy_gap={energy_gap:.3e} \
         probability_of_one_gap={:.3e} (site {}) register_one_max={register_one:.3e} \
         pauli_gap={:.3e} (string {} of {}) largest={largest:.3e} tolerance={tolerance:.3e} \
         energy_tolerance={energy_tolerance:.3e} seconds={:.1} verdict={verdict:?}",
        checks.energy.unwrap_or(f64::NAN),
        probability_gap.0,
        probability_gap.1,
        pauli_gap.0,
        pauli_gap.1,
        checks.pauli_strings.len(),
        started.elapsed().as_secs_f64(),
    );
    Ok(verdict)
}
