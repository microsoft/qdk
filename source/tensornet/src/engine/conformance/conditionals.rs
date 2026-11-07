// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The case of conditional probabilities: every condition on the
//! scrambled state, tiny ones, and the rounding an engine may leave on a
//! condition impossible in exact arithmetic.
//!
//! The kit asks P(One) of each site, alone and given the values of other
//! sites, in increasing, decreasing and (for three conditions) a mixed site
//! order, and checks the state unchanged after them.
//!
//! A condition of probability zero on a state that only gates prepared (a
//! GHZ state) must be refused: the reference, and an engine that holds the
//! state as its gates prepared it, compute its probability as exactly 0,
//! but an engine whose factorizations mix equal Schmidt values could leave
//! it a rounding probability, and would fail there.
//!
//! After truncations or projections, an engine's factorizations may leave
//! an impossible condition a probability of rounding size, which the
//! contract lets it answer. There the kit accepts the refusal, or an answer
//! (not compared, being meaningless) when the engine's own probability of
//! the condition is within [`IMPOSSIBLE_OUTCOME_ROUNDING`] of zero, either
//! side, both ways the kit reads it: the product of the probabilities its
//! projections of a fork return, and the product of its answers P(vᵢ | the
//! values before vᵢ) along the condition (a bound that may be revisited
//! once an engine computes conditionals its own way). Along the condition,
//! a P(Zero) within 10⁻¹² of zero, either side
//! ([`DEFAULT_CERTAINTY_ALLOWANCE`], the rounding of a certain One), counts
//! as zero, and so does a value whose P(One) the engine refuses as
//! impossible. The kit does the same where the reference's own rounding
//! leaves the condition a probability of at most that bound instead of
//! zero (after a gate on three sites that truncates, about 10⁻³³).
//!
//! Conditions of probability 1e-18 and 1e-200 (below the square of `f64`'s
//! epsilon) must be answered, since only exactly zero is refused; every
//! other condition the kit asks has probability 1e-4 or more, far from
//! rounding.

use super::gates::hadamard;
use super::twin::{REFERENCE, Twin, close, describe, refused, scrambled};
use super::{DEFAULT_CERTAINTY_ALLOWANCE, IMPOSSIBLE_OUTCOME_ROUNDING, cnot, ry};
use crate::reference::ReferenceState;
use crate::{MpsEngine, MpsEngineError, Refusal};

/// Compares the engine's P(One) of every site given every condition on the
/// sites, each absent, Zero or One, in increasing and in decreasing site
/// order, and a condition on three sites or more also in a mixed order
/// (the increasing one rotated left by one), with the reference's on a
/// normalized `expected`: its answer, or its refusal. Where the reference
/// refuses a condition of probability zero, the engine may answer instead,
/// as the contract lets it when rounding leaves the condition a tiny
/// probability, provided that probability, as the engine computes it both
/// ways the kit reads it, is within [`IMPOSSIBLE_OUTCOME_ROUNDING`] of
/// zero ([`impossible_by_rounding`]), with [`DEFAULT_CERTAINTY_ALLOWANCE`]
/// for a certain One along its conditional path; the answer is then meaningless
/// and not compared. Where the reference's own
/// rounding leaves the condition a probability of at most
/// [`IMPOSSIBLE_OUTCOME_ROUNDING`], the engine may refuse it, or answer it
/// on the same proviso.
pub(super) fn compare_conditionals<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    expected: &ReferenceState,
    when: &str,
) -> Result<(), String> {
    let n = expected.site_count();
    for site in 0..n {
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
            let mut reversed = given.clone();
            reversed.reverse();
            let mut orders = vec![given, reversed];
            if orders[0].len() >= 3 {
                // Neither increasing nor decreasing: [1, 2, 0] of three.
                let mut mixed = orders[0].clone();
                mixed.rotate_left(1);
                orders.push(mixed);
            }
            for given in orders {
                let what = format!("{when}: P(One) of site {site} given {given:?}");
                let wanted = REFERENCE.conditional_probability_of_one(expected, site, &given);
                let actual = engine.conditional_probability_of_one(state, site, &given);
                match wanted {
                    // The reference's own rounding, as after a gate on three
                    // sites that truncates: an engine that computes exactly
                    // zero refuses it, and another answer is meaningless.
                    Ok(_)
                        if condition_probability(expected, &given)
                            <= IMPOSSIBLE_OUTCOME_ROUNDING =>
                    {
                        match actual {
                            Ok(_) => impossible_by_rounding(engine, state, &given, &what)?,
                            actual => refused(&what, actual, &Refusal::ZeroProbabilityOutcome)?,
                        }
                    }
                    Ok(expected) => close(&what, describe(&what, actual)?, expected)?,
                    Err(error) => match (error.refusal(), actual) {
                        (Some(Refusal::ZeroProbabilityOutcome), Ok(_)) => {
                            impossible_by_rounding(engine, state, &given, &what)?;
                        }
                        (Some(refusal), actual) => refused(&what, actual, refusal)?,
                        (None, _) => return Err(format!("{what}: the reference failed: {error}")),
                    },
                }
            }
        }
    }
    Ok(())
}

