// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A context as data: started or restarted, cloned, resumed in any order,
//! on another machine of its program, moved to another thread.

use super::super::{
    binary, call, call_return, jump, measure, program, read, record_bool, record_int,
    record_result, ret, set,
};
use super::{answers, built, records, records_or_trap};
use crate::MeasurementResult::{One, Zero};
use crate::OutputRecord;
use crate::bytecode::encoding::{
    FLAG_SRC0_IMM, FLAG_SRC1_IMM, OP_ALLOCA, OP_LOAD, OP_READ_RESULT, OP_SDIV, OP_STORE, OPID_MZ,
    VOID_RETURN,
};
use crate::bytecode::{AdaptiveProgram, Function};
use crate::qir::{
    QirMachine, QirOutcome, QirProgram, QirReadout, QirShotContext, QirStop, QirTrap,
};

/// A program of two batches: a measurement read and recorded as a `Bool`,
/// then a second measurement, recorded.
fn two_batches() -> AdaptiveProgram<u64> {
    program(&[&[
        measure(OPID_MZ, 0, 0),
        read(OP_READ_RESULT, 0, 0),
        record_bool(0),
        measure(OPID_MZ, 1, 1),
        record_result(1),
        ret(),
    ]])
}

/// A context of [`two_batches`] cloned between its two batches, both
/// answered alike, and another clone answered otherwise.
#[test]
fn a_cloned_context_runs_on_as_the_original_would() {
    let program = built(&two_batches());
    let machine = QirMachine::new(&program);
    let finish = |context: &mut _, first, second| {
        machine
            .resume(
                context,
                &[QirOutcome {
                    result: 0,
                    value: first,
                }],
            )
            .expect("the first batch is answered");
        match machine.resume(
            context,
            &[QirOutcome {
                result: 1,
                value: second,
            }],
        ) {
            Ok(QirStop::Ended(records)) => records.to_vec(),
            stop => panic!("the context runs on: {stop:?}"),
        }
    };
    let mut original = machine.start_shot();
    machine
        .resume(&mut original, &[])
        .expect("a context starts");
    let mut clone = original.clone();
    let mut other = original.clone();
    let records = finish(&mut original, QirReadout::One, QirReadout::Zero);
    assert_eq!(
        records,
        [OutputRecord::Bool(true), OutputRecord::Result(Zero)]
    );
    assert_eq!(
        finish(&mut clone, QirReadout::One, QirReadout::Zero),
        records
    );
    assert_eq!(
        finish(&mut other, QirReadout::Zero, QirReadout::One),
        [OutputRecord::Bool(false), OutputRecord::Result(One)]
    );
}

/// Two contexts of [`two_batches`], one answered `One` and the other `Zero`,
/// resumed in turn, the second first.
#[test]
fn contexts_resumed_in_any_order_run_as_each_would_alone() {
    let bytecode = two_batches();
    let program = built(&bytecode);
    let machine = QirMachine::new(&program);
    let answer = [QirReadout::One, QirReadout::Zero];
    let mut contexts = [machine.start_shot(), machine.start_shot()];
    let mut outcomes = [Vec::new(), Vec::new()];
    let mut ended = [None, None];
    while ended.iter().any(Option::is_none) {
        for index in [1, 0] {
            if ended[index].is_some() {
                continue;
            }
            match machine
                .resume(&mut contexts[index], &outcomes[index])
                .expect("the context does not trap")
            {
                QirStop::Batch(batch) => outcomes[index] = answers(batch, |_| answer[index]),
                QirStop::Ended(done) => ended[index] = Some(done.to_vec()),
            }
        }
    }
    assert_eq!(
        ended,
        answer.map(|outcome| Some(records(&bytecode, |_| outcome)))
    );
}

/// A context of [`two_batches`] started on one machine and run to its end
/// on another machine of the same program.
#[test]
fn a_context_resumes_on_another_machine_of_its_program() {
    let bytecode = two_batches();
    let program = built(&bytecode);
    let starter = QirMachine::new(&program);
    let mut context = starter.start_shot();
    starter.resume(&mut context, &[]).expect("a context starts");
    let other = QirMachine::new(&program);
    let mut outcomes = vec![QirOutcome {
        result: 0,
        value: QirReadout::One,
    }];
    let records_there = loop {
        match other
            .resume(&mut context, &outcomes)
            .expect("the context does not trap")
        {
            QirStop::Batch(batch) => outcomes = answers(batch, |_| QirReadout::One),
            QirStop::Ended(records) => break records.to_vec(),
        }
    };
    assert_eq!(records_there, records(&bytecode, |_| QirReadout::One));
}

