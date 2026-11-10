// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot's results: what a read gives for each outcome, the outcomes
//! that answer a batch, a result with no value, and the traps on a result
//! past the program's or a value that is no result.

use super::super::{
    SIZE, binary, gate, measure, program, read, record, record_bool, record_int, record_result,
    ret, set, write,
};
use super::{
    answers, batches_then_trap_answered, both, built, measure_into_register, records, run, trap,
};
use crate::MeasurementResult::{One, Zero};
use crate::OutputRecord;
use crate::bytecode::AdaptiveProgram;
use crate::bytecode::encoding::{
    FLAG_SRC0_IMM, FLAG_SRC1_IMM, OP_READ_LOSS, OP_READ_RESULT, OP_WRITE_RESULT, OPID_MZ, OPID_X,
};
use crate::qir::{QirMachine, QirOutcome, QirReadout, QirStop, QirTrap};

/// A measurement read and recorded as a `Bool`, for each outcome.
#[test]
fn a_read_result_is_one_only_for_the_outcome_one() {
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 0),
        read(OP_READ_RESULT, 0, 0),
        record_bool(0),
        ret(),
    ]]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_READ_RESULT`: a loss reads 0), hence we
    // keep it so that the machine is a drop-in replacement.
    for (outcome, read) in [
        (QirReadout::One, true),
        (QirReadout::Zero, false),
        (QirReadout::Loss, false),
    ] {
        assert_eq!(
            records(&bytecode, |_| outcome),
            [OutputRecord::Bool(read)],
            "{outcome:?}"
        );
    }
}

/// Qubit 0 flipped and measured into result 2, read from the constant 2,
/// while register 2 holds 0, and from register 3, which holds 2: each
/// read is of result 2, which holds `One`.
#[test]
fn read_result_reads_its_result_as_a_constant_or_a_register_by_its_flag() {
    let bytecode = program(&[&[
        gate(OPID_X, 0, 0),
        measure(OPID_MZ, 0, 2),
        read(OP_READ_RESULT, 1, 2),
        set(3, 2),
        binary(OP_READ_RESULT, 0, 0, 3, 0),
        record_int(1),
        record_int(0),
        ret(),
    ]]);
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(1), OutputRecord::Int(1)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_READ_RESULT`: `resolve_u64` of its result
    // by its flag), hence we keep it so that the machine is a drop-in
    // replacement.
    assert_eq!(machine, run_shot);
}

/// A loss check of a measurement into result 1 recorded as a `Bool`, for
/// each outcome.
#[test]
fn a_read_loss_is_one_only_for_the_outcome_loss() {
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 1),
        read(OP_READ_LOSS, 0, 1),
        record_bool(0),
        ret(),
    ]]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_READ_LOSS`), hence we keep it so that the
    // machine is a drop-in replacement.
    for (outcome, lost) in [
        (QirReadout::Loss, true),
        (QirReadout::One, false),
        (QirReadout::Zero, false),
    ] {
        assert_eq!(
            records(&bytecode, |_| outcome),
            [OutputRecord::Bool(lost)],
            "{outcome:?}"
        );
    }
}

/// Qubit 0 measured into result 2, lost, checked for loss from the
/// constant 2, while register 2 holds 0, and from register 3, which holds
/// 2: each check is of result 2.
#[test]
fn read_loss_reads_its_result_as_a_constant_or_a_register_by_its_flag() {
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 2),
        read(OP_READ_LOSS, 1, 2),
        set(3, 2),
        binary(OP_READ_LOSS, 0, 0, 3, 0),
        record_int(1),
        record_int(0),
        ret(),
    ]]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_READ_LOSS`: `resolve_u64` of its result by
    // its flag), hence we keep it so that the machine is a drop-in
    // replacement.
    assert_eq!(
        records(&bytecode, |_| QirReadout::Loss),
        [OutputRecord::Int(1), OutputRecord::Int(1)]
    );
}

/// A result written 1, and one written 0, then read and recorded: no
/// quantum call carries either.
#[test]
fn a_written_result_is_read_back_without_a_call() {
    for (bit, read_back, recorded) in [(1, true, One), (0, false, Zero)] {
        let bytecode = program(&[&[
            write(bit, 0),
            read(OP_READ_RESULT, 0, 0),
            record_bool(0),
            record_result(0),
            ret(),
        ]]);
        let (batches, records) = run(&bytecode, |_| QirReadout::Zero);
        assert!(batches.is_empty(), "{batches:?}");
        assert_eq!(
            records.expect("the context does not trap"),
            [
                OutputRecord::Bool(read_back),
                OutputRecord::Result(recorded)
            ]
        );
    }
}

