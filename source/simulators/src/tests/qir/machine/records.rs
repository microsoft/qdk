// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a context records: its records in order, the whole register for
//! each kind, and every result when the program records nothing.

use super::super::{
    branch, measure, op, program, record, record_bool, record_int, record_result, ret, set, write,
};
use super::{both, records, trap};
use crate::MeasurementResult::{Loss, One, Zero};
use crate::OutputRecord;
use crate::bytecode::Instruction;
use crate::bytecode::encoding::{FLAG_SRC0_IMM, OP_RECORD_OUTPUT, OPID_MZ};
use crate::qir::{QirReadout, QirTrap};

/// A program with no `RECORD_OUTPUT`: two results measured, one written,
/// one untouched.
#[test]
fn a_program_that_records_nothing_records_every_result() {
    let bytecode = program(&[&[
        measure(OPID_MZ, 0, 0),
        measure(OPID_MZ, 1, 1),
        write(1, 3),
        ret(),
    ]]);
    let records = records(&bytecode, |result| {
        if result == 0 {
            QirReadout::One
        } else {
            QirReadout::Loss
        }
    });
    // DISCUSS COMPATIBILITY: this behavior was present in the package's
    // `run_adaptive`, which takes every result from the simulator after
    // `run_shot` (which records nothing here), but it makes a program's
    // output depend on whether it records anything, so we keep it here
    // meanwhile but we expect discussion/feedback as part of the review.
    // We think this is OK because it is the output users of `run_adaptive`
    // get today, and every program Q# emits records (all the corpus's
    // cases do), so we recommend keeping it.
    assert_eq!(records, [One, Loss, Zero, One].map(OutputRecord::Result));
}

/// A program that measures result 0, then branches on register 3, which
/// holds 0, past the block recording it: its run records nothing, though
/// the program holds a record.
#[test]
fn a_program_holding_a_record_its_run_does_not_reach_records_nothing() {
    let bytecode = program(&[
        &[measure(OPID_MZ, 0, 0), branch(3, 1, 2)],
        &[record_result(0), ret()],
        &[ret()],
    ]);
    // COMPATIBILITY NOTE: this behavior was present in the package's
    // `run_adaptive` (qdk_package, qir_simulation/cpu_simulators.rs), whose
    // `has_output_recording` looks for a `RECORD_OUTPUT` among the
    // program's instructions, not the run's, hence we keep it so that the
    // machine is a drop-in replacement.
    assert_eq!(records(&bytecode, |_| QirReadout::One), []);
}

/// A program that measures result 0 and records only one output marker:
/// that of an empty tuple (kind 2), as every Q# entry point returning
/// `Unit` does, or of an empty array (kind 1).
#[test]
fn a_program_recording_only_an_output_marker_records_nothing() {
    for (marker, kind) in [("tuple", 2), ("array", 1)] {
        let bytecode = program(&[&[measure(OPID_MZ, 0, 0), record(kind, 0), ret()]]);
        // COMPATIBILITY NOTE: this behavior was present in the package's
        // `run_adaptive` (qdk_package, qir_simulation/cpu_simulators.rs),
        // whose `has_output_recording` counts every `RECORD_OUTPUT`, a
        // tuple's or an array's marker too, and then takes `run_shot`'s
        // records, to which a marker adds none, hence we keep it so that
        // the machine is a drop-in replacement.
        assert_eq!(records(&bytecode, |_| QirReadout::One), [], "{marker}");
        assert_eq!(both(&bytecode).1, [], "the records of `run_shot`, {marker}");
    }
}

/// A register holding 0, 1, 2 or 3 (2 as `add i1 true, true` leaves it
/// in the pass's 64-bit registers), recorded as a `Bool`.
#[test]
fn a_bool_record_of_a_value_other_than_0_or_1_is_a_trap() {
    let recording = |value| program(&[&[set(1, value), record_bool(1), ret()]]);
    for (value, recorded) in [(0, false), (1, true)] {
        assert_eq!(
            records(&recording(value), |_| QirReadout::One),
            [OutputRecord::Bool(recorded)],
            "{value}"
        );
    }
    for value in [2, 3] {
        // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs,
        // `RECORD_OUTPUT` of kind 3) reads any value but 0 as true; the
        // machine traps instead, because such a value can only come from
        // the pass, which does not wrap narrow integer results (`add i1
        // true, true` leaves 2, where LLVM gives false), and reading it as
        // true hides the bug.
        assert_eq!(
            trap(&recording(value)),
            QirTrap::NotABit {
                instruction: 1,
                value
            },
            "{value}"
        );
    }
}

/// A register holding the bits of the double -1.5, recorded as a
/// `Double`.
#[test]
fn a_double_record_is_its_register_s_bits_read_as_a_double() {
    let bytecode = program(&[&[set(1, (-1.5f64).to_bits()), record(5, 1), ret()]]);
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Double(-1.5)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `RECORD_OUTPUT` of kind 5: `resolve_f64`, the
    // register's bits as a double), hence we keep it so that the machine
    // is a drop-in replacement.
    assert_eq!(machine, run_shot);
}

/// The constant 1 recorded as a `Bool`, an `Int` and a `Double` (the
/// double whose bits are 1), while register 1 holds 0, then register 1,
/// set to 7, recorded as an `Int`.
#[test]
fn a_record_reads_its_value_as_a_constant_or_a_register_by_its_flag() {
    let constant = |kind| Instruction {
        src0: 1,
        aux1: kind,
        ..op(OP_RECORD_OUTPUT, FLAG_SRC0_IMM)
    };
    let bytecode = program(&[&[
        constant(3),
        constant(4),
        constant(5),
        set(1, 7),
        record_int(1),
        ret(),
    ]]);
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(
        machine,
        [
            OutputRecord::Bool(true),
            OutputRecord::Int(1),
            OutputRecord::Double(f64::from_bits(1)),
            OutputRecord::Int(7),
        ]
    );
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `RECORD_OUTPUT`: `resolve_u64`, `resolve_i64`
    // and `resolve_f64` of its value by its flag), hence we keep it so that
    // the machine is a drop-in replacement.
    assert_eq!(machine, run_shot);
}
