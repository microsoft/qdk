// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): a diagnostic of
//! `CuTensorNetEngine::with_dev_wide_operators`, whose three-site operators
//! go to cuTensorNet as one MPO (`cutensornetNetworkOperatorAppendMPO`, then
//! `cutensornetStateApplyNetworkOperator`). On the A100 that engine read the
//! polyene N=8 within its truncation but failed H₂ (TVD 0.34); this
//! diagnostic tells whether the MPO's result is cuTensorNet's fault or our
//! tensors'.
//!
//! Each case prepares an entangled random state with one- and two-site
//! operators (the tensor-operator path the kit checks), applies one random
//! non-symmetric three-site unitary `U` on the case's sites, mixes every
//! amplitude's phase into the probabilities by a readout layer of one- and
//! two-site operators, and reads all 2ⁿ probabilities by chained
//! conditional probabilities. The same steps run on a reference engine
//! (the CPU engine) with `U` and with its transpose `Uᵀ`. Our MPO tensors
//! put the operator's input on the mode cuTensorNet's docs say acts on the
//! ket, as a tensor operator's first half is contracted with the input
//! state; a match with `Uᵀ` alone would say the library reads the MPO's
//! ket and bra the other way round.
//!
//! The cases:
//! - `adjacent`: sites 1, 2, 3, ascending;
//! - `far`: sites 0, 2, 5, ascending, not adjacent;
//! - `unordered`: sites 5, 0, 2 (the engine sorts them and permutes `U`);
//! - `two-site-padded`: a random two-site unitary on sites 0 and 5 with the
//!   identity on site 2, as one three-site MPO (a two-site operator never
//!   reaches the MPO path, which takes three sites only);
//! - `sequence`: twelve operators on random site triples, each followed by
//!   a read, with a projection halfway: more computes than the engine's
//!   restart period, so the MPOs cross restarts and a projection.
//!
//! On the A100 the first four agreed and `sequence` differed, so the cases
//! below separate what `sequence` mixes. Each starts and ends with its own
//! preparation and readout layers; a read computes the state if an operator
//! was applied since the last compute, and the readout ends with one.
//! - `two-mpos-no-read`: two MPOs, one compute;
//! - `two-mpos-read-between`: an MPO, a read, another MPO: two computes, no
//!   restart, no projection;
//! - `mpo-then-narrow-after-read`: an MPO, a read, a two-site operator:
//!   whether the MPO comes back at the compute after its own;
//! - `mpo-after-read`: a read, then an MPO, on a captured state (a
//!   control: no MPO before the first compute);
//! - `mpo-after-restart`: `RESTART_PERIOD` two-site operators each followed
//!   by a read, then an MPO, the first operator of the restarted state;
//! - `mpo-then-restart`: an MPO, a read, then `RESTART_PERIOD` two-site
//!   operators each followed by a read, across a restart;
//! - `mpo-across-projection`: an MPO, a read, a projection of the read
//!   site, another MPO;
//! - `projection-then-mpo`: a read, a projection, an MPO, with no MPO
//!   before the projection.
//!
//! Hypothesis (a), the reapplied column: `cutensornetStateCaptureMPS` is
//! documented to delete "all tensor operators and tensor network operators
//! that have been applied to the state"; if it kept the network operators,
//! each later compute would start from the captured MPS, then apply the
//! kept MPOs again in the order they were applied, then the operators
//! applied since the last compute (one list in application order, the
//! tensor operators taken out). A restart's new state has none of them.
//! The reads after a compute see its output only, as the single-MPO cases'
//! agreement shows. The reference run under that hypothesis applies the
//! kept MPOs on the CPU at the first operator after each compute, after the
//! engine's restart if one is due; a projection's probability is read
//! before them, as the engine reads it on the computed state.
//!
//! Expected reading:
//! - `far` or `unordered` failing while `adjacent` passes points at the
//!   library's MPO on non-adjacent sites; every case failing, or every case
//!   matching `Uᵀ`, points at our tensors;
//! - a case whose engine run matches the reapplied column (its difference
//!   within 1e-9) while differing from the reference is hypothesis (a), a
//!   cuTensorNet bug: `two-mpos-read-between` and
//!   `mpo-then-narrow-after-read` then reproduce it with one restart-free,
//!   projection-free program;
//! - `two-mpos-no-read` failing points at several MPOs in one compute;
//!   `mpo-after-read` failing, at an MPO on a captured state;
//!   `mpo-after-restart` failing alone, at the restart (our code or the
//!   library's start from our MPS); `projection-then-mpo` failing alone,
//!   at an MPO after a non-unitary operator;
//! - a case failing that matches neither column is unexplained.

