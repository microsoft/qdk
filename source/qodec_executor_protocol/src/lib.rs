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
//! bracketed.

mod action_simulator_executor;
mod qodec_instruction_set;
mod qodec_layer_adapter;

pub use action_simulator_executor::ActionSimulatorExecutor;
pub use qodec_instruction_set::{QodecInstructionSet, TranslationFault};
pub use qodec_layer_adapter::{QodecLayerAdapter, QodecLayerAdapterError};