/// A readout with no value, for another reason than a loss.
const NO_VALUE: QirReadout = QirReadout::Missing;

/// The records of a context of `bytecode`, or the trap it reaches, each
/// result as `answer` gives it.
fn records_or_trap_answered(
    bytecode: &AdaptiveProgram<u64>,
    answer: impl Fn(u32) -> QirReadout,
) -> Result<Vec<OutputRecord>, QirTrap> {
    run(bytecode, answer).1
}

/// Qubits 0 and 1 measured into results 0 and 1, then a read, a loss
/// check or a record of result 1, and a record of result 0: result 1's
/// readout has no value, or is `One`, which runs.
#[test]
fn a_use_of_a_result_with_no_value_is_a_trap() {
    for (name, instruction, expected) in [
        (
            "read_result",
            read(OP_READ_RESULT, 0, 1),
            vec![OutputRecord::Result(One)],
        ),
        (
            "read_loss",
            read(OP_READ_LOSS, 0, 1),
            vec![OutputRecord::Result(One)],
        ),
        (
            "record",
            record_result(1),
            vec![OutputRecord::Result(One), OutputRecord::Result(One)],
        ),
    ] {
        let bytecode = program(&[&[
            measure(OPID_MZ, 0, 0),
            measure(OPID_MZ, 1, 1),
            instruction,
            record_result(0),
            ret(),
        ]]);
        let no_value = |result| {
            if result == 1 {
                NO_VALUE
            } else {
                QirReadout::One
            }
        };
        assert_eq!(
            records_or_trap_answered(&bytecode, no_value),
            Err(QirTrap::MissingValue {
                instruction: 2,
                result: 1
            }),
            "{name}"
        );
        assert_eq!(
            records_or_trap_answered(&bytecode, |_| QirReadout::One),
            Ok(expected),
            "{name} of a result with a value"
        );
    }
}

/// Qubits 0 and 1 measured into results 0 and 1, a read of result 1 that
/// yields them, result 0's readout with no value, then qubit 2 measured
/// into result 2 and a read or a loss check of result 0. The context does
/// not stop before that use, so its trap comes at once, the measurement of
/// qubit 2 not yielded.
#[test]
fn a_use_of_a_result_with_no_value_traps_before_the_batch_is_yielded() {
    for (name, op) in [("read_result", OP_READ_RESULT), ("read_loss", OP_READ_LOSS)] {
        let bytecode = program(&[&[
            measure(OPID_MZ, 0, 0),
            measure(OPID_MZ, 1, 1),
            read(OP_READ_RESULT, 2, 1),
            measure(OPID_MZ, 2, 2),
            read(op, 3, 0),
            ret(),
        ]]);
        let no_value = |result| {
            if result == 0 {
                NO_VALUE
            } else {
                QirReadout::One
            }
        };
        assert_eq!(
            batches_then_trap_answered(&bytecode, no_value),
            (
                vec!["mz(qubit 0, result 0), mz(qubit 1, result 1)".to_string()],
                QirTrap::MissingValue {
                    instruction: 4,
                    result: 0
                }
            ),
            "{name}"
        );
    }
}

/// A program with no `RECORD_OUTPUT` measuring qubits 0 and 1 into results
/// 0 and 1: result 1's readout has no value, or every readout is `One`.
#[test]
fn the_end_of_a_program_that_records_nothing_traps_on_a_result_with_no_value() {
    let bytecode = program(&[&[measure(OPID_MZ, 0, 0), measure(OPID_MZ, 1, 1), ret()]]);
    let no_value = |result| {
        if result == 1 {
            NO_VALUE
        } else {
            QirReadout::One
        }
    };
    // It records every result, so it records result 1, which traps as its
    // record would.
    assert_eq!(
        records_or_trap_answered(&bytecode, no_value),
        Err(QirTrap::MissingValue {
            instruction: 2,
            result: 1
        })
    );
    assert_eq!(
        records_or_trap_answered(&bytecode, |_| QirReadout::One),
        Ok([One, One, Zero, Zero].map(OutputRecord::Result).to_vec())
    );
}

/// Qubits 0 and 1 measured into results 0 and 1, and a record of result 0
/// only: result 1's readout has no value.
#[test]
fn a_result_with_no_value_that_the_program_does_not_use_is_no_trap() {
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 0),
        measure(OPID_MZ, 1, 1),
        record_result(0),
        ret(),
    ]]);
    assert_eq!(
        records_or_trap_answered(&bytecode, |result| if result == 1 {
            NO_VALUE
        } else {
            QirReadout::One
        }),
        Ok(vec![OutputRecord::Result(One)])
    );
}

