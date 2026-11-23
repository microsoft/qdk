// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a resumer answers a [`QirMachine`] with ([`QirOutcome`]): for
//! each call of a batch that writes a result, the value it gives that
//! result ([`QirReadout`]).
//!
//! They exist so that the machine, which runs a program's classical part
//! but no quantum call, learns each result the program reads, checks for
//! loss or records; and so that a readout that came back with no value
//! can be told from a loss, which QIR can read.

#[cfg(doc)]
use super::{QirCall, QirMachine, QirTrap};

/// The outcome of a call that writes a result ([`QirCall::result`]): the
/// value the call gives the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QirOutcome {
    /// The result the call writes.
    pub result: u32,
    /// The value it gives the result.
    pub value: QirReadout,
}

/// The value a call gives the result it writes, and so the value a result
/// holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QirReadout {
    /// The readout `Zero`; for a loss check, the qubit is not lost.
    Zero,
    /// The readout `One`; for a loss check, the qubit is lost.
    One,
    /// A lost qubit's readout: the result holds `Loss`, which QIR reads as
    /// `Zero` and a loss check (`READ_LOSS`) as lost.
    Loss,
    /// A readout that came back with no value, for a reason other than a
    /// loss (a device's failed readout): QIR has no value for it, so the
    /// result holds none, and an instruction that uses it traps
    /// ([`QirTrap::MissingValue`]).
    Missing,
}
