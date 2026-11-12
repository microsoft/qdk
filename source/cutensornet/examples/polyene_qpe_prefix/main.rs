// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the polyene QPE prefix on an MPS
//! engine, `CuTensorNetEngine` or the CPU engine, read and checked.
//!
//! Polyene (the PPP model of a carbon chain, `N` sites, `2N + 1` qubits) is
//! a development case for large bonds, and a candidate relevant case,
//! pending the chemistry team. Its program's first round of iterative QPE
//! prepares the trial state |K⟩, applies H to the ancilla, the controlled
//! Trotter step `U` 32 times, and H again, then measures the ancilla. This
//! binary runs that prefix (the inputs are in this folder, `inputs.rs` says
//! their format) and reads, through the engine contract only:
//!
//! - after each controlled step `p`, the retained weight W (`1 − W`, which
//!   tracks the few-body errors) and the largest bond; on `CuTensorNetEngine`
//!   reading W is what makes the engine compute the operators applied;
//! - after `p` = 1, 2, 4, …, 32 steps, the Hadamard test's `c_p` =
//!   ⟨K|U^p|K⟩ = ⟨X⟩ + i⟨Y⟩ of the ancilla (its ⟨Z⟩ is 0 on the exact state);
//! - on the trial state and at the end, ⟨H⟩, the particle number of each
//!   spin and its variance, and the ancilla's P(One), which equals
//!   (1 − Re `c_32`) / 2;
//! - at the end, every qubit's P(One) and the Pauli strings of the reference
//!   file, and how far each reading is from the reference (`check`).
//!
//! Every line it prints starts with `POLYENE`, then what it reports.
//!
//! ```text
//! cargo run --release -p qdk_cutensornet --example polyene_qpe_prefix -- \
//!     --engine cutensornet|cpu --n 8|12|16 --chi 128 \
//!     [--reference ppp8.exact.reference] [--steps 32] \
//!     [--weight-reads-per-step 1]
//! ```
//!
//! `--reference` names a file in this folder. `--steps` runs fewer steps
//! than the program's (a quick test; the reference is then not checked).
//! `--weight-reads-per-step K` reads W `K` times per step, so that
//! `CuTensorNetEngine` computes a step's operators in `K` parts.

mod inputs;

use inputs::{Hamiltonian, QpePrefix, Reference, ReferenceKind, Res, number_sums};
use num_complex::Complex64;
use std::time::Instant;
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

/// The folder of the inputs, in the checkout the binary was built from.
const INPUTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/polyene_qpe_prefix");

/// How closely a run must agree with an exact reference: `1e-6` plus
/// twice the weight it lost. The CPU engine at χ 128 on N = 8 (1 − W
/// 1.4e-9) was off the exact state by at most 2.2e-7 on every reading;
/// across the polyene runs at χ 8–96, the worst few-body reading was off by
/// 0.6–1.4 × (1 − W).
const EXACT_FLOOR: f64 = 1e-6;
const ERRORS_PER_LOST_WEIGHT: f64 = 2.0;

/// How closely two runs at the same χ must agree to have truncated alike:
/// the CPU engine's N = 16 χ 64 run with Jacobi instead of its own SVDs
/// agreed with it within 4.5e-5 on every reading, about half of this.
const SAME_TRUNCATION: f64 = 1e-4;

/// How closely two runs at the same χ that truncated differently must
/// agree: each is off the exact state by up to about 1.4 × its own
/// 1 − W on few-body readings, so their gap by up to the sum.
const ERRORS_PER_LOST_WEIGHT_OBSERVED: f64 = 1.4;

struct Options {
    engine: String,
    n: usize,
    chi: usize,
    reference: Option<String>,
    steps: Option<usize>,
    weight_reads_per_step: usize,
}

impl Options {
    fn parse() -> Res<Self> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let flag = |name: &str| {
            args.iter()
                .position(|arg| arg == name)
                .and_then(|at| args.get(at + 1).cloned())
        };
        let number = |name: &str| -> Res<Option<usize>> {
            flag(name)
                .map(|value| value.parse())
                .transpose()
                .map_err(|error| format!("{name}: {error}").into())
        };
        Ok(Self {
            engine: flag("--engine").ok_or("--engine cutensornet|cpu")?,
            n: number("--n")?.ok_or("--n 8|12|16")?,
            chi: number("--chi")?.ok_or("--chi <bond>")?,
            reference: flag("--reference"),
            steps: number("--steps")?,
            weight_reads_per_step: number("--weight-reads-per-step")?.unwrap_or(1).max(1),
        })
    }
}

