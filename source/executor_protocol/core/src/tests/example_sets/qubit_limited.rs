// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`QubitLimited`]: one gate on one qubit, whose capabilities are a count
//! of qubits; and that count, [`Qubits`], which the translating tests'
//! sets state too.

use std::convert::Infallible;
use std::error::Error;
use std::fmt;

use super::{BitReadout, ShotFlags};
use crate::{BoundaryCompatible, Capabilities, Features, InstructionSet, ReadoutSlots, Resources};

/// One gate, on one qubit: a receiver states the qubits it holds
/// ([`Qubits`]), and refuses a gate on a qubit beyond them. Any two
/// `QubitLimited` sets fit, binding nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QubitLimited;

/// The gate on a qubit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OnQubit(pub(crate) u32);

/// A count of qubits: those a receiver holds, or those a sender requires.
/// Unconstrained, it is `u32::MAX`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Qubits(pub(crate) u32);

/// The capabilities of `n` qubits, and any resources and features.
pub(crate) fn on_qubits<S: InstructionSet<Capabilities = Qubits>>(n: u32) -> Capabilities<S> {
    Capabilities::new(
        Resources::unconstrained(),
        Features::unconstrained(),
        Qubits(n),
    )
}

impl BoundaryCompatible for Qubits {
    type Fit = ();
    type Mismatch = TooFewQubits;

    fn fits(&self, provided: &Self) -> Result<(), TooFewQubits> {
        if self.0 <= provided.0 {
            Ok(())
        } else {
            Err(TooFewQubits {
                required: self.0,
                provided: provided.0,
            })
        }
    }
}

impl BoundaryCompatible for QubitLimited {
    type Fit = ();
    type Mismatch = Infallible;

    fn fits(&self, _accepted: &Self) -> Result<(), Infallible> {
        Ok(())
    }
}

impl InstructionSet for QubitLimited {
    type Instruction = OnQubit;
    type Capabilities = Qubits;
    type Requirement = u32;
    type Fault = QubitBeyond;
    type Readout = BitReadout;
    type ShotConditions = ShotFlags;
    const UNWRITTEN_READOUT: BitReadout = BitReadout::Unwritten;

    fn unconstrained() -> &'static Capabilities<Self> {
        static UNCONSTRAINED: Capabilities<QubitLimited> = Capabilities::new(
            Resources::unconstrained(),
            Features::unconstrained(),
            Qubits(u32::MAX),
        );
        &UNCONSTRAINED
    }

    fn readout_slots(&self, _gate: &OnQubit) -> ReadoutSlots {
        0
    }

    fn requirement(&self, required: &Qubits) -> u32 {
        required.0
    }

    fn check(&self, gate: &OnQubit, qubits: &u32) -> Result<ReadoutSlots, QubitBeyond> {
        if gate.0 < *qubits {
            Ok(0)
        } else {
            Err(QubitBeyond {
                qubit: gate.0,
                qubits: *qubits,
            })
        }
    }
}

/// More qubits required than provided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TooFewQubits {
    pub(crate) required: u32,
    pub(crate) provided: u32,
}

impl fmt::Display for TooFewQubits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "qubits: {} required, {} provided",
            self.required, self.provided
        )
    }
}

impl Error for TooFewQubits {}

/// A gate on a qubit beyond those required.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QubitBeyond {
    pub(crate) qubit: u32,
    pub(crate) qubits: u32,
}

impl fmt::Display for QubitBeyond {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a gate on qubit {} beyond the {} required",
            self.qubit, self.qubits
        )
    }
}

impl Error for QubitBeyond {}
