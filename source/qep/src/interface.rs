// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_isa::QirInstruction;
use tokio::sync::mpsc::{Receiver, Sender};

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