use crate::engine::policy::RESTART_PERIOD;
use num_complex::Complex64;
use tensornet::{MpsEngine, MpsShape};

/// The sites of every case's chain.
pub const SITES: usize = 6;

/// One case's result.
#[derive(Clone, Debug)]
pub struct MpoCaseResult {
    /// The case's name.
    pub case: &'static str,
    /// The sites of its (first) three-site operator, as given to `apply`.
    pub sites: Vec<usize>,
    /// The largest difference between the engine's and the reference's
    /// probabilities (every basis state's, and the reads in between).
    pub max_difference: f64,
    /// The same against the reference run with every three-site operator
    /// transposed.
    pub max_difference_transposed: f64,
    /// The same against the reference run under hypothesis (a) (the module
    /// doc): every MPO applied again at each later compute until the
    /// engine's next restart. `None` when that hypothesis applies no MPO
    /// again, so that it predicts the reference itself.
    pub max_difference_reapplied: Option<f64>,
    /// `max|U - Uᵀ|` of its (first) three-site operator: far from zero, so
    /// the transposed run differs.
    pub asymmetry: f64,
}

/// Runs every case on `engine` and on `reference`.
///
/// # Errors
///
/// The case and the error of the first engine call that fails.
pub fn run_cases<E: MpsEngine, R: MpsEngine>(
    engine: &E,
    reference: &R,
) -> Result<Vec<MpoCaseResult>, String> {
    let mut rng = SplitMix(0x6d70_6f5f_6469_6167);
    let mut results = Vec::new();
    let single = |sites: [usize; 3], rng: &mut SplitMix| {
        let unitary = random_unitary(8, rng);
        vec![Step::Wide(sites.to_vec(), unitary)]
    };
    let mut cases: Vec<(&'static str, Vec<Step>)> = vec![
        ("adjacent", single([1, 2, 3], &mut rng)),
        ("far", single([0, 2, 5], &mut rng)),
        ("unordered", single([5, 0, 2], &mut rng)),
    ];
    let pair = random_unitary(4, &mut rng);
    cases.push((
        "two-site-padded",
        vec![Step::Wide(vec![0, 2, 5], padded_middle(&pair))],
    ));
    let mut sequence = Vec::new();
    for at in 0..12 {
        let sites = distinct_triple(&mut rng);
        sequence.push(Step::Wide(sites.to_vec(), random_unitary(8, &mut rng)));
        sequence.push(Step::Read(sites[at % 3]));
        if at == 5 {
            sequence.push(Step::Project(3, false));
        }
    }
    cases.push(("sequence", sequence));
    // The cases above draw their steps first, then each its preparation and
    // readout, so their numbers stay those of earlier runs; the cases below
    // draw after them.
    for (case, steps) in cases {
        let prepare = layer(&mut rng);
        let readout = layer(&mut rng);
        results.push(run_case(
            engine,
            reference,
            case,
            &[prepare, steps, readout].concat(),
        )?);
    }
    let wide = |rng: &mut SplitMix| {
        let sites = distinct_triple(rng);
        (sites[0], Step::Wide(sites.to_vec(), random_unitary(8, rng)))
    };
    let narrow = |rng: &mut SplitMix| {
        let site = usize::try_from(rng.next() % (SITES - 1) as u64).unwrap_or(0);
        Step::Narrow(vec![site, site + 1], random_unitary(4, rng))
    };
    let mut separating: Vec<(&'static str, Vec<Step>)> = Vec::new();
    let (_, first) = wide(&mut rng);
    let (_, second) = wide(&mut rng);
    separating.push(("two-mpos-no-read", vec![first, second]));
    let (site, first) = wide(&mut rng);
    let (_, second) = wide(&mut rng);
    separating.push((
        "two-mpos-read-between",
        vec![first, Step::Read(site), second],
    ));
    let (site, first) = wide(&mut rng);
    separating.push((
        "mpo-then-narrow-after-read",
        vec![first, Step::Read(site), narrow(&mut rng)],
    ));
    let (site, first) = wide(&mut rng);
    separating.push(("mpo-after-read", vec![Step::Read(site), first]));
    let mut steps = Vec::new();
    for at in 0..RESTART_PERIOD {
        steps.extend([narrow(&mut rng), Step::Read(at % SITES)]);
    }
    steps.push(wide(&mut rng).1);
    separating.push(("mpo-after-restart", steps));
    let (site, first) = wide(&mut rng);
    let mut steps = vec![first, Step::Read(site)];
    for at in 0..RESTART_PERIOD {
        steps.extend([narrow(&mut rng), Step::Read(at % SITES)]);
    }
    separating.push(("mpo-then-restart", steps));
    let (site, first) = wide(&mut rng);
    let (_, second) = wide(&mut rng);
    separating.push((
        "mpo-across-projection",
        vec![first, Step::Read(site), Step::Project(site, false), second],
    ));
    let (_, first) = wide(&mut rng);
    separating.push((
        "projection-then-mpo",
        vec![Step::Read(3), Step::Project(3, false), first],
    ));
    for (case, steps) in separating {
        let prepare = layer(&mut rng);
        let readout = layer(&mut rng);
        results.push(run_case(
            engine,
            reference,
            case,
            &[prepare, steps, readout].concat(),
        )?);
    }
    Ok(results)
}

/// Runs `program` on `engine`, and on `reference` as given, with its
/// three-site operators transposed, and under hypothesis (a).
fn run_case<E: MpsEngine, R: MpsEngine>(
    engine: &E,
    reference: &R,
    case: &'static str,
    program: &[Step],
) -> Result<MpoCaseResult, String> {
    let transposed: Vec<Step> = program
        .iter()
        .map(|step| match step {
            Step::Wide(sites, unitary) => Step::Wide(sites.clone(), transpose(unitary)),
            other => other.clone(),
        })
        .collect();
    let fail = |error: String| format!("{case}: {error}");
    let (on_engine, _) = run(engine, program, Library::AsDocumented).map_err(fail)?;
    let (on_reference, _) = run(reference, program, Library::AsDocumented).map_err(fail)?;
    let (on_transposed, _) = run(reference, &transposed, Library::AsDocumented).map_err(fail)?;
    let (on_reapplied, reapplied) =
        run(reference, program, Library::KeepsNetworkOperators).map_err(fail)?;
    let (sites, asymmetry) = program
        .iter()
        .find_map(|step| match step {
            Step::Wide(sites, unitary) => Some((sites.clone(), asymmetry(unitary))),
            _ => None,
        })
        .expect("every case has a three-site operator");
    Ok(MpoCaseResult {
        case,
        sites,
        max_difference: largest_difference(&on_engine, &on_reference),
        max_difference_transposed: largest_difference(&on_engine, &on_transposed),
        max_difference_reapplied: reapplied.then(|| largest_difference(&on_engine, &on_reapplied)),
        asymmetry,
    })
}

#[derive(Clone)]
enum Step {
    /// An operator on one or two sites, the tensor-operator path.
    Narrow(Vec<usize>, Vec<Complex64>),
    /// A three-site operator, the MPO path under test.
    Wide(Vec<usize>, Vec<Complex64>),
    /// P(One) of a site, read and compared.
    Read(usize),
    /// A projection of a site on a value.
    Project(usize, bool),
}

/// Random one-site unitaries on every site, then random two-site unitaries
/// on every adjacent pair: an entangled state, every phase mixed.
fn layer(rng: &mut SplitMix) -> Vec<Step> {
    let mut steps: Vec<Step> = (0..SITES)
        .map(|site| Step::Narrow(vec![site], random_unitary(2, rng)))
        .collect();
    steps.extend(
        (0..SITES - 1).map(|site| Step::Narrow(vec![site, site + 1], random_unitary(4, rng))),
    );
    steps
}

/// What a run assumes of cuTensorNet, as `CuTensorNetEngine` drives it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Library {
    /// The engine's own calls, or the reference run exactly: capture deletes
    /// every operator, as documented.
    AsDocumented,
    /// Hypothesis (a): capture deletes the tensor operators but keeps the
    /// network operators (the MPOs), so each later compute applies every
    /// MPO kept since the native state's last restart, in the order they
    /// were applied, before the operators applied since the last compute.
    KeepsNetworkOperators,
}

