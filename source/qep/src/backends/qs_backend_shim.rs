// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    BatchResult,
    Parameter::{Float, QubitId, ResultId},
    QuantumBackend, QuantumBackendInstruction, QuantumBackendResult,
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

impl<B: Backend + Default> Default for QsBackendShim<B> {
    /// Wraps a default backend with empty qubit and result mappings.
    fn default() -> Self {
        Self::new(B::default())
    }
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
    fn mz_store(&mut self, target: usize, result: usize) -> Result<(), String> {
        let target = self.qubit(target)?;
        let value = self.backend.m(target)?;
        self.results.insert(result, value);
        Ok(())
    }

    /// Records a measurement-and-reset in the requested QIR result slot.
    fn mz_store_reset(&mut self, target: usize, result: usize) -> Result<(), String> {
        let target = self.qubit(target)?;
        let value = self.backend.mresetz(target)?;
        self.results.insert(result, value);
        Ok(())
    }

    /// Reads a previously populated QIR result slot.
    fn read_result(&self, result: usize) -> Result<QuantumBackendResult, String> {
        let value = self
            .results
            .get(&result)
            .copied()
            .ok_or_else(|| format!("Result ID {result} has not been written"))?;
        Self::result_value(value).map_err(|error| format!("Result ID {result} contains {error}"))
    }

    /// Converts a concrete Q# result, rejecting unresolved IDs and lost measurements.
    fn result_value(value: val::Result) -> Result<QuantumBackendResult, String> {
        match value {
            val::Result::Val(value) => Ok(QuantumBackendResult::from(value)),
            val::Result::Id(id) => Err(format!("unresolved Q# result ID {id}")),
            val::Result::Loss => Err("a lost measurement".into()),
        }
    }

