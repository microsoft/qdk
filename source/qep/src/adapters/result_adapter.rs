// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    BatchResult, ClassicalParameter::ResultId, QuantumBackend, QuantumBackendInstruction,
    QuantumBackendResult, interface::Driver,
};

/// Adapts result-register instructions for a backend without a result register.
///
/// Use this adapter between the QIR runner and a backend that does not support
/// the result-register model. It maintains a local result register and replaces
/// `MZ_STORE` with `MZ` and `MZ_STORE_RESET` with `MZ_RESET`.
///
/// The transformation guarantees that:
/// - Batches sent downstream contain no `MZ_STORE`, `MZ_STORE_RESET`, or
///   `READ_RESULT` instructions.
/// - The quantum computation and returned measurements are equivalent to those
///   produced without the adapter by a backend supporting the result-register
///   model.
///
/// The adapter generally loses the potential benefit of deferring measurements
/// until their results are needed. Use it only when the backend does not support
/// a result register.
///
/// # Implementation
///
/// The local result register is a vector that grows as needed.
///
/// Input instructions must have valid names and operand shapes. These are
/// checked only when debug assertions are enabled.
///
/// For each input batch, the adapter replaces `MZ_STORE` with `MZ` and
/// `MZ_STORE_RESET` with `MZ_RESET`, removes `READ_RESULT` instructions, and
/// forwards the transformed batch downstream. All other instructions remain
/// unchanged, including any `MZ` and `MZ_RESET` instructions in the input.
///
/// After receiving the downstream `BatchResult`, the adapter replays the input
/// batch in order while consuming the returned results and building the output
/// `BatchResult`:
/// - For each `MZ` or `MZ_RESET`, it consumes the next downstream result and
///   appends it to the output.
/// - For each `MZ_STORE` or `MZ_STORE_RESET`, it consumes the next downstream
///   result and stores it in `results_register`, without appending it to the
///   output.
/// - For each `READ_RESULT(rid)`, it reads `results_register[rid]` and appends
///   that value to the output.
///
/// # Backend contract
///
/// A `BatchResult` must contain exactly one result for each `MZ`, `MZ_RESET`,
/// or `READ_RESULT` instruction, in instruction order, and no other results.
/// The adapter relies on the downstream backend following this contract and
/// guarantees that its own output follows it.
///
/// # Errors
///
/// Returns an error if:
/// - The downstream backend returns fewer or more results than the number of
///   `MZ` and `MZ_RESET` instructions it received.
/// - The downstream backend returns an error.
/// - A result slot is read before it has been written in the current batch or
///   any previous batch.
///
/// # Panics
///
/// Panics if debug assertions are enabled and an input instruction has an invalid name
/// or operand shape.
pub struct ResultAdapter<B> {
    backend: B,
    results_register: Vec<Option<QuantumBackendResult>>,
}

impl<B: QuantumBackend> ResultAdapter<B> {
    /// Wraps a backend with an initially empty local result register.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            results_register: vec![],
        }
    }

    /// Grows the register to include the requested result slot.
    fn ensure_result_size(&mut self, result: usize) {
        if result >= self.results_register.len() {
            self.results_register.resize(result + 1, None);
        }
    }
}

impl<B: QuantumBackend> Driver for ResultAdapter<B> {
    type Backend = B;

    /// Returns the wrapped backend.
    fn get_backend(&self) -> &B {
        &self.backend
    }
}

impl<B: QuantumBackend> QuantumBackend for ResultAdapter<B> {
    /// Transforms a batch and reconstructs its results using the local register.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        let mut transformed = Vec::with_capacity(instructions.len());
        for instruction in instructions {
            #[cfg(debug_assertions)]
            crate::validate_instruction(instruction);
            let mut instruction = instruction.clone();
            match instruction.instruction_name {
                "mz_store" => {
                    instruction.instruction_name = "mz";
                    instruction.classical_parameters.clear();
                }
                "mz_store_reset" => {
                    instruction.instruction_name = "mz_reset";
                    instruction.classical_parameters.clear();
                }
                "read_result" => continue,
                _ => {}
            }
            transformed.push(instruction);
        }

