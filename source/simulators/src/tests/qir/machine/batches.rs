// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Where a context stops, and what its batches hold: it runs on until
//! it needs an outcome its batch has not given, or the program ends, and
//! yields its batch there; a trap after a batch comes once it is yielded.

use super::super::{
    SIZE, binary, branch, compare, gate, jump, measure, op, program, read, record, record_bool,
    record_int, record_result, reset, ret, set, write,
};
use super::{batches, batches_then_trap, built, measure_into_register, records};
use crate::MeasurementResult::{One, Zero};
use crate::OutputRecord;
use crate::bytecode::encoding::{
    FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_SRC0_IMM, FLAG_SRC1_IMM, ICMP_EQ, ICMP_ULT,
    OP_ADD, OP_ALLOCA, OP_ICMP, OP_LOAD, OP_MEASURE, OP_READ_LOSS, OP_READ_RESULT,
    OP_READOUT_NOISE, OP_STORE, OP_UDIV, OP_WRITE_RESULT, OPID_CX, OPID_MRESETZ, OPID_MZ, OPID_X,
};
use crate::bytecode::{AdaptiveProgram, Instruction};
use crate::qir::{QirMachine, QirReadout, QirStop, QirTrap};

/// A Bell-like pair of measurements: the first read before the second is
/// made, the first measuring a constant qubit, and one from a register.
#[test]
fn a_context_stops_before_reading_a_result_its_batch_writes() {
    let qubit_from_register = Instruction {
        aux0: OPID_MZ,
        aux1: 3,
        aux2: 0,
        ..op(OP_MEASURE, FLAG_AUX2_IMM)
    };
    for first in [measure(OPID_MZ, 0, 0), qubit_from_register] {
        let bytecode = program(&[&[
            gate(OPID_X, 0, 0),
            set(3, 0),
            first,
            read(OP_READ_RESULT, 0, 0),
            gate(OPID_X, 1, 0),
            measure(OPID_MZ, 1, 1),
            ret(),
        ]]);
        assert_eq!(
            batches(&bytecode),
            [
                "x(qubit 0), mz(qubit 0, result 0)",
                "x(qubit 1), mz(qubit 1, result 1)"
            ]
        );
    }
}

/// A result read again, after the batch that wrote it: the gate after the
/// second read joins the next measurement's batch.
#[test]
fn a_read_of_an_earlier_batch_s_result_does_not_stop_a_context() {
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 0),
        read(OP_READ_RESULT, 0, 0),
        gate(OPID_X, 1, 0),
        read(OP_READ_RESULT, 1, 0),
        measure(OPID_MZ, 1, 1),
        ret(),
    ]]);
    assert_eq!(
        batches(&bytecode),
        ["mz(qubit 0, result 0)", "x(qubit 1), mz(qubit 1, result 1)"]
    );
}

/// A loss check of a measurement, before the next gate.
#[test]
fn a_context_stops_before_reading_a_loss_its_batch_writes() {
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 0),
        read(OP_READ_LOSS, 0, 0),
        gate(OPID_X, 1, 0),
        ret(),
    ]]);
    assert_eq!(batches(&bytecode), ["mz(qubit 0, result 0)", "x(qubit 1)"]);
}

/// A measurement recorded before the next gate.
#[test]
fn a_context_stops_before_recording_a_result_its_batch_writes() {
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 0),
        record_result(0),
        gate(OPID_X, 1, 0),
        ret(),
    ]]);
    assert_eq!(batches(&bytecode), ["mz(qubit 0, result 0)", "x(qubit 1)"]);
}

/// A measured result overwritten, before the next gate.
#[test]
fn a_context_stops_before_writing_a_result_its_batch_writes() {
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 0),
        write(1, 0),
        gate(OPID_X, 1, 0),
        ret(),
    ]]);
    assert_eq!(batches(&bytecode), ["mz(qubit 0, result 0)", "x(qubit 1)"]);
}

/// A qubit measured twice into the same result, the second time by a
/// constant result and by one in a register.
#[test]
fn a_context_stops_before_a_quantum_call_writing_a_result_its_batch_writes() {
    for second in [measure(OPID_MZ, 0, 0), measure_into_register(0, 3)] {
        let bytecode = program(&[&[measure(OPID_MZ, 0, 0), gate(OPID_X, 0, 0), second, ret()]]);
        assert_eq!(
            batches(&bytecode),
            ["mz(qubit 0, result 0), x(qubit 0)", "mz(qubit 0, result 0)"]
        );
    }
}

