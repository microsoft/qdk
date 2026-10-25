// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Conformance checks for an [`MpsEngine`], with [`ExactReference`] as the
//! oracle: every engine runs the same operations as the reference and must
//! give the same answers.
//!
//! An engine's own tests call [`check`] and fail if it returns a failure:
//!
//! ```text
//! let failures = tensornet::conformance::check(&engine);
//! assert!(failures.is_empty(), "{failures:#?}");
//! ```
//!
//! Each case prepares a state of a few sites with a capacity that never
//! truncates, applies the same matrices to the engine and to the reference,
//! and compares what the engine reads back:
//!
//! - every Pauli string's expectation, which fixes the state up to a global
//!   phase, so a wrong matrix layout or site order is caught;
//! - P(One) of each site, alone and given the values of other sites;
//! - projections, then more gates after them, so an engine that caches a
//!   computed state must recompute it;
//! - forks, which must be independent of the state they copy;
//! - samples, by their distribution and by their seed, not bit for bit,
//!   since an engine samples with its own generator;
//! - the realized chain, by [`shape`](MpsEngine::shape);
//! - what both must refuse.
//!
//! The answers agree within [`TOLERANCE`]. A failure names its case and
//! what differed; nothing here panics, so a failing engine reports every
//! case it fails. The checks need the `conformance` feature, so they cost
//! nothing in a build that does not test an engine.

use crate::{
    Mps, MpsEngine, Pauli, PauliSum, PauliTerm, SampleSeed,
    reference::{ExactReference, ReferenceState},
};
use num_complex::Complex64;
use std::fmt::{self, Write};

/// How far an engine's probability or expectation may be from the
/// reference's. An fp64 engine that does not truncate is far closer.
pub const TOLERANCE: f64 = 1e-9;

/// How many shots the sampling case draws.
const SHOTS: usize = 4000;

/// How far a sampled frequency may be from its probability: about six
/// standard deviations at [`SHOTS`] shots, so a correct engine fails only
/// with negligible probability.
const FREQUENCY_TOLERANCE: f64 = 0.05;

/// One way an engine disagreed with the reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The case that failed.
    pub case: &'static str,
    /// What differed.
    pub message: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.case, self.message)
    }
}

/// One case: `Err` says what differed.
type Case<E> = fn(&E) -> Result<(), String>;

/// Runs every case against `engine` and returns the failures, empty when
/// the engine conforms.
pub fn check<E: MpsEngine>(engine: &E) -> Vec<Failure> {
    let cases: [(&'static str, Case<E>); 8] = [
        ("bell", bell),
        ("layout", layout),
        ("conditional", conditional),
        ("project", project),
        ("fork", fork),
        ("sample", sample),
        ("shape", shape),
        ("refusals", refusals),
    ];
    cases
        .into_iter()
        .filter_map(|(case, run)| run(engine).err().map(|message| Failure { case, message }))
        .collect()
}

/// A capacity of `sites` qubits whose bonds never truncate: each cut's bond
/// is the smaller side's full dimension.
///
/// # Errors
///
/// None for `sites` from 1 to 62; a larger chain overflows its element
/// counts, which [`Mps::new`] reports.
pub fn exact_capacity(sites: usize) -> Result<Mps, crate::MpsError> {
    let bond = |cut: usize| {
        let smaller = (cut + 1).min(sites - cut - 1);
        1usize.checked_shl(u32::try_from(smaller).unwrap_or(u32::MAX))
    };
    let chain = (0..sites)
        .map(|site| {
            let left = site.checked_sub(1).and_then(bond);
            let right = (site + 1 < sites).then(|| bond(site)).flatten();
            left.into_iter().chain([2]).chain(right).collect()
        })
        .collect();
    Mps::new(chain)
}

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

fn hadamard() -> Vec<Complex64> {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    vec![c(h, 0.0), c(h, 0.0), c(h, 0.0), c(-h, 0.0)]
}

fn pauli_x() -> Vec<Complex64> {
    Pauli::X.matrix().to_vec()
}

/// `Ry(θ)`, real and not symmetric.
fn ry(theta: f64) -> Vec<Complex64> {
    let (s, k) = (theta / 2.0).sin_cos();
    vec![c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)]
}

/// `S·H`, complex and not symmetric.
fn s_h() -> Vec<Complex64> {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    vec![c(h, 0.0), c(h, 0.0), c(0.0, h), c(0.0, -h)]
}

/// CNOT with its first site as the control.
fn cnot() -> Vec<Complex64> {
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    vec![l, o, o, o, o, l, o, o, o, o, o, l, o, o, l, o]
}