/// The engine's computes and restarts, followed step by step: a compute
/// at each read that follows an operator (the first read included), a
/// restart before the first operator after `RESTART_PERIOD` computes;
/// and, under `Library::KeepsNetworkOperators`, the MPOs kept since.
#[derive(Default)]
struct Computes {
    computed: bool,
    since_restart: usize,
    /// The three-site operators applied since the last compute.
    window: Vec<(Vec<usize>, Vec<Complex64>)>,
    /// The three-site operators of earlier computes since the restart.
    kept: Vec<(Vec<usize>, Vec<Complex64>)>,
    /// Whether a kept operator was applied again.
    reapplied: bool,
}

impl Computes {
    /// A read: the engine computes and captures if an operator was applied
    /// since its last compute.
    fn read(&mut self) {
        if !self.computed {
            self.computed = true;
            self.since_restart += 1;
            self.kept.append(&mut self.window);
        }
    }

    /// Before an operator (a projector included): the engine restarts if
    /// due; the first operator after a compute is where the library, under
    /// hypothesis (a), would apply the kept MPOs, before this window's
    /// operators.
    fn before_operator<E: MpsEngine>(
        &mut self,
        engine: &E,
        state: &mut E::State,
        library: Library,
    ) -> Result<(), String> {
        if !self.computed {
            return Ok(());
        }
        self.computed = false;
        if self.since_restart >= RESTART_PERIOD {
            self.kept.clear();
            self.since_restart = 0;
        }
        if library == Library::KeepsNetworkOperators {
            for (sites, matrix) in &self.kept {
                engine
                    .apply(state, sites, matrix)
                    .map_err(|error| error.to_string())?;
                self.reapplied = true;
            }
        }
        Ok(())
    }
}