/// The reference's probability of `given` on `expected`: the product of
/// the probabilities its projections on each value of `given` return, or
/// zero when one of them is refused.
pub(crate) fn condition_probability(expected: &ReferenceState, given: &[(usize, bool)]) -> f64 {
    let mut projected = expected.clone();
    let mut probability = 1.0;
    for &(at, one) in given {
        match REFERENCE.project(&mut projected, at, one) {
            Ok(p) => probability *= p,
            Err(_) => return 0.0,
        }
    }
    probability
}

/// Checks that the engine's own probability of `given`, impossible in
/// exact arithmetic, is within [`IMPOSSIBLE_OUTCOME_ROUNDING`] of zero,
/// either side ([`within_rounding`]), both ways the kit reads it: the
/// product of the probabilities its projections of a fork on each value of
/// `given` return (zero when one of them is refused), and the product
/// along its conditional path ([`conditional_path_probability`]). The
/// second bound holds an engine that computes conditionals its own way,
/// not by projections, to the same bound; it may be revisited once such an
/// engine runs the kit.
fn impossible_by_rounding<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    given: &[(usize, bool)],
    what: &str,
) -> Result<(), String> {
    let mut fork = describe(&format!("{what}: engine fork"), engine.fork(state))?;
    let mut probability = 1.0;
    for &(at, one) in given {
        match engine.project(&mut fork, at, one) {
            Ok(p) => probability *= p,
            Err(error) if error.refusal() == Some(&Refusal::ZeroProbabilityOutcome) => {
                probability = 0.0;
                break;
            }
            Err(error) => {
                return Err(format!(
                    "{what}: the projection of a fork on site {at} failed: {error}"
                ));
            }
        }
    }
    // Not `>`: a NaN is not within rounding.
    if !within_rounding(probability) {
        return Err(format!(
            "{what} was answered, though the condition is impossible and the engine's own \
             probability of it is {probability}, not within rounding"
        ));
    }
    let path = conditional_path_probability(engine, state, given, what)?;
    if within_rounding(path) {
        Ok(())
    } else {
        Err(format!(
            "{what} was answered, though the condition is impossible and its probability \
             along the engine's conditional path is {path}, not within rounding"
        ))
    }
}

/// Whether the engine's probability of an impossible condition is within
/// [`IMPOSSIBLE_OUTCOME_ROUNDING`] of zero, either side: a negative
/// probability is as much a leak as a positive one; a NaN is not within.
fn within_rounding(probability: f64) -> bool {
    probability.abs() <= IMPOSSIBLE_OUTCOME_ROUNDING
}

