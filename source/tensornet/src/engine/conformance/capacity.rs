// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The cases of the chain and its truncation: the shape, the retained
//! weight, and every read of a state its capacity truncated, a lone site
//! included.

use super::capped_twin::{CappedTwin, cap_of_one};
use super::gates::{hadamard, pauli_x};
use super::observables::z_on;
use super::sampling::{SHOTS, sampled};
use super::twin::{REFERENCE, Twin, close, describe, refused, scrambled};
use super::{cnot, controlled_y, kron, normalized, product, ry, s_h};
use crate::{MpsEngine, MpsShape, Refusal};

#[cfg(doc)]
use super::TOLERANCE;

/// The smallest squared Schmidt value, relative to its cut's weight, that
/// the kit's states hold: every engine it checks keeps such a value (its
/// cutoff is below it).
const SMALLEST_SCHMIDT: f64 = 1e-9;

/// The shape after gates, exact on every engine: a prepared state, a
/// product state, entangled chains, and a state with a Schmidt value whose
/// square is [`SMALLEST_SCHMIDT`] of its cut's weight, which every engine
/// the kit checks keeps (an engine whose cutoff drops it fails here, and
/// the truncation cases' exact shapes and reads could not be checked on
/// it).
pub(super) fn shape<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut twin = Twin::new(engine, 4)?;
    twin.same_shape("the prepared state")?;
    twin.apply(&[1], &pauli_x())?;
    twin.apply(&[3], &hadamard())?;
    twin.same_shape("a product state")?;
    twin.apply(&[0], &hadamard())?;
    twin.apply(&[0, 1], &cnot())?;
    twin.same_shape("one Bell pair")?;
    twin.apply(&[2], &hadamard())?;
    twin.apply(&[2, 0], &cnot())?;
    twin.apply(&[3, 1], &cnot())?;
    twin.same_shape("an entangled chain")?;
    let mut small = Twin::new(engine, 2)?;
    small.apply(&[0], &ry(2.0 * SMALLEST_SCHMIDT.sqrt().asin()))?;
    small.apply(&[0, 1], &cnot())?;
    small.same_shape("a Schmidt value whose square is 1e-9 of the weight")
}

/// The engine's retained weight, read and checked to be 1 to [`TOLERANCE`].
fn unit_weight<E: MpsEngine>(engine: &E, state: &E::State, when: &str) -> Result<f64, String> {
    let weight = describe("engine retained weight", engine.retained_weight(state))?;
    close(&format!("{when}: the retained weight"), weight, 1.0)?;
    Ok(weight)
}

/// The retained weight: 1 while nothing truncates, whatever the gates and
/// projections, and on a lone site; then, under a cap of one between two
/// sites, what [`CappedTwin`] checks after each step (two entangling gates,
/// each truncating, around a projection); and on a fork of the truncated
/// state, which must read what the state reads and keep its weight, then
/// truncate on its own, while each stays as it was when the other changes.
/// The chains of more sites are [`truncated_chain`]'s.
pub(super) fn retained_weight<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut twin = scrambled(engine)?;
    unit_weight(engine, &twin.state, "an entangled state that fits")?;
    twin.project(1, true)?;
    twin.apply(&[1, 2], &controlled_y())?;
    twin.project(2, false)?;
    unit_weight(engine, &twin.state, "after projections and gates")?;
    let lone = Twin::new(engine, 1)?;
    unit_weight(engine, &lone.state, "a lone site")?;

    let mut capped = CappedTwin::new(engine, cap_of_one()?)?;
    capped.apply(&[0], &ry(0.7), "a product state under a cap of one")?;
    capped.apply(&[1], &ry(-0.4), "a product state under a cap of one")?;
    capped.apply(&[0, 1], &cnot(), "an entangling gate under a cap of one")?;
    capped.apply(&[1], &ry(1.1), "a one-site gate under a cap of one")?;
    // A value whose probability is neither 0 nor 1 on any engine.
    let p1 = describe(
        "reference P(One)",
        REFERENCE.probability_of_one(&normalized(&capped.held), 1),
    )?;
    capped.project(1, p1 >= 0.5, "after a projection under a cap of one")?;
    capped.apply(&[0], &ry(0.9), "a one-site gate under a cap of one")?;
    capped.apply(
        &[0, 1],
        &cnot(),
        "a second entangling gate under a cap of one",
    )?;
    // The truncation kept a product of basis states: a turn makes the
    // fork's every read count.
    capped.apply(&[1], &ry(0.5), "a one-site gate after the truncation")?;
    let when = "a fork of the truncated state";
    let mut copy = capped.fork(when)?;
    copy.compare(when)?;
    copy.apply(&[0], &ry(1.3), "a one-site gate on a fork")?;
    copy.apply(&[0, 1], &controlled_y(), "a truncating gate on a fork")?;
    capped.check("the original after its fork truncated")?;
    capped.apply(&[1], &ry(0.6), "a gate on the original of a fork")?;
    copy.check("a fork after its original changed")
}