/// Controlled-Y with its first site as the control: complex, and neither
/// symmetric nor symmetric under swapping its sites.
fn controlled_y() -> Vec<Complex64> {
    let (o, l, i) = (c(0.0, 0.0), c(1.0, 0.0), c(0.0, 1.0));
    vec![l, o, o, o, o, l, o, o, o, o, o, -i, o, o, i, o]
}

/// One engine state and the reference state it must match.
struct Pair<'e, E: MpsEngine> {
    engine: &'e E,
    state: E::State,
    reference: ReferenceState,
}

const REFERENCE: ExactReference = ExactReference;

fn describe<T>(what: &str, result: Result<T, impl fmt::Display>) -> Result<T, String> {
    result.map_err(|e| format!("{what}: {e}"))
}

impl<'e, E: MpsEngine> Pair<'e, E> {
    fn new(engine: &'e E, sites: usize) -> Result<Self, String> {
        let capacity = describe("capacity", exact_capacity(sites))?;
        Ok(Self {
            engine,
            state: describe("engine prepare", engine.prepare(&capacity))?,
            reference: describe("reference prepare", REFERENCE.prepare(&capacity))?,
        })
    }

    fn sites(&self) -> usize {
        self.reference.sites()
    }

    fn apply(&mut self, sites: &[usize], matrix: &[Complex64]) -> Result<(), String> {
        describe(
            "reference apply",
            REFERENCE.apply(&mut self.reference, sites, matrix),
        )?;
        describe(
            &format!("engine apply on {sites:?}"),
            self.engine.apply(&mut self.state, sites, matrix),
        )
    }

    fn fork(&self) -> Result<Self, String> {
        Ok(Self {
            engine: self.engine,
            state: describe("engine fork", self.engine.fork(&self.state))?,
            reference: describe("reference fork", REFERENCE.fork(&self.reference))?,
        })
    }

    fn project(&mut self, site: usize, one: bool) -> Result<(), String> {
        let p1 = describe(
            "reference P(One)",
            REFERENCE.probability_of_one(&self.reference, site),
        )?;
        let p = if one { p1 } else { 1.0 - p1 };
        describe(
            "reference project",
            REFERENCE.project(&mut self.reference, site, one, p),
        )?;
        describe(
            &format!("engine project of site {site} on {one}"),
            self.engine.project(&mut self.state, site, one, p),
        )
    }

    /// Compares P(One) of every site and every Pauli string's expectation.
    fn compare(&self, when: &str) -> Result<(), String> {
        for site in 0..self.sites() {
            let expected = describe(
                "reference",
                REFERENCE.probability_of_one(&self.reference, site),
            )?;
            let actual = describe(
                &format!("engine P(One) of site {site}"),
                self.engine.probability_of_one(&self.state, site),
            )?;
            close(&format!("{when}: P(One) of site {site}"), actual, expected)?;
        }
        for observable in pauli_strings(self.sites()) {
            let expected = describe(
                "reference",
                REFERENCE.expectation(&self.reference, &observable),
            )?;
            let actual = describe(
                "engine expectation",
                self.engine.expectation(&self.state, &observable),
            )?;
            if (actual - expected).norm() > TOLERANCE {
                return Err(format!(
                    "{when}: ⟨{}⟩ is {actual}, not {expected}",
                    name(&observable)
                ));
            }
        }
        Ok(())
    }
}

fn close(what: &str, actual: f64, expected: f64) -> Result<(), String> {
    if (actual - expected).abs() <= TOLERANCE {
        Ok(())
    } else {
        Err(format!("{what} is {actual}, not {expected}"))
    }
}

/// Every Pauli string on `sites` sites but the identity, each with
/// coefficient one: 4ⁿ − 1 observables.
fn pauli_strings(sites: usize) -> Vec<PauliSum> {
    let paulis = [None, Some(Pauli::X), Some(Pauli::Y), Some(Pauli::Z)];
    (1..4usize.pow(u32::try_from(sites).unwrap_or(0)))
        .filter_map(|mut index| {
            let mut factors = Vec::new();
            for site in 0..sites {
                if let Some(Some(pauli)) = paulis.get(index % 4) {
                    factors.push((site, *pauli));
                }
                index /= 4;
            }
            let term = PauliTerm::new(c(1.0, 0.0), factors).ok()?;
            PauliSum::new(c(0.0, 0.0), vec![term]).ok()
        })
        .collect()
}

