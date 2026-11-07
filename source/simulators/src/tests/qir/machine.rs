// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A [`QirMachine`]'s contexts, resumed with the outcomes each test
//! chooses: where a context stops, what it computes and records, and the
//! traps it reports.
//!
//! The tests are filed by behaviour, one file each; this file holds the
//! helpers they share.

mod batches;
mod classical_ops;
mod contexts;
mod control_flow;
mod memory;
mod quantum_calls;
mod records;
mod results;

use std::sync::Arc;

use super::{SIZE, op, program, record_int, ret, set};
use crate::bytecode::encoding::{FLAG_AUX1_IMM, OP_MEASURE, OPID_MZ};
use crate::bytecode::runtime::run_shot;
use crate::bytecode::{AdaptiveProgram, Instruction};
use crate::cpu_full_state_simulator::FullStateSimulator;
use crate::qir::{
    QirCall, QirGate, QirMachine, QirOutcome, QirProgram, QirReadout, QirShotContext, QirStop,
    QirTrap,
};
use crate::{OutputRecord, Simulator};

/// Builds `bytecode`, which has no fault.
fn built(bytecode: &AdaptiveProgram<u64>) -> QirProgram {
    QirProgram::new(bytecode).expect("the program has no fault")
}

/// The outcomes of `batch`, each result as `answer` gives it.
fn answers(batch: &[QirCall], answer: impl Fn(u32) -> QirReadout) -> Vec<QirOutcome> {
    batch
        .iter()
        .filter_map(QirCall::result)
        .map(|result| QirOutcome {
            result,
            value: answer(result),
        })
        .collect()
}

/// A context of `bytecode` run to its end, each result as `answer` gives it:
/// the batches it sends, each written as its quantum calls, and its
/// records.
fn run(
    bytecode: &AdaptiveProgram<u64>,
    answer: impl Fn(u32) -> QirReadout,
) -> (Vec<String>, Vec<OutputRecord>) {
    let program = built(bytecode);
    run_on(
        &program,
        &mut QirMachine::new(&program).start_shot(),
        answer,
    )
}

/// The rest of `context`, of `program`, as [`run`] gives it.
fn run_on(
    program: &QirProgram,
    context: &mut QirShotContext,
    answer: impl Fn(u32) -> QirReadout,
) -> (Vec<String>, Vec<OutputRecord>) {
    let machine = QirMachine::new(program);
    let mut outcomes = Vec::new();
    let mut batches = Vec::new();
    loop {
        match machine
            .resume(context, &outcomes)
            .expect("the context does not trap")
        {
            QirStop::Batch(batch) => {
                // No test program yields this many: a context that does
                // runs on without end.
                assert!(batches.len() < 1_000, "the context runs on without end");
                let calls: Vec<String> = batch.iter().map(QirCall::to_string).collect();
                batches.push(calls.join(", "));
                outcomes = answers(batch, &answer);
            }
            QirStop::Ended(records) => return (batches, records.to_vec()),
        }
    }
}

/// The batches a context of `bytecode` yields, each result `One`.
fn batches(bytecode: &AdaptiveProgram<u64>) -> Vec<String> {
    run(bytecode, |_| QirReadout::One).0
}

/// The records of a context of `bytecode`, each result as `answer` gives it.
fn records(
    bytecode: &AdaptiveProgram<u64>,
    answer: impl Fn(u32) -> QirReadout,
) -> Vec<OutputRecord> {
    run(bytecode, answer).1
}

/// The trap a context of `bytecode` reaches, each result `One`.
fn trap(bytecode: &AdaptiveProgram<u64>) -> QirTrap {
    batches_then_trap(bytecode).1
}

/// The batches a context of `bytecode` yields, written as [`run`] writes
/// them, each result `One`, and the trap it then reaches.
fn batches_then_trap(bytecode: &AdaptiveProgram<u64>) -> (Vec<String>, QirTrap) {
    batches_then_trap_answered(bytecode, |_| QirReadout::One)
}

/// What [`batches_then_trap`] gives, each result as `answer` gives it.
fn batches_then_trap_answered(
    bytecode: &AdaptiveProgram<u64>,
    answer: impl Fn(u32) -> QirReadout,
) -> (Vec<String>, QirTrap) {
    let program = built(bytecode);
    let machine = QirMachine::new(&program);
    let mut context = machine.start_shot();
    let mut outcomes = Vec::new();
    let mut batches = Vec::new();
    loop {
        match machine.resume(&mut context, &outcomes) {
            Ok(QirStop::Batch(batch)) => {
                // No test program yields this many: a context that does
                // yields without end.
                assert!(batches.len() < 1_000, "the context yields without end");
                let calls: Vec<String> = batch.iter().map(QirCall::to_string).collect();
                batches.push(calls.join(", "));
                outcomes = answers(batch, &answer);
            }
            Ok(QirStop::Ended(records)) => panic!("the context ends with {records:?}"),
            Err(trap) => return (batches, trap),
        }
    }
}

