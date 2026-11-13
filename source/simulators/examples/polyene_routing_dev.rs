// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development branch only (not for review): the polyene QPE prefix of
//! `qdk_cutensornet`'s `polyene_qpe_prefix` harness (same inputs, same
//! readings, same checks) on the CPU engine, with each batch of gates
//! routed and fused by the resumer's own code (`DevBatchRouter`) before it
//! reaches the engine. A batch is the trial state, each H on the ancilla,
//! and each controlled step, so that `rb` restores the layout's order at
//! the end of every controlled step. The layout is the harness's: qubit
//! `i` on site `i`.
//!
//! Variants (`--variant`):
//!
//! - `base`: `Routing::Engine`, `GateFusion::Unfused`: the engine's
//!   `SwapBack` and pending-pair fusion, the harness's calls exactly;
//! - `rb`: `MeetHalfway` restored at each batch end, unfused;
//! - `rb+fuse`: `rb` with `GateFusion::Pairs` (the resumer's default, which
//!   H₂'s `rb` runs used);
//! - `fuse`: `Routing::Engine` with `GateFusion::Pairs`.
//!
//! `--bond-guard P` sets the routing's capacity guard to P % of χ (100,
//! the default, is `MeetHalfway`'s; 1000 turns it off: a dev switch only).
//! `--drift-budget D` restores the layout's order, within a batch too,
//! whenever a route leaves the drift above D thousandths (1000, the
//! default: never; `routing_programs_dev`'s `rb-db250` is 250), so that the
//! drift fallback at one half rarely fires.
//!
//! ```text
//! cargo run --release -p qdk_simulators --example polyene_routing_dev -- \
//!     --variant base|rb|rb+fuse|fuse --n 8|12|16 --chi 128 \
//!     [--reference ppp8.exact.reference] [--steps 32] [--bond-guard 100] \
//!     [--drift-budget 1000]
//! ```
//!
//! Every line starts with `POLYENE`; `step` lines add the split count, the
//! capacity-bound splits (`qdk_cpu_mps::dev_trace`) and the routing's
//! counters (`routing_trace`).

#[path = "../../cutensornet/examples/polyene_qpe_prefix/inputs.rs"]
mod inputs;

use inputs::{Gate, Hamiltonian, QpePrefix, Reference, ReferenceKind, Res, number_sums};
use num_complex::Complex64;
use qdk_cpu_mps::{CanonicalMps, CpuMpsEngine};
use qdk_simulators::mps_simulator::{
    DevBatchRouter, GateFusion, Restore, RouteTuning, Routing, reset_routing_trace, routing_trace,
};
use std::time::Instant;
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

/// The folder of the harness's inputs, in the checkout the binary was
/// built from.
const INPUTS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../cutensornet/examples/polyene_qpe_prefix"
);

/// The harness's tolerances (`polyene_qpe_prefix/main.rs`).
const EXACT_FLOOR: f64 = 1e-6;
const ERRORS_PER_LOST_WEIGHT: f64 = 2.0;
const SAME_TRUNCATION: f64 = 1e-4;
const ERRORS_PER_LOST_WEIGHT_OBSERVED: f64 = 1.4;