/// Every read of a state a cap of one between two sites truncated,
/// conditional probabilities and samples included: after a one-site gate
/// that follows the truncation, those of the normalized state the cap kept,
/// its larger Schmidt pair, with the gate applied; and after a second
/// truncation, those of the state the cap keeps then. On an engine that
/// does not truncate, they are the exact state's. Right after the first
/// truncation, [`CappedTwin`] checks P(One) of each site; the second
/// truncation is read in full right after it.
///
/// The cap drops about 39% of the weight at the first truncation, so a
/// read divided by the weight once too often, or not at all, is far from
/// the right one, and so is a sampled frequency drawn from the state not
/// normalized.
pub(super) fn truncated_reads<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut capped = CappedTwin::new(engine, cap_of_one()?)?;
    capped.apply(&[0], &ry(1.6), "a product state")?;
    capped.apply(&[1], &ry(0.2), "a product state")?;
    capped.apply(&[0, 1], &cnot(), "the truncated state")?;
    let when = "a gate after the truncation";
    capped.apply(&[0], &s_h(), when)?;
    capped.compare(when)?;
    capped.apply(&[0], &ry(1.5), when)?;
    capped.apply(&[1], &ry(0.9), when)?;
    let when = "a second truncation";
    capped.apply(&[0, 1], &cnot(), when)?;
    capped.compare(when)
}

