// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    BatchResult, Driver,
    Parameter::{Float, QubitId, ResultId},
    QuantumBackend, QuantumBackendInstruction,
    QuantumBackendResult::{Error, One, Zero},
    StreamBackend, StreamDriver, instructions as instr,
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
/// Preserves every instruction shape, operand order, and result variant over two buffered pipes.
fn backend_driver_round_trip() -> Result<(), String> {
    let mut instructions = Vec::new();
    for (ids, parameters) in [
        (
            &[instr::CCX, instr::AND_ADJ][..],
            smallvec![QubitId(1), QubitId(2), QubitId(3)],
        ),
        (
            &[instr::CX, instr::CY, instr::CZ, instr::SWAP][..],
            smallvec![QubitId(1), QubitId(2)],
        ),
        (
            &[
                instr::H,
                instr::S,
                instr::SADJ,
                instr::SX,
                instr::T,
                instr::TADJ,
                instr::X,
                instr::Y,
                instr::Z,
                instr::RESET,
                instr::MZ,
                instr::MZ_RESET,
            ][..],
            smallvec![QubitId(1)],
        ),
        (
            &[instr::RX, instr::RY, instr::RZ][..],
            smallvec![QubitId(3), Float(-0.125)],
        ),
        (
            &[instr::RXX, instr::RYY, instr::RZZ][..],
            smallvec![QubitId(2), QubitId(3), Float(-0.25)],
        ),
        (
            &[instr::MZ_STORE, instr::MZ_STORE_RESET][..],
            smallvec![QubitId(3), ResultId(7)],
        ),
        (&[instr::READ_RESULT][..], smallvec![ResultId(7)]),
    ] {
        instructions.extend(ids.iter().map(|&instruction_id| QuantumBackendInstruction {
            instruction_id,
            parameters: parameters.clone(),
        }));
    }
    let cases = [
        (
            instructions,
            BatchResult {
                results: smallvec![Zero, One, Error],
            },
        ),
        (
            vec![QuantumBackendInstruction {
                instruction_id: instr::READ_RESULT,
                parameters: smallvec![ResultId(7)],
            }],
            BatchResult {
                results: smallvec![One],
            },
        ),
        (
            vec![QuantumBackendInstruction {
                instruction_id: instr::X,
                parameters: smallvec![QubitId(2)],
            }],
            BatchResult::default(),
        ),
        (vec![], BatchResult::default()),
    ];
    assert_stream_round_trips(&cases)
}

#[test]
/// Preserves numeric boundaries and messages larger than stream buffers, then clears retained state.
fn numeric_boundaries_and_large_batches() -> Result<(), String> {
    let mut instructions = Vec::new();
    for angle in [
        0.0,
        -0.0,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        f64::EPSILON,
        std::f64::consts::PI,
        f64::from_bits(1.0_f64.to_bits() + 1),
    ] {
        instructions.push(QuantumBackendInstruction {
            instruction_id: instr::RZZ,
            parameters: smallvec![QubitId(0), QubitId(usize::MAX), Float(angle)],
        });
    }
    instructions.push(QuantumBackendInstruction {
        instruction_id: instr::MZ_STORE,
        parameters: smallvec![QubitId(usize::MAX), ResultId(0)],
    });
    instructions.extend((0..16_384).map(|index| QuantumBackendInstruction {
        instruction_id: instr::READ_RESULT,
        parameters: smallvec![ResultId(if index % 2 == 0 { 0 } else { usize::MAX })],
    }));
    let results = (0..16_384)
        .map(|index| [Zero, One, Error][index % 3])
        .collect();
    assert_stream_round_trips(&[
        (instructions, BatchResult { results }),
        (vec![], BatchResult::default()),
        (
            vec![QuantumBackendInstruction {
                instruction_id: instr::MZ,
                parameters: smallvec![QubitId(0)],
            }],
            BatchResult {
                results: smallvec![One],
            },
        ),
    ])
}

/// Checks typed values independently of the wire format, including signed-zero preservation.
fn assert_stream_round_trips(
    cases: &[(Vec<QuantumBackendInstruction>, BatchResult)],
) -> Result<(), String> {
    let (instruction_reader, instruction_writer) = io::pipe().map_err(|e| e.to_string())?;
    let (result_reader, result_writer) = io::pipe().map_err(|e| e.to_string())?;
    let (batch_sender, batch_receiver) = channel();
    let (result_sender, result_receiver) = channel();
    std::thread::scope(|scope| {
        let driver = scope.spawn(|| {
            let mut driver = StreamDriver::new(
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
            let mut backend = StreamBackend::new(BufWriter::new(instruction_writer), result_reader);
            for (instructions, expected) in cases {
                assert_eq!(&backend.execute_batch(instructions)?, expected);
            }
            Ok::<_, String>(())
        });
        for (expected, result) in cases {
            let received = batch_receiver
                .recv_timeout(Duration::from_secs(10))
                .map_err(|error| error.to_string())?;
            assert_eq!(&received, expected);
            for (received, expected) in received.iter().zip(expected) {
                for (received, expected) in received.parameters.iter().zip(&expected.parameters) {
                    if let (Float(received), Float(expected)) = (received, expected) {
                        assert_eq!(received.to_bits(), expected.to_bits());
                    }
                }
            }
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
    let mut driver = StreamDriver::new(FailingBackend, Cursor::new(b""), &mut output);
    assert_eq!(driver.process_next_batch(), Ok(false));
    for input in ["[", "[]", "invalid\n", "[\"unknown(1)\"]\n", "[]\n"] {
        let mut driver = StreamDriver::new(FailingBackend, Cursor::new(input), &mut output);
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
        let mut backend = StreamBackend::new(&mut output, Cursor::new(response));
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
    let mut backend = StreamBackend::new(BrokenStream, Cursor::new(b"\"\"\n"));
    assert!(
        backend
            .execute_batch(&[])
            .expect_err("write fails")
            .contains("write failed")
    );
    let mut backend = StreamBackend::new(Vec::new(), BrokenStream);
    assert!(
        backend
            .execute_batch(&[])
            .expect_err("read fails")
            .contains("read failed")
    );
    let mut driver = StreamDriver::new(FailingBackend, BrokenStream, Vec::new());
    assert!(
        driver
            .process_next_batch()
            .expect_err("read fails")
            .contains("read failed")
    );
}
