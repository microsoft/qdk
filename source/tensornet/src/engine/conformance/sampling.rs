// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The case of sampling: the frequencies and independence of the shots,
//! and the seed's effect on them.

use super::gates::hadamard;
use super::statistics::POWER_SHOTS;
use super::twin::{REFERENCE, Twin, describe, scrambled};
use super::{cnot, ry, statistics};
use crate::MpsEngine;
use crate::reference::ReferenceState;

/// How many shots the sampling cases draw, but the one that measures the
/// kit's power.
pub(super) const SHOTS: usize = 4000;

/// How many shots the seed checks draw: enough that two streams agree on
/// every value with probability 2⁻¹²⁸.
const SEED_SHOTS: usize = 64;

/// Draws `shots` shots of `sites` from the engine's `state` with `seed`,
/// and checks their number of values and the frequency of every joint
/// outcome against its probability in the normalized `expected`, within
/// the Chernoff bound of [`statistics`].
pub(super) fn sampled<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    expected: &ReferenceState,
    sites: &[usize],
    shots: usize,
    seed: u64,
    when: &str,
) -> Result<Vec<bool>, String> {
    let values = describe("engine sample", engine.sample(state, sites, shots, seed))?;
    if values.len() != shots * sites.len() {
        return Err(format!(
            "{when}: {shots} shots of {} sites gave {} values",
            sites.len(),
            values.len()
        ));
    }
    let mut counts = vec![0usize; 1 << sites.len()];
    for shot in values.chunks_exact(sites.len()) {
        let outcome = (0..sites.len()).fold(0, |outcome, t| outcome | usize::from(shot[t]) << t);
        counts[outcome] += 1;
    }
    let mut probabilities = vec![0.0; counts.len()];
    for index in 0..1usize << expected.site_count() {
        let outcome =
            (0..sites.len()).fold(0, |outcome, t| outcome | ((index >> sites[t]) & 1) << t);
        probabilities[outcome] += expected.amplitude(index).norm_sqr();
    }
    for (outcome, (&count, &probability)) in counts.iter().zip(&probabilities).enumerate() {
        if !statistics::frequency_fits(count, shots, probability) {
            let values: Vec<bool> = (0..sites.len()).map(|t| outcome >> t & 1 == 1).collect();
            return Err(format!(
                "{when}: {sites:?} were {values:?} in {count} of {shots} shots, too far from \
                 a probability of {probability}"
            ));
        }
    }
    Ok(values)
}

/// Checks that the shots of `values`, drawn of `sites` from a state whose
/// normalized reference is `expected`, are independent, by the sums of
/// [`statistics::independent`].
fn independent(
    values: &[bool],
    sites: &[usize],
    expected: &ReferenceState,
    when: &str,
) -> Result<(), String> {
    let marginals = sites
        .iter()
        .map(|&site| describe("reference", REFERENCE.probability_of_one(expected, site)))
        .collect::<Result<Vec<f64>, String>>()?;
    statistics::independent(values, sites.len(), &marginals)
        .map_err(|sum| format!("{when}: the shots are not independent: {sum}"))
}

/// Three sites: a Bell pair on sites 0 and 2, and site 1 One with
/// probability `p1`.
fn bell_beside<E: MpsEngine>(engine: &E, p1: f64) -> Result<Twin<'_, E>, String> {
    let mut twin = Twin::new(engine, 3)?;
    twin.apply(&[0], &hadamard())?;
    twin.apply(&[0, 2], &cnot())?;
    twin.apply(&[1], &ry(2.0 * p1.sqrt().asin()))?;
    Ok(twin)
}

/// Samples, drawn as sites `[2, 0, 1]` (not in increasing order) of a Bell
/// pair on sites 0 and 2 beside site 1:
///
/// - [`POWER_SHOTS`] shots with site 1 One with probability 0.02, so the
///   joint outcomes have probabilities 0.49, 0.49, 0.01 and 0.01: a bias of
///   0.02, or a dropped rare outcome, fails ([`statistics`]), and so does
///   a single shot that splits the Bell pair, an outcome of probability
///   zero, and [`REPEAT_WINDOW`](statistics::REPEAT_WINDOW) consecutive
///   shots that repeat those from an earlier shot;
/// - [`SHOTS`] shots with site 1 One with probability one half, every
///   position then a fair coin, which must be independent at every lag
///   between two of them;
/// - [`SHOTS`] shots of the scrambled state, whose amplitudes are complex;
/// - the seeds ([`seeds`]);
/// - no values for no sites, or no shots;
///
/// and the states unchanged by sampling.
pub(super) fn sample<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let (seed, sites) = (7, [2, 0, 1]);
    let rare = bell_beside(engine, 0.02)?;
    let rare_when = "a Bell pair beside a site One with probability 0.02";
    let power = sampled(
        engine,
        &rare.state,
        &rare.reference,
        &sites,
        POWER_SHOTS,
        seed,
        rare_when,
    )?;
    let even = bell_beside(engine, 0.5)?;
    let when = "a Bell pair beside a site One with probability one half";
    let values = sampled(
        engine,
        &even.state,
        &even.reference,
        &sites,
        SHOTS,
        seed,
        when,
    )?;
    independent(&values, &sites, &even.reference, when)?;
    // After the sums, which also name the distance of a repeat within the
    // shots they cover.
    statistics::unrepeated(&power, sites.len())
        .map_err(|repeat| format!("{rare_when}: the shots are not independent: {repeat}"))?;
    let scrambled = scrambled(engine)?;
    sampled(
        engine,
        &scrambled.state,
        &scrambled.reference,
        &[1, 2, 0],
        SHOTS,
        seed,
        "the scrambled state",
    )?;
    seeds(engine, &even.state)?;
    for (sites, shots) in [(&[][..], 5), (&[0, 1][..], 0)] {
        let values = describe(
            "engine sample",
            engine.sample(&even.state, sites, shots, seed),
        )?;
        if !values.is_empty() {
            return Err(format!("{shots} shots of {sites:?} gave {values:?}"));
        }
    }
    rare.compare("after sampling")?;
    even.compare("after sampling")
}

/// The seeds of [`SEED_SHOTS`] shots of `state`, the Bell pair beside a
/// site of probability one half, whose values hold two fair coins a shot:
/// the same seed gives the same values; seeds that differ in their lowest
/// bit only (7 and 6, and `u64::MAX` and the seed below it, beyond the
/// integers an `f64` holds exactly), or their highest bit only, and the
/// seeds 0 and `u64::MAX`, give other values, as every bit of a seed must
/// select the values. A correct engine gives the same values for two seeds
/// with probability 2⁻¹²⁸, unless it maps the seed into a narrower
/// library's range by a hash, which maps two of these seeds to one with
/// probability about 2⁻³¹ a pair, about 2⁻²⁹ for the four.
fn seeds<E: MpsEngine>(engine: &E, state: &E::State) -> Result<(), String> {
    let draw = |seed: u64| {
        describe(
            &format!("engine sample with the seed {seed}"),
            engine.sample(state, &[2, 0, 1], SEED_SHOTS, seed),
        )
    };
    if draw(7)? != draw(7)? {
        return Err("the same seed gave different samples".to_string());
    }
    for (a, b) in [
        (7, 7 ^ 1),
        (7, 7 ^ 1 << 63),
        (0, u64::MAX),
        (u64::MAX, u64::MAX ^ 1),
    ] {
        if draw(a)? == draw(b)? {
            return Err(format!("the seeds {a} and {b} gave the same samples"));
        }
    }
    Ok(())
}