/// Runs `steps` from the product state of `SITES` zeros, exact (bond 64),
/// and returns the reads, then every basis state's probability; and, under
/// `Library::KeepsNetworkOperators`, whether a kept MPO was applied again.
fn run<E: MpsEngine>(
    engine: &E,
    steps: &[Step],
    library: Library,
) -> Result<(Vec<f64>, bool), String> {
    let capacity = MpsShape::qubit_chain(SITES, 64).map_err(|error| error.to_string())?;
    let mut state = engine
        .prepare(&capacity)
        .map_err(|error| error.to_string())?;
    let mut computes = Computes::default();
    let mut values = Vec::new();
    for step in steps {
        match step {
            Step::Narrow(sites, matrix) | Step::Wide(sites, matrix) => {
                computes.before_operator(engine, &mut state, library)?;
                engine
                    .apply(&mut state, sites, matrix)
                    .map_err(|error| error.to_string())?;
                if let Step::Wide(..) = step {
                    computes.window.push((sites.clone(), matrix.clone()));
                }
            }
            Step::Read(site) => {
                computes.read();
                values.push(
                    engine
                        .probability_of_one(&state, *site)
                        .map_err(|error| error.to_string())?,
                );
            }
            // The engine reads the value's probability on the computed
            // state, then applies the projector, which the library would
            // apply after the kept MPOs; so under hypothesis (a) the
            // probability is read before the kept MPOs are applied, and the
            // reference's own (after them) only scales the state, which
            // every read normalizes.
            Step::Project(site, one) if library == Library::KeepsNetworkOperators => {
                computes.read();
                let p_one = engine
                    .probability_of_one(&state, *site)
                    .map_err(|error| error.to_string())?;
                computes.before_operator(engine, &mut state, library)?;
                engine
                    .project(&mut state, *site, *one)
                    .map_err(|error| error.to_string())?;
                values.push(if *one { p_one } else { 1.0 - p_one });
            }
            Step::Project(site, one) => {
                computes.read();
                computes.before_operator(engine, &mut state, library)?;
                values.push(
                    engine
                        .project(&mut state, *site, *one)
                        .map_err(|error| error.to_string())?,
                );
            }
        }
    }
    let mut prefixes = vec![1.0];
    for site in 0..SITES {
        let mut next = Vec::with_capacity(2 * prefixes.len());
        for (prefix, &probability) in prefixes.iter().enumerate() {
            if probability < 1e-14 {
                next.extend([0.0, 0.0]);
                continue;
            }
            let given: Vec<(usize, bool)> = (0..site)
                .map(|earlier| (earlier, (prefix >> (site - 1 - earlier)) & 1 == 1))
                .collect();
            let one = engine
                .conditional_probability_of_one(&state, site, &given)
                .map_err(|error| error.to_string())?;
            next.extend([probability * (1.0 - one), probability * one]);
        }
        prefixes = next;
    }
    values.extend(prefixes);
    Ok((values, computes.reapplied))
}

