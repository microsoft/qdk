// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development branch only (not for review): the polyene QPE prefix of
//! `qdk_cutensornet`'s `polyene_qpe_prefix` harness (same inputs, same
//! readings, same checks) on the CPU engine, to make a controlled step
//! cheaper by fewer and smaller SVD splits: fusion modes that keep the
//! gates' order, other qubit orders on the chain, and far gates applied as
//! a matrix product operator (MPO) instead of routed by SWAPs.
//!
//! Variants (`--variant`):
//!
//! - `base`: the engine's `SwapBack` and pending-pair fusion, gate by gate
//!   (through `DevBatchRouter` with `Routing::Engine`, `GateFusion::Unfused`,
//!   as `polyene_routing_dev`'s `base`);
//! - `fuse`: `GateFusion::Pairs` (the resumer's), engine routing;
//! - `<fusion>/<apply>`: the gates fused here, in order, then applied.
//!   `<fusion>`: `none` (each gate alone); `same` (consecutive gates on the
//!   same sites); `seq` (the open block takes the next gate while their
//!   sites are at most two; it never moves a gate past another); `seq3`
//!   (the same, up to three sites: a whole controlled Pauli rotation of
//!   weight two). `<apply>`: `swap` (the engine's `apply`, `SwapBack` for
//!   far pairs) or `mpo` (an operator on sites that are not neighbours goes
//!   through `CpuMpsEngine::dev_apply_operator`).
//!
//! `--order` places the qubits on the chain: `identity` (qubit `i` on site
//! `i`), `centre` (the ancilla in the middle), `interleave` (α and β modes
//! alternate, the ancilla first), `interleave-centre` (alternating, the
//! ancilla in the middle), or a comma list, site by site, of qubits. The
//! gates, the Hamiltonian and the reference are relabelled alike, so every
//! reading is the same observable.
//!
//! ```text
//! cargo run --release -p qdk_simulators --example polyene_swaps_dev -- \
//!     --variant base|fuse|seq/swap|seq/mpo|seq3/mpo|... --n 8|12|16 \
//!     --chi 128 [--order identity] [--reference ppp8.exact.reference] \
//!     [--steps 32]
//! ```
//!
//! Every line starts with `POLYENE`; `step` lines add the split counts, the
//! capacity-bound splits and the split time by kind
//! (`qdk_cpu_mps::dev_trace`), cumulative.

#[path = "../../cutensornet/examples/polyene_qpe_prefix/inputs.rs"]
mod inputs;