fn name(observable: &PauliSum) -> String {
    observable
        .terms()
        .iter()
        .map(|term| {
            term.factors()
                .iter()
                .fold(String::new(), |mut text, (site, pauli)| {
                    let _ = write!(text, "{pauli:?}{site}");
                    text
                })
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Three sites in a state with no symmetry to hide a layout error: complex
/// one-site gates, a gate on non-adjacent sites, and gates whose first site
/// is the higher one.
fn scrambled<E: MpsEngine>(engine: &E) -> Result<Pair<'_, E>, String> {
    let mut pair = Pair::new(engine, 3)?;
    pair.apply(&[0], &ry(0.7))?;
    pair.apply(&[1], &s_h())?;
    pair.apply(&[0, 2], &cnot())?;
    pair.apply(&[2, 1], &controlled_y())?;
    pair.apply(&[2], &ry(-1.1))?;
    pair.apply(&[1, 0], &cnot())?;
    Ok(pair)
}

fn bell<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut pair = Pair::new(engine, 2)?;
    pair.apply(&[0], &hadamard())?;
    pair.apply(&[0, 1], &cnot())?;
    pair.compare("bell")?;
    let zz = PauliSum::new(
        c(0.5, 0.0),
        vec![describe(
            "term",
            PauliTerm::new(c(2.0, 0.0), vec![(0, Pauli::Z), (1, Pauli::Z)]),
        )?],
    );
    let zz = describe("observable", zz)?;
    let value = describe("engine expectation", engine.expectation(&pair.state, &zz))?;
    if (value - c(2.5, 0.0)).norm() > TOLERANCE {
        return Err(format!("⟨0.5 + 2·Z0Z1⟩ is {value}, not 2.5"));
    }
    Ok(())
}

fn layout<E: MpsEngine>(engine: &E) -> Result<(), String> {
    scrambled(engine)?.compare("scrambled")
}

fn conditional<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let pair = scrambled(engine)?;
    let n = pair.sites();
    for site in 0..n {
        // Every condition on the sites, each absent, Zero or One.
        for code in 0..3usize.pow(u32::try_from(n).unwrap_or(0)) {
            let mut given = Vec::new();
            let mut rest = code;
            for at in 0..n {
                match rest % 3 {
                    1 => given.push((at, false)),
                    2 => given.push((at, true)),
                    _ => {}
                }
                rest /= 3;
            }
            let expected = REFERENCE.conditional_probability_of_one(&pair.reference, site, &given);
            let actual = engine.conditional_probability_of_one(&pair.state, site, &given);
            match (actual, expected) {
                (Ok(actual), Ok(expected)) => close(
                    &format!("P(One) of site {site} given {given:?}"),
                    actual,
                    expected,
                )?,
                (Err(_), Err(_)) => {}
                (actual, expected) => {
                    return Err(format!(
                        "P(One) of site {site} given {given:?}: engine {}, reference {}",
                        outcome(actual),
                        outcome(expected)
                    ));
                }
            }
        }
    }
    let mut ghz = Pair::new(engine, 3)?;
    ghz.apply(&[0], &hadamard())?;
    ghz.apply(&[0, 1], &cnot())?;
    ghz.apply(&[1, 2], &cnot())?;
    if engine
        .conditional_probability_of_one(&ghz.state, 2, &[(0, true), (1, false)])
        .is_ok()
    {
        return Err("a condition of probability zero was accepted".to_string());
    }
    Ok(())
}

fn outcome<T: fmt::Debug, X: fmt::Display>(result: Result<T, X>) -> String {
    match result {
        Ok(value) => format!("{value:?}"),
        Err(error) => format!("refused ({error})"),
    }
}

fn project<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut pair = scrambled(engine)?;
    pair.project(1, true)?;
    pair.compare("after projecting site 1 on One")?;
    pair.apply(&[1], &hadamard())?;
    pair.apply(&[1, 2], &controlled_y())?;
    pair.compare("after gates following the projection")?;
    pair.project(2, false)?;
    pair.project(0, true)?;
    pair.compare("after two more projections")?;
    for probability in [0.0, -0.5, 1.5, f64::NAN] {
        if engine
            .project(&mut pair.state, 1, true, probability)
            .is_ok()
        {
            return Err(format!(
                "a projection with probability {probability} was accepted"
            ));
        }
    }
    Ok(())
}

fn fork<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut original = scrambled(engine)?;
    let mut copy = original.fork()?;
    copy.apply(&[0], &pauli_x())?;
    copy.project(2, true)?;
    original.compare("the original after its fork changed")?;
    copy.compare("the fork")?;
    original.apply(&[2, 0], &controlled_y())?;
    copy.compare("the fork after its original changed")?;
    original.compare("the original after changing")?;
    let mut again = copy.fork()?;
    again.apply(&[1, 2], &cnot())?;
    again.compare("a fork of a fork")
}

