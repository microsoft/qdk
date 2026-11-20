// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A qodec's layers, and the action simulators that run its bottom ISA, as
//! parts of an executor-protocol chain.
//!
//! Qodec's runtime lowers a program, a batch of calls at a time, through a
//! qodec's layers down to a bottom ISA, each layer calling the one below as
//! often as it needs. This crate makes those layers an adapter of the
//! protocol, so that a driver of the qodec's top ISA, the layers, bracketed
//! as a user chooses, and an executor of its bottom ISA compose with `>>`:
//!
//! - [`QodecLayerAdapter`]: any run of consecutive layers of a qodec,
//!   accepting the ISA above them and sending the one below;
//! - [`ActionSimulatorExecutor`]: a qodec's bottom ISA, run on
//!   `qdk_simulators`' action simulators;
//! - [`QodecInstructionSet`]: a Qodec ISA as the protocol's instruction
//!   set, which both share.
//!
//! A chain of them gives the readouts the qodec's runtime gives driven
//! directly over the same simulator, seeded alike, however the layers are
//! bracketed, for a program that never prepares a block at the index of a
//! live block of another type: Qodec has one label per block, whatever its
//! type, so such a call (a `surface` patch prepared at the index of a live
//! `merged` one) fails, naming both blocks ([`TranslationFault`]). The
//! parts keep the rules of Qodec's runtime, not those of Qodec's drivers,
//! which first check each batch's types (Qodec's `TypeChecker`, not
//! ported): a call on a block that is not live loses the shot here, where
//! such a driver rejects its batch and keeps the shot.

mod action_simulator_executor;
mod qodec_instruction_set;
mod qodec_layer_adapter;

pub use action_simulator_executor::ActionSimulatorExecutor;
pub use qodec_instruction_set::{QodecInstructionSet, TranslationFault};
pub use qodec_layer_adapter::{QodecLayerAdapter, QodecLayerAdapterError};
