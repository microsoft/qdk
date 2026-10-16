// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    BatchResult,
    ClassicalParameter::{Float, ResultId},
    QuantumBackend, QuantumBackendResult, QuantumInstruction, instructions,
};
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
    fn read_result(&self, result: usize) -> Result<QuantumBackendResult, String> {
        match self.results.get(&result).copied() {
            Some(val::Result::Val(value)) => Ok(QuantumBackendResult::from(value)),
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
    #[allow(clippy::too_many_lines)] // Keep all supported instruction patterns in one dispatch table.
    fn dispatch(
        &mut self,
        instruction: &QuantumInstruction,
    ) -> Result<Option<QuantumBackendResult>, String> {
        let result = match (
            instruction.instruction_id,
            instruction.targets.as_slice(),
            instruction.classical_parameters.as_slice(),
        ) {
            (instructions::CCX, &[ctl0, ctl1, target], []) => {
                let ctl0 = self.qubit(ctl0)?;
                let ctl1 = self.qubit(ctl1)?;
                let target = self.qubit(target)?;
                self.backend.ccx(ctl0, ctl1, target)
            }
            (instructions::CX, &[control, target], []) => {
                let control = self.qubit(control)?;
                let target = self.qubit(target)?;
                self.backend.cx(control, target)
            }
            (instructions::CY, &[control, target], []) => {
                let control = self.qubit(control)?;
                let target = self.qubit(target)?;
                self.backend.cy(control, target)
            }
            (instructions::CZ, &[control, target], []) => {
                let control = self.qubit(control)?;
                let target = self.qubit(target)?;
                self.backend.cz(control, target)
            }
            (instructions::RX, &[target], &[Float(theta)]) => {
                let target = self.qubit(target)?;
                self.backend.rx(theta, target)
            }
            (instructions::RXX, &[target0, target1], &[Float(theta)]) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.rxx(theta, target0, target1)
            }
            (instructions::RY, &[target], &[Float(theta)]) => {
                let target = self.qubit(target)?;
                self.backend.ry(theta, target)
            }
            (instructions::RYY, &[target0, target1], &[Float(theta)]) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.ryy(theta, target0, target1)
            }
            (instructions::RZ, &[target], &[Float(theta)]) => {
                let target = self.qubit(target)?;
                self.backend.rz(theta, target)
            }
            (instructions::RZZ, &[target0, target1], &[Float(theta)]) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.rzz(theta, target0, target1)
            }
            (instructions::H, &[target], []) => {
                let target = self.qubit(target)?;
                self.backend.h(target)
            }
            (instructions::S, &[target], []) => {
                let target = self.qubit(target)?;
                self.backend.s(target)
            }
            (instructions::SADJ, &[target], []) => {
                let target = self.qubit(target)?;
                self.backend.sadj(target)
            }
            (instructions::SX, &[target], []) => {
                let target = self.qubit(target)?;
                self.backend.sx(target)
            }
            (instructions::T, &[target], []) => {
                let target = self.qubit(target)?;
                self.backend.t(target)
            }
            (instructions::TADJ, &[target], []) => {
                let target = self.qubit(target)?;
                self.backend.tadj(target)
            }
            (instructions::X, &[target], []) => {
                let target = self.qubit(target)?;
                self.backend.x(target)
            }
            (instructions::Y, &[target], []) => {
                let target = self.qubit(target)?;
                self.backend.y(target)
            }
            (instructions::Z, &[target], []) => {
                let target = self.qubit(target)?;
                self.backend.z(target)
            }
            (instructions::SWAP, &[target0, target1], []) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.swap(target0, target1)
            }
            (instructions::RESET, &[target], []) => {
                let target = self.qubit(target)?;
                self.backend.reset(target)
            }
            (instructions::MZ, &[target], &[ResultId(result)]) => self.mz(target, result),
            (instructions::MRESETZ, &[target], &[ResultId(result)]) => self.mresetz(target, result),
            (instructions::READ_RESULT, [], &[ResultId(result)]) => {
                return self.read_result(result).map(Some);
            }
            _ => {
                return Err(format!(
                    "Unsupported or malformed QIR instruction: {instruction:?}"
                ));
            }
        };
        result?;
        Ok(None)
    }
}

impl<B: Backend> QuantumBackend for QsBackendShim<B> {
    /// Delegates a batch of QIR instructions to the wrapped Q# evaluator backend.
    fn execute_batch(&mut self, instructions: &[QuantumInstruction]) -> BatchResult {
        let mut result = BatchResult::default();
        for instruction in instructions {
            if let Some(value) = self
                .dispatch(instruction)
                .unwrap_or_else(|error| panic!("{error}"))
            {
                result.results.push(value);
            }
        }
        result
    }
}