/// A resumer whose worker threads resume their contexts on one machine,
/// shared by reference, and hand a context from one thread to another:
/// the compiler accepts both.
#[test]
fn a_machine_may_be_shared_and_a_context_moved_between_threads() {
    fn shareable<T: Sync>() {}
    fn sendable<T: Send>() {}
    shareable::<QirMachine<'_>>();
    sendable::<QirShotContext>();
}

/// Five programs, restarted:
/// - one that reads result 0 and records register 1, the word at address
///   0 and the word an `ALLOCA` adds at address 1 before changing all
///   four, then measures a qubit and records the result as a `Bool`;
/// - one of one result, measured and recorded by default;
/// - one that calls a function, which returns register 1, kept in
///   register 2, then jumps from block 0 to 1 and on to 2, and measures a
///   qubit into result 3, past the second program's results;
/// - one that calls a function which measures a qubit, reads the result,
///   then divides by register 3, which holds 0: a trap inside the
///   function call;
/// - one of one word of constant data that loads the word after it, which
///   the first program allocates.
fn restarted_programs() -> [QirProgram; 5] {
    let mut changes_its_state = program(&[&[
        read(OP_READ_RESULT, 3, 0),
        record_bool(3),
        record_int(1),
        binary(OP_LOAD, 0, 2, 0, 0),
        record_int(2),
        binary(OP_STORE, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 0, 9, 0),
        binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 2, 1, 1),
        binary(OP_LOAD, FLAG_SRC0_IMM, 2, 1, 0),
        record_int(2),
        binary(OP_STORE, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 0, 9, 1),
        set(1, 5),
        measure(OPID_MZ, 0, 0),
        read(OP_READ_RESULT, 3, 0),
        record_bool(3),
        ret(),
    ]]);
    changes_its_state.constant_data = vec![7];
    let one_result = AdaptiveProgram {
        num_results: 1,
        ..program(&[&[measure(OPID_MZ, 0, 0), ret()]])
    };
    let mut calls_then_jumps = program(&[
        &[set(1, 5), call(0, 0, 0, 2), record_int(2), jump(1)],
        &[jump(2)],
        &[measure(OPID_MZ, 0, 3), ret()],
        &[call_return(1)],
    ]);
    calls_then_jumps.function_table = vec![Function::from_tuple((3, 0, 0))];
    let mut traps_in_a_call = program(&[
        &[call(0, 0, 0, VOID_RETURN), ret()],
        &[
            measure(OPID_MZ, 0, 0),
            read(OP_READ_RESULT, 1, 0),
            binary(OP_SDIV, FLAG_SRC0_IMM, 2, 5, 3),
            call_return(0),
        ],
    ]);
    traps_in_a_call.function_table = vec![Function::from_tuple((1, 0, 0))];
    let mut loads_past_its_data = program(&[&[binary(OP_LOAD, FLAG_SRC0_IMM, 1, 1, 0), ret()]]);
    loads_past_its_data.constant_data = vec![7];
    [
        built(&changes_its_state),
        built(&one_result),
        built(&calls_then_jumps),
        built(&traps_in_a_call),
        built(&loads_past_its_data),
    ]
}

/// Each program's context, left after its first resume (its batch
/// unanswered) or at its end or trap, its results `One`, restarted by the
/// machine of each program.
#[test]
fn a_restarted_context_runs_as_a_started_one() {
    let programs = restarted_programs();
    let started = programs.each_ref().map(|program| {
        records_or_trap(program, &mut QirMachine::new(program).start_shot(), |_| {
            QirReadout::One
        })
    });
    assert_eq!(
        started,
        [
            Ok(vec![
                OutputRecord::Bool(false),
                OutputRecord::Int(0),
                OutputRecord::Int(7),
                OutputRecord::Int(0),
                OutputRecord::Bool(true)
            ]),
            Ok(vec![OutputRecord::Result(One)]),
            Ok(vec![OutputRecord::Int(5)]),
            Err(QirTrap::DivisionByZero { instruction: 4 }),
            Err(QirTrap::LoadPastMemory {
                instruction: 0,
                address: 1,
                words: 1
            })
        ],
        "a started context"
    );
    for (left, left_program) in programs.iter().enumerate() {
        for (restarting, program) in programs.iter().enumerate() {
            for resumes in [1, usize::MAX] {
                let machine = QirMachine::new(left_program);
                let mut context = machine.start_shot();
                let mut outcomes = Vec::new();
                for _ in 0..resumes {
                    match machine.resume(&mut context, &outcomes) {
                        Ok(QirStop::Batch(batch)) => outcomes = answers(batch, |_| QirReadout::One),
                        Ok(QirStop::Ended(_)) | Err(_) => break,
                    }
                }
                QirMachine::new(program).restart_shot(&mut context);
                assert_eq!(
                    records_or_trap(program, &mut context, |_| QirReadout::One),
                    started[restarting],
                    "program {left} after {resumes} resumes, restarted by program {restarting}"
                );
            }
        }
    }
}