/// A noisy measurement, as compiled for Stim: `mz` into result 0, then
/// readout noise on result 0.
#[test]
fn a_context_stops_before_readout_noise_on_a_result_its_batch_measures() {
    let noise = Instruction {
        aux0: 0.25_f64.to_bits(),
        aux1: 0.5_f64.to_bits(),
        aux2: 0,
        ..op(
            OP_READOUT_NOISE,
            FLAG_AUX0_IMM | FLAG_AUX1_IMM | FLAG_AUX2_IMM,
        )
    };
    let bytecode = program(&[&[measure(OPID_MZ, 0, 0), noise, ret()]]);
    assert_eq!(
        batches(&bytecode),
        [
            "mz(qubit 0, result 0)",
            "readout_noise(zero_as_one 0.25, one_as_zero 0.5, result 0)"
        ]
    );
}

/// A measurement into result 0, then the value 2, held in a register,
/// written to result 0: the context stops before the write, which traps on
/// the next resume.
#[test]
fn a_context_yields_its_batch_before_a_second_write_that_traps() {
    let write_register = binary(OP_WRITE_RESULT, FLAG_SRC1_IMM, 0, 3, 0);
    let bytecode = program(&[&[measure(OPID_MZ, 0, 0), set(3, 2), write_register, ret()]]);
    assert_eq!(
        batches_then_trap(&bytecode),
        (
            vec!["mz(qubit 0, result 0)".to_string()],
            QirTrap::NotABit {
                instruction: 2,
                value: 2
            }
        )
    );
}

/// A measurement into result 0, then a measurement of the qubit just
/// past the program's, from a register, into result 0, as a constant and
/// from a register: the context stops before the quantum call, which
/// traps on the next resume.
#[test]
fn a_context_yields_its_batch_before_a_second_writing_quantum_call_that_traps() {
    let into_constant = Instruction {
        aux0: OPID_MZ,
        aux1: 2,
        aux2: 0,
        ..op(OP_MEASURE, FLAG_AUX2_IMM)
    };
    let into_register = Instruction {
        aux0: OPID_MZ,
        aux1: 2,
        aux2: 3,
        ..op(OP_MEASURE, 0)
    };
    for (name, second) in [("constant", into_constant), ("register", into_register)] {
        let bytecode = program(&[&[
            measure(OPID_MZ, 0, 0),
            set(2, u64::from(SIZE)),
            set(3, 0),
            second,
            ret(),
        ]]);
        assert_eq!(
            batches_then_trap(&bytecode),
            (
                vec!["mz(qubit 0, result 0)".to_string()],
                QirTrap::NoSuchQubit {
                    instruction: 3,
                    qubit: u64::from(SIZE),
                    qubits: SIZE
                }
            ),
            "result {name}"
        );
    }
}

/// A measurement in the batch, then an instruction that traps but writes
/// no result the batch writes: a division by a register holding 0, a
/// write of 2 to result 1, a measurement of the qubit just past the
/// program's into result 1, a write to, a measurement into and a read of
/// result 2^32 + 1, which is the pending result 1 cut to 32 bits, and a
/// load and a store past the context's memory. The context does not stop
/// before any of them, so the trap comes at once, the measurement not
/// yielded.
#[test]
#[allow(clippy::too_many_lines)]
fn a_trap_at_an_instruction_the_context_does_not_stop_before_comes_at_once() {
    let measure_past = Instruction {
        aux0: OPID_MZ,
        aux1: 2,
        aux2: 1,
        ..op(OP_MEASURE, FLAG_AUX2_IMM)
    };
    let cases = [
        (
            "division",
            vec![
                measure(OPID_MZ, 0, 0),
                binary(OP_UDIV, FLAG_SRC0_IMM, 1, 5, 1),
                ret(),
            ],
            QirTrap::DivisionByZero { instruction: 1 },
        ),
        (
            "write",
            vec![
                measure(OPID_MZ, 0, 0),
                set(3, 2),
                binary(OP_WRITE_RESULT, FLAG_SRC1_IMM, 0, 3, 1),
                ret(),
            ],
            QirTrap::NotABit {
                instruction: 2,
                value: 2,
            },
        ),
        (
            "call",
            vec![measure(OPID_MZ, 0, 0), set(2, 4), measure_past, ret()],
            QirTrap::NoSuchQubit {
                instruction: 2,
                qubit: 4,
                qubits: 4,
            },
        ),
        (
            "write past the results",
            vec![
                measure(OPID_MZ, 0, 1),
                set(3, (1 << 32) + 1),
                binary(OP_WRITE_RESULT, FLAG_SRC0_IMM, 0, 1, 3),
                ret(),
            ],
            QirTrap::NoSuchResult {
                instruction: 2,
                result: (1 << 32) + 1,
                results: 4,
            },
        ),
        (
            "measurement past the results",
            vec![
                measure(OPID_MZ, 0, 1),
                set(3, (1 << 32) + 1),
                measure_into_register(1, 3),
                ret(),
            ],
            QirTrap::NoSuchResult {
                instruction: 2,
                result: (1 << 32) + 1,
                results: 4,
            },
        ),
        (
            "read past the results",
            vec![
                measure(OPID_MZ, 0, 1),
                set(3, (1 << 32) + 1),
                binary(OP_READ_RESULT, 0, 2, 3, 0),
                ret(),
            ],
            QirTrap::NoSuchResult {
                instruction: 2,
                result: (1 << 32) + 1,
                results: 4,
            },
        ),
        (
            "load past memory",
            vec![
                measure(OPID_MZ, 0, 0),
                set(3, 5),
                binary(OP_LOAD, 0, 1, 3, 0),
                ret(),
            ],
            QirTrap::LoadPastMemory {
                instruction: 2,
                address: 5,
                words: 0,
            },
        ),
        (
            "store past memory",
            vec![
                measure(OPID_MZ, 0, 0),
                set(3, 5),
                binary(OP_STORE, 0, 0, 1, 3),
                ret(),
            ],
            QirTrap::StorePastMemory {
                instruction: 2,
                address: 5,
                words: 0,
            },
        ),
    ];
    for (name, instructions, expected) in cases {
        let bytecode = program(&[&instructions]);
        assert_eq!(batches_then_trap(&bytecode), (vec![], expected), "{name}");
    }
}

