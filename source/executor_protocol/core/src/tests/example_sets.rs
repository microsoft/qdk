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
//! - [`Resettable`]: the same gate, whose capabilities are two independent
//!   needs, qubits and a reset, its mismatch split per need. What the tests
//!   of a fault naming the part that stated each need send.
//! - [`WithToffoli`] and [`WithoutToffoli`]: `h`, a three-qubit `ccz` and
//!   `mz`, and Clifford+T (`h`, `cx`, `t`, `tdg` and `mz`), what a `ccz`
//!   decomposes into. What the tests of a translating adapter send.
//! - [`Wide`]: each instruction the count of readout slots it writes. What
//!   the tests of a batch's readout slots beyond a `u32` send.
//!
//! Each is built only from the crate's public items, as a user's set is,
//! and all read back a [`BitReadout`] per readout slot and note
//! [`ShotFlags`] on a shot.

mod bit_readout;
mod measuring;
mod qubit_limited;
mod resettable;
mod shot_flags;
mod wide;
mod with_toffoli;
mod without_toffoli;

pub(crate) use bit_readout::BitReadout;
pub(crate) use shot_flags::ShotFlags;

pub(crate) use measuring::{Gate, Measuring, NoCapabilities, UnacceptedNames, UnknownGate};
pub(crate) use qubit_limited::{
    OnQubit, QubitBeyond, QubitLimited, Qubits, TooFewQubits, on_qubits,
};
pub(crate) use resettable::{QubitsAndReset, QubitsAndResetShort, Resettable, qubits_and_reset};
pub(crate) use wide::Wide;
pub(crate) use with_toffoli::{ToffoliGate, WithToffoli};
pub(crate) use without_toffoli::{CliffordT, WithoutToffoli};