/// The engine's probability of `given` along its conditional path: the
/// product of P(vᵢ | the values of `given` before it) over the values vᵢ
/// of `given`, in its order, each read with
/// [`MpsEngine::conditional_probability_of_one`]; zero when the engine
/// refuses one of them as impossible, as it may once the values before it
/// are. P(Zero) is one minus P(One), which fp64 resolves only to about ε
/// near one, so a P(Zero) within [`DEFAULT_CERTAINTY_ALLOWANCE`] of zero,
/// either side, counts as zero.
fn conditional_path_probability<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    given: &[(usize, bool)],
    what: &str,
) -> Result<f64, String> {
    let mut probability = 1.0;
    for (before, &(at, one)) in given.iter().enumerate() {
        let earlier = &given[..before];
        let p_one = match engine.conditional_probability_of_one(state, at, earlier) {
            Ok(p) => p,
            Err(error) if error.refusal() == Some(&Refusal::ZeroProbabilityOutcome) => {
                return Ok(0.0);
            }
            Err(error) => {
                return Err(format!(
                    "{what}: P(One) of site {at} given {earlier:?}, on the engine's \
                     conditional path, failed: {error}"
                ));
            }
        };
        let p = match one {
            true => p_one,
            false if (1.0 - p_one).abs() <= DEFAULT_CERTAINTY_ALLOWANCE => 0.0,
            false => 1.0 - p_one,
        };
        probability *= p;
    }
    Ok(probability)
}

/// Conditional probabilities: every condition on the scrambled state,
/// which must leave the state unchanged; a condition of probability zero
/// on a GHZ state, in both orders, which must be refused (only gates
/// prepared it, see the module doc); and conditions of probability 1e-18
/// and 1e-200 ([`TINY_ONES`]), which must be answered, since only exactly
/// zero is refused.
pub(super) fn conditional<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let twin = scrambled(engine)?;
    compare_conditionals(engine, &twin.state, &twin.reference, "scrambled")?;
    twin.compare("after conditional probabilities")?;
    let mut ghz = Twin::new(engine, 3)?;
    ghz.apply(&[0], &hadamard())?;
    ghz.apply(&[0, 1], &cnot())?;
    ghz.apply(&[1, 2], &cnot())?;
    // Stricter than the contract: an engine whose SVD mixes equal Schmidt
    // values (a randomized SVD, such as cuTensorNet's gesvdr) may leave a
    // rounding probability here, which the contract lets it answer. If
    // such an engine fails here for rounding alone, switch to the lenient
    // check (`impossible_by_rounding`) here, plus a strict check of a
    // condition on a site no gate touched, a product with an exact zero on
    // every engine.
    for given in [[(0, true), (1, false)], [(1, false), (0, true)]] {
        refused(
            &format!("a condition of probability zero, {given:?}"),
            engine.conditional_probability_of_one(&ghz.state, 2, &given),
            &Refusal::ZeroProbabilityOutcome,
        )?;
    }
    ghz.compare("after a refused condition")?;
    for (probability, angle) in TINY_ONES {
        let tiny = tiny_one(engine, angle)?;
        let given = [(0, true)];
        let what =
            format!("P(One) of site 1 given {given:?}, a condition of probability {probability}");
        let expected = describe(
            "reference",
            REFERENCE.conditional_probability_of_one(&tiny.reference, 1, &given),
        )?;
        let actual = describe(
            &what,
            engine.conditional_probability_of_one(&tiny.state, 1, &given),
        )?;
        close(&what, actual, expected)?;
    }
    Ok(())
}

/// The tiny probabilities of One on which a projection, or a condition,
/// must be accepted, each with the angle of the `Ry` that prepares it:
/// 1e-18, and 1e-200, below the square of `f64`'s epsilon (about 4.9e-32),
/// which an engine that takes so small a probability for rounding refuses.
pub(super) const TINY_ONES: [(&str, f64); 2] = [("1e-18", 2e-9), ("1e-200", 2e-100)];

/// Two sites in a product state whose site 0 is One with a tiny
/// probability, `sin²(angle / 2)` (`Ry(angle)`), above zero in fp64 on
/// every engine that computes it as a squared amplitude, so a projection
/// on it, or a condition on it, must be accepted: only exactly zero is
/// refused.
pub(super) fn tiny_one<E: MpsEngine>(engine: &E, angle: f64) -> Result<Twin<'_, E>, String> {
    let mut twin = Twin::new(engine, 2)?;
    twin.apply(&[0], &ry(angle))?;
    twin.apply(&[1], &ry(0.7))?;
    Ok(twin)
}