/// The largest `|left - right|`, NaN if any is (`f64::max` would drop it).
fn largest_difference(left: &[f64], right: &[f64]) -> f64 {
    if left.len() != right.len() {
        return f64::NAN;
    }
    left.iter()
        .zip(right)
        .map(|(left, right)| (left - right).abs())
        .fold(0.0, |largest, difference| {
            if difference.is_nan() || difference > largest {
                difference
            } else {
                largest
            }
        })
}

/// A `dim`×`dim` unitary, row-major, from random complex entries by
/// Gram–Schmidt on its rows: dense, complex and not symmetric.
fn random_unitary(dim: usize, rng: &mut SplitMix) -> Vec<Complex64> {
    let mut rows: Vec<Vec<Complex64>> = Vec::with_capacity(dim);
    while rows.len() < dim {
        let mut row: Vec<Complex64> = (0..dim)
            .map(|_| Complex64::new(rng.symmetric(), rng.symmetric()))
            .collect();
        for earlier in &rows {
            let overlap: Complex64 = earlier
                .iter()
                .zip(&row)
                .map(|(earlier, value)| earlier.conj() * value)
                .sum();
            for (value, earlier) in row.iter_mut().zip(earlier) {
                *value -= overlap * earlier;
            }
        }
        let norm = row.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
        if norm > 1e-3 {
            rows.push(row.into_iter().map(|value| value / norm).collect());
        }
    }
    rows.concat()
}

/// The three-site operator `pair` on the outer sites and the identity on
/// the middle one, row-major, the first site most significant.
fn padded_middle(pair: &[Complex64]) -> Vec<Complex64> {
    let outer = |index: usize| 2 * (index >> 2) + (index & 1);
    let middle = |index: usize| (index >> 1) & 1;
    (0..64)
        .map(|at| {
            let (row, col) = (at / 8, at % 8);
            if middle(row) == middle(col) {
                pair[4 * outer(row) + outer(col)]
            } else {
                Complex64::ZERO
            }
        })
        .collect()
}

fn transpose(matrix: &[Complex64]) -> Vec<Complex64> {
    let dim = matrix.len().isqrt();
    (0..matrix.len())
        .map(|at| matrix[(at % dim) * dim + at / dim])
        .collect()
}

fn asymmetry(matrix: &[Complex64]) -> f64 {
    let transposed = transpose(matrix);
    matrix
        .iter()
        .zip(&transposed)
        .map(|(value, transposed)| (value - transposed).norm())
        .fold(0.0, f64::max)
}

fn distinct_triple(rng: &mut SplitMix) -> [usize; 3] {
    loop {
        let draw = |rng: &mut SplitMix| usize::try_from(rng.next() % SITES as u64).unwrap_or(0);
        let triple = [draw(rng), draw(rng), draw(rng)];
        if triple[0] != triple[1] && triple[1] != triple[2] && triple[0] != triple[2] {
            return triple;
        }
    }
}

/// `SplitMix64`: a seeded stream, so every run draws the same cases.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in [-1, 1).
    #[expect(
        clippy::cast_precision_loss,
        reason = "53 random bits, exact in an f64"
    )]
    fn symmetric(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 52) as f64 - 1.0
    }
}
