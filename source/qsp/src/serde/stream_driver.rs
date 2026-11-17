// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    QuantumBackend, QuantumBackendInstruction,
    interface::Driver,
    serde::{deserialize_batch_into, read_message, serialize_result, write_message},
};
use std::io::{self, BufReader, Read, Stdin, Stdout, Write};

/// Serves instruction batches from a stream using an owned backend.
pub struct StreamDriver<B, R, W> {
    backend: B,
    instruction_stream: BufReader<R>,
    result_stream: W,
    message: String,
    instructions: Vec<QuantumBackendInstruction>,
}

impl<B: QuantumBackend, R: Read, W: Write> StreamDriver<B, R, W> {
    /// Connects a backend to an instruction reader and result writer.
    pub fn new(backend: B, instruction_stream: R, result_stream: W) -> Self {
        Self {
            backend,
            instruction_stream: BufReader::new(instruction_stream),
            result_stream,
            message: String::new(),
            instructions: Vec::new(),
        }
    }

    /// Executes and replies to one batch, returning false on clean instruction EOF.
    pub fn process_next_batch(&mut self) -> Result<bool, String> {
        if !read_message(&mut self.instruction_stream, &mut self.message)? {
            return Ok(false);
        }
        deserialize_batch_into(&self.message, &mut self.instructions)?;
        let result = self.backend.execute_batch(&self.instructions)?;
        write_message(&mut self.result_stream, &serialize_result(&result)?)?;
        Ok(true)
    }

    /// Serves batches until instruction EOF; transport or backend errors stop the driver.
    pub fn run(&mut self) -> Result<(), String> {
        while self.process_next_batch()? {}
        Ok(())
    }
}

impl<B: QuantumBackend> StreamDriver<B, Stdin, Stdout> {
    /// Receives instructions from stdin and sends results to stdout.
    pub fn stdio(backend: B) -> Self {
        Self::new(backend, io::stdin(), io::stdout())
    }
}

impl<B: QuantumBackend, R, W> Driver for StreamDriver<B, R, W> {
    type Backend = B;

    /// Returns the backend executing received instruction batches.
    fn get_backend(&self) -> &B {
        &self.backend
    }
}
