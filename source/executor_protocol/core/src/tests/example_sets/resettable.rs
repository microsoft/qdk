// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Resettable`]: one gate on one qubit, whose capabilities are two
//! independent needs, qubits and a mid-circuit reset
//! ([`QubitsAndReset`]), and whose mismatch splits into one piece per need.

use std::convert::Infallible;
use std::error::Error;
use std::fmt;

use super::{BitReadout, OnQubit, Qubits, ShotFlags, TooFewQubits};
use crate::{
    BoundaryCompatible, Capabilities, Features, InstructionSet, ReadoutSlots, Resources, SameNeed,
};

/// One gate on one qubit, whose capabilities state qubits and a reset
/// ([`QubitsAndReset`]); it refuses no gate. Any two `Resettable` sets
/// fit, binding nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Resettable;

/// A count of qubits, and whether a qubit is reset mid-circuit: needed, as
/// a requirement; held, as a provision. Needing nothing, it is no qubit
/// and no reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QubitsAndReset {
    pub(crate) qubits: u32,
    pub(crate) reset: bool,
}

/// The capabilities of `qubits` qubits, with a reset or not, and any
/// resources and features.
pub(crate) fn qubits_and_reset(qubits: u32, reset: bool) -> Capabilities<Resettable> {
    Capabilities::new(
        Resources::nothing_stated(),
        Features::nothing_stated(),
        QubitsAndReset { qubits, reset },
    )
}

impl BoundaryCompatible for QubitsAndReset {
    type Fit = ();
    type Mismatch = QubitsAndResetShort;

    fn fits(&self, provided: &Self) -> Result<(), QubitsAndResetShort> {
        let short = QubitsAndResetShort {
            qubits: Qubits(self.qubits).fits(&Qubits(provided.qubits)).err(),
            reset: self.reset && !provided.reset,
        };
        if short == QubitsAndResetShort::default() {
            Ok(())
        } else {
            Err(short)
        }
    }
}

impl BoundaryCompatible for Resettable {
    type Fit = ();
    type Mismatch = Infallible;

    fn fits(&self, _accepted: &Self) -> Result<(), Infallible> {
        Ok(())
    }
}

impl InstructionSet for Resettable {
    type Instruction = OnQubit;
    type SetCapabilities = QubitsAndReset;
    type Requirement = ();
    type Fault = Infallible;
    type Readout = BitReadout;
    type ShotConditions = ShotFlags;
    const UNWRITTEN_READOUT: BitReadout = BitReadout::Unwritten;

    fn needs_nothing() -> QubitsAndReset {
        QubitsAndReset {
            qubits: 0,
            reset: false,
        }
    }

    /// One piece for the qubits, stated alike by an equal count, and one
    /// for the reset, stated alike by an equal need.
    fn split_mismatch(short: QubitsAndResetShort) -> Vec<(QubitsAndResetShort, SameNeed<Self>)> {
        let qubits = short.qubits.map(|qubits| {
            let piece = QubitsAndResetShort {
                qubits: Some(qubits),
                reset: false,
            };
            let same: SameNeed<Self> = Box::new(|a, b| a.qubits == b.qubits);
            (piece, same)
        });
        let reset = short.reset.then(|| {
            let piece = QubitsAndResetShort {
                qubits: None,
                reset: true,
            };
            let same: SameNeed<Self> = Box::new(|a, b| a.reset == b.reset);
            (piece, same)
        });
        qubits.into_iter().chain(reset).collect()
    }

    fn readout_slots(&self, _gate: &OnQubit) -> ReadoutSlots {
        0
    }

    fn requirement(&self, _required: &QubitsAndReset) {}

    fn check(&self, _gate: &OnQubit, (): &()) -> Result<ReadoutSlots, Infallible> {
        Ok(0)
    }
}

/// How qubits and a reset fall short: too few qubits, a reset needed and
/// not held, or both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct QubitsAndResetShort {
    pub(crate) qubits: Option<TooFewQubits>,
    pub(crate) reset: bool,
}

/// Each need that falls short, joined by `; `.
impl fmt::Display for QubitsAndResetShort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reset = self.reset.then_some("reset: required, not provided");
        let qubits = self.qubits.map(|qubits| qubits.to_string());
        let needs: Vec<String> = qubits.into_iter().chain(reset.map(String::from)).collect();
        f.write_str(&needs.join("; "))
    }
}

impl Error for QubitsAndResetShort {}