struct Options {
    variant: String,
    routing: Routing,
    fusion: GateFusion,
    bond_guard: u16,
    n: usize,
    chi: usize,
    reference: Option<String>,
    steps: Option<usize>,
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
        let variant = flag("--variant").ok_or("--variant base|rb|rb+fuse|fuse")?;
        let bond_guard = u16::try_from(number("--bond-guard")?.unwrap_or(100))?;
        let drift_budget = u16::try_from(number("--drift-budget")?.unwrap_or(1000))?;
        let rb = Routing::Tuned(RouteTuning {
            restore: Restore::BatchEnd,
            bond_guard_percent: bond_guard,
            drift_budget_permille: drift_budget,
            ..RouteTuning::default()
        });
        let (routing, fusion) = match variant.as_str() {
            "base" => (Routing::Engine, GateFusion::Unfused),
            "rb" => (rb, GateFusion::Unfused),
            "rb+fuse" => (rb, GateFusion::Pairs),
            "fuse" => (Routing::Engine, GateFusion::Pairs),
            other => return Err(format!("--variant {other}: base, rb, rb+fuse or fuse").into()),
        };
        Ok(Self {
            variant,
            routing,
            fusion,
            bond_guard,
            n: number("--n")?.ok_or("--n 8|12|16")?,
            chi: number("--chi")?.ok_or("--chi <bond>")?,
            reference: flag("--reference"),
            steps: number("--steps")?,
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
        "POLYENE case n={} qubits={} chi={} engine=cpu variant={} routing={:?} fusion={:?} \
         bond_guard={} steps={} gates={} two_qubit={}",
        options.n,
        prefix.qubits,
        options.chi,
        options.variant,
        options.routing,
        options.fusion,
        options.bond_guard,
        prefix.steps,
        prefix.gate_count(),
        prefix.two_qubit_count(),
    );
    let case = Case {
        prefix,
        hamiltonian,
        reference,
    };
    let verdict = run(&case, &options)?;
    if verdict == Verdict::Disagrees {
        std::process::exit(2);
    }
    Ok(())
}

/// Applies one batch of `gates`, routed and fused by `router`, then reads
/// the state's largest bond for the router's guard, as the resumer does
/// after each batch; gives the largest bond and the operators applied.
fn apply_batch(
    engine: &CpuMpsEngine,
    state: &mut CanonicalMps,
    router: &mut DevBatchRouter,
    gates: &[Gate],
) -> Res<(usize, usize)> {
    let operators = router.batch(
        gates
            .iter()
            .map(|gate| (gate.qubits.as_slice(), gate.matrix.as_slice())),
    );
    for (sites, matrix) in &operators {
        engine.apply(state, sites, matrix)?;
    }
    let largest = engine.shape(state)?.largest_bond();
    router.bond_read(largest);
    Ok((largest, operators.len()))
}

/// The prefix on the CPU engine, with every reading printed, checked
/// against the case's reference.
#[expect(
    clippy::too_many_lines,
    reason = "development only: one run, read top to bottom"
)]
fn run(case: &Case, options: &Options) -> Res<Verdict> {
    let engine = CpuMpsEngine::default();
    let (prefix, hamiltonian) = (&case.prefix, &case.hamiltonian);
    let ancilla = hamiltonian.ancilla;
    let hadamard = [inputs::hadamard(ancilla)];
    qdk_cpu_mps::dev_trace::reset();
    reset_routing_trace();
    let mut router =
        DevBatchRouter::new(prefix.qubits, options.chi, options.routing, options.fusion);
    let started = Instant::now();
    let mut state = engine.prepare(&MpsShape::qubit_chain(prefix.qubits, options.chi)?)?;
    let (_, trial_operators) = apply_batch(&engine, &mut state, &mut router, &prefix.trial)?;
    let trial = Readings::of(&engine, &state, hamiltonian)?;
    println!(
        "POLYENE trial seconds={:.1} operators={trial_operators} energy={:.12} \
         energy_minus_kekule={:+.3e} n_alpha={:.12} n_beta={:.12} splits={}",
        started.elapsed().as_secs_f64(),
        trial.energy,
        trial.energy - hamiltonian.kekule,
        trial.n_alpha,
        trial.n_beta,
        state.split_statistics().splits(),
    );
    apply_batch(&engine, &mut state, &mut router, &hadamard)?;
    let mut last_c = None;
    let mut step_seconds_sum = 0.0;
    for p in 1..=prefix.steps {
        let step_started = Instant::now();
        let (largest, operators) =
            apply_batch(&engine, &mut state, &mut router, &prefix.controlled_step)?;
        let weight = engine.retained_weight(&state)?;
        let step_seconds = step_started.elapsed().as_secs_f64();
        step_seconds_sum += step_seconds;
        let statistics = state.split_statistics();
        let (splits, routing) = (qdk_cpu_mps::dev_trace::get(), routing_trace());
        println!(
            "POLYENE step p={p} seconds={:.1} step_seconds={step_seconds:.2} \
             one_minus_w={:.4e} largest_bond={largest} operators={operators} splits={} \
             swaps={} transient_bond={} cap_bound={} cap_lost_sum={:.3e} routed={} \
             route_swaps={} restore_swaps={} drift_fb={} bond_fb={} first_fb_batch={} \
             max_drift_permille={}",
            started.elapsed().as_secs_f64(),
            1.0 - weight,
            statistics.splits(),
            statistics.swaps(),
            statistics.largest_transient_bond(),
            splits.capacity_bound,
            splits.capacity_lost,
            routing.routed,
            routing.route_swaps,
            routing.restore_swaps,
            routing.drift_fallbacks,
            routing.bond_fallbacks,
            routing
                .first_fallback_batch
                .map_or("none".to_string(), |batch| batch.to_string()),
            routing.max_drift_permille,
        );
        if p.is_power_of_two() || p == prefix.steps {
            let (c, ancilla_z) = hadamard_test(&engine, &state, ancilla)?;
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
    apply_batch(&engine, &mut state, &mut router, &hadamard)?;
    let one_minus_w = 1.0 - engine.retained_weight(&state)?;
    let prefix_seconds = started.elapsed().as_secs_f64();
    let end = Readings::of(&engine, &state, hamiltonian)?;
    let bonds = bonds(&engine.shape(&state)?);
    let statistics = state.split_statistics();
    println!(
        "POLYENE end prefix_seconds={prefix_seconds:.1} step_seconds_sum={step_seconds_sum:.1} \
         energy={:.12} energy_minus_trial={:+.3e} n_alpha={:.12} n_beta={:.12} \
         ancilla_one={:.12} one_minus_w={one_minus_w:.4e} largest_bond={} \
         largest_bond_at_rest={} transient_bond={} splits={} swaps={} bonds={bonds:?}",
        end.energy,
        end.energy - trial.energy,
        end.n_alpha,
        end.n_beta,
        end.ancilla_one,
        bonds.iter().max().unwrap_or(&1),
        statistics.largest_bond_at_rest(),
        statistics.largest_transient_bond(),
        statistics.splits(),
        statistics.swaps(),
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
            &engine,
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

/// The harness's readings: ⟨H⟩ (core energy included), the particle
/// numbers of each spin, and the ancilla's P(One).
struct Readings {
    energy: f64,
    n_alpha: f64,
    n_beta: f64,
    ancilla_one: f64,
}

impl Readings {
    fn of<E: MpsEngine>(engine: &E, state: &E::State, hamiltonian: &Hamiltonian) -> Res<Self> {
        let value = |sum: &PauliSum| -> Res<f64> { Ok(engine.expectation(state, sum)?.re) };
        let (alpha, _) = number_sums(&hamiltonian.alpha)?;
        let (beta, _) = number_sums(&hamiltonian.beta)?;
        Ok(Self {
            energy: value(&hamiltonian.energy_sum()?)? + hamiltonian.core,
            n_alpha: value(&alpha)?,
            n_beta: value(&beta)?,
            ancilla_one: engine.probability_of_one(state, hamiltonian.ancilla)?,
        })
    }
}

/// How a run's end readings compare with its reference (the harness's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    AgreesWithExact,
    SameTruncation,
    WithinTruncationErrors,
    Disagrees,
    NotChecked,
}

/// The harness's check: every qubit's P(One) and the reference's Pauli
/// strings, the largest gap of each kind, judged by the reference's kind.
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
        } if reference_chi != chi => (
            Verdict::NotChecked,
            format!("truncated chi={reference_chi}"),
        ),
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
            (verdict, format!("truncated truncation_errors={errors:.3e}"))
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
