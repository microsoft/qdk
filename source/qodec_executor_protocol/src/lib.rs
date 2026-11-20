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
//! bracketed, as long as no call that reaches a part, the program's or the
//! layers' own, prepares a block at the index of a live block of another
//! type: Qodec has one label per block, whatever its type, so such a call
//! (a `surface` patch prepared at the index of a live `merged` one) fails,
//! naming both blocks ([`TranslationFault`]). The layers make such a call
//! themselves below a seam of several block types whose lower ISA has no
//! release instruction: Qodec's encoder releases a block there by taking
//! its label back, while the block stays live below, and may prepare a
//! block of another type at that label. Where the seam is a part's
//! boundary (the bottom part's, or that of an adapter of the layers below
//! it), that part fails the call too, and the shot is lost, where Qodec's
//! runtime, and an adapter holding the layers on both sides of the seam,
//! end the old block and run on. So at such a seam the readouts depend on
//! the bracketing; none of Qodec's examples has one.
//! The parts keep the rules of Qodec's runtime, not those of Qodec's drivers,
//! which first check each batch's types (Qodec's `TypeChecker`, not
//! ported): a call on a block that is not live loses the shot here, where
//! such a driver rejects its batch and keeps the shot.

mod action_simulator_executor;
mod qodec_instruction_set;
mod qodec_layer_adapter;

pub use action_simulator_executor::ActionSimulatorExecutor;
pub use qodec_instruction_set::{QodecInstructionSet, TranslationFault};
pub use qodec_layer_adapter::{QodecLayerAdapter, QodecLayerAdapterError};
