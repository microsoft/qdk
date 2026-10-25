// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tensor-network simulator: matrix product states over any
//! [`tensornet::MpsEngine`], run through the executor protocol.
//!
//! The engine (`tensornet`, implemented on the GPU by `qdk_cutensornet` and
//! exactly by [`tensornet::reference::ExactReference`]) knows matrices on
//! sites; the protocol (`executor_protocol`) knows shots, sessions and
//! outcomes; `qdk_simulators::execution` knows programs, their walk and the
//! region ISA. This crate joins them:
//!
//! ```text
//! MpsStack (stack.rs)                    M1's stack, for ⑤′: Sample and state queries
//!   ├─ program.rs                        M0's preflight; each shot's row replayed
//!   │                                    through the walk (RuntimeAdapter)
//!   ├─ MpsBatchSimulator (batch.rs)      a ShotStrategy over the region ISA: one
//!   │                                    MPS, every shot sampled from it at once
//!   ├─ Evaluate (queries.rs)             state queries on ψ, outside the shot protocol
//!   └─ per_shot: PerShot(TensorNetShots) one TensorNetTerminal (terminal.rs) per
//!                                        shot, an L3 executor of the region ISA
//! lowering.rs                            region gates to matrices (③'s tables)
//! capacity.rs                            the capacity for a qubit count and a bond cap
//! ```
//!
//! M1 samples Base programs with the batch strategy, as M0's ④ did
//! (`run_mps_shots`), and answers state queries as M0's
//! `evaluate_state_queries`; the per-shot terminal is the one `mps-a` runs
//! Adaptive programs with. See the README for each type's role and uses.

mod batch;
mod capacity;
mod lowering;
mod program;
mod queries;
mod stack;
mod terminal;

#[cfg(test)]
mod tests;

pub use batch::{MpsBatchReport, MpsBatchSession, MpsBatchSimulator, sampler_seed};
pub use capacity::mps_capacity;
pub use program::ProgramError;
pub use queries::{Evaluate, MpsCost, QueryError, StateQuery, StateQueryValue};
pub use stack::{DEFAULT_MAX_BOND, MpsStack, StackError};
pub use terminal::{TensorNetReport, TensorNetShots, TensorNetTerminal};