        let downstream = self.backend.execute_batch(&transformed)?;
        let mut results = downstream.results.into_iter();
        let mut output = BatchResult::default();
        for instruction in instructions {
            match (
                instruction.instruction_name,
                instruction.classical_parameters.as_slice(),
            ) {
                ("mz" | "mz_reset", []) => {
                    output.results.push(
                        results
                            .next()
                            .ok_or("Result adapter backend returned too few results")?,
                    );
                }
                ("mz_store" | "mz_store_reset", &[ResultId(result)]) => {
                    let value = results
                        .next()
                        .ok_or("Result adapter backend returned too few results")?;
                    self.ensure_result_size(result);
                    self.results_register[result] = Some(value);
                }
                ("read_result", &[ResultId(result)]) => {
                    let value = self
                        .results_register
                        .get(result)
                        .copied()
                        .flatten()
                        .ok_or_else(|| format!("Result ID {result} has not been written"))?;
                    output.results.push(value);
                }
                _ => {}
            }
        }
        if results.next().is_some() {
            return Err("Result adapter backend returned too many results".into());
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::ResultAdapter;
    use crate::{
        BatchResult, ClassicalParameter, QuantumBackend, QuantumBackendInstruction,
        QuantumBackendResult,
        QuantumBackendResult::{Error, One, Zero},
        SparseSimBackend, validate_instruction,
    };
    use std::collections::VecDeque;

    struct RecordingBackend {
        batches: Vec<Vec<QuantumBackendInstruction>>,
        results: VecDeque<BatchResult>,
    }

    impl RecordingBackend {
        /// Configures the result list returned for each downstream batch.
        fn new(results: impl IntoIterator<Item = Vec<QuantumBackendResult>>) -> Self {
            Self {
                batches: Vec::new(),
                results: results
                    .into_iter()
                    .map(|results| BatchResult {
                        results: results.into_iter().collect(),
                    })
                    .collect(),
            }
        }
    }

    impl QuantumBackend for RecordingBackend {
        /// Records valid register-free instructions and returns the next configured batch.
        fn execute_batch(
            &mut self,
            instructions: &[QuantumBackendInstruction],
        ) -> Result<BatchResult, String> {
            for instruction in instructions {
                validate_instruction(instruction);
                assert!(!matches!(
                    instruction.instruction_name,
                    "mz_store" | "mz_store_reset" | "read_result"
                ));
            }
            self.batches.push(instructions.to_vec());
            let result = self
                .results
                .pop_front()
                .expect("unexpected downstream batch");
            if result.results.contains(&Error) {
                return Err("backend returned an error result".into());
            }
            Ok(result)
        }
    }

    /// Builds an instruction with explicit quantum and classical operands.
    fn instruction(
        name: &'static str,
        targets: &[usize],
        parameters: &[ClassicalParameter],
    ) -> QuantumBackendInstruction {
        QuantumBackendInstruction {
            instruction_name: name,
            targets: targets.iter().copied().collect(),
            classical_parameters: parameters.iter().cloned().collect(),
        }
    }

    #[test]
    /// Keeps stored results across batches, including repeated reads and later overwrites.
    fn preserves_results_across_batches() {
        use ClassicalParameter::ResultId;

        let backend = RecordingBackend::new([vec![One, Zero], vec![], vec![Zero]]);
        let mut adapter = ResultAdapter::new(backend);
        let output = adapter
            .execute_batch(&[
                instruction("mz_store", &[0], &[ResultId(5)]),
                instruction("mz_store_reset", &[1], &[ResultId(2)]),
            ])
            .expect("batch should succeed");
        assert!(output.results.is_empty());

        let output = adapter
            .execute_batch(&[
                instruction("read_result", &[], &[ResultId(5)]),
                instruction("read_result", &[], &[ResultId(2)]),
                instruction("read_result", &[], &[ResultId(5)]),
            ])
            .expect("batch should succeed");
        assert_eq!(output.results.as_slice(), &[One, Zero, One]);

        let output = adapter
            .execute_batch(&[
                instruction("x", &[0], &[]),
                instruction("mz_store", &[0], &[ResultId(5)]),
                instruction("read_result", &[], &[ResultId(5)]),
                instruction("read_result", &[], &[ResultId(2)]),
            ])
            .expect("batch should succeed");
        assert_eq!(output.results.as_slice(), &[Zero, Zero]);
        assert_eq!(
            adapter.backend.batches,
            [
                vec![
                    instruction("mz", &[0], &[]),
                    instruction("mz_reset", &[1], &[]),
                ],
                vec![],
                vec![instruction("x", &[0], &[]), instruction("mz", &[0], &[])],
            ]
        );
    }

    #[test]
    /// Preserves output order for interleaved stores, reads, and immediate measurements.
    fn replays_multiple_measurements_and_readouts() {
        use ClassicalParameter::{Float, ResultId};

        let backend = RecordingBackend::new([vec![One, Zero, Zero, One]]);
        let mut adapter = ResultAdapter::new(backend);
        let output = adapter
            .execute_batch(&[
                instruction("h", &[0], &[]),
                instruction("rx", &[1], &[Float(0.5)]),
                instruction("mz_store", &[0], &[ResultId(3)]),
                instruction("read_result", &[], &[ResultId(3)]),
                instruction("mz", &[1], &[]),
                instruction("mz_store_reset", &[2], &[ResultId(3)]),
                instruction("read_result", &[], &[ResultId(3)]),
                instruction("mz_reset", &[4], &[]),
                instruction("read_result", &[], &[ResultId(3)]),
            ])
            .expect("batch should succeed");
        assert_eq!(output.results.as_slice(), &[One, Zero, Zero, One, Zero]);
        assert_eq!(
            adapter.backend.batches,
            [vec![
                instruction("h", &[0], &[]),
                instruction("rx", &[1], &[Float(0.5)]),
                instruction("mz", &[0], &[]),
                instruction("mz", &[1], &[]),
                instruction("mz_reset", &[2], &[]),
                instruction("mz_reset", &[4], &[]),
            ]]
        );
    }

    #[test]
    /// Executes both measurements even when the first stored value is overwritten unread.
    fn overwritten_results_do_not_skip_measurements() {
        use ClassicalParameter::ResultId;

        let backend = RecordingBackend::new([vec![One, Zero]]);
        let mut adapter = ResultAdapter::new(backend);
        let output = adapter
            .execute_batch(&[
                instruction("mz_store", &[0], &[ResultId(7)]),
                instruction("mz_store", &[1], &[ResultId(7)]),
                instruction("read_result", &[], &[ResultId(7)]),
            ])
            .expect("batch should succeed");
        assert_eq!(output.results.as_slice(), &[Zero]);
        assert_eq!(
            adapter.backend.batches,
            [vec![
                instruction("mz", &[0], &[]),
                instruction("mz", &[1], &[]),
            ]]
        );
    }

    #[test]
    /// Preserves measurement and reset behavior with a real simulator across batches.
    fn simulates_measurement_and_reset() {
        use ClassicalParameter::ResultId;

        let mut adapter = ResultAdapter::new(SparseSimBackend::default());
        let output = adapter
            .execute_batch(&[
                instruction("x", &[0], &[]),
                instruction("mz_store_reset", &[0], &[ResultId(0)]),
                instruction("mz", &[0], &[]),
            ])
            .expect("batch should succeed");
        assert_eq!(output.results.as_slice(), &[Zero]);

        let output = adapter
            .execute_batch(&[
                instruction("read_result", &[], &[ResultId(0)]),
                instruction("x", &[0], &[]),
                instruction("mz_store", &[0], &[ResultId(0)]),
                instruction("mz_store", &[0], &[ResultId(0)]),
                instruction("read_result", &[], &[ResultId(0)]),
                instruction("mz_reset", &[0], &[]),
                instruction("mz", &[0], &[]),
            ])
            .expect("batch should succeed");
        assert_eq!(output.results.as_slice(), &[One, One, One, Zero]);
    }

    #[test]
    /// Forwards empty batches and batches without result-producing instructions.
    fn forwards_batches_without_measurements() {
        let backend = RecordingBackend::new([vec![], vec![]]);
        let mut adapter = ResultAdapter::new(backend);
        assert!(
            adapter
                .execute_batch(&[])
                .expect("batch should succeed")
                .results
                .is_empty()
        );
        let gates = [instruction("h", &[2], &[]), instruction("cx", &[2, 0], &[])];
        assert!(
            adapter
                .execute_batch(&gates)
                .expect("batch should succeed")
                .results
                .is_empty()
        );
        assert_eq!(adapter.backend.batches, [vec![], gates.to_vec()]);
    }

    #[test]
    /// Rejects a downstream batch missing a measurement result.
    fn rejects_missing_results() {
        let mut adapter = ResultAdapter::new(RecordingBackend::new([vec![]]));
        assert_eq!(
            adapter
                .execute_batch(&[instruction("mz", &[0], &[])])
                .expect_err("missing result"),
            "Result adapter backend returned too few results"
        );
    }

    #[test]
    /// Rejects leftover downstream results after replay.
    fn rejects_extra_results() {
        let mut adapter = ResultAdapter::new(RecordingBackend::new([vec![One, Zero]]));
        assert_eq!(
            adapter
                .execute_batch(&[instruction("mz", &[0], &[])])
                .expect_err("extra results"),
            "Result adapter backend returned too many results"
        );
    }

    #[test]
    /// Rejects reads beyond the end of the local register.
    fn rejects_unwritten_result() {
        let mut adapter = ResultAdapter::new(RecordingBackend::new([vec![]]));
        let error = adapter
            .execute_batch(&[instruction(
                "read_result",
                &[],
                &[ClassicalParameter::ResultId(0)],
            )])
            .expect_err("unwritten result");
        assert_eq!(error, "Result ID 0 has not been written");
    }

    #[test]
    /// Rejects unwritten holes created by storing into a higher result slot.
    fn rejects_unwritten_hole() {
        use ClassicalParameter::ResultId;

        let mut adapter = ResultAdapter::new(RecordingBackend::new([vec![One]]));
        let error = adapter
            .execute_batch(&[
                instruction("mz_store", &[0], &[ResultId(3)]),
                instruction("read_result", &[], &[ResultId(1)]),
            ])
            .expect_err("unwritten result slot");
        assert_eq!(error, "Result ID 1 has not been written");
    }

    #[test]
    /// Rejects reading a slot before its first store later in the same batch.
    fn rejects_read_before_store() {
        use ClassicalParameter::ResultId;

        let mut adapter = ResultAdapter::new(RecordingBackend::new([vec![One]]));
        let error = adapter
            .execute_batch(&[
                instruction("read_result", &[], &[ResultId(0)]),
                instruction("mz_store", &[0], &[ResultId(0)]),
            ])
            .expect_err("read before store");
        assert_eq!(error, "Result ID 0 has not been written");
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "Unsupported or malformed")]
    /// Rejects malformed store operands before sending anything downstream.
    fn rejects_malformed_store() {
        let mut adapter = ResultAdapter::new(RecordingBackend::new([]));
        let _ = adapter.execute_batch(&[instruction("mz_store", &[0], &[])]);
    }

    #[test]
    /// Reports a failed stored measurement even when its result is never read.
    fn rejects_downstream_error() {
        let mut adapter = ResultAdapter::new(RecordingBackend::new([vec![Error]]));
        let error = adapter
            .execute_batch(&[instruction(
                "mz_store",
                &[0],
                &[ClassicalParameter::ResultId(0)],
            )])
            .expect_err("downstream failure");
        assert_eq!(error, "backend returned an error result");
    }
}
