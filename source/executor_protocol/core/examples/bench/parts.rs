// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The parts the bench composes, each role's in a file of its own, and the
//! instruction sets they speak:
//!
//! - drivers: [`ProgramInBatches`], which sends a program in batches, and
//!   [`Direct`], which sends it past the chain, the baseline;
//! - adapters: [`Forward`], which forwards every batch, and
//!   [`Repetition3`], a toy lowering;
//! - executors: [`Profiler`] and [`Sink`], which count what they get, and
//!   [`FromWorker`], which answers from a worker thread.

mod adapters;
mod drivers;
mod executors;

pub(crate) use adapters::*;
pub(crate) use drivers::*;
pub(crate) use executors::*;

use executor_protocol_core::instruction_set;

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

instruction_set! {
    /// A code's logical instructions, each logical qubit a block of three
    /// physical ones.
    pub(crate) LogicalGates {
        PREPARE_Z = prepare_z(qubit),
        X = x(qubit),
        CX = cx(qubit, qubit),
        MEASURE_Z = measure_z(qubit) -> 1 outcome,
    }
}

instruction_set! {
    /// The physical instructions the lowering sends.
    pub(crate) PhysicalGates {
        RESET = reset(qubit),
        X = x(qubit),
        CX = cx(qubit, qubit),
        MZ = mz(qubit) -> 1 outcome,
    }
}
