// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    BatchResult, QuantumBackend, QuantumBackendInstruction,
    serde::{deserialize_result, read_message, serialize_batch, write_message},
};
use std::io::{self, BufReader, Read, Stdin, Stdout, Write};

/// Executes batches through a synchronous newline-delimited JSON connection.
pub struct StdioBackend<W, R> {
    instruction_stream: W,
    result_stream: BufReader<R>,
}

impl<W: Write, R: Read> StdioBackend<W, R> {
    /// Connects an instruction writer and result reader.
    pub fn new(instruction_stream: W, result_stream: R) -> Self {
        Self {
            instruction_stream,
            result_stream: BufReader::new(result_stream),
        }
    }
}

impl StdioBackend<Stdout, Stdin> {
    /// Sends instructions to stdout and receives results from stdin.
    #[must_use]
    pub fn stdio() -> Self {
        Self::new(io::stdout(), io::stdin())
    }
}

impl<W: Write, R: Read> QuantumBackend for StdioBackend<W, R> {
    /// Sends and flushes a batch, then waits for its corresponding result.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        let message = serialize_batch(instructions)?;
        write_message(&mut self.instruction_stream, &message)?;
        let response = read_message(&mut self.result_stream)?
            .ok_or_else(|| "Result stream closed before returning a batch result".to_string())?;
        deserialize_result(&response)
    }
}
