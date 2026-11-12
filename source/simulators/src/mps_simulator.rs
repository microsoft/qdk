// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The MPS simulator: runs QIR programs on matrix product states (MPS), on
//! any engine of the `tensornet` contract (for instance `tensornet`'s exact
//! reference, or one over cuTensorNet, NVIDIA's tensor network library, on
//! a GPU).
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
//! each shot on an MPS that an engine holds. Everything that maps qubits to
//! sites is here, in `mps_simulator/`: [`MpsResumer`] and its settings in
//! `resumer.rs`, and what it runs a program with in `resumer/`, one file
//! per part, each saying in its header what it holds.
//!
//! Shots that drew the same values share one state, forked where their
//! draws differ, so a run costs what its distinct histories cost; each shot
//! draws from its own stream, so its records do not depend on the shots
//! beside it.
//!
//! An overview, with the module's files and which uses which, the phases of
//! a run and an example, is in `src/mps_simulator/README.md`; the calls
//! each phase makes, in `src/mps_simulator/design.md`.
//!
//! [`QirResumer`]: crate::qir::QirResumer

pub(crate) mod resumer;

pub use resumer::report::{MpsRunReport, TRUNCATION_TOLERANCE, TruncationWarning};
pub use resumer::resumer_error::MpsResumerError;
pub use resumer::rewrites::fusion::GateFusion;
pub use resumer::{DEFAULT_MAX_BOND_DIMENSION, MpsResumer};