/// Chains of more sites truncated at every cut, by gates on each pair of
/// neighbours, so that an engine that truncates only some cuts, keeps one
/// value too many, or weighs only some truncations, is caught:
///
/// - three sites under a bond of one, where each truncation keeps one
///   Schmidt pair of two;
/// - three sites under bonds of two and one, where the truncation at cut 1
///   also lowers the Schmidt rank at cut 0, which an engine may still hold
///   as it was; and the same chain projected before it truncates, where
///   the projection lowers the rank at cut 0;
/// - five sites under a bond of two, whose middle cuts would otherwise
///   reach rank four, so a bond of three kept is caught; then projections
///   of an interior site and of site 0, which lowers the rank at cut 0,
///   and another gate. A fork of the truncated chain must read what it
///   reads and have its retained weight.
///
/// [`CappedTwin`] checks the weight, the chain and every P(One) after each
/// step, and every read after each group of steps.
pub(super) fn truncated_chain<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let g = product(
        &controlled_y(),
        &product(&kron(&ry(0.8), &s_h()), &cnot(), 4),
        4,
    );
    let g2 = product(
        &cnot(),
        &product(&kron(&s_h(), &ry(-1.1)), &controlled_y(), 4),
        4,
    );
    let cap = describe("capacity", MpsShape::qubit_chain(3, 1))?;
    let mut three = CappedTwin::new(engine, cap)?;
    let when = "three sites under a bond of one";
    for (site, angle) in [(0, 1.6), (1, 0.5), (2, -0.9)] {
        three.apply(&[site], &ry(angle), when)?;
    }
    three.apply(&[0, 1], &cnot(), when)?;
    three.apply(&[1], &ry(0.8), when)?;
    three.apply(&[1, 2], &controlled_y(), when)?;
    three.apply(&[2], &ry(1.2), when)?;
    three.apply(&[2, 1], &g, when)?;
    three.compare(when)?;

    let cap = describe(
        "capacity",
        MpsShape::new(vec![vec![2, 2], vec![2, 2, 1], vec![1, 2]]),
    )?;
    let mut uneven = CappedTwin::new(engine, cap)?;
    let when = "three sites under bonds of two and one";
    uneven.apply(&[0], &ry(1.2), when)?;
    uneven.apply(&[0, 1], &cnot(), when)?;
    uneven.apply(&[1, 2], &cnot(), when)?;
    uneven.compare(when)?;
    let mut projected = CappedTwin::new(engine, uneven.capacity)?;
    let when = "three sites under bonds of two and one, projected before a truncation";
    projected.apply(&[0], &ry(1.2), when)?;
    projected.apply(&[0, 1], &cnot(), when)?;
    let p1 = describe(
        "reference P(One)",
        REFERENCE.probability_of_one(&normalized(&projected.held), 1),
    )?;
    projected.project(1, p1 >= 0.5, when)?;
    projected.apply(&[1], &ry(0.9), when)?;
    projected.apply(&[1, 2], &cnot(), when)?;
    projected.compare(when)?;

    let cap = describe("capacity", MpsShape::qubit_chain(5, 2))?;
    let mut five = CappedTwin::new(engine, cap)?;
    let when = "five sites under a bond of two";
    for (site, matrix) in [
        (0, ry(0.9)),
        (1, s_h()),
        (2, ry(1.3)),
        (3, s_h()),
        (4, ry(-0.6)),
    ] {
        five.apply(&[site], &matrix, when)?;
    }
    for (sites, matrix) in [
        ([0, 1], &g),
        ([2, 3], &g2),
        ([1, 2], &g),
        ([3, 4], &g),
        ([2, 3], &g2),
        ([1, 2], &g),
    ] {
        five.apply(&sites, matrix, when)?;
    }
    five.compare(when)?;
    let fork = "a fork of five sites truncated under a bond of two";
    five.fork(fork)?.compare(fork)?;
    let when = "after projections of five sites under a bond of two";
    for site in [2, 0] {
        let p1 = describe(
            "reference P(One)",
            REFERENCE.probability_of_one(&normalized(&five.held), site),
        )?;
        five.project(site, p1 >= 0.5, when)?;
    }
    five.compare(when)?;
    let when = "a gate after a projection of five sites under a bond of two";
    five.apply(&[2, 3], &g, when)?;
    five.compare(when)
}

/// A capacity whose bonds differ from cut to cut, two, one and two, holding
/// two entangled pairs (sites 0 and 1, 2 and 3), which fit it exactly:
///
/// - gates on sites 0 and 3, in both orders, each a product of one-site
///   gates, so the state still fits the capacity, and the retained weight
///   (1), the chain and P(One) of every site must be the reference's, as
///   [`CappedTwin`] checks after each gate. An engine that moves a site to
///   bring a gate's sites together holds, on the way, a cut whose rank
///   exceeds that cut's cap (four at cut 1, between the two pairs, under a
///   cap of one), and must not truncate there: the state fits the capacity
///   in the caller's order;
/// - a gate across the narrow cut, which raises its Schmidt rank to two,
///   above its cap of one: an engine that truncates must truncate there,
///   as [`CappedTwin`] checks, so one that caps every cut at the largest
///   bond fails; then every read of the state the cap keeps, and of a fork
///   of it, which must have its retained weight.
pub(super) fn uneven_capacity<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let cap = describe(
        "capacity",
        MpsShape::new(vec![vec![2, 2], vec![2, 2, 1], vec![1, 2, 2], vec![2, 2]]),
    )?;
    let mut pairs = CappedTwin::new(engine, cap)?;
    let when = "two pairs under bonds of two, one and two";
    pairs.apply(&[0], &ry(1.2), when)?;
    pairs.apply(&[0, 1], &cnot(), when)?;
    pairs.apply(&[2], &hadamard(), when)?;
    pairs.apply(&[2, 3], &cnot(), when)?;
    let when = "gates on sites 0 and 3 under bonds of two, one and two";
    pairs.apply(&[0, 3], &kron(&s_h(), &ry(0.7)), when)?;
    pairs.apply(&[3, 0], &kron(&ry(-0.4), &s_h()), when)?;
    let when = "a gate across the narrow cut of bonds of two, one and two";
    pairs.apply(&[1, 2], &cnot(), when)?;
    pairs.compare(when)?;
    let when = "a fork of a state truncated at the narrow cut";
    pairs.fork(when)?.compare(when)
}

