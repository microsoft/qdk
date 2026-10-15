// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{MessageIn, MessageOut, QirInstruction, QuantumBackend, QuantumBackendEndpoint};
use qsc_eval::{backend::Backend, val};
use rustc_hash::FxHashMap;

/// Adapts a Q# evaluator backend to the streamed QIR backend interface.
pub struct QsBackendShim<B> {
    /// The Q# evaluator backend that executes delegated operations.
    backend: B,
    /// Maps static QIR qubit IDs to qubit IDs allocated by the wrapped backend.
    qubits: FxHashMap<usize, usize>,
    /// Stores measurement values by their QIR result IDs.
    results: FxHashMap<usize, val::Result>,
}

impl<B: Backend> QsBackendShim<B> {
    /// Creates an adapter around the given Q# evaluator backend.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            qubits: FxHashMap::default(),
            results: FxHashMap::default(),
        }
    }

    /// Returns a shared reference to the wrapped backend.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Returns a mutable reference to the wrapped backend.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Consumes the adapter and returns the wrapped backend.
    pub fn into_backend(self) -> B {
        self.backend
    }

    /// Resolves a static QIR qubit ID to a lazily allocated backend qubit ID.
    fn qubit(&mut self, id: usize) -> Result<usize, String> {
        if let Some(qubit) = self.qubits.get(&id) {
            return Ok(*qubit);
        }

        let qubit = self.backend.qubit_allocate()?;
        self.qubits.insert(id, qubit);
        Ok(qubit)
    }

    /// Records a measurement in the requested QIR result slot.
    fn mz(&mut self, target: usize, result: usize) -> Result<(), String> {
        let target = self.qubit(target)?;
        let value = self.backend.m(target)?;
        self.results.insert(result, value);
        Ok(())
    }

    /// Records a measurement-and-reset in the requested QIR result slot.
    fn mresetz(&mut self, target: usize, result: usize) -> Result<(), String> {
        let target = self.qubit(target)?;
        let value = self.backend.mresetz(target)?;
        self.results.insert(result, value);
        Ok(())
    }

    /// Reads a previously populated QIR result slot.
    fn read_result(&self, result: usize) -> Result<bool, String> {
        match self.results.get(&result).copied() {
            Some(val::Result::Val(value)) => Ok(value),
            Some(val::Result::Id(id)) => Err(format!(
                "Result ID {result} contains unresolved Q# result ID {id}"
            )),
            Some(val::Result::Loss) => {
                Err(format!("Result ID {result} contains a lost measurement"))
            }
            None => Err(format!("Result ID {result} has not been written")),
        }
    }

    /// Delegates one instruction and returns a requested result value, if any.
    fn dispatch(&mut self, instruction: QirInstruction) -> Result<Option<bool>, String> {
        let result = match instruction {
            QirInstruction::ccx(ctl0, ctl1, target) => {
                let ctl0 = self.qubit(ctl0)?;
                let ctl1 = self.qubit(ctl1)?;
                let target = self.qubit(target)?;
                self.backend.ccx(ctl0, ctl1, target)
            }
            QirInstruction::cx(control, target) => {
                let control = self.qubit(control)?;
                let target = self.qubit(target)?;
                self.backend.cx(control, target)
            }
            QirInstruction::cy(control, target) => {
                let control = self.qubit(control)?;
                let target = self.qubit(target)?;
                self.backend.cy(control, target)
            }
            QirInstruction::cz(control, target) => {
                let control = self.qubit(control)?;
                let target = self.qubit(target)?;
                self.backend.cz(control, target)
            }
            QirInstruction::rx(theta, target) => {
                let target = self.qubit(target)?;
                self.backend.rx(theta, target)
            }
            QirInstruction::rxx(theta, target0, target1) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.rxx(theta, target0, target1)
            }
            QirInstruction::ry(theta, target) => {
                let target = self.qubit(target)?;
                self.backend.ry(theta, target)
            }
            QirInstruction::ryy(theta, target0, target1) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.ryy(theta, target0, target1)
            }
            QirInstruction::rz(theta, target) => {
                let target = self.qubit(target)?;
                self.backend.rz(theta, target)
            }
            QirInstruction::rzz(theta, target0, target1) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.rzz(theta, target0, target1)
            }
            QirInstruction::h(target) => {
                let target = self.qubit(target)?;
                self.backend.h(target)
            }
            QirInstruction::s(target) => {
                let target = self.qubit(target)?;
                self.backend.s(target)
            }
            QirInstruction::sadj(target) => {
                let target = self.qubit(target)?;
                self.backend.sadj(target)
            }
            QirInstruction::sx(target) => {
                let target = self.qubit(target)?;
                self.backend.sx(target)
            }
            QirInstruction::t(target) => {
                let target = self.qubit(target)?;
                self.backend.t(target)
            }
            QirInstruction::tadj(target) => {
                let target = self.qubit(target)?;
                self.backend.tadj(target)
            }
            QirInstruction::x(target) => {
                let target = self.qubit(target)?;
                self.backend.x(target)
            }
            QirInstruction::y(target) => {
                let target = self.qubit(target)?;
                self.backend.y(target)
            }
            QirInstruction::z(target) => {
                let target = self.qubit(target)?;
                self.backend.z(target)
            }
            QirInstruction::swap(target0, target1) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.swap(target0, target1)
            }
            QirInstruction::reset(target) => {
                let target = self.qubit(target)?;
                self.backend.reset(target)
            }
            QirInstruction::mz(target, result) => self.mz(target, result),
            QirInstruction::mresetz(target, result) => self.mresetz(target, result),
            QirInstruction::read_result(result) => return self.read_result(result).map(Some),
        };
        result?;
        Ok(None)
    }
}

impl<B: Backend> QuantumBackend for QsBackendShim<B> {
    /// Delegates streamed QIR instructions to the wrapped Q# evaluator backend.
    async fn run(&mut self, mut endpoint: QuantumBackendEndpoint) {
        while let Some(MessageIn::QirInstruction(instruction)) =
            endpoint.instructions_receiver.recv().await
        {
            if let Some(value) = self
                .dispatch(instruction)
                .unwrap_or_else(|error| panic!("{error}"))
                && endpoint
                    .results_sender
                    .send(MessageOut::MeasurementResult(value))
                    .await
                    .is_err()
            {
                return;
            }
        }
    }
}
