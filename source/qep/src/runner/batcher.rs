// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    Parameter, QuantumBackend, QuantumBackendInstruction, QuantumBackendResult, interface::Driver,
};
use smallvec::smallvec;

/// Default number of instructions sent to a backend in one batch.
pub const DEFAULT_MAX_BATCH_SIZE: usize = 10_000;

/// Buffers instructions for an owned or borrowed backend.
///
/// Flushes at capacity or a `read_result`, `mz`, or `mz_reset` barrier.
/// Call [`Self::finish`] to submit trailing instructions; dropping discards them.
pub struct Batcher<B> {
    backend: B,
    instructions: Vec<QuantumBackendInstruction>,
    max_batch_size: usize,
}

/// Identifies instructions that require an immediate backend result.
fn produces_result(instruction: &QuantumBackendInstruction) -> bool {
    matches!(
        instruction.instruction_name,
        "read_result" | "mz" | "mz_reset"
    )
}

impl<B: QuantumBackend> Driver for Batcher<B> {
    type Backend = B;

    /// Returns the backend without flushing pending instructions.
    fn get_backend(&self) -> &B {
        &self.backend
    }
}

impl<B: QuantumBackend> Batcher<B> {
    /// Creates a batcher with a reusable instruction buffer.
    pub fn new(backend: B, max_batch_size: usize) -> Self {
        assert!(max_batch_size > 0);
        Self {
            backend,
            instructions: Vec::with_capacity(max_batch_size),
            max_batch_size,
        }
    }

    /// Buffers a non-blocking instruction and flushes a full batch.
    pub fn push(&mut self, instruction: QuantumBackendInstruction) -> Result<(), String> {
        debug_assert!(!produces_result(&instruction));
        self.instructions.push(instruction);
        if self.instructions.len() == self.max_batch_size {
            self.flush(false)?;
        }
        Ok(())
    }

    /// Flushes pending instructions with a final result read and returns its value.
    pub fn read_result(&mut self, result_id: usize) -> Result<bool, String> {
        let result = self.push_with_result(QuantumBackendInstruction {
            instruction_name: "read_result",
            parameters: smallvec![Parameter::ResultId(result_id)],
        })?;
        match result {
            QuantumBackendResult::Zero => Ok(false),
            QuantumBackendResult::One => Ok(true),
            QuantumBackendResult::Error => Err("QIR backend returned an error result".into()),
        }
    }

    /// Flushes at a result barrier, preserving a single backend-reported error value.
    ///
    /// Unlike [`Self::push_with_result`], this returns `QuantumBackendResult::Error`
    /// as a value. Execution failures and invalid result counts remain errors.
    pub fn push_with_result(
        &mut self,
        instruction: QuantumBackendInstruction,
    ) -> Result<QuantumBackendResult, String> {
        debug_assert!(produces_result(&instruction));
        let name = instruction.instruction_name;
        self.instructions.push(instruction);
        self.flush(true)?
            .ok_or_else(|| format!("QIR backend returned no result for a batch ending in {name}"))
    }

    /// Flushes instructions remaining at the end of execution.
    pub fn finish(&mut self) -> Result<(), String> {
        if !self.instructions.is_empty() {
            self.flush(false)?;
        }
        Ok(())
    }