fn sample<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let seed = describe("seed", SampleSeed::try_from(7))?;
    let mut bell = Pair::new(engine, 3)?;
    bell.apply(&[0], &hadamard())?;
    bell.apply(&[0, 2], &cnot())?;
    bell.apply(&[1], &ry(2.0 * 0.2f64.sqrt().asin()))?;
    let sites = [2, 0, 1];
    let values = describe(
        "engine sample",
        engine.sample(&bell.state, &sites, SHOTS, seed),
    )?;
    if values.len() != SHOTS * sites.len() {
        return Err(format!(
            "{SHOTS} shots of {} sites gave {} values",
            sites.len(),
            values.len()
        ));
    }
    let shots: Vec<&[bool]> = values.chunks_exact(sites.len()).collect();
    if let Some(shot) = shots.iter().find(|shot| shot[0] != shot[1]) {
        return Err(format!(
            "a shot {shot:?} breaks the Bell pair on sites 2 and 0"
        ));
    }
    #[expect(clippy::cast_precision_loss, reason = "shot counts are small")]
    let frequency = |at: usize| shots.iter().filter(|shot| shot[at]).count() as f64 / SHOTS as f64;
    for (at, expected) in [(0, 0.5), (2, 0.2)] {
        let actual = frequency(at);
        if (actual - expected).abs() > FREQUENCY_TOLERANCE {
            return Err(format!(
                "site {} was One in {actual} of the shots, not about {expected}",
                sites[at]
            ));
        }
    }
    let again = describe(
        "engine sample",
        engine.sample(&bell.state, &sites, SHOTS, seed),
    )?;
    if again != values {
        return Err("the same seed gave different samples".to_string());
    }
    let other = describe("seed", SampleSeed::try_from(8))?;
    let other = describe(
        "engine sample",
        engine.sample(&bell.state, &sites, SHOTS, other),
    )?;
    if other == values {
        return Err("two seeds gave the same samples".to_string());
    }
    for (sites, shots) in [(&[][..], 5), (&[0, 1][..], 0)] {
        let values = describe(
            "engine sample",
            engine.sample(&bell.state, sites, shots, seed),
        )?;
        if !values.is_empty() {
            return Err(format!("{shots} shots of {sites:?} gave {values:?}"));
        }
    }
    bell.compare("after sampling")
}

fn shape<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut pair = Pair::new(engine, 4)?;
    let same = |pair: &Pair<'_, E>, when: &str| -> Result<(), String> {
        let actual = describe("engine shape", engine.shape(&pair.state))?;
        let expected = describe("reference shape", REFERENCE.shape(&pair.reference))?;
        if actual == expected {
            Ok(())
        } else {
            Err(format!("{when}: the chain is {actual:?}, not {expected:?}"))
        }
    };
    same(&pair, "the prepared state")?;
    pair.apply(&[1], &pauli_x())?;
    pair.apply(&[3], &hadamard())?;
    same(&pair, "a product state")?;
    pair.apply(&[0], &hadamard())?;
    pair.apply(&[0, 1], &cnot())?;
    same(&pair, "one Bell pair")?;
    pair.apply(&[2], &hadamard())?;
    pair.apply(&[2, 0], &cnot())?;
    pair.apply(&[3, 1], &cnot())?;
    same(&pair, "an entangled chain")
}

fn refusals<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut pair = Pair::new(engine, 2)?;
    let out = PauliSum::new(
        c(0.0, 0.0),
        vec![describe(
            "term",
            PauliTerm::new(c(1.0, 0.0), vec![(2, Pauli::Z)]),
        )?],
    );
    let out = describe("observable", out)?;
    let seed = describe("seed", SampleSeed::try_from(1))?;
    let refused = [
        (
            "apply on a site out of range",
            engine.apply(&mut pair.state, &[2], &pauli_x()).is_err(),
        ),
        (
            "apply on a repeated site",
            engine.apply(&mut pair.state, &[1, 1], &cnot()).is_err(),
        ),
        (
            "apply of a matrix of the wrong size",
            engine.apply(&mut pair.state, &[0], &cnot()).is_err(),
        ),
        (
            "P(One) of a site out of range",
            engine.probability_of_one(&pair.state, 2).is_err(),
        ),
        (
            "a condition out of range",
            engine
                .conditional_probability_of_one(&pair.state, 0, &[(5, true)])
                .is_err(),
        ),
        (
            "a projection out of range",
            engine.project(&mut pair.state, 2, false, 1.0).is_err(),
        ),
        (
            "an expectation out of range",
            engine.expectation(&pair.state, &out).is_err(),
        ),
        (
            "a sample out of range",
            engine.sample(&pair.state, &[0, 2], 3, seed).is_err(),
        ),
        (
            "a sample of a repeated site",
            engine.sample(&pair.state, &[1, 1], 3, seed).is_err(),
        ),
    ];
    if let Some((what, _)) = refused.iter().find(|(_, refused)| !refused) {
        return Err(format!("{what} was accepted"));
    }
    pair.compare("after the refusals")
}

#[cfg(test)]
mod tests;
