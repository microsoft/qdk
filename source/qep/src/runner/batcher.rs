// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{BatchResult, QirBatchResult, QirInstruction, QuantumBackend};

/// Default number of instructions sent to a backend in one batch.
pub const DEFAULT_MAX_BATCH_SIZE: usize = 10_000;

/// Buffers QIR instructions and executes batches at capacity or result barriers.
pub(crate) struct Batcher<'a> {
    backend: &'a mut dyn QuantumBackend,
    instructions: Vec<QirInstruction>,
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
    pub(crate) fn push(&mut self, instruction: QirInstruction) -> Result<(), String> {
        if matches!(instruction, QirInstruction::read_result(_)) {
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
        self.instructions
            .push(QirInstruction::read_result(result_id));
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
        let BatchResult::QirBatchResult(QirBatchResult { read_result }) =
            self.backend.execute_batch(&self.instructions);
        self.instructions.clear();

        if expects_result {
            Ok(read_result)
        } else if read_result.is_some() {
            Err("QIR backend returned a result for a batch without read_result".into())
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Batcher;
    use crate::{BatchResult, QirBatchResult, QirInstruction, QuantumBackend};
    use std::collections::VecDeque;

    #[derive(Default)]
    struct RecordingBackend {
        batches: Vec<Vec<QirInstruction>>,
        results: VecDeque<Option<bool>>,
    }

    impl QuantumBackend for RecordingBackend {
        /// Records each batch and returns the next configured result.
        fn execute_batch(&mut self, instructions: &[QirInstruction]) -> BatchResult {
            self.batches.push(instructions.to_vec());
            BatchResult::QirBatchResult(QirBatchResult {
                read_result: self.results.pop_front().flatten(),
            })
        }
    }

    #[test]
    /// Flushes at capacity and reuses the buffer for later instructions.
    fn flushes_at_configured_capacity() -> Result<(), String> {
        let mut backend = RecordingBackend::default();
        {
            let mut batcher = Batcher::new(&mut backend, 2)?;
            batcher.push(QirInstruction::h(0))?;
            batcher.push(QirInstruction::x(1))?;
            batcher.push(QirInstruction::z(2))?;
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
            results: VecDeque::from([Some(true)]),
            ..Default::default()
        };
        let value = {
            let mut batcher = Batcher::new(&mut backend, 10)?;
            batcher.push(QirInstruction::mz(0, 1))?;
            batcher.read_result(1)?
        };

        assert!(value);
        assert!(matches!(
            backend.batches[0].as_slice(),
            [QirInstruction::mz(0, 1), QirInstruction::read_result(1)]
        ));
        Ok(())
    }
}
