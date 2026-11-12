// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The cases of projections and forks: what a projection leaves, and a
//! fork independent of its original.

use super::conditionals::{TINY_ONES, tiny_one};
use super::gates::{hadamard, pauli_x};
use super::twin::{Twin, refused, scrambled};
use super::{cnot, controlled_y};
use crate::{MpsEngine, Refusal};

/// Projections: on the scrambled state, then gates after them, so that an
/// engine that caches a computed state must recompute it, with each
/// site's P(One), every Pauli string's expectation, and each bond between
/// the Schmidt rank and the capacity; a projection of probability zero,
/// which must be refused and leave the state unchanged; and ones of
/// probability 1e-18 and 1e-200 ([`TINY_ONES`]), which must be accepted.
pub(super) fn project<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut twin = scrambled(engine)?;
    twin.project(1, true)?;
    twin.compare("after projecting site 1 on One")?;
    twin.bonds_between_rank_and_capacity("after projecting site 1 on One")?;
    twin.apply(&[1], &hadamard())?;
    twin.apply(&[1, 2], &controlled_y())?;
    twin.compare("after gates following the projection")?;
    twin.project(2, false)?;
    twin.project(0, true)?;
    twin.compare("after two more projections")?;
    twin.bonds_between_rank_and_capacity("after two more projections")?;
    // Site 2 is untouched: its P(One) is a product with an exact zero.
    let mut bell = Twin::new(engine, 3)?;
    bell.apply(&[0], &hadamard())?;
    bell.apply(&[0, 1], &cnot())?;
    refused(
        "a projection of site 2, in |0⟩, on One",
        engine.project(&mut bell.state, 2, true),
        &Refusal::ZeroProbabilityOutcome,
    )?;
    bell.compare("after a refused projection")?;
    for (probability, angle) in TINY_ONES {
        let mut tiny = tiny_one(engine, angle)?;
        tiny.project(0, true)?;
        tiny.compare(&format!(
            "after projecting site 0 on One, of probability {probability}"
        ))?;
    }
    Ok(())
}

/// Forks: a fresh fork reads what its original reads and has its chain;
/// then gates and a projection on either, and on a fork of a fork, leave
/// the others' reads unchanged.
pub(super) fn fork<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut original = scrambled(engine)?;
    let mut copy = original.fork()?;
    copy.compare("a fresh fork")?;
    copy.same_shape("a fresh fork")?;
    copy.apply(&[0], &pauli_x())?;
    copy.project(2, true)?;
    original.same_shape("the original after its fork changed")?;
    copy.compare("the fork")?;
    original.apply(&[2, 0], &controlled_y())?;
    copy.compare("the fork after its original changed")?;
    original.compare("the original after changing")?;
    let mut again = copy.fork()?;
    again.apply(&[1, 2], &cnot())?;
    again.compare("a fork of a fork")?;
    copy.compare("a fork after its own fork changed")
}