/// Result 1 measured with no value, then written 1 by a `WRITE_RESULT`,
/// and recorded.
#[test]
fn a_write_gives_a_result_with_no_value_a_value() {
    let written = program(&[&[measure(OPID_MZ, 0, 1), write(1, 1), record_result(1), ret()]]);
    assert_eq!(
        records_or_trap_answered(&written, |_| NO_VALUE),
        Ok(vec![OutputRecord::Result(One)])
    );
}

/// Result 1 measured with no value, then measured again, `One`, and
/// recorded.
#[test]
fn a_new_measurement_gives_a_result_with_no_value_a_value() {
    let measured_again = program(&[&[
        measure(OPID_MZ, 0, 1),
        measure(OPID_MZ, 0, 1),
        record_result(1),
        ret(),
    ]]);
    let readouts = std::cell::Cell::new(0);
    let first_without_value = |_| {
        readouts.set(readouts.get() + 1);
        if readouts.get() == 1 {
            NO_VALUE
        } else {
            QirReadout::One
        }
    };
    assert_eq!(
        records_or_trap_answered(&measured_again, first_without_value),
        Ok(vec![OutputRecord::Result(One)])
    );
}

/// The two results of a batch, answered last first.
#[test]
fn outcomes_may_come_in_any_order() {
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 0),
        measure(OPID_MZ, 1, 1),
        record_result(0),
        record_result(1),
        ret(),
    ]]);
    let program = built(&bytecode);
    let machine = QirMachine::new(&program);
    let mut context = machine.start_shot();
    machine.resume(&mut context, &[]).expect("a context starts");
    let outcomes = [
        QirOutcome {
            result: 1,
            value: QirReadout::One,
        },
        QirOutcome {
            result: 0,
            value: QirReadout::Zero,
        },
    ];
    let records = [OutputRecord::Result(Zero), OutputRecord::Result(One)];
    assert_eq!(
        machine.resume(&mut context, &outcomes),
        Ok(QirStop::Ended(&records[..]))
    );
}

/// A program measuring two qubits, then recording both.
fn two_measurements() -> AdaptiveProgram<u64> {
    program(&[&[
        measure(OPID_MZ, 0, 0),
        measure(OPID_MZ, 1, 1),
        record_result(0),
        record_result(1),
        ret(),
    ]])
}

/// The resume after the first batch of [`two_measurements`] is answered by
/// `outcomes`.
fn answered(outcomes: &[QirOutcome]) -> Result<(), QirTrap> {
    answered_by(&two_measurements(), outcomes)
}

/// The resume after the first batch of `bytecode` is answered by
/// `outcomes`.
fn answered_by(bytecode: &AdaptiveProgram<u64>, outcomes: &[QirOutcome]) -> Result<(), QirTrap> {
    let program = built(bytecode);
    let machine = QirMachine::new(&program);
    let mut context = machine.start_shot();
    machine.resume(&mut context, &[]).expect("a context starts");
    machine.resume(&mut context, outcomes).map(|_| ())
}

/// The first result of two answered, the second measured into a constant
/// result, and into one held in a register.
#[test]
fn a_missing_outcome_is_a_trap() {
    let into_register = program(&[&[
        set(3, 1),
        measure(OPID_MZ, 0, 0),
        measure_into_register(1, 3),
        record_result(1),
        ret(),
    ]]);
    for bytecode in [two_measurements(), into_register] {
        assert_eq!(
            answered_by(
                &bytecode,
                &[QirOutcome {
                    result: 0,
                    value: QirReadout::One
                }]
            ),
            Err(QirTrap::MissingOutcome { result: 1 })
        );
    }
}

/// Two batches measuring qubits 0 and 1, the second in the other order:
/// the first answered, the second not at all.
#[test]
fn a_missing_outcome_names_the_first_quantum_call_of_the_batch_without_one() {
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 0),
        measure(OPID_MZ, 1, 1),
        read(OP_READ_RESULT, 0, 0),
        measure(OPID_MZ, 1, 1),
        measure(OPID_MZ, 0, 0),
        read(OP_READ_RESULT, 0, 0),
        ret(),
    ]]);
    let program = built(&bytecode);
    let machine = QirMachine::new(&program);
    let mut context = machine.start_shot();
    let QirStop::Batch(first) = machine.resume(&mut context, &[]).expect("a context starts") else {
        panic!("the context measures");
    };
    let outcomes = answers(first, |_| QirReadout::One);
    machine
        .resume(&mut context, &outcomes)
        .expect("the first batch is answered");
    assert_eq!(
        machine.resume(&mut context, &[]),
        Err(QirTrap::MissingOutcome { result: 1 })
    );
}

