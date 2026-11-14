// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    QuantumBackend,
    interface::Driver,
    serde::{deserialize_batch, read_message, serialize_result, write_message},
};
use std::io::{self, BufReader, Read, Stdin, Stdout, Write};

/// Serves instruction batches from a stream using an owned backend.
pub struct StdioDriver<B, R, W> {
    backend: B,
    instruction_stream: BufReader<R>,
    result_stream: W,
}

impl<B: QuantumBackend, R: Read, W: Write> StdioDriver<B, R, W> {
    /// Connects a backend to an instruction reader and result writer.
    pub fn new(backend: B, instruction_stream: R, result_stream: W) -> Self {
        Self {
            backend,
            instruction_stream: BufReader::new(instruction_stream),
            result_stream,
        }
    }

    /// Executes and replies to one batch, returning false on clean instruction EOF.
    pub fn process_next_batch(&mut self) -> Result<bool, String> {
        let Some(message) = read_message(&mut self.instruction_stream)? else {
            return Ok(false);
        };
        let instructions = deserialize_batch(&message)?;
        let result = self.backend.execute_batch(&instructions)?;
        write_message(&mut self.result_stream, &serialize_result(&result)?)?;
        Ok(true)
    }

    /// Serves batches until instruction EOF; transport or backend errors stop the driver.
    pub fn run(&mut self) -> Result<(), String> {
        while self.process_next_batch()? {}
        Ok(())
    }
}

impl<B: QuantumBackend> StdioDriver<B, Stdin, Stdout> {
    /// Receives instructions from stdin and sends results to stdout.
    pub fn stdio(backend: B) -> Self {
        Self::new(backend, io::stdin(), io::stdout())
    }
}

impl<B: QuantumBackend, R, W> Driver for StdioDriver<B, R, W> {
    type Backend = B;

    /// Returns the backend executing received instruction batches.
    fn get_backend(&self) -> &B {
        &self.backend
    }
}