/// A shot of `machine`, from `context` restarted, each batch run by `run`,
/// which gives the batch's outcomes: the loop of the machine's module doc.
fn shot(
    machine: QirMachine<'_>,
    context: &mut QirShotContext,
    mut run: impl FnMut(&[QirCall], &mut Vec<QirOutcome>),
) -> Result<Vec<OutputRecord>, QirTrap> {
    machine.restart_shot(context);
    let mut outcomes = Vec::new();
    loop {
        match machine.resume(context, &outcomes)? {
            QirStop::Batch(batch) => {
                outcomes.clear();
                run(batch, &mut outcomes);
            }
            QirStop::Ended(records) => return Ok(records.to_vec()),
        }
    }
}

/// Runs `batch` on `bits`, one bit per qubit, and gives the outcome of each
/// measurement. The programs compared with `run_shot` make only `x`, `reset`,
/// `mz` and `mresetz` quantum calls, from qubits all `Zero`, so that each
/// outcome is certain, and a bit per qubit holds what a simulator's state does.
fn run_on_bits(bits: &mut [bool], batch: &[QirCall], outcomes: &mut Vec<QirOutcome>) {
    for call in batch {
        match *call {
            QirCall::Gate {
                gate: QirGate::X,
                qubits: (qubit, None),
                angle: None,
            } => bits[qubit as usize] ^= true,
            QirCall::Reset { qubit } => bits[qubit as usize] = false,
            QirCall::Measure { qubit, result } | QirCall::MeasureReset { qubit, result } => {
                let bit = &mut bits[qubit as usize];
                outcomes.push(QirOutcome {
                    result,
                    value: if *bit {
                        QirReadout::One
                    } else {
                        QirReadout::Zero
                    },
                });
                if matches!(call, QirCall::MeasureReset { .. }) {
                    *bit = false;
                }
            }
            ref other => panic!("a program compared with `run_shot` makes no call {other}"),
        }
    }
}

/// The records of a shot of `bytecode` by a machine, its batches run on a
/// bit per qubit ([`run_on_bits`]), and by `run_shot` on a simulator.
fn both(bytecode: &AdaptiveProgram<u64>) -> (Vec<OutputRecord>, Vec<OutputRecord>) {
    let program = built(bytecode);
    let machine = QirMachine::new(&program);
    let mut context = machine.start_shot();
    let mut bits = [false; SIZE as usize];
    let records = shot(machine, &mut context, |batch, outcomes| {
        run_on_bits(&mut bits, batch, outcomes);
    })
    .expect("the shot does not trap");
    let mut simulator = FullStateSimulator::new(SIZE as usize, SIZE as usize, 7, Arc::default());
    (records, run_shot(bytecode, &mut simulator))
}

/// `mz` of the constant `qubit` into the result in register `result`.
fn measure_into_register(qubit: u64, result: u64) -> Instruction<u64> {
    Instruction {
        aux0: OPID_MZ,
        aux1: qubit,
        aux2: result,
        ..op(OP_MEASURE, FLAG_AUX1_IMM)
    }
}

/// What the rest of `context`, of `program`, gives, each result as
/// `answer` gives it: its records, or the trap it reaches. Each program
/// ends within a few batches, so a context that does not end within 16
/// fails rather than hangs.
fn records_or_trap(
    program: &QirProgram,
    context: &mut QirShotContext,
    answer: impl Fn(u32) -> QirReadout,
) -> Result<Vec<OutputRecord>, QirTrap> {
    let machine = QirMachine::new(program);
    let mut outcomes = Vec::new();
    for _ in 0..16 {
        match machine.resume(context, &outcomes)? {
            QirStop::Batch(batch) => outcomes = answers(batch, &answer),
            QirStop::Ended(records) => return Ok(records.to_vec()),
        }
    }
    panic!("the context ends within 16 batches");
}

/// The value of register 1, set to 3, after `instruction` runs.
fn computed(instruction: Instruction<u64>) -> i64 {
    computed_by(&[instruction])
}

/// The value of register 1, set to 3, after `instructions` run.
fn computed_by(instructions: &[Instruction<u64>]) -> i64 {
    let block = [&[set(1, 3)][..], instructions, &[record_int(1), ret()]].concat();
    let bytecode = program(&[&block]);
    let (machine, run_shot) = both(&bytecode);
    // COMPATIBILITY NOTE: every value computed here is also the one
    // `run_shot` (bytecode/runtime.rs) computes, so that the machine is a
    // drop-in replacement for it, also where LLVM leaves the value open.
    assert_eq!(machine, run_shot, "the value `run_shot` computes");
    match machine[..] {
        [OutputRecord::Int(value)] => value,
        _ => panic!("one `Int` is recorded: {machine:?}"),
    }
}