/// Two entangled pairs (sites 0 and 1, 2 and 3) under a bond of two at
/// every cut, then a controlled gate on sites 0 and 3, which at most
/// doubles a cut's Schmidt rank (its operator Schmidt rank is two), so cut
/// 1, of rank one before, has rank two after it, and the state still fits
/// the capacity.
/// The gate must truncate nothing, though an engine that brings site 0
/// beside site 3 holds, on the way, rank four at cut 1: one that caps its
/// routing at the capacity's bond loses weight. [`CappedTwin`] checks the
/// retained weight (1), the chain and P(One) of every site after each
/// gate, and every read after the last. [`uneven_capacity`] routes only
/// products of one-site gates, which an engine may apply without routing.
pub(super) fn distant_gate_within_capacity<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let cap = describe("capacity", MpsShape::qubit_chain(4, 2))?;
    let mut pairs = CappedTwin::new(engine, cap)?;
    let when = "two pairs under a bond of two";
    // Pairs not maximally entangled, so that no Schmidt values tie on the
    // way: the tests' double of an engine that caps its routing truncates
    // with `truncated`, which refuses a cut where a kept value ties with a
    // dropped one, so with ties it would truncate nothing.
    pairs.apply(&[0], &ry(1.2), when)?;
    pairs.apply(&[0, 1], &cnot(), when)?;
    pairs.apply(&[2], &ry(0.5), when)?;
    pairs.apply(&[2, 3], &cnot(), when)?;
    let when = "an entangling gate on sites 0 and 3 under a bond of two";
    pairs.apply(&[0, 3], &controlled_y(), when)?;
    pairs.compare(when)
}

/// Gates on distant sites whose results do not fit the capacity, so that
/// an engine that truncates only after gates on neighbours, or does not
/// weigh what its routing drops, is caught:
///
/// - three sites under a bond of one, a gate on sites 0 and 2, then one on
///   sites 2 and 0, each entangling a site with another of a product
///   state: whatever route an engine takes, it keeps the larger Schmidt
///   pair of the two the gate made, so every read and the retained weight
///   (about 0.68, then 0.55) must be the reference's;
/// - three sites under bonds of two and one, a pair on sites 0 and 1, then
///   a gate on sites 0 and 2, which only the cut of bond one cannot hold,
///   so the state kept is the same again; after it, the reference's own
///   rounding leaves some impossible conditions a probability of about
///   10⁻³⁴, which an engine may refuse (`conditionals.rs`);
/// - five sites under a bond of two, entangled by gates on each pair of
///   neighbours, then a gate on sites 3 and 1, which raises an inner cut
///   above rank two: the state an engine keeps may then depend on its
///   routing (the contract leaves it free there), so the kit checks only
///   that the chain fits the capacity and that the weight fell but stays
///   above zero ([`CappedTwin::apply_beyond_capacity`]): the only gate of
///   the kit that truncates a bond above one after a routing.
pub(super) fn wide_gate_truncation<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let cap = describe("capacity", MpsShape::qubit_chain(3, 1))?;
    let mut three = CappedTwin::new(engine, cap)?;
    let when = "a product state of three sites under a bond of one";
    three.apply(&[0], &ry(1.2), when)?;
    three.apply(&[1], &ry(0.5), when)?;
    let when = "a gate on sites 0 and 2 under a bond of one";
    three.apply(&[0, 2], &cnot(), when)?;
    three.compare(when)?;
    three.apply(&[2], &ry(0.9), when)?;
    let when = "a gate on sites 2 and 0 under a bond of one";
    three.apply(&[2, 0], &cnot(), when)?;
    three.compare(when)?;

    let cap = describe(
        "capacity",
        MpsShape::new(vec![vec![2, 2], vec![2, 2, 1], vec![1, 2]]),
    )?;
    let mut uneven = CappedTwin::new(engine, cap)?;
    let when = "a pair beside a site under bonds of two and one";
    uneven.apply(&[0], &ry(1.2), when)?;
    uneven.apply(&[0, 1], &cnot(), when)?;
    uneven.apply(&[2], &ry(0.4), when)?;
    let when = "a gate on sites 0 and 2 under bonds of two and one";
    uneven.apply(&[0, 2], &controlled_y(), when)?;
    uneven.compare(when)?;

    let when = "a gate on sites 3 and 1 of five sites under a bond of two";
    entangled_five(engine)?.apply_beyond_capacity(&[3, 1], &controlled_y(), when)
}