/// Both results answered, and a third: one of the program's results, or
/// one past them (as a buggy resumer might give); and an outcome before
/// the first resume.
#[test]
fn an_outcome_for_no_quantum_call_of_the_batch_is_a_trap() {
    for extra in [2, SIZE, u32::MAX] {
        let outcomes = [0, 1, extra].map(|result| QirOutcome {
            result,
            value: QirReadout::One,
        });
        assert_eq!(
            answered(&outcomes),
            Err(QirTrap::UnexpectedOutcome { result: extra }),
            "result {extra}"
        );
    }
    let outcomes = [0, 1].map(|result| QirOutcome {
        result,
        value: QirReadout::One,
    });
    let program = built(&two_measurements());
    let machine = QirMachine::new(&program);
    assert_eq!(
        machine.resume(&mut machine.start_shot(), &outcomes[..1]),
        Err(QirTrap::UnexpectedOutcome { result: 0 })
    );
}

/// The first result answered twice.
#[test]
fn an_outcome_given_twice_is_a_trap() {
    let outcomes = [0, 0, 1].map(|result| QirOutcome {
        result,
        value: QirReadout::One,
    });
    assert_eq!(
        answered(&outcomes),
        Err(QirTrap::UnexpectedOutcome { result: 0 })
    );
}

/// A measurement into, and a write of, the result just past the program's,
/// from a register, and of result 2^32 + 1, which is result 1 cut to 32
/// bits; and each into the program's last result, from a register, which
/// runs.
#[test]
fn a_result_past_the_program_s_is_a_trap() {
    let write_to_register = binary(OP_WRITE_RESULT, FLAG_SRC0_IMM, 0, 1, 0);
    for (name, instruction) in [
        ("mz", measure_into_register(0, 0)),
        ("write_result", write_to_register),
    ] {
        for result in [u64::from(SIZE), (1 << 32) + 1] {
            let bytecode = program(&[&[set(0, result), instruction, ret()]]);
            assert_eq!(
                trap(&bytecode),
                QirTrap::NoSuchResult {
                    instruction: 1,
                    result,
                    results: SIZE
                },
                "{name} into result {result}"
            );
        }
        let last = program(&[&[set(0, u64::from(SIZE) - 1), instruction, ret()]]);
        assert_eq!(
            records(&last, |_| QirReadout::One),
            [Zero, Zero, Zero, One].map(OutputRecord::Result),
            "{name} into the last result"
        );
    }
}

/// A read, a loss check and a record of the result just past the
/// program's, held in a register, and of result 2^32 + 1, which is result
/// 1 cut to 32 bits; and each of the program's last result, written `One`,
/// which runs.
#[test]
fn a_result_read_past_the_program_s_is_a_trap() {
    let last = u64::from(SIZE) - 1;
    for (name, instruction, expected) in [
        (
            "read_result",
            binary(OP_READ_RESULT, 0, 1, 0, 0),
            vec![OutputRecord::Int(1)],
        ),
        (
            "read_loss",
            binary(OP_READ_LOSS, 0, 1, 0, 0),
            vec![OutputRecord::Int(0)],
        ),
        (
            "record",
            record(0, 0),
            vec![OutputRecord::Result(One), OutputRecord::Int(0)],
        ),
    ] {
        for result in [u64::from(SIZE), (1 << 32) + 1] {
            let bytecode = program(&[&[set(0, result), instruction, ret()]]);
            // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs,
            // `OP_READ_RESULT`, `OP_READ_LOSS`, `OP_RECORD_OUTPUT`) reads
            // `Zero` here; the machine refuses, because the read can never
            // see a written value: it is a producer's bug that would give
            // plausible output (see `QirMachine`'s doc).
            assert_eq!(
                trap(&bytecode),
                QirTrap::NoSuchResult {
                    instruction: 1,
                    result,
                    results: SIZE
                },
                "{name} of result {result}"
            );
        }
        let bytecode = program(&[&[
            write(1, last),
            set(0, last),
            instruction,
            record_int(1),
            ret(),
        ]]);
        assert_eq!(
            records(&bytecode, |_| QirReadout::One),
            expected,
            "{name} of the last result"
        );
    }
}

/// A result written with the value 2, and with 2^32 (whose low 32 bits
/// are 0), from a register.
#[test]
fn writing_a_result_other_than_0_or_1_is_a_trap() {
    let write_register = binary(OP_WRITE_RESULT, FLAG_SRC1_IMM, 0, 0, 0);
    for value in [2, 1 << 32] {
        let bytecode = program(&[&[set(0, value), write_register, ret()]]);
        assert_eq!(
            trap(&bytecode),
            QirTrap::NotABit {
                instruction: 1,
                value
            },
            "{value}"
        );
    }
}
