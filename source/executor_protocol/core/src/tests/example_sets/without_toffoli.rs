// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`WithoutToffoli`]: `h`, `cx`, `t`, `tdg` and `mz`, with no
//! three-qubit gate.

use std::convert::Infallible;

use super::{BitReadout, ShotFlags};
use super::{QubitBeyond, Qubits};
use crate::{BoundaryCompatible, Capabilities, Features, InstructionSet, ReadoutSlots, Resources};

/// `h`, `cx`, `t`, `tdg` and `mz`, on the qubits a receiver states
/// ([`Qubits`]): the set a decomposition's tests decompose into. Any two fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WithoutToffoli;

/// A gate of [`WithoutToffoli`], on its qubits: `Cx(control, target)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CliffordT {
    H(u32),
    Cx(u32, u32),
    T(u32),
    Tdg(u32),
    Mz(u32),
}

impl CliffordT {
    fn qubits(self) -> impl Iterator<Item = u32> {
        let (first, second) = match self {
            Self::H(q) | Self::T(q) | Self::Tdg(q) | Self::Mz(q) => (q, None),
            Self::Cx(control, target) => (control, Some(target)),
        };
        std::iter::once(first).chain(second)
    }
}

impl BoundaryCompatible for WithoutToffoli {
    type Fit = ();
    type Mismatch = Infallible;

    fn fits(&self, _accepted: &Self) -> Result<(), Infallible> {
        Ok(())
    }
}

impl InstructionSet for WithoutToffoli {
    type Instruction = CliffordT;
    type Capabilities = Qubits;
    type Requirement = u32;
    type Fault = QubitBeyond;
    type Readout = BitReadout;
    type ShotConditions = ShotFlags;
    const UNWRITTEN_READOUT: BitReadout = BitReadout::Unwritten;

    fn unconstrained() -> &'static Capabilities<Self> {
        static UNCONSTRAINED: Capabilities<WithoutToffoli> = Capabilities::new(
            Resources::unconstrained(),
            Features::unconstrained(),
            Qubits(u32::MAX),
        );
        &UNCONSTRAINED
    }

    fn readout_slots(&self, gate: &CliffordT) -> ReadoutSlots {
        u32::from(matches!(gate, CliffordT::Mz(_)))
    }

    fn requirement(&self, required: &Qubits) -> u32 {
        required.0
    }

    fn check(&self, gate: &CliffordT, qubits: &u32) -> Result<ReadoutSlots, QubitBeyond> {
        match gate.qubits().find(|qubit| qubit >= qubits) {
            Some(qubit) => Err(QubitBeyond {
                qubit,
                qubits: *qubits,
            }),
            None => Ok(self.readout_slots(gate)),
        }
    }
}
