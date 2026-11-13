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
//! Expected reading: `far` or `unordered` failing while `adjacent` passes
//! points at the library; every case failing, or every case matching `Uᵀ`,
//! points at our tensors; `sequence` failing alone points at the engine's
//! handling across restarts or projections.

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
    for (case, steps) in cases {
        let prepare = layer(&mut rng);
        let readout = layer(&mut rng);
        let program = |transposed: bool| {
            prepare
                .iter()
                .cloned()
                .chain(steps.iter().map(|step| match step {
                    Step::Wide(sites, unitary) if transposed => {
                        Step::Wide(sites.clone(), transpose(unitary))
                    }
                    other => other.clone(),
                }))
                .chain(readout.iter().cloned())
                .collect::<Vec<_>>()
        };
        let fail = |error: String| format!("{case}: {error}");
        let on_engine = run(engine, &program(false)).map_err(fail)?;
        let on_reference = run(reference, &program(false)).map_err(fail)?;
        let on_transposed = run(reference, &program(true)).map_err(fail)?;
        let (sites, asymmetry) = match &steps[0] {
            Step::Wide(sites, unitary) => (sites.clone(), asymmetry(unitary)),
            _ => unreachable!("every case starts with a three-site operator"),
        };
        results.push(MpoCaseResult {
            case,
            sites,
            max_difference: largest_difference(&on_engine, &on_reference),
            max_difference_transposed: largest_difference(&on_engine, &on_transposed),
            asymmetry,
        });
    }
    Ok(results)
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

/// Runs `steps` from the product state of `SITES` zeros, exact (bond 64),
/// and returns the reads, then every basis state's probability.
fn run<E: MpsEngine>(engine: &E, steps: &[Step]) -> Result<Vec<f64>, String> {
    let capacity = MpsShape::qubit_chain(SITES, 64).map_err(|error| error.to_string())?;
    let mut state = engine
        .prepare(&capacity)
        .map_err(|error| error.to_string())?;
    let mut values = Vec::new();
    for step in steps {
        match step {
            Step::Narrow(sites, matrix) | Step::Wide(sites, matrix) => engine
                .apply(&mut state, sites, matrix)
                .map_err(|error| error.to_string())?,
            Step::Read(site) => values.push(
                engine
                    .probability_of_one(&state, *site)
                    .map_err(|error| error.to_string())?,
            ),
            Step::Project(site, one) => values.push(
                engine
                    .project(&mut state, *site, *one)
                    .map_err(|error| error.to_string())?,
            ),
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
    Ok(values)
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