/// Five sites under a bond of two, each turned by its own gate, then
/// entangled by a gate on each pair of neighbours: a state that fits the
/// capacity.
fn entangled_five<E: MpsEngine>(engine: &E) -> Result<CappedTwin<'_, E>, String> {
    let cap = describe("capacity", MpsShape::qubit_chain(5, 2))?;
    let mut five = CappedTwin::new(engine, cap)?;
    let when = "five sites entangled under a bond of two";
    for (site, matrix) in [
        (0, ry(0.9)),
        (1, s_h()),
        (2, ry(1.3)),
        (3, ry(-0.7)),
        (4, ry(0.4)),
    ] {
        five.apply(&[site], &matrix, when)?;
    }
    for (sites, matrix) in [
        ([0, 1], cnot()),
        ([1, 2], cnot()),
        ([2, 3], controlled_y()),
        ([3, 4], cnot()),
    ] {
        five.apply(&sites, &matrix, when)?;
    }
    Ok(five)
}

/// Every operation on a state of one site, and the refusal of the site
/// after it: an engine that pads the chain for its library must neither
/// report nor accept the padding.
pub(super) fn lone_site<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut lone = Twin::new(engine, 1)?;
    lone.compare("the prepared lone site")?;
    lone.same_shape("the prepared lone site")?;
    lone.apply(&[0], &ry(0.7))?;
    lone.apply(&[0], &s_h())?;
    lone.compare("after one-site gates")?;
    lone.same_shape("after one-site gates")?;
    let mut copy = lone.fork()?;
    copy.apply(&[0], &pauli_x())?;
    lone.compare("the original after its fork changed")?;
    copy.compare("the fork")?;
    let seed = 3;
    sampled(
        engine,
        &lone.state,
        &lone.reference,
        &[0],
        SHOTS,
        seed,
        "a lone site",
    )?;
    let beyond = Refusal::SiteOutOfRange { site: 1, sites: 1 };
    refused(
        "apply on site 1 of a lone site",
        engine.apply(&mut lone.state, &[1], &pauli_x()),
        &beyond,
    )?;
    refused(
        "apply on sites 0 and 1 of a lone site",
        engine.apply(&mut lone.state, &[0, 1], &cnot()),
        &beyond,
    )?;
    refused(
        "P(One) of site 1 of a lone site",
        engine.probability_of_one(&lone.state, 1),
        &beyond,
    )?;
    refused(
        "a projection of site 1 of a lone site",
        engine.project(&mut lone.state, 1, false),
        &beyond,
    )?;
    refused(
        "an expectation on site 1 of a lone site",
        engine.expectation(&lone.state, &z_on(1)?),
        &beyond,
    )?;
    refused(
        "a sample of site 1 of a lone site",
        engine.sample(&lone.state, &[0, 1], 3, seed),
        &beyond,
    )?;
    lone.project(0, true)?;
    lone.compare("after projecting the lone site")?;
    lone.bonds_between_rank_and_capacity("after projecting the lone site")
}
