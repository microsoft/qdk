// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    BatchResult, Driver, QuantumBackend, QuantumBackendInstruction,
    QuantumBackendResult::{Error, One, Zero},
    StdioBackend, StdioDriver,
    serde::deserialize_batch,
};
use smallvec::smallvec;
use std::{
    io::{self, BufWriter, Cursor, Read, Write},
    sync::mpsc::{Receiver, Sender, channel},
    time::Duration,
};

struct ControlledBackend {
    batches: Sender<Vec<QuantumBackendInstruction>>,
    results: Receiver<BatchResult>,
}

impl QuantumBackend for ControlledBackend {
    /// Reports each received batch and waits for the test to supply its response.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        self.batches
            .send(instructions.to_vec())
            .map_err(|error| error.to_string())?;
        self.results
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| error.to_string())
    }
}

#[test]
/// Exchanges multiple batches over two pipes, checking both flushes and ordered results.
fn backend_driver_round_trip() -> Result<(), String> {
    let (instruction_reader, instruction_writer) = io::pipe().map_err(|e| e.to_string())?;
    let (result_reader, result_writer) = io::pipe().map_err(|e| e.to_string())?;
    let (batch_sender, batch_receiver) = channel();
    let (result_sender, result_receiver) = channel();
    let cases = [
        (
            deserialize_batch(r#"["h(1)","ccx(1,2,3)","mz(1)","mz_reset(2)","mz(3)"]"#)?,
            BatchResult {
                results: smallvec![Zero, One, Zero],
            },
        ),
        (
            deserialize_batch(r#"["rz(2,-0.125)","mz_store(2,7)","read_result(7)"]"#)?,
            BatchResult {
                results: smallvec![Error],
            },
        ),
        (deserialize_batch(r#"["x(2)"]"#)?, BatchResult::default()),
        (vec![], BatchResult::default()),
    ];
    std::thread::scope(|scope| {
        let driver = scope.spawn(|| {
            let mut driver = StdioDriver::new(
                ControlledBackend {
                    batches: batch_sender,
                    results: result_receiver,
                },
                instruction_reader,
                BufWriter::new(result_writer),
            );
            let _ = driver.get_backend();
            driver.run()
        });
        let client = scope.spawn(|| {
            let mut backend = StdioBackend::new(BufWriter::new(instruction_writer), result_reader);
            for (instructions, expected) in &cases {
                assert_eq!(&backend.execute_batch(instructions)?, expected);
            }
            Ok::<_, String>(())
        });
        for (expected, result) in &cases {
            let received = batch_receiver
                .recv_timeout(Duration::from_secs(10))
                .map_err(|error| error.to_string())?;
            assert_eq!(&received, expected);
            result_sender
                .send(BatchResult {
                    results: result.results.clone(),
                })
                .map_err(|error| error.to_string())?;
        }
        client.join().expect("client thread should not panic")?;
        driver.join().expect("driver thread should not panic")
    })
}

struct FailingBackend;

impl QuantumBackend for FailingBackend {
    /// Supplies a backend failure to verify driver error propagation.
    fn execute_batch(&mut self, _: &[QuantumBackendInstruction]) -> Result<BatchResult, String> {
        Err("execution failed".into())
    }
}

#[test]
/// Distinguishes clean instruction EOF from malformed, truncated, and failed requests.
fn driver_errors_and_eof() {
    let mut output = Vec::new();
    let mut driver = StdioDriver::new(FailingBackend, Cursor::new(b""), &mut output);
    assert_eq!(driver.process_next_batch(), Ok(false));
    for input in ["[", "[]", "invalid\n", "[\"unknown(1)\"]\n", "[]\n"] {
        let mut driver = StdioDriver::new(FailingBackend, Cursor::new(input), &mut output);
        let error = driver
            .process_next_batch()
            .expect_err("request should fail");
        if input == "[]\n" {
            assert_eq!(error, "execution failed");
        }
    }
    assert!(output.is_empty());
}

#[test]
/// Rejects missing or malformed responses and preserves the exact instruction frame.
fn backend_response_errors() {
    for response in ["", "\"01\"", "\"03\"\n", "01\n"] {
        let mut output = Vec::new();
        let mut backend = StdioBackend::new(&mut output, Cursor::new(response));
        assert!(backend.execute_batch(&[]).is_err());
        assert_eq!(output, b"[]\n");
    }
}

struct BrokenStream;

impl Read for BrokenStream {
    /// Injects an instruction or result read failure.
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("read failed"))
    }
}

impl Write for BrokenStream {
    /// Injects a message write failure.
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("write failed"))
    }

    /// Injects a message flush failure.
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("flush failed"))
    }
}

#[test]
/// Propagates transport errors instead of treating them as empty messages or results.
fn transport_errors() {
    let mut backend = StdioBackend::new(BrokenStream, Cursor::new(b"\"\"\n"));
    assert!(
        backend
            .execute_batch(&[])
            .expect_err("write fails")
            .contains("write failed")
    );
    let mut backend = StdioBackend::new(Vec::new(), BrokenStream);
    assert!(
        backend
            .execute_batch(&[])
            .expect_err("read fails")
            .contains("read failed")
    );
    let mut driver = StdioDriver::new(FailingBackend, BrokenStream, Vec::new());
    assert!(
        driver
            .process_next_batch()
            .expect_err("read fails")
            .contains("read failed")
    );
}
