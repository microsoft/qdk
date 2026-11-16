// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Action simulators: [`ActionBackend`]s that run the steps of qodec
//! actions on qubits, so a qodec runs without a backend written for its
//! bottom ISA.
//!
//! ```text
//!   │ Vec<Call>   bottom ISA       ▲ Result<Vec<BasicReadout>, String>
//!  ┌▼──────────────────────────────┴┐
//!  │ ActionTranslator               │  (qodec_runtime_pieces)
//!  └┬──────────────────────────────▲┘
//!   │ Vec<ActionOp>                │ Result<Vec<bool>, String>
//!  ┌▼──────────────────────────────┴┐
//!  │ StabilizerActionSimulator  or  │  (this module)
//!  │ FullStateActionSimulator       │
//!  └────────────────────────────────┘
//! ```
//!
//! | Simulator | State | Runs |
//! |---|---|---|
//! | [`StabilizerActionSimulator`] | a `paulimer` Clifford frame | Paulis, Cliffords, measurements, preparations, rotations by multiples of π/2 |
//! | [`FullStateActionSimulator`] | a state vector ([`cpu_full_state_simulator`](crate::cpu_full_state_simulator)) | all of that, and any rotation |
//!
//! Both are noiseless, and lose no qubit. Both prepare a `stabilize` step's
//! state the same way: they measure each Pauli, and flip a `-1` outcome with
//! a Pauli that anticommutes with that Pauli alone.
//!
//! [`ActionBackend`]: qodec_runtime_interface::ActionBackend

mod full_state;
mod paulis;
mod stabilizer;

pub use full_state::FullStateActionSimulator;
pub use stabilizer::StabilizerActionSimulator;

#[cfg(test)]
mod tests;
