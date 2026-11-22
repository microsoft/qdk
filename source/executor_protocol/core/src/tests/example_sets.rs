// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The instruction sets the core's tests run on, each made for the rules
//! one group of tests checks, so that the protocol is tested apart from any
//! set a user writes:
//!
//! - [`Measuring`]: gates named by string, of which `mz` writes a readout
//!   slot; a sender fits a receiver that accepts every name it sends. What
//!   the composition, run and batch tests send.
//! - [`QubitLimited`]: one gate on one qubit, whose capabilities are a count
//!   of qubits. What the tests of a set's own capabilities, and of a checked
//!   batch's faults, send.
//! - [`WithToffoli`] and [`WithoutToffoli`]: the same gates, the first with
//!   a three-qubit `ccz`, the second without it. What the tests of a
//!   lowering send.
//!
//! Each is built only from the crate's public items, as a user's set is.

mod measuring;
mod qubit_limited;
mod with_toffoli;
mod without_toffoli;

pub(crate) use measuring::{Gate, Measuring, NoCapabilities, UnacceptedNames, UnknownGate};
pub(crate) use qubit_limited::{
    OnQubit, QubitBeyond, QubitLimited, Qubits, TooFewQubits, on_qubits,
};
pub(crate) use with_toffoli::{ToffoliGate, WithToffoli};
pub(crate) use without_toffoli::{CliffordT, WithoutToffoli};