/// Three measurements into different results, `mz` and `mresetz` by
/// constants and `mz` by a register: one batch.
#[test]
fn a_measurement_does_not_stop_a_context() {
    let bytecode = program(&[&[
        set(3, 2),
        measure(OPID_MZ, 0, 0),
        measure(OPID_MRESETZ, 1, 1),
        measure_into_register(2, 3),
        ret(),
    ]]);
    assert_eq!(
        batches(&bytecode),
        ["mz(qubit 0, result 0), mresetz(qubit 1, result 1), mz(qubit 2, result 2)"]
    );
}

/// `MeasurementComparison` of the Python package's tests
/// (`tests-integration/resources/adaptive/output/MeasurementComparison.ll`),
/// written by hand as the bytecode of `AdaptiveProfilePass`, but for its
/// functions `X`, `CNOT` and `Reset`, each a wrapper of one gate, written as
/// that gate, so with no registers for the constant arguments of their
/// calls, which the pass materialises. It puts qubits 0 and 1 in 1, measures
/// them into results 0 and 1, then reads them: each read into a register
/// stored in memory, and a branch on one; it records four `Bool`s from
/// memory, after a tuple's header, which records nothing.
fn measurement_comparison() -> AdaptiveProgram<u64> {
    // Registers 0 to 7 hold the addresses of its eight `alloca`s, in the
    // order of the source: 9, 12, 16, 19, 20, 21, 22, 23.
    let alloca = |n: u64| binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, n, 1, n);
    let store = |value: u64, address: u64| binary(OP_STORE, 0, 0, value, address);
    let store_constant =
        |value: u64, address: u64| binary(OP_STORE, FLAG_SRC0_IMM, 0, value, address);
    let load = |dst: u64, address: u64| binary(OP_LOAD, 0, dst, address, 0);
    let equals = |dst: u64, a: u64, b: u64, flags: u64| Instruction {
        dst,
        src0: a,
        src1: b,
        opcode: compare(OP_ICMP, ICMP_EQ).opcode | flags,
        ..Instruction::default()
    };
    let mut block_0: Vec<Instruction<u64>> = (0..8).map(alloca).collect();
    block_0.extend([
        gate(OPID_X, 0, 0),
        gate(OPID_CX, 0, 1),
        measure(OPID_MZ, 0, 0),
        measure(OPID_MZ, 1, 1),
        reset(0),
        reset(1),
        read(OP_READ_RESULT, 8, 0),
        store(8, 0),
        read(OP_READ_RESULT, 9, 1),
        equals(10, 9, 0, FLAG_SRC1_IMM),
        store(10, 1),
        read(OP_READ_RESULT, 11, 0),
        read(OP_READ_RESULT, 12, 1),
        equals(13, 11, 12, 0),
        store(13, 2),
        read(OP_READ_RESULT, 14, 0),
        equals(15, 14, 0, FLAG_SRC1_IMM),
        branch(15, 1, 2),
    ]);
    let mut bytecode = program(&[
        &block_0,
        &[store_constant(0, 3), jump(3)],
        &[store_constant(1, 3), jump(3)],
        &[
            load(16, 3),
            store(16, 4),
            load(17, 0),
            store(17, 5),
            load(18, 1),
            store(18, 6),
            load(19, 2),
            store(19, 7),
            record(2, 0),
            load(20, 5),
            record_bool(20),
            load(21, 6),
            record_bool(21),
            load(22, 7),
            record_bool(22),
            load(23, 4),
            record_bool(23),
            ret(),
        ],
    ]);
    bytecode.num_qubits = 2;
    bytecode.num_results = 2;
    bytecode.num_registers = 24;
    bytecode
}