/// The case a run reads: the prefix, its Hamiltonian, and the reference
/// it is checked against, if any.
struct Case {
    prefix: QpePrefix,
    hamiltonian: Hamiltonian,
    reference: Option<(String, Reference)>,
}

fn main() {
    if let Err(error) = run_case() {
        println!("POLYENE error {error}");
        std::process::exit(1);
    }
}

fn run_case() -> Res<()> {
    let options = Options::parse()?;
    let mut prefix = QpePrefix::read(&format!("{INPUTS}/ppp{}.circuit", options.n))?;
    let program_steps = prefix.steps;
    if let Some(steps) = options.steps {
        prefix.steps = steps;
    }
    let hamiltonian = Hamiltonian::read(&format!("{INPUTS}/ppp{}.ham", options.n))?;
    let reference = match &options.reference {
        Some(name) if prefix.steps == program_steps => {
            Some((name.clone(), Reference::read(&format!("{INPUTS}/{name}"))?))
        }
        Some(name) => {
            println!(
                "POLYENE note reference {name} not checked: {} steps of {program_steps}",
                prefix.steps
            );
            None
        }
        None => None,
    };
    println!(
        "POLYENE case n={} qubits={} chi={} engine={} steps={} gates={} two_qubit={} \
         weight_reads_per_step={} label=development-case-for-large-bonds,\
         candidate-relevant-case-pending-the-chemistry-team",
        options.n,
        prefix.qubits,
        options.chi,
        options.engine,
        prefix.steps,
        prefix.gate_count(),
        prefix.two_qubit_count(),
        options.weight_reads_per_step,
    );
    let case = Case {
        prefix,
        hamiltonian,
        reference,
    };
    let verdict = match options.engine.as_str() {
        "cpu" => run(&qdk_cpu_mps::CpuMpsEngine::default(), &case, &options)?,
        "cutensornet" => run_on_cutensornet(&case, &options)?,
        other => return Err(format!("--engine {other}: cutensornet or cpu").into()),
    };
    if verdict == Verdict::Disagrees {
        std::process::exit(2);
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_on_cutensornet(case: &Case, options: &Options) -> Res<Verdict> {
    let availability = qdk_cutensornet::discover()?;
    println!("POLYENE availability {:?}", availability.report());
    let engine = qdk_cutensornet::CuTensorNetEngine::new(&availability)?;
    engine.dev_reset_times();
    let verdict = run(&engine, case, options)?;
    let times = engine.dev_times();
    let clock = |name: &str, clock: &qdk_cutensornet::PhaseClock| {
        format!("{name}={:.3}s/{}", clock.seconds, clock.count)
    };
    println!(
        "POLYENE engine_phases {} {} {} {} {} {} allocations={}",
        clock("prepare", &times.state_prepare),
        clock("compute", &times.state_compute),
        clock("capture", &times.capture),
        clock("expectation", &times.expectation),
        clock("apply", &times.apply),
        clock("restart", &times.restart),
        times.allocations,
    );
    Ok(verdict)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn run_on_cutensornet(_case: &Case, _options: &Options) -> Res<Verdict> {
    let error = qdk_cutensornet::discover()
        .err()
        .map(|error| error.to_string());
    Err(format!("cuTensorNet runs on Linux x86-64 only: {error:?}").into())
}

/// The prefix on `engine`, with every reading printed, checked against the
/// case's reference.
fn run<E: MpsEngine>(engine: &E, case: &Case, options: &Options) -> Res<Verdict> {
    let (prefix, hamiltonian) = (&case.prefix, &case.hamiltonian);
    let ancilla = hamiltonian.ancilla;
    let hadamard = inputs::hadamard(ancilla);
    let started = Instant::now();
    let mut state = engine.prepare(&MpsShape::qubit_chain(prefix.qubits, options.chi)?)?;
    for gate in &prefix.trial {
        engine.apply(&mut state, &gate.qubits, &gate.matrix)?;
    }
    let trial = Readings::of(engine, &state, hamiltonian)?;
    println!(
        "POLYENE trial seconds={:.1} energy={:.12} energy_minus_kekule={:+.3e} n_alpha={:.12} \
         n_beta={:.12}",
        started.elapsed().as_secs_f64(),
        trial.energy,
        trial.energy - hamiltonian.kekule,
        trial.n_alpha,
        trial.n_beta,
    );
    engine.apply(&mut state, &hadamard.qubits, &hadamard.matrix)?;
    let part_length = prefix
        .controlled_step
        .len()
        .div_ceil(options.weight_reads_per_step)
        .max(1);
    let mut last_c = None;
    for p in 1..=prefix.steps {
        let mut weight = 1.0;
        for part in prefix.controlled_step.chunks(part_length) {
            for gate in part {
                engine.apply(&mut state, &gate.qubits, &gate.matrix)?;
            }
            weight = engine.retained_weight(&state)?;
        }
        let bonds = bonds(&engine.shape(&state)?);
        println!(
            "POLYENE step p={p} seconds={:.1} one_minus_w={:.4e} largest_bond={}",
            started.elapsed().as_secs_f64(),
            1.0 - weight,
            bonds.iter().max().unwrap_or(&1),
        );
        if p.is_power_of_two() || p == prefix.steps {
            let (c, ancilla_z) = hadamard_test(engine, &state, ancilla)?;
            println!(
                "POLYENE cp p={p} re={:.9} im={:.9} ancilla_z={ancilla_z:+.3e} \
                 one_minus_w={:.4e} seconds={:.1}",
                c.re,
                c.im,
                1.0 - weight,
                started.elapsed().as_secs_f64(),
            );
            last_c = Some(c);
        }
    }
    engine.apply(&mut state, &hadamard.qubits, &hadamard.matrix)?;
    let one_minus_w = 1.0 - engine.retained_weight(&state)?;
    let prefix_seconds = started.elapsed().as_secs_f64();
    let end = Readings::of(engine, &state, hamiltonian)?;
    let bonds = bonds(&engine.shape(&state)?);
    println!(
        "POLYENE end prefix_seconds={prefix_seconds:.1} energy={:.12} energy_minus_trial={:+.3e} \
         n_alpha={:.12} n_beta={:.12} var_alpha={:.3e} var_beta={:.3e} ancilla_one={:.12} \
         one_minus_w={one_minus_w:.4e} largest_bond={} bonds={bonds:?}",
        end.energy,
        end.energy - trial.energy,
        end.n_alpha,
        end.n_beta,
        end.var_alpha,
        end.var_beta,
        end.ancilla_one,
        bonds.iter().max().unwrap_or(&1),
    );
    if let Some(c) = last_c {
        let from_c = (1.0 - c.re) / 2.0;
        println!(
            "POLYENE hadamard_test ancilla_one={:.12} one_minus_re_c_over_2={from_c:.12} \
             gap={:.3e}",
            end.ancilla_one,
            (end.ancilla_one - from_c).abs(),
        );
    }
    let verdict = match &case.reference {
        Some(reference) => check(
            engine,
            &state,
            &end,
            one_minus_w,
            options.chi,
            reference,
            ancilla,
        )?,
        None => Verdict::NotChecked,
    };
    println!(
        "POLYENE done verdict={verdict:?} prefix_seconds={prefix_seconds:.1} seconds={:.1}",
        started.elapsed().as_secs_f64()
    );
    Ok(verdict)
}

/// The ancilla's `c_p` = ⟨X⟩ + i⟨Y⟩, and its ⟨Z⟩.
fn hadamard_test<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    ancilla: usize,
) -> Res<(Complex64, f64)> {
    let read = |pauli| -> Res<f64> {
        Ok(engine
            .expectation(state, &pauli_string(vec![(ancilla, pauli)])?)?
            .re)
    };
    Ok((
        Complex64::new(read(Pauli::X)?, read(Pauli::Y)?),
        read(Pauli::Z)?,
    ))
}

/// One Pauli string, as an observable.
fn pauli_string(factors: Vec<(usize, Pauli)>) -> Res<PauliSum> {
    Ok(PauliSum::new(
        Complex64::new(0.0, 0.0),
        vec![PauliTerm::new(Complex64::new(1.0, 0.0), factors)?],
    )?)
}

/// The bond at each cut of a chain.
fn bonds(shape: &MpsShape) -> Vec<usize> {
    (0..shape.site_count().saturating_sub(1))
        .filter_map(|cut| shape.bond_dim(cut))
        .collect()
}

/// The readings of a state that the polyene quality study made: ⟨H⟩ (core
/// energy included), the particle numbers of each spin and their
/// variances, and the ancilla's P(One).
struct Readings {
    energy: f64,
    n_alpha: f64,
    n_beta: f64,
    var_alpha: f64,
    var_beta: f64,
    ancilla_one: f64,
}

impl Readings {
    fn of<E: MpsEngine>(engine: &E, state: &E::State, hamiltonian: &Hamiltonian) -> Res<Self> {
        let value = |sum: &PauliSum| -> Res<f64> { Ok(engine.expectation(state, sum)?.re) };
        let (alpha, alpha_squared) = number_sums(&hamiltonian.alpha)?;
        let (beta, beta_squared) = number_sums(&hamiltonian.beta)?;
        let (n_alpha, n_beta) = (value(&alpha)?, value(&beta)?);
        Ok(Self {
            energy: value(&hamiltonian.energy_sum()?)? + hamiltonian.core,
            n_alpha,
            n_beta,
            var_alpha: value(&alpha_squared)? - n_alpha * n_alpha,
            var_beta: value(&beta_squared)? - n_beta * n_beta,
            ancilla_one: engine.probability_of_one(state, hamiltonian.ancilla)?,
        })
    }
}

/// How a run's end readings compare with its reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// Within the tolerance of an exact reference.
    AgreesWithExact,
    /// Within [`SAME_TRUNCATION`] of a run at the same χ.
    SameTruncation,
    /// Within the errors both runs' 1 − W allow, beyond
    /// [`SAME_TRUNCATION`]: the runs truncated differently.
    WithinTruncationErrors,
    /// Beyond the tolerance of the reference's kind: a fail (exit code 2).
    Disagrees,
    /// No reference, fewer steps than the program's, or a reference at
    /// another χ.
    NotChecked,
}

/// Reads every qubit's P(One) and the reference's Pauli strings, prints
/// the largest gap from the reference of each kind of reading, and judges
/// the largest of all by the reference's kind.
fn check<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    end: &Readings,
    one_minus_w: f64,
    chi: usize,
    (name, reference): &(String, Reference),
    ancilla: usize,
) -> Res<Verdict> {
    let started = Instant::now();
    let mut probability_gap = (0.0_f64, 0);
    for &(qubit, expected) in &reference.probabilities_of_one {
        let gap = (engine.probability_of_one(state, qubit)? - expected).abs();
        if gap > probability_gap.0 {
            probability_gap = (gap, qubit);
        }
    }
    let mut pauli_gap = (0.0_f64, 0);
    for (at, (expected, factors)) in reference.pauli_strings.iter().enumerate() {
        let observable = pauli_string(factors.clone())?;
        let gap = (engine.expectation(state, &observable)?.re - expected).abs();
        if gap > pauli_gap.0 {
            pauli_gap = (gap, at);
        }
    }
    let ancilla_gap = reference
        .probabilities_of_one
        .iter()
        .find(|(qubit, _)| *qubit == ancilla)
        .map_or(f64::NAN, |(_, expected)| (end.ancilla_one - expected).abs());
    let energy_gap = (end.energy - reference.energy).abs();
    let number_gap = (end.n_alpha - reference.n_alpha)
        .abs()
        .max((end.n_beta - reference.n_beta).abs());
    let largest = [probability_gap.0, pauli_gap.0, energy_gap, number_gap]
        .into_iter()
        .fold(0.0, f64::max);
    let (verdict, rule) = match reference.kind {
        ReferenceKind::Exact => {
            let tolerance = EXACT_FLOOR + ERRORS_PER_LOST_WEIGHT * one_minus_w;
            let verdict = if largest <= tolerance {
                Verdict::AgreesWithExact
            } else {
                Verdict::Disagrees
            };
            (verdict, format!("exact tolerance={tolerance:.3e}"))
        }
        ReferenceKind::Truncated {
            chi: reference_chi, ..
        } if reference_chi != chi => {
            println!("POLYENE note reference {name} is at chi {reference_chi}, not {chi}");
            (
                Verdict::NotChecked,
                format!("truncated chi={reference_chi}"),
            )
        }
        ReferenceKind::Truncated {
            one_minus_w: reference_one_minus_w,
            ..
        } => {
            let errors = ERRORS_PER_LOST_WEIGHT_OBSERVED * (one_minus_w + reference_one_minus_w);
            let verdict = if largest <= SAME_TRUNCATION {
                Verdict::SameTruncation
            } else if largest <= errors {
                Verdict::WithinTruncationErrors
            } else {
                Verdict::Disagrees
            };
            (
                verdict,
                format!(
                    "truncated same_truncation={SAME_TRUNCATION:.1e} truncation_errors={errors:.3e} \
                     reference_one_minus_w={reference_one_minus_w:.4e} one_minus_w={one_minus_w:.4e}"
                ),
            )
        }
    };
    println!(
        "POLYENE check reference={name} {rule} largest={largest:.3e} energy={energy_gap:.3e} \
         numbers={number_gap:.3e} ancilla_one={ancilla_gap:.3e} probability_of_one={:.3e} \
         (qubit {}) pauli={:.3e} (string {} of {}) seconds={:.1} verdict={verdict:?}",
        probability_gap.0,
        probability_gap.1,
        pauli_gap.0,
        pauli_gap.1,
        reference.pauli_strings.len(),
        started.elapsed().as_secs_f64(),
    );
    Ok(verdict)
}
