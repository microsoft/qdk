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
pub use backends::{CountingBackend, CountingBackendCounts, SparseSimBackend};
pub use runner::{OutputRecord, OutputValue, RunOutput, run_qir};
use tokio::sync::mpsc::{Receiver, Sender, channel};

/// A backend for the quantum intrinsics supported by QDK.
///
/// Qubits and result slots are identified by `usize` IDs; rotation angles are
/// in radians. Measurement issuance is separate from reading its classical
/// value, allowing backends to schedule work without waiting for completion.
/// Result values use `false` for zero and `true` for one.
/// Methods return an error if the backend cannot perform the operation.
pub trait QirBackend {
    /// Applies a doubly controlled X gate.
    fn ccx(&mut self, control1: usize, control2: usize, target: usize) -> Result<(), String>;

    /// Applies a controlled X gate.
    fn cx(&mut self, control: usize, target: usize) -> Result<(), String>;

    /// Applies a controlled Y gate.
    fn cy(&mut self, control: usize, target: usize) -> Result<(), String>;

    /// Applies a controlled Z gate.
    fn cz(&mut self, control: usize, target: usize) -> Result<(), String>;

    /// Applies an X-axis rotation.
    fn rx(&mut self, angle: f64, target: usize) -> Result<(), String>;

    /// Applies a two-qubit XX rotation.
    fn rxx(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String>;

    /// Applies a Y-axis rotation.
    fn ry(&mut self, angle: f64, target: usize) -> Result<(), String>;

    /// Applies a two-qubit YY rotation.
    fn ryy(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String>;

    /// Applies a Z-axis rotation.
    fn rz(&mut self, angle: f64, target: usize) -> Result<(), String>;

    /// Applies a two-qubit ZZ rotation.
    fn rzz(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String>;

    /// Applies a Hadamard gate.
    fn h(&mut self, target: usize) -> Result<(), String>;

    /// Applies an S gate.
    fn s(&mut self, target: usize) -> Result<(), String>;

    /// Applies an adjoint S gate.
    fn sadj(&mut self, target: usize) -> Result<(), String>;

    /// Applies a square-root X gate.
    fn sx(&mut self, target: usize) -> Result<(), String>;

    /// Applies a T gate.
    fn t(&mut self, target: usize) -> Result<(), String>;

    /// Applies an adjoint T gate.
    fn tadj(&mut self, target: usize) -> Result<(), String>;

    /// Applies an X gate.
    fn x(&mut self, target: usize) -> Result<(), String>;

    /// Applies a Y gate.
    fn y(&mut self, target: usize) -> Result<(), String>;

    /// Applies a Z gate.
    fn z(&mut self, target: usize) -> Result<(), String>;

    /// Exchanges two qubit states.
    fn swap(&mut self, target1: usize, target2: usize) -> Result<(), String>;

    /// Issue a computational-basis measurement into the given result slot.
    ///
    /// This may return before the measurement completes. Reusing a result ID
    /// replaces its previous measurement or explicitly written value.
    fn mz(&mut self, target: usize, result: usize) -> Result<(), String>;

    /// Reset a qubit to zero.
    fn reset(&mut self, target: usize) -> Result<(), String>;

    /// Issue a measurement into the given result slot followed by a qubit reset.
    ///
    /// Like `mz`, this need not wait for physical completion.
    fn mresetz(&mut self, target: usize, result: usize) -> Result<(), String>;

    /// Resolve a result slot to its classical value, waiting or advancing
    /// modeled time if necessary. Repeated reads return the same value until
    /// another measurement or write replaces the slot.
    fn read_result(&mut self, result: usize) -> Result<bool, String>;
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
enum QirInstruction {
    ccx(usize, usize, usize), // control_qubit_id_1, control_qubit_id_2, target_qubit_id
    cx(usize, usize),         // control_qubit_id, target_qubit_id
    cnot(usize, usize),       // control_qubit_id, target_qubit_id
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
    s_adj(usize),             // qubit_id
    sx(usize),                // qubit_id
    t(usize),                 // qubit_id
    t_adj(usize),             // qubit_id
    x(usize),                 // qubit_id
    y(usize),                 // qubit_id
    z(usize),                 // qubit_id
    swap(usize, usize),       // qubit_id_1, qubit_id_2
    reset(usize),             // qubit_id
    m(usize, usize),          // qubit_id, result_id
    mz(usize, usize),         // qubit_id, result_id
    mresetz(usize, usize),    // qubit_id, result_id
    read_result(usize),       // result_id
}

#[derive(Debug)]
enum MessageIn {
    QirInstruction(QirInstruction),
}

#[derive(PartialEq, Debug)]
enum MessageOut {
    MeasurmentResult(i64),
}

struct QuantumDriverEndpoint {
    instructions_sender: Sender<MessageIn>,
    results_receiver: Receiver<MessageOut>,
}

// Structure that is passed to any qauntum backend, which consists of 2 streams.
struct QuantumBackendEndpoint {
    instructions_receiver: Receiver<MessageIn>,
    results_sender: Sender<MessageOut>,
}

// Anything that can execute a stream of quantum instrutions and produce measurments.
trait QuantumBackend {
    // Must process received instrutions until instructions channel is closed.
    // Must write results to the results channel.
    async fn run(&mut self, endpoint: QuantumBackendEndpoint);
}

// Tets Implementation of QuantumBackend.
// Qubits are classical bits. Size is pre-defined.
// Only supports X and Measurements.
#[derive(Default)]
struct MyTestBackend {
    qubits: Vec<bool>,
    results: Vec<i64>,
}

impl MyTestBackend {
    fn new(size: usize) -> Self {
        Self {
            qubits: vec![false; size],
            results: vec![0; size],
        }
    }
}

impl QuantumBackend for MyTestBackend {
    async fn run(&mut self, mut endpoint: QuantumBackendEndpoint) {
        while let Some(event) = endpoint.instructions_receiver.recv().await {
            println!("Engine received instruction: {:?}", event);
            match event {
                MessageIn::QirInstruction(QirInstruction::x(qubit_id)) => {
                    self.qubits[qubit_id] ^= true;
                }
                MessageIn::QirInstruction(QirInstruction::m(qubit_id, result_id)) => {
                    self.results[result_id] = if self.qubits[qubit_id] { 1 } else { 0 };
                }
                MessageIn::QirInstruction(QirInstruction::read_result(result_id)) => {
                    let r = MessageOut::MeasurmentResult(self.results[result_id]);
                    endpoint.results_sender.send(r).await.unwrap();
                }
                _ => println!("Command ignored."),
            }
        }
    }
}

struct QirRunner {
    endpoint: Option<QuantumDriverEndpoint>,
    backend_channel: Option<QuantumBackendEndpoint>,
    debug_output: Vec<i64>,
}

impl QirRunner {
    fn default() -> Self {
        let (instructions_sender, instructions_receiver) = channel::<MessageIn>(32);
        let (results_sender, results_receiver) = channel::<MessageOut>(1024);
        let endpoint = QuantumDriverEndpoint {
            instructions_sender,
            results_receiver,
        };
        let backend_endpoint = QuantumBackendEndpoint {
            instructions_receiver,
            results_sender,
        };

        // TODO either use map or take size of result form QIR.
        QirRunner {
            endpoint: Some(endpoint),
            backend_channel: Some(backend_endpoint),
            debug_output: vec![],
        }
    }

    async fn run_qir(&mut self, backend: &mut impl QuantumBackend) {
        let channel = self.backend_channel.take().unwrap();
        tokio::join!(backend.run(channel), self.execute_program());
    }

    async fn execute_program(&mut self) {
        println!("Start program");
        // Pretend we are executing a quantum program.
        // In reality this will interpret QIR.
        self.send(MessageIn::QirInstruction(QirInstruction::x(0)))
            .await;
        self.measure(0, 0).await; // results[0]:=1.
        self.measure(1, 1).await; // results[1]:=0.
        self.send(MessageIn::QirInstruction(QirInstruction::x(2)))
            .await;
        self.measure(2, 1).await; // results[1]:=1.
        self.send(MessageIn::QirInstruction(QirInstruction::x(0)))
            .await;
        self.measure(0, 0).await; // results[0]:=0.

        let result = self.read_result(1).await;
        self.debug_output.push(result);

        let result = self.read_result(0).await;
        self.debug_output.push(result);

        drop(self.endpoint.take());
    }

    async fn send(&mut self, message: MessageIn) {
        self.endpoint
            .as_mut()
            .unwrap()
            .instructions_sender
            .send(message)
            .await
            .unwrap();
    }

    // Sends measure request.
    async fn measure(&mut self, qubit_id: usize, result_id: usize) {
        self.send(MessageIn::QirInstruction(QirInstruction::m(
            qubit_id, result_id,
        )))
        .await;
    }

    // Sends ReadResult, then waits for the result to come back.
    async fn read_result(&mut self, result_id: usize) -> i64 {
        self.send(MessageIn::QirInstruction(QirInstruction::read_result(
            result_id,
        )))
        .await;
        let next_message = self
            .endpoint
            .as_mut()
            .unwrap()
            .results_receiver
            .recv()
            .await
            .unwrap();
        match next_message {
            MessageOut::MeasurmentResult(mr) => {
                return mr;
            }
            _ => {
                panic!("unexpected message.");
            }
        }
    }
}

#[tokio::test]
async fn test_driver() -> () {
    let mut backend = MyTestBackend::new(10);
    let mut driver: QirRunner = QirRunner::default();
    driver.run_qir(&mut backend).await;

    assert_eq!(driver.debug_output, vec![1, 0]);
}