    /// Executes the buffered instructions and validates the backend response.
    fn flush(&mut self, expects_result: bool) -> Result<Option<QuantumBackendResult>, String> {
        let batch = self.backend.execute_batch(&self.instructions);
        self.instructions.clear();
        let batch = batch?;

        if batch.results.contains(&QuantumBackendResult::Error)
            && !(expects_result && batch.results.len() == 1)
        {
            return Err("QIR backend returned an error result".into());
        }
        match (expects_result, batch.results.as_slice()) {
            (_, []) => Ok(None),
            (true, [result]) => Ok(Some(*result)),
            (false, _) => {
                Err("QIR backend returned a result for a batch without read_result".into())
            }
            (true, _) => Err(
                "QIR backend returned multiple results for a batch ending in read_result".into(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Batcher;
    use crate::{BatchResult, QuantumBackend, QuantumBackendInstruction, QuantumBackendResult};
    use std::collections::VecDeque;

    #[derive(Default)]
    struct RecordingBackend {
        batches: Vec<Vec<QuantumBackendInstruction>>,
        results: VecDeque<BatchResult>,
    }

    impl QuantumBackend for RecordingBackend {
        /// Records each batch and returns the next configured result.
        fn execute_batch(
            &mut self,
            instructions: &[QuantumBackendInstruction],
        ) -> Result<BatchResult, String> {
            self.batches.push(instructions.to_vec());
            Ok(self.results.pop_front().unwrap_or_default())
        }
    }

    #[test]
    /// Flushes an owned backend at capacity and preserves a backend error value.
    fn owns_backend_and_preserves_error_result() -> Result<(), String> {
        use crate::interface::Driver;

        let backend = RecordingBackend {
            results: VecDeque::from([
                BatchResult::default(),
                BatchResult {
                    results: [QuantumBackendResult::Error].into_iter().collect(),
                },
            ]),
            ..Default::default()
        };
        let mut batcher = Batcher::new(backend, 1);
        batcher.push(QuantumBackendInstruction {
            instruction_name: "h",
            parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
        })?;
        assert_eq!(batcher.get_backend().batches.len(), 1);
        assert_eq!(
            batcher.push_with_result(QuantumBackendInstruction {
                instruction_name: "mz",
                parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
            })?,
            QuantumBackendResult::Error
        );
        batcher.finish()?;
        assert_eq!(batcher.get_backend().batches.len(), 2);
        Ok(())
    }

    #[test]
    /// Flushes at capacity and reuses the buffer for later instructions.
    fn flushes_at_configured_capacity() -> Result<(), String> {
        let mut backend = RecordingBackend::default();
        {
            let mut batcher = Batcher::new(&mut backend, 2);
            batcher.push(crate::QuantumBackendInstruction {
                instruction_name: "h",
                parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
            })?;
            batcher.push(crate::QuantumBackendInstruction {
                instruction_name: "x",
                parameters: smallvec::smallvec![crate::Parameter::QubitId(1)],
            })?;
            batcher.push(crate::QuantumBackendInstruction {
                instruction_name: "z",
                parameters: smallvec::smallvec![crate::Parameter::QubitId(2)],
            })?;
            batcher.finish()?;
        }

        assert_eq!(backend.batches.len(), 2);
        assert_eq!(backend.batches[0].len(), 2);
        assert_eq!(backend.batches[1].len(), 1);
        Ok(())
    }

    #[test]
    /// Places `read_result` last and returns the corresponding backend value.
    fn flushes_at_result_barrier() -> Result<(), String> {
        let mut backend = RecordingBackend {
            results: VecDeque::from([BatchResult {
                results: [QuantumBackendResult::One].into_iter().collect(),
            }]),
            ..Default::default()
        };
        let value = {
            let mut batcher = Batcher::new(&mut backend, 10);
            batcher.push(crate::QuantumBackendInstruction {
                instruction_name: "mz_store",
                parameters: smallvec::smallvec![
                    crate::Parameter::QubitId(0),
                    crate::Parameter::ResultId(1)
                ],
            })?;
            batcher.read_result(1)?
        };

        assert!(value);
        assert_eq!(
            backend.batches[0].as_slice(),
            &[
                crate::QuantumBackendInstruction {
                    instruction_name: "mz_store",
                    parameters: smallvec::smallvec![
                        crate::Parameter::QubitId(0),
                        crate::Parameter::ResultId(1)
                    ]
                },
                crate::QuantumBackendInstruction {
                    instruction_name: "read_result",
                    parameters: smallvec::smallvec![crate::Parameter::ResultId(1)]
                }
            ]
        );
        Ok(())
    }

    #[test]
    /// Converts a zero result to false at the result barrier.
    fn reads_zero_result() -> Result<(), String> {
        let mut backend = RecordingBackend {
            results: VecDeque::from([BatchResult {
                results: [QuantumBackendResult::Zero].into_iter().collect(),
            }]),
            ..Default::default()
        };
        let mut batcher = Batcher::new(&mut backend, 10);
        assert!(!batcher.read_result(0)?);
        Ok(())
    }

    #[test]
    /// Rejects missing, extra, unexpected, and error results.
    fn rejects_invalid_results() -> Result<(), String> {
        use QuantumBackendResult::{Error, One, Zero};

        let cases: &[(bool, &[QuantumBackendResult], &str)] = &[
            (true, &[], "returned no result"),
            (true, &[Zero, One], "returned multiple results"),
            (false, &[Zero], "returned a result"),
            (false, &[One, Zero], "returned a result"),
            (true, &[Error], "returned an error result"),
            (false, &[Error], "returned an error result"),
            (true, &[Zero, Error], "returned an error result"),
            (false, &[Error, One], "returned an error result"),
        ];
        for &(expects_result, results, expected) in cases {
            let mut backend = RecordingBackend {
                results: VecDeque::from([BatchResult {
                    results: results.iter().copied().collect(),
                }]),
                ..Default::default()
            };
            let mut batcher = Batcher::new(&mut backend, 10);
            let error = if expects_result {
                batcher.read_result(0).expect_err("invalid result")
            } else {
                batcher.push(crate::QuantumBackendInstruction {
                    instruction_name: "h",
                    parameters: smallvec::smallvec![crate::Parameter::QubitId(0)],
                })?;
                batcher.finish().expect_err("unexpected result")
            };
            assert!(error.contains(expected), "{error}");
        }
        Ok(())
    }
}
