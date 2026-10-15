// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Quantum Execution Profiler: backend interfaces for quantum instructions.
//!
//! QIR execution requires LLVM 20 development libraries
//! (`LLVM_SYS_201_PREFIX` can specify their installation).
//!
//! The runner supports static qubit/result IDs, adaptive control flow, and
//! structured output recording. Dynamic allocation and legacy runtime calls
//! are rejected. It does not depend on or modify QIR Runner.
//!
//! Run the test suite with `cargo test -p qep`.

#[cfg(test)]
mod tests;

mod backends;
mod runner;
pub use backends::{CountingBackend, CountingBackendCounts, QsBackendShim, SparseSimBackend};
pub use runner::{OutputRecord, OutputValue, RunOutput, run_qir};
use tokio::sync::mpsc::{Receiver, Sender};

/// A quantum instruction emitted by the QIR runner.
#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum QirInstruction {
    ccx(usize, usize, usize), // control_qubit_id_1, control_qubit_id_2, target_qubit_id
    cx(usize, usize),         // control_qubit_id, target_qubit_id
    cy(usize, usize),         // control_qubit_id, target_qubit_id
    cz(usize, usize),         // control_qubit_id, target_qubit_id
    rx(f64, usize),           // angle, qubit_id
    rxx(f64, usize, usize),   // angle, qubit_id_1, qubit_id_2
    ry(f64, usize),           // angle, qubit_id
    ryy(f64, usize, usize),   // angle, qubit_id_1, qubit_id_2
    rz(f64, usize),           // angle, qubit_id
    rzz(f64, usize, usize),   // angle, qubit_id_1, qubit_id_2
    h(usize),                 // qubit_id
    s(usize),                 // qubit_id
    sadj(usize),              // qubit_id
    sx(usize),                // qubit_id
    t(usize),                 // qubit_id
    tadj(usize),              // qubit_id
    x(usize),                 // qubit_id
    y(usize),                 // qubit_id
    z(usize),                 // qubit_id
    swap(usize, usize),       // qubit_id_1, qubit_id_2
    reset(usize),             // qubit_id
    mz(usize, usize),         // qubit_id, result_id
    mresetz(usize, usize),    // qubit_id, result_id
    read_result(usize),       // result_id
}

/// A message sent from a quantum driver to a backend.
#[derive(Debug)]
pub enum MessageIn {
    QirInstruction(QirInstruction),
}

/// A message sent from a quantum backend to its driver.
#[derive(PartialEq, Debug)]
pub enum MessageOut {
    MeasurementResult(bool),
}

/// The instruction sender and result receiver owned by a quantum driver.
pub struct QuantumDriverEndpoint {
    pub instructions_sender: Sender<MessageIn>,
    pub results_receiver: Receiver<MessageOut>,
}

/// The instruction receiver and result sender owned by a quantum backend.
pub struct QuantumBackendEndpoint {
    pub instructions_receiver: Receiver<MessageIn>,
    pub results_sender: Sender<MessageOut>,
}

/// Executes streamed quantum instructions and returns requested measurement results.
#[allow(async_fn_in_trait)]
pub trait QuantumBackend {
    /// Processes instructions until the driver closes the instruction channel.
    async fn run(&mut self, endpoint: QuantumBackendEndpoint);
}
