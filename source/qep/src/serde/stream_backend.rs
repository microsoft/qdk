// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    BatchResult, QuantumBackend, QuantumBackendInstruction,
    serde::{deserialize_result, read_message, serialize_batch_into, write_message},
};
use std::io::{self, BufReader, Read, Stdin, Stdout, Write};

/// Executes batches through a synchronous newline-delimited JSON connection.
pub struct StreamBackend<W, R> {
    instruction_stream: W,
    result_stream: BufReader<R>,
    message: String,
}

impl<W: Write, R: Read> StreamBackend<W, R> {
    /// Connects an instruction writer and result reader.
    pub fn new(instruction_stream: W, result_stream: R) -> Self {
        Self {
            instruction_stream,
            result_stream: BufReader::new(result_stream),
            message: String::new(),
        }
    }
}

impl StreamBackend<Stdout, Stdin> {
    /// Sends instructions to stdout and receives results from stdin.
    #[must_use]
    pub fn stdio() -> Self {
        Self::new(io::stdout(), io::stdin())
    }
}

impl<W: Write, R: Read> QuantumBackend for StreamBackend<W, R> {
    /// Sends and flushes a batch, then waits for its corresponding result.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        serialize_batch_into(instructions, &mut self.message)?;
        write_message(&mut self.instruction_stream, &self.message)?;
        if !read_message(&mut self.result_stream, &mut self.message)? {
            return Err("Result stream closed before returning a batch result".into());
        }
        deserialize_result(&self.message)
    }
}