    /// Delegates one instruction and returns a requested result value, if any.
    #[allow(clippy::too_many_lines)] // Keep all supported instruction patterns in one dispatch table.
    fn dispatch(
        &mut self,
        instruction: &QuantumBackendInstruction,
    ) -> Result<Option<QuantumBackendResult>, String> {
        let result = match (
            instruction.instruction_name,
            instruction.parameters.as_slice(),
        ) {
            ("ccx", &[QubitId(ctl0), QubitId(ctl1), QubitId(target)]) => {
                let ctl0 = self.qubit(ctl0)?;
                let ctl1 = self.qubit(ctl1)?;
                let target = self.qubit(target)?;
                self.backend.ccx(ctl0, ctl1, target)
            }
            ("cx", &[QubitId(control), QubitId(target)]) => {
                let control = self.qubit(control)?;
                let target = self.qubit(target)?;
                self.backend.cx(control, target)
            }
            ("cy", &[QubitId(control), QubitId(target)]) => {
                let control = self.qubit(control)?;
                let target = self.qubit(target)?;
                self.backend.cy(control, target)
            }
            ("cz", &[QubitId(control), QubitId(target)]) => {
                let control = self.qubit(control)?;
                let target = self.qubit(target)?;
                self.backend.cz(control, target)
            }
            ("rx", &[QubitId(target), Float(theta)]) => {
                let target = self.qubit(target)?;
                self.backend.rx(theta, target)
            }
            ("rxx", &[QubitId(target0), QubitId(target1), Float(theta)]) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.rxx(theta, target0, target1)
            }
            ("ry", &[QubitId(target), Float(theta)]) => {
                let target = self.qubit(target)?;
                self.backend.ry(theta, target)
            }
            ("ryy", &[QubitId(target0), QubitId(target1), Float(theta)]) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.ryy(theta, target0, target1)
            }
            ("rz", &[QubitId(target), Float(theta)]) => {
                let target = self.qubit(target)?;
                self.backend.rz(theta, target)
            }
            ("rzz", &[QubitId(target0), QubitId(target1), Float(theta)]) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.rzz(theta, target0, target1)
            }
            ("h", &[QubitId(target)]) => {
                let target = self.qubit(target)?;
                self.backend.h(target)
            }
            ("s", &[QubitId(target)]) => {
                let target = self.qubit(target)?;
                self.backend.s(target)
            }
            ("sadj", &[QubitId(target)]) => {
                let target = self.qubit(target)?;
                self.backend.sadj(target)
            }
            ("sx", &[QubitId(target)]) => {
                let target = self.qubit(target)?;
                self.backend.sx(target)
            }
            ("t", &[QubitId(target)]) => {
                let target = self.qubit(target)?;
                self.backend.t(target)
            }
            ("tadj", &[QubitId(target)]) => {
                let target = self.qubit(target)?;
                self.backend.tadj(target)
            }
            ("x", &[QubitId(target)]) => {
                let target = self.qubit(target)?;
                self.backend.x(target)
            }
            ("y", &[QubitId(target)]) => {
                let target = self.qubit(target)?;
                self.backend.y(target)
            }
            ("z", &[QubitId(target)]) => {
                let target = self.qubit(target)?;
                self.backend.z(target)
            }
            ("swap", &[QubitId(target0), QubitId(target1)]) => {
                let target0 = self.qubit(target0)?;
                let target1 = self.qubit(target1)?;
                self.backend.swap(target0, target1)
            }
            ("reset", &[QubitId(target)]) => {
                let target = self.qubit(target)?;
                self.backend.reset(target)
            }
            ("mz_store", &[QubitId(target), ResultId(result)]) => self.mz_store(target, result),
            ("mz_store_reset", &[QubitId(target), ResultId(result)]) => {
                self.mz_store_reset(target, result)
            }
            ("mz" | "mz_reset", &[QubitId(target)]) => {
                let target = self.qubit(target)?;
                let value = if instruction.instruction_name == "mz" {
                    self.backend.m(target)?
                } else {
                    self.backend.mresetz(target)?
                };
                return Self::result_value(value)
                    .map(Some)
                    .map_err(|error| format!("Measurement contains {error}"));
            }
            ("read_result", &[ResultId(result)]) => {
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
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        let mut result = BatchResult::default();
        for instruction in instructions {
            if let Some(value) = self
                .dispatch(instruction)
                .unwrap_or_else(|error| panic!("{error}"))
            {
                result.results.push(value);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::QsBackendShim;
    use crate::tests::compile_qsharp_to_qir;
    use crate::{OutputValue, run_qir};
    use expect_test::expect;
    use qsc::target::Profile;
    use qsc_eval::backend::{CliffordSim, SparseSim};
    use resource_estimator::counts::LogicalCounter;

    #[test]
    /// Verifies gate, measurement, reset, and result-read delegation to a Q# backend.
    fn delegates_to_sparse_sim() -> Result<(), String> {
        let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                X(q);
                MResetZ(q)
            }
        }
    "#;
        let file = compile_qsharp_to_qir(source, Profile::Base);
        let mut backend = QsBackendShim::new(SparseSim::new());

        // SAFETY: The QIR was generated by the in-tree Q# compiler from trusted source.
        let output = unsafe { run_qir(&file.0, &mut backend) }?;

        assert_eq!(output.records.len(), 1);
        assert_eq!(output.records[0].value, OutputValue::Result(true));
        Ok(())
    }

    #[test]
    /// Verifies deterministic Clifford operations and measurements through the shim.
    fn delegates_to_clifford_sim() -> Result<(), String> {
        let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use q = Qubit[2];
                X(q[0]);
                CNOT(q[0], q[1]);
                [MResetZ(q[0]), MResetZ(q[1])]
            }
        }
    "#;
        let file = compile_qsharp_to_qir(source, Profile::Base);
        let mut backend = QsBackendShim::new(CliffordSim::new(2));

        // SAFETY: The QIR was generated by the in-tree Q# compiler from trusted source.
        let output = unsafe { run_qir(&file.0, &mut backend) }?;

        assert_eq!(
            output
                .records
                .iter()
                .map(|record| &record.value)
                .collect::<Vec<_>>(),
            vec![
                &OutputValue::Array(2),
                &OutputValue::Result(true),
                &OutputValue::Result(true),
            ]
        );
        Ok(())
    }

    #[test]
    /// Verifies that QIR execution delegates resource accounting to a logical counter.
    fn delegates_to_counter() -> Result<(), String> {
        let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit[10];
                ApplyQFT(q);
            }
        }
    "#;
        let file = compile_qsharp_to_qir(source, Profile::Base);
        let mut backend = QsBackendShim::new(LogicalCounter::default());

        // SAFETY: The QIR was generated by the in-tree Q# compiler from trusted source.
        unsafe { run_qir(&file.0, &mut backend) }?;

        expect![[r#"
        LogicalResourceCounts {
            num_qubits: 10,
            t_count: 27,
            rotation_count: 108,
            rotation_depth: 31,
            ccz_count: 0,
            ccix_count: 0,
            measurement_count: 0,
            num_compute_qubits: None,
            read_from_memory_count: None,
            write_to_memory_count: None,
        }
    "#]]
        .assert_debug_eq(&backend.backend().logical_resources());
        Ok(())
    }
}