use inputs::{Gate, Hamiltonian, QpePrefix, Reference, ReferenceKind, Res, number_sums};
use num_complex::Complex64;
use qdk_cpu_mps::{CanonicalMps, CpuMpsEngine};
use qdk_simulators::mps_simulator::{DevBatchRouter, GateFusion, Routing};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fusion {
    None,
    Same,
    Seq(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Apply {
    Swap,
    Mpo,
}

#[derive(Clone, Copy, Debug)]
enum Pipeline {
    Router(GateFusion),
    Local(Fusion, Apply),
}

struct Options {
    variant: String,
    pipeline: Pipeline,
    order: String,
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
        let variant = flag("--variant").ok_or("--variant base|fuse|<fusion>/<apply>")?;
        let pipeline = match variant.as_str() {
            "base" => Pipeline::Router(GateFusion::Unfused),
            "fuse" => Pipeline::Router(GateFusion::Pairs),
            other => {
                let (fusion, apply) = other
                    .split_once('/')
                    .ok_or_else(|| format!("--variant {other}: base, fuse or <fusion>/<apply>"))?;
                let fusion = match fusion {
                    "none" => Fusion::None,
                    "same" => Fusion::Same,
                    "seq" => Fusion::Seq(2),
                    "seq3" => Fusion::Seq(3),
                    _ => return Err(format!("fusion {fusion}: none, same, seq or seq3").into()),
                };
                let apply = match apply {
                    "swap" => Apply::Swap,
                    "mpo" => Apply::Mpo,
                    _ => return Err(format!("apply {apply}: swap or mpo").into()),
                };
                if apply == Apply::Swap && fusion == Fusion::Seq(3) {
                    return Err("seq3 needs mpo: the engine applies two sites at most".into());
                }
                Pipeline::Local(fusion, apply)
            }
        };
        Ok(Self {
            variant,
            pipeline,
            order: flag("--order").unwrap_or_else(|| "identity".to_string()),
            n: number("--n")?.ok_or("--n 8|12|16")?,
            chi: number("--chi")?.ok_or("--chi <bond>")?,
            reference: flag("--reference"),
            steps: number("--steps")?,
        })
    }
}

/// The case a run reads: the prefix, its Hamiltonian, and the reference
/// it is checked against, if any, relabelled to the chain's order.
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
    let mut hamiltonian = Hamiltonian::read(&format!("{INPUTS}/ppp{}.ham", options.n))?;
    let mut reference = match &options.reference {
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
    let position = positions(&options.order, &hamiltonian, prefix.qubits)?;
    relabel(
        &position,
        &mut prefix,
        &mut hamiltonian,
        reference.as_mut().map(|(_, reference)| reference),
    );
    let mut order = vec![0; position.len()];
    for (qubit, &site) in position.iter().enumerate() {
        order[site] = qubit;
    }
    println!(
        "POLYENE case n={} qubits={} chi={} engine=cpu variant={} pipeline={:?} order={} \
         sites={order:?} steps={} gates={} two_qubit={}",
        options.n,
        prefix.qubits,
        options.chi,
        options.variant,
        options.pipeline,
        options.order,
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

/// The site of each qubit for the order `name`.
fn positions(name: &str, hamiltonian: &Hamiltonian, qubits: usize) -> Res<Vec<usize>> {
    let ancilla = hamiltonian.ancilla;
    let interleaved: Vec<usize> = hamiltonian
        .alpha
        .iter()
        .zip(&hamiltonian.beta)
        .flat_map(|(&a, &b)| [a, b])
        .collect();
    let mut system: Vec<usize> = (0..qubits).filter(|&q| q != ancilla).collect();
    let order: Vec<usize> = match name {
        "identity" => (0..qubits).collect(),
        "centre" => {
            system.insert(system.len() / 2, ancilla);
            system
        }
        "interleave" => std::iter::once(ancilla).chain(interleaved).collect(),
        "interleave-centre" => {
            let mut order = interleaved;
            order.insert(order.len() / 2, ancilla);
            order
        }
        list => list
            .split(',')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|error| format!("--order {list}: {error}"))?,
    };
    let mut position = vec![usize::MAX; qubits];
    for (site, &qubit) in order.iter().enumerate() {
        if qubit >= qubits || position[qubit] != usize::MAX {
            return Err(format!("--order {name}: not a permutation of 0..{qubits}").into());
        }
        position[qubit] = site;
    }
    if order.len() != qubits {
        return Err(format!("--order {name}: not a permutation of 0..{qubits}").into());
    }
    Ok(position)
}

/// Moves every qubit `q` of the case to site `position[q]`.
fn relabel(
    position: &[usize],
    prefix: &mut QpePrefix,
    hamiltonian: &mut Hamiltonian,
    reference: Option<&mut Reference>,
) {
    for gate in prefix.trial.iter_mut().chain(&mut prefix.controlled_step) {
        for qubit in &mut gate.qubits {
            *qubit = position[*qubit];
        }
    }
    for mode in hamiltonian.alpha.iter_mut().chain(&mut hamiltonian.beta) {
        *mode = position[*mode];
    }
    hamiltonian.ancilla = position[hamiltonian.ancilla];
    for (_, factors) in &mut hamiltonian.terms {
        for (qubit, _) in factors {
            *qubit = position[*qubit];
        }
    }
    if let Some(reference) = reference {
        for (qubit, _) in &mut reference.probabilities_of_one {
            *qubit = position[*qubit];
        }
        for (_, factors) in &mut reference.pauli_strings {
            for (qubit, _) in factors {
                *qubit = position[*qubit];
            }
        }
    }
}

/// An operator on ascending sites, row-major, the first site the most
/// significant bit.
struct Block {
    sites: Vec<usize>,
    matrix: Vec<Complex64>,
}

impl Block {
    fn identity(sites: Vec<usize>) -> Self {
        let dim = 1 << sites.len();
        let mut matrix = vec![Complex64::ZERO; dim * dim];
        for i in 0..dim {
            matrix[i * dim + i] = Complex64::ONE;
        }
        Self { sites, matrix }
    }

    /// `gate` (on `qubits`, a subset of the block's sites, in any order),
    /// as a matrix on the block's sites.
    fn embed(&self, qubits: &[usize], gate: &[Complex64]) -> Vec<Complex64> {
        let k = self.sites.len();
        let dim = 1 << k;
        let bit = |site: usize| {
            let at = self
                .sites
                .iter()
                .position(|&s| s == site)
                .expect("a site of the block");
            k - 1 - at
        };
        let bits: Vec<usize> = qubits.iter().map(|&q| bit(q)).collect();
        let mask: usize = bits.iter().map(|&b| 1 << b).sum();
        let index = |x: usize| bits.iter().fold(0, |acc, &b| (acc << 1) | ((x >> b) & 1));
        let gdim = 1 << qubits.len();
        let mut out = vec![Complex64::ZERO; dim * dim];
        for row in 0..dim {
            for col in 0..dim {
                if row & !mask == col & !mask {
                    out[row * dim + col] = gate[index(row) * gdim + index(col)];
                }
            }
        }
        out
    }

    /// Applies `gate` after the block; `qubits` must be among its sites.
    fn then(&mut self, qubits: &[usize], gate: &[Complex64]) {
        let g = self.embed(qubits, gate);
        let dim = 1 << self.sites.len();
        let mut product = vec![Complex64::ZERO; dim * dim];
        for row in 0..dim {
            for mid in 0..dim {
                let x = g[row * dim + mid];
                if x == Complex64::ZERO {
                    continue;
                }
                for col in 0..dim {
                    product[row * dim + col] += x * self.matrix[mid * dim + col];
                }
            }
        }
        self.matrix = product;
    }

    /// The block extended to `sites` (a superset, ascending).
    fn widened(&self, sites: Vec<usize>) -> Self {
        let mut wide = Self::identity(sites);
        wide.then(&self.sites.clone(), &self.matrix);
        wide
    }
}

/// `gates` fused in order by `fusion`, each operator on ascending sites.
fn fuse(gates: &[Gate], fusion: Fusion) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut open: Option<Block> = None;
    for gate in gates {
        let mut sites = gate.qubits.clone();
        sites.sort_unstable();
        if let Some(block) = open.as_mut() {
            let mut union = block.sites.clone();
            union.extend(&sites);
            union.sort_unstable();
            union.dedup();
            let joins = match fusion {
                Fusion::None => false,
                Fusion::Same => union == block.sites && sites == block.sites,
                Fusion::Seq(max) => union.len() <= max,
            };
            if joins {
                if union != block.sites {
                    *block = block.widened(union);
                }
                block.then(&gate.qubits, &gate.matrix);
                continue;
            }
            blocks.extend(open.take());
        }
        let mut block = Block::identity(sites);
        block.then(&gate.qubits, &gate.matrix);
        open = Some(block);
    }
    blocks.extend(open);
    blocks
}

/// Applies one batch of `gates` by the run's pipeline; gives the operators
/// applied.
fn apply_batch(
    engine: &CpuMpsEngine,
    state: &mut CanonicalMps,
    router: &mut DevBatchRouter,
    pipeline: Pipeline,
    gates: &[Gate],
) -> Res<usize> {
    match pipeline {
        Pipeline::Router(_) => {
            let operators = router.batch(
                gates
                    .iter()
                    .map(|gate| (gate.qubits.as_slice(), gate.matrix.as_slice())),
            );
            for (sites, matrix) in &operators {
                engine.apply(state, sites, matrix)?;
            }
            router.bond_read(engine.shape(state)?.largest_bond());
            Ok(operators.len())
        }
        Pipeline::Local(fusion, apply) => {
            let blocks = fuse(gates, fusion);
            for block in &blocks {
                let span = block.sites[block.sites.len() - 1] - block.sites[0];
                if apply == Apply::Mpo && block.sites.len() >= 2 && span >= 2 {
                    engine.dev_apply_operator(state, &block.sites, &block.matrix)?;
                } else {
                    engine.apply(state, &block.sites, &block.matrix)?;
                }
            }
            Ok(blocks.len())
        }
    }
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
    let fusion = match options.pipeline {
        Pipeline::Router(fusion) => fusion,
        Pipeline::Local(..) => GateFusion::Unfused,
    };
    let mut router = DevBatchRouter::new(prefix.qubits, options.chi, Routing::Engine, fusion);
    let pipeline = options.pipeline;
    let started = Instant::now();
    let mut state = engine.prepare(&MpsShape::qubit_chain(prefix.qubits, options.chi)?)?;
    let trial_operators = apply_batch(&engine, &mut state, &mut router, pipeline, &prefix.trial)?;
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
    apply_batch(&engine, &mut state, &mut router, pipeline, &hadamard)?;
    let mut step_seconds_sum = 0.0;
    for p in 1..=prefix.steps {
        let step_started = Instant::now();
        let operators = apply_batch(
            &engine,
            &mut state,
            &mut router,
            pipeline,
            &prefix.controlled_step,
        )?;
        let largest = engine.shape(&state)?.largest_bond();
        let weight = engine.retained_weight(&state)?;
        let step_seconds = step_started.elapsed().as_secs_f64();
        step_seconds_sum += step_seconds;
        let statistics = state.split_statistics();
        let (splits, time) = (
            qdk_cpu_mps::dev_trace::get(),
            qdk_cpu_mps::dev_trace::time(),
        );
        println!(
            "POLYENE step p={p} seconds={:.1} step_seconds={step_seconds:.2} \
             one_minus_w={:.4e} largest_bond={largest} operators={operators} splits={} \
             swaps={} rest_bond={} transient_bond={} need={} cap_bound={} \
             cap_lost_sum={:.3e} swap_splits={} swap_s={:.2} swap_work={:.3e} gate_splits={} \
             gate_s={:.2} gate_work={:.3e} mpo_ops={} mpo_svds={} mpo_qrs={} mpo_s={:.2} \
             mpo_work={:.3e}",
            started.elapsed().as_secs_f64(),
            1.0 - weight,
            statistics.splits(),
            statistics.swaps(),
            statistics.largest_bond_at_rest(),
            statistics.largest_transient_bond(),
            splits.largest_need,
            splits.capacity_bound,
            splits.capacity_lost,
            time.swap_splits,
            time.swap_seconds,
            time.swap_work,
            time.gate_splits,
            time.gate_seconds,
            time.gate_work,
            time.mpo_operators,
            time.mpo_svds,
            time.mpo_qrs,
            time.mpo_seconds,
            time.mpo_work,
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
        }
    }
    apply_batch(&engine, &mut state, &mut router, pipeline, &hadamard)?;
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
