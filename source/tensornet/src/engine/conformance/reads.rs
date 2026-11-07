// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The cases of reads of states no truncation touched: P(One), every
//! Pauli string's expectation and the chain, after gates on neighbours, on
//! distant sites and on three sites.

use super::gates::{c, hadamard, toffoli};
use super::twin::{REFERENCE, Twin, close_complex, describe, scrambled, unsupported};
use super::{cnot, controlled_y, kron, product, ry, s_h};
use crate::{MpsEngine, PauliSum};

/// A Bell pair, the smallest entangled state, then each site's P(One),
/// every Pauli string's expectation, and the expectation of a multiple of
/// the identity alone, whose value is its complex coefficient: an engine
/// must not drop the identity's coefficient or its imaginary part.
pub(super) fn bell<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut twin = Twin::new(engine, 2)?;
    twin.apply(&[0], &hadamard())?;
    twin.apply(&[0, 1], &cnot())?;
    twin.compare("bell")?;
    let identity = describe("observable", PauliSum::new(c(0.25, -0.5), Vec::new()))?;
    let value = describe(
        "engine expectation",
        engine.expectation(&twin.state, &identity),
    )?;
    close_complex("⟨(0.25 − 0.5i)·I⟩", value, c(0.25, -0.5))
}

/// The scrambled state (complex gates, on sites far apart and in both
/// orders), then each site's P(One) and every Pauli string's expectation:
/// a wrong layout of a gate's matrix (rows for columns, or the bits of its
/// sites swapped) changes some read.
pub(super) fn layout<E: MpsEngine>(engine: &E) -> Result<(), String> {
    scrambled(engine)?.compare("scrambled")
}

/// Gates on sites far apart in the chain, in both orders, then every read
/// and the chain: an engine that moves sites to bring a gate's sites
/// together (its routing) must leave every answer in the caller's sites.
pub(super) fn distant_sites<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut twin = Twin::new(engine, 5)?;
    twin.apply(&[0], &ry(0.7))?;
    twin.apply(&[2], &s_h())?;
    twin.apply(&[4], &ry(-1.3))?;
    twin.apply(&[0, 4], &controlled_y())?;
    twin.compare("after a gate on sites 0 and 4")?;
    twin.same_shape("after a gate on sites 0 and 4")?;
    twin.apply(&[4, 1], &cnot())?;
    twin.apply(&[3], &s_h())?;
    twin.apply(&[3, 0], &controlled_y())?;
    twin.compare("after gates on sites 4 and 1, then 3 and 0")?;
    twin.same_shape("after gates on sites 4 and 1, then 3 and 0")
}

/// A gate on three sites, not neighbours and in an order that cycles them
/// (one that only swaps two would be its own inverse), of an entangled
/// state: neither symmetric nor real, so a wrong layout of its matrix or a
/// wrong order of its sites, inverted included, is caught. An engine may
/// fail it, as one that applies operators on at most two sites does, but
/// not refuse it, since it breaks no rule; when the engine applies it,
/// every read and the chain must be the reference's.
pub(super) fn three_site_gate<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut twin = Twin::new(engine, 4)?;
    twin.apply(&[0], &ry(0.7))?;
    twin.apply(&[1], &s_h())?;
    twin.apply(&[0, 1], &cnot())?;
    twin.apply(&[2], &ry(-1.1))?;
    twin.apply(&[2, 3], &controlled_y())?;
    twin.apply(&[1, 2], &cnot())?;
    let sites = [3, 0, 2];
    let gate = product(&toffoli(), &kron(&kron(&ry(0.4), &s_h()), &ry(-1.2)), 8);
    if let Err(error) = twin.engine.apply(&mut twin.state, &sites, &gate) {
        return unsupported(&error, &sites, "an entangled state of four sites");
    }
    describe(
        "reference apply",
        REFERENCE.apply(&mut twin.reference, &sites, &gate),
    )?;
    twin.compare("after a gate on sites [3, 0, 2]")?;
    twin.same_shape("after a gate on sites [3, 0, 2]")
}
