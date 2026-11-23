// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The parts the bench composes, each role's in a file of its own, and the
//! instruction sets they speak:
//!
//! - drivers: [`ProgramInBatches`], which sends a program in batches, and
//!   [`Direct`], which sends it past the chain, the baseline;
//! - adapters: [`Forward`], which forwards every batch;
//! - executors: [`Profiler`], which counts what it gets, and
//!   [`FromWorker`], which answers from a worker thread.

mod adapters;
mod drivers;
mod executors;

pub(crate) use adapters::*;
pub(crate) use drivers::*;
pub(crate) use executors::*;

use block_instruction_set::instruction_set;

instruction_set! {
    /// The gates of the mixes: QIR's, without measurements, whose draws
    /// cost the same on every chain, and an `x` a readout parity controls,
    /// a feed-forward correction.
    pub(crate) MixGates {
        CCX = ccx(qubit, qubit, qubit),
        CX = cx(qubit, qubit),
        CZ = cz(qubit, qubit),
        RZ = rz(qubit; theta: Number),
        H = h(qubit),
        S = s(qubit),
        T = t(qubit),
        T_ADJ = t_adj(qubit),
        X = x(qubit),
        SWAP = swap(qubit, qubit),
        RESET = reset(qubit),
        X_IF = x_if(qubit; c: Bit),
    }
}
