// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`WithToffoli`]: `h`, `mz`, and the three-qubit `ccz`.

use std::convert::Infallible;

use super::{BitReadout, ShotFlags};
use super::{QubitBeyond, Qubits};
use crate::{BoundaryCompatible, Capabilities, Features, InstructionSet, ReadoutSlots, Resources};

/// `h`, `mz` and the three-qubit `ccz`, on the qubits a receiver states
/// ([`Qubits`]): the set a lowering's tests lower from. Any two fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WithToffoli;

/// A gate of [`WithToffoli`], on its qubits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToffoliGate {
    H(u32),
    Ccz(u32, u32, u32),
    Mz(u32),
}

impl ToffoliGate {
    fn qubits(self) -> impl Iterator<Item = u32> {
        let (first, more) = match self {
            Self::H(q) | Self::Mz(q) => (q, None),
            Self::Ccz(a, b, c) => (a, Some([b, c])),
        };
        std::iter::once(first).chain(more.into_iter().flatten())
    }
}

impl BoundaryCompatible for WithToffoli {
    type Fit = ();
    type Mismatch = Infallible;

    fn fits(&self, _accepted: &Self) -> Result<(), Infallible> {
        Ok(())
    }
}

impl InstructionSet for WithToffoli {
    type Instruction = ToffoliGate;
    type Capabilities = Qubits;
    type Requirement = u32;
    type Fault = QubitBeyond;
    type Readout = BitReadout;
    type ShotConditions = ShotFlags;
    const UNWRITTEN_READOUT: BitReadout = BitReadout::Unwritten;

    fn unconstrained() -> &'static Capabilities<Self> {
        static UNCONSTRAINED: Capabilities<WithToffoli> = Capabilities::new(
            Resources::unconstrained(),
            Features::unconstrained(),
            Qubits(u32::MAX),
        );
        &UNCONSTRAINED
    }

    fn readout_slots(&self, gate: &ToffoliGate) -> ReadoutSlots {
        u32::from(matches!(gate, ToffoliGate::Mz(_)))
    }

    fn requirement(&self, required: &Qubits) -> u32 {
        required.0
    }

    fn check(&self, gate: &ToffoliGate, qubits: &u32) -> Result<ReadoutSlots, QubitBeyond> {
        match gate.qubits().find(|qubit| qubit >= qubits) {
            Some(qubit) => Err(QubitBeyond {
                qubit,
                qubits: *qubits,
            }),
            None => Ok(self.readout_slots(gate)),
        }
    }
}
