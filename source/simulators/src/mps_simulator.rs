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
//! is here, in `mps_simulator/`:
//!
//! - `resumer.rs`: [`MpsResumer`] and its settings (the max bond dimension
//!   χ among them), and in `resumer/` what it runs a program with:
//!   - `grouped_run.rs`: the run of a resumer's shots in groups that share
//!     a state;
//!   - `qubit_order.rs`: which site holds each qubit, an order chosen from
//!     the program's two-qubit gates;
//!   - `lowering.rs`: a batch's calls as the engine's steps on sites, the
//!     gates as matrices;
//!   - `fusion.rs`: [`GateFusion`], gates multiplied into fewer one- and
//!     two-site operators;
//!   - `shot_stream.rs`: the stream each shot draws from, seeded as the
//!     executor protocol will seed it;
//!   - `report.rs`: [`MpsRunReport`] and [`TruncationWarning`], what a run
//!     cost and the warning when truncation lost weight;
//!   - `resumer_error.rs`: [`MpsResumerError`].
//!
//! Shots that drew the same values share one state, forked where their
//! draws differ, so a run costs what its distinct histories cost; each shot
//! draws from its own stream, so its records do not depend on the shots
//! beside it.
//!
//! An overview, with the phases of a run and an example, is in
//! `src/mps_simulator/README.md`; the calls each phase makes, in
//! `src/mps_simulator/design.md`.
//!
//! [`QirResumer`]: crate::qir::QirResumer

pub(crate) mod resumer;

pub use resumer::fusion::GateFusion;
pub use resumer::report::{MpsRunReport, TRUNCATION_TOLERANCE, TruncationWarning};
pub use resumer::resumer_error::MpsResumerError;
pub use resumer::{DEFAULT_MAX_BOND_DIMENSION, MpsResumer};