/// `MeasurementComparison`, whose reads all follow its measurements: its
/// gates, measurements and resets go in one batch, as `qir/design.md`
/// traces it.
#[test]
fn a_program_reading_only_after_its_measurements_yields_one_batch() {
    assert_eq!(
        batches(&measurement_comparison()),
        [concat!(
            "x(qubit 0), cx(qubit 0, qubit 1), mz(qubit 0, result 0), ",
            "mz(qubit 1, result 1), reset(qubit 0), reset(qubit 1)"
        )]
    );
}

/// A loop of 20 000 rounds of `x` on qubit 0, counted in register 0 and
/// reading no result: the context never stops, so its one batch holds
/// every quantum call (twice the 10 000 at which a batcher of a bounded buffer
/// would cut).
#[test]
fn a_batch_holds_every_quantum_call_a_context_makes_before_it_stops() {
    const ROUNDS: usize = 20_000;
    let counts = binary(OP_ADD, FLAG_SRC1_IMM, 0, 0, 1);
    let below = Instruction {
        dst: 1,
        src0: 0,
        src1: ROUNDS as u64,
        opcode: compare(OP_ICMP, ICMP_ULT).opcode | FLAG_SRC1_IMM,
        ..Instruction::default()
    };
    let bytecode = program(&[
        &[set(0, 0), jump(1)],
        &[gate(OPID_X, 0, 0), counts, below, branch(1, 1, 2)],
        &[ret()],
    ]);
    let batches = batches(&bytecode);
    assert_eq!(batches.len(), 1, "one batch");
    assert_eq!(
        batches[0],
        vec!["x(qubit 0)"; ROUNDS].join(", "),
        "every round's call"
    );
}

/// `MeasurementComparison`, both outcomes `One`, as its qubits are: the
/// four `Bool`s in the order it records them, the output of the Python
/// package's test (`MeasurementComparison.out`).
#[test]
fn a_context_ends_with_the_records_its_program_makes_in_order() {
    let records = records(&measurement_comparison(), |_| QirReadout::One);
    assert_eq!(
        records,
        [
            OutputRecord::Bool(true),
            OutputRecord::Bool(false),
            OutputRecord::Bool(true),
            OutputRecord::Bool(true)
        ]
    );
}

/// A program that ends with a gate.
#[test]
fn a_context_yields_its_last_batch_before_it_ends() {
    let bytecode = program(&[&[gate(OPID_X, 0, 0), ret()]]);
    let program = built(&bytecode);
    let machine = QirMachine::new(&program);
    let mut context = machine.start_shot();
    let stop = machine.resume(&mut context, &[]).expect("a context starts");
    assert!(matches!(stop, QirStop::Batch([_])), "{stop:?}");
    let stop = machine
        .resume(&mut context, &[])
        .expect("the gate has no outcome");
    assert!(matches!(stop, QirStop::Ended(_)), "{stop:?}");
}

/// A program that records an integer, and one that records nothing (so
/// every result), each resumed after its end.
#[test]
fn an_ended_context_gives_its_records_again() {
    let cases = [
        (
            program(&[&[set(0, 42), record_int(0), ret()]]),
            vec![OutputRecord::Int(42)],
        ),
        (
            program(&[&[write(1, 1), ret()]]),
            [Zero, One, Zero, Zero].map(OutputRecord::Result).to_vec(),
        ),
    ];
    for (bytecode, records) in cases {
        let program = built(&bytecode);
        let machine = QirMachine::new(&program);
        let mut context = machine.start_shot();
        for _ in 0..2 {
            assert_eq!(
                machine.resume(&mut context, &[]),
                Ok(QirStop::Ended(&records[..]))
            );
        }
    }
}
