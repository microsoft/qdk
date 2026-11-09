// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The MPS simulator: runs QIR programs on matrix product states (MPS), on
//! any engine of the `tensornet` contract (for instance `tensornet`'s exact
//! reference, or cuTensorNet's on a GPU).
//!
//! An MPS simulates many more qubits than a state vector when a program
//! entangles them little, as chemistry programs do. A caller builds an
//! [`MpsResumer`] over an engine and runs a program's shots on it, as on
//! any [`QirResumer`]; it may rely on each shot recording what it would
//! record alone, and on a report of what truncation cost.
//!
//! [`qir`](crate::qir) runs a program's classical part and hands each
//! shot's quantum calls to a resumer ([`QirResumer`]); an engine
//! ([`MpsEngine`](tensornet::MpsEngine)) holds states and knows only sites
//! and matrices. This module joins them: [`MpsResumer`] runs the calls of
//! each shot on an MPS that an engine holds. Everything that knows qubits
//! is here: which site holds each qubit (an order chosen from the
//! program's two-qubit gates), the gates as matrices, gates multiplied into
//! fewer operators ([`GateFusion`]), the max bond dimension χ, each shot's
//! random draws, and the warning when truncation lost weight
//! ([`MpsRunReport`]).
//!
//! Shots that drew the same values share one state, forked where their
//! draws differ, so a run costs what its distinct histories cost; each shot
//! draws from its own stream, so its records do not depend on the shots
//! beside it.
//!
//! Its files, in `mps_simulator/`:
//!
//! - `resumer.rs`: [`MpsResumer`], its settings and the grouped run, with
//!   `resumer/lowering.rs` (a batch's calls as the engine's steps, on
//!   sites), `resumer/order.rs` (the site of each qubit) and
//!   `resumer/splitmix64.rs` (the generator the shots draw from);
//! - `fusion.rs`: [`GateFusion`], runs of gates as one- and two-site
//!   operators;
//! - `report.rs`: [`MpsRunReport`] and [`TruncationWarning`];
//! - `error.rs`: [`MpsResumerError`].
//!
//! An overview, with an example, is in `src/mps_simulator/README.md`.
//!
//! [`QirResumer`]: crate::qir::QirResumer

mod error;
mod fusion;
mod report;
pub(crate) mod resumer;

pub use error::MpsResumerError;
pub use fusion::GateFusion;
pub use report::{MpsRunReport, TRUNCATION_TOLERANCE, TruncationWarning};
pub use resumer::{DEFAULT_MAX_BOND_DIMENSION, MpsResumer};
