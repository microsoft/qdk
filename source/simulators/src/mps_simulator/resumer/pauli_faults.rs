// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): Pauli faults injected as
//! gates, to measure how noise changes the shots' histories and the shapes
//! of their batches. The MPS resumer refuses the program's own noise calls;
//! these faults are the resumer's, drawn per shot after each two-site gate.

use crate::qir::{QirGate, QirShotId};

use super::lowering::{Operator, Step};
use super::shot_stream::{SplitMix64, shot_stream};

/// Pauli faults after every two-site gate: on each of its two sites,
/// independently with probability `probability`, a uniformly drawn X, Y
/// or Z, applied as a one-site gate right after it. A shot draws its faults
/// from a stream of its own, derived from `seed` and its id, apart from the
/// stream it draws its measurements from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PauliFaults {
    pub probability: f64,
    pub seed: u64,
}

/// Mixed into the seed, so that the fault stream of a shot differs from
/// its draw stream under the same seed.
const FAULT_SALT: u64 = 0x5041_554C_4946_4C54;

impl PauliFaults {
    /// The stream `shot` draws its faults from.
    pub(crate) fn stream(&self, shot: QirShotId) -> SplitMix64 {
        shot_stream(self.seed ^ FAULT_SALT, shot)
    }

    /// `steps` with the faults `stream` draws inserted after each two-site
    /// gate.
    pub(crate) fn inject(&self, steps: Vec<Step>, stream: &mut SplitMix64) -> Vec<Step> {
        let mut faulted = Vec::with_capacity(steps.len());
        for step in steps {
            let sites = match &step {
                Step::Apply(operator) if operator.sites().len() == 2 => Some(operator.site_slots),
                _ => None,
            };
            faulted.push(step);
            for site in sites.into_iter().flatten() {
                if stream.next_uniform() < self.probability {
                    let pauli = match stream.next_u64() % 3 {
                        0 => QirGate::X,
                        1 => QirGate::Y,
                        _ => QirGate::Z,
                    };
                    faulted.push(Step::Apply(Operator {
                        site_slots: [site, 0],
                        matrix: pauli.matrix_at(None),
                    }));
                }
            }
        }
        faulted
    }
}
