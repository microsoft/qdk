// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{ClassicalParameter, QuantumBackend, QuantumBackendResult, QuantumInstruction};
use smallvec::smallvec;

/// Default number of instructions sent to a backend in one batch.
pub const DEFAULT_MAX_BATCH_SIZE: usize = 10_000;

/// Buffers QIR instructions and executes batches at capacity or result barriers.
pub(crate) struct Batcher<'a> {
    backend: &'a mut dyn QuantumBackend,
    instructions: Vec<QuantumInstruction>,
    max_batch_size: usize,
}

impl<'a> Batcher<'a> {
    /// Creates a batcher with a reusable instruction buffer.
    pub(crate) fn new(
        backend: &'a mut dyn QuantumBackend,
        max_batch_size: usize,
    ) -> Result<Self, String> {
        if max_batch_size == 0 {
            return Err("Maximum QIR batch size must be greater than zero".into());
        }
        Ok(Self {
            backend,
            instructions: Vec::with_capacity(max_batch_size),
            max_batch_size,
        })
    }

    /// Buffers a non-blocking instruction and flushes a full batch.
    pub(crate) fn push(&mut self, instruction: QuantumInstruction) -> Result<(), String> {
        if instruction.instruction_id == crate::instructions::READ_RESULT {
            return Err("read_result must be submitted through the result barrier".into());
        }
        self.instructions.push(instruction);
        if self.instructions.len() == self.max_batch_size {
            self.flush(false)?;
        }
        Ok(())
    }

    /// Flushes pending instructions with a final result read and returns its value.
    pub(crate) fn read_result(&mut self, result_id: usize) -> Result<bool, String> {
        self.instructions.push(QuantumInstruction {
            instruction_id: crate::instructions::READ_RESULT,
            targets: smallvec![],
            classical_parameters: smallvec![ClassicalParameter::ResultId(result_id)],
        });
        self.flush(true)?.ok_or_else(|| {
            "QIR backend returned no result for a batch ending in read_result".into()
        })
    }

    /// Flushes instructions remaining at the end of execution.
    pub(crate) fn finish(&mut self) -> Result<(), String> {
        if !self.instructions.is_empty() {
            self.flush(false)?;
        }
        Ok(())
    }

    /// Executes the buffered instructions and validates the backend response.
    fn flush(&mut self, expects_result: bool) -> Result<Option<bool>, String> {
        let batch = self.backend.execute_batch(&self.instructions);
        self.instructions.clear();

        if batch.results.contains(&QuantumBackendResult::Error) {
            return Err("QIR backend returned an error result".into());
        }
        match (expects_result, batch.results.as_slice()) {
            (_, []) => Ok(None),
            (true, [QuantumBackendResult::Zero]) => Ok(Some(false)),
            (true, [QuantumBackendResult::One]) => Ok(Some(true)),
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
    use crate::{BatchResult, QuantumBackend, QuantumBackendResult, QuantumInstruction};
    use std::collections::VecDeque;

    #[derive(Default)]
    struct RecordingBackend {
        batches: Vec<Vec<QuantumInstruction>>,
        results: VecDeque<BatchResult>,
    }

    impl QuantumBackend for RecordingBackend {
        /// Records each batch and returns the next configured result.
        fn execute_batch(&mut self, instructions: &[QuantumInstruction]) -> BatchResult {
            self.batches.push(instructions.to_vec());
            self.results.pop_front().unwrap_or_default()
        }
    }

    #[test]
    /// Flushes at capacity and reuses the buffer for later instructions.
    fn flushes_at_configured_capacity() -> Result<(), String> {
        let mut backend = RecordingBackend::default();
        {
            let mut batcher = Batcher::new(&mut backend, 2)?;
            batcher.push(crate::QuantumInstruction {
                instruction_id: crate::instructions::H,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![],
            })?;
            batcher.push(crate::QuantumInstruction {
                instruction_id: crate::instructions::X,
                targets: smallvec::smallvec![1],
                classical_parameters: smallvec::smallvec![],
            })?;
            batcher.push(crate::QuantumInstruction {
                instruction_id: crate::instructions::Z,
                targets: smallvec::smallvec![2],
                classical_parameters: smallvec::smallvec![],
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
            let mut batcher = Batcher::new(&mut backend, 10)?;
            batcher.push(crate::QuantumInstruction {
                instruction_id: crate::instructions::MZ,
                targets: smallvec::smallvec![0],
                classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(1)],
            })?;
            batcher.read_result(1)?
        };

        assert!(value);
        assert_eq!(
            backend.batches[0].as_slice(),
            &[
                crate::QuantumInstruction {
                    instruction_id: crate::instructions::MZ,
                    targets: smallvec::smallvec![0],
                    classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(
                        1
                    )]
                },
                crate::QuantumInstruction {
                    instruction_id: crate::instructions::READ_RESULT,
                    targets: smallvec::smallvec![],
                    classical_parameters: smallvec::smallvec![crate::ClassicalParameter::ResultId(
                        1
                    )]
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
        let mut batcher = Batcher::new(&mut backend, 10)?;
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
            let mut batcher = Batcher::new(&mut backend, 10)?;
            let error = if expects_result {
                batcher.read_result(0).expect_err("invalid result")
            } else {
                batcher.push(crate::QuantumInstruction {
                    instruction_id: crate::instructions::H,
                    targets: smallvec::smallvec![0],
                    classical_parameters: smallvec::smallvec![],
                })?;
                batcher.finish().expect_err("unexpected result")
            };
            assert!(error.contains(expected), "{error}");
        }
        Ok(())
    }
}
