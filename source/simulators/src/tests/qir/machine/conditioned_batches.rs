// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The conditioned-batch policy ([`QirMachine::resume_conditioned`]): a
//! shot runs the same quantum calls and records the same as under
//! [`QirMachine::resume`], in fewer batches, its result-conditioned gates
//! carried as conditions; where a deferred value cannot be carried, the
//! batch ends as under `resume`.

use super::super::{
    binary, branch, gate, jump, measure, phi, program, read, record_result, ret, set,
};
use super::{answers, built};
use crate::OutputRecord;
use crate::bytecode::encoding::{
    FLAG_SRC1_IMM, OP_READ_RESULT, OP_XOR, OPID_MRESETZ, OPID_MZ, OPID_X,
};
use crate::bytecode::{AdaptiveProgram, Instruction, PhiNodeEntry};
use crate::qir::{QirCall, QirConditionedStop, QirMachine, QirReadout, QirStop, QirTrap};

/// What a shot of `bytecode` runs under the conditioned policy, each result
/// as `answer` gives it: every call it runs (a conditioned call only where
/// its condition holds), its batches (each call written with its
/// condition), and its records or trap.
fn conditioned(
    bytecode: &AdaptiveProgram<u64>,
    answer: impl Fn(u32) -> QirReadout,
) -> (Vec<String>, Vec<String>, Result<Vec<OutputRecord>, QirTrap>) {
    let program = built(bytecode);
    let machine = QirMachine::new(&program);
    let mut context = machine.start_shot();
    let (mut ran, mut batches, mut outcomes) = (Vec::new(), Vec::new(), Vec::new());
    loop {
        match machine.resume_conditioned(&mut context, &outcomes) {
            Ok(QirConditionedStop::Batch { calls, conditions }) => {
                assert!(batches.len() < 1_000, "the context yields without end");
                let mut written = Vec::new();
                for (index, call) in calls.iter().enumerate() {
                    let condition = conditions.iter().find(|c| c.call == index);
                    written.push(match condition {
                        None => call.to_string(),
                        Some(c) => format!(
                            "{call} if {:?}:{:#b}",
                            c.condition.results(),
                            c.condition.truth_table()
                        ),
                    });
                    if condition.is_none_or(|c| {
                        c.condition
                            .holds(|result| answer(result) == QirReadout::One)
                    }) {
                        ran.push(call.to_string());
                    }
                }
                assert!(
                    conditions.windows(2).all(|w| w[0].call < w[1].call),
                    "conditions in the order of their calls, one per call"
                );
                batches.push(written.join(", "));
                outcomes = answers(calls, &answer);
            }
            Ok(QirConditionedStop::Ended(records)) => {
                return (ran, batches, Ok(records.to_vec()));
            }
            Err(trap) => return (ran, batches, Err(trap)),
        }
    }
}

/// What a shot of `bytecode` runs under `resume`, as [`conditioned`] gives
/// it: every call of every batch runs.
fn unconditioned(
    bytecode: &AdaptiveProgram<u64>,
    answer: impl Fn(u32) -> QirReadout,
) -> (Vec<String>, Vec<String>, Result<Vec<OutputRecord>, QirTrap>) {
    let program = built(bytecode);
    let machine = QirMachine::new(&program);
    let mut context = machine.start_shot();
    let (mut ran, mut batches, mut outcomes) = (Vec::new(), Vec::new(), Vec::new());
    loop {
        match machine.resume(&mut context, &outcomes) {
            Ok(QirStop::Batch(calls)) => {
                assert!(batches.len() < 1_000, "the context yields without end");
                let written: Vec<String> = calls.iter().map(QirCall::to_string).collect();
                batches.push(written.join(", "));
                ran.extend(written);
                outcomes = answers(calls, &answer);
            }
            Ok(QirStop::Ended(records)) => return (ran, batches, Ok(records.to_vec())),
            Err(trap) => return (ran, batches, Err(trap)),
        }
    }
}

/// Each answer of results `0..count`, as a bit per result.
fn every_answer(count: u32) -> impl Iterator<Item = impl Fn(u32) -> QirReadout + Copy> {
    (0..1u32 << count).map(|bits| {
        move |result: u32| {
            if bits >> result & 1 == 1 {
                QirReadout::One
            } else {
                QirReadout::Zero
            }
        }
    })
}

/// A repetition code's round, as Q# lowers it: two syndrome bits measured
/// (`mresetz`) and read at once, then the lookup decoder's corrections as
/// short-circuit triangles (`if s0 and not s1`, `if s0 and s1`), a diamond
/// (`if s1 {X} else {X}`), and the data measured and recorded.
fn decoder_round() -> AdaptiveProgram<u64> {
    let mut bytecode = program(&[
        &[
            measure(OPID_MRESETZ, 2, 0),
            read(OP_READ_RESULT, 1, 0),
            measure(OPID_MRESETZ, 3, 1),
            read(OP_READ_RESULT, 2, 1),
            branch(1, 1, 2),
        ],
        &[binary(OP_XOR, FLAG_SRC1_IMM, 3, 2, 1), jump(2)],
        &[set(4, 0), phi(5, 0, 2), branch(5, 3, 4)],
        &[gate(OPID_X, 0, 0), jump(4)],
        &[branch(1, 5, 6)],
        &[jump(6)],
        &[set(6, 0), phi(7, 2, 2), branch(7, 7, 8)],
        &[gate(OPID_X, 1, 0), jump(8)],
        &[branch(2, 9, 10)],
        &[gate(OPID_X, 2, 0), jump(11)],
        &[gate(OPID_X, 3, 0), jump(11)],
        &[
            measure(OPID_MZ, 0, 2),
            measure(OPID_MZ, 1, 3),
            record_result(0),
            record_result(1),
            record_result(2),
            record_result(3),
            ret(),
        ],
    ]);
    bytecode.num_registers = 8;
    bytecode.phi_entries = vec![
        PhiNodeEntry::from_tuple((0, 4)),
        PhiNodeEntry::from_tuple((1, 3)),
        PhiNodeEntry::from_tuple((4, 6)),
        PhiNodeEntry::from_tuple((5, 2)),
    ];
    bytecode
}

/// For every outcome of the syndrome and the data, the decoder's round
/// runs the corrections `resume` runs, and records the same.
#[test]
fn a_shot_runs_and_records_as_under_resume() {
    let bytecode = decoder_round();
    for answer in every_answer(4) {
        let (ran, _, end) = conditioned(&bytecode, answer);
        let (expected, _, expected_end) = unconditioned(&bytecode, answer);
        assert_eq!(ran, expected, "the calls run");
        assert_eq!(end, expected_end, "the records");
    }
}

/// The decoder's round is one batch, its corrections conditioned on the
/// syndrome measured in that batch, where `resume` stops at each read.
#[test]
fn reads_of_measurements_and_branches_on_them_do_not_end_the_batch() {
    let bytecode = decoder_round();
    let answer = |_| QirReadout::Zero;
    let (_, batches, _) = conditioned(&bytecode, answer);
    assert_eq!(
        batches,
        ["mresetz(qubit 2, result 0), mresetz(qubit 3, result 1), \
             x(qubit 0) if [0, 1]:0b10, x(qubit 1) if [0, 1]:0b1000, \
             x(qubit 2) if [1]:0b10, x(qubit 3) if [1]:0b1, \
             mz(qubit 0, result 2), mz(qubit 1, result 3)"]
    );
    let (_, batches, _) = unconditioned(&bytecode, answer);
    assert_eq!(batches.len(), 3, "resume stops at each read: {batches:?}");
}

/// A branch whose arm measures is not converted: the batch ends at the
/// branch, which runs once its condition is known.
#[test]
fn a_branch_whose_arm_measures_ends_the_batch() {
    let mut bytecode = program(&[
        &[
            measure(OPID_MZ, 0, 0),
            read(OP_READ_RESULT, 1, 0),
            branch(1, 1, 2),
        ],
        &[measure(OPID_MZ, 1, 1), jump(2)],
        &[record_result(0), ret()],
    ]);
    bytecode.num_registers = 2;
    for answer in every_answer(2) {
        let (ran, batches, end) = conditioned(&bytecode, answer);
        let (expected, expected_batches, expected_end) = unconditioned(&bytecode, answer);
        assert_eq!((ran, end), (expected, expected_end));
        assert_eq!(batches, expected_batches, "the same stops as under resume");
    }
}

/// A value of more than five results is not deferred: the batch ends at
/// the instruction that would compute it.
#[test]
fn a_value_of_more_than_five_results_ends_the_batch() {
    let mut block: Vec<Instruction<u64>> = Vec::new();
    for k in 0..6 {
        block.push(measure(OPID_MZ, k, k));
        block.push(read(OP_READ_RESULT, k + 1, k));
    }
    // Register 7 = r0 ^ r1, then ^ r2, … ^ r5.
    block.push(binary(OP_XOR, 0, 7, 1, 2));
    for k in 3..7 {
        block.push(binary(OP_XOR, 0, 7 + k - 2, 7 + k - 3, k));
    }
    block.push(branch(11, 1, 2));
    let mut bytecode = program(&[
        &block,
        &[gate(OPID_X, 6, 0), jump(2)],
        &[record_result(0), ret()],
    ]);
    bytecode.num_qubits = 8;
    bytecode.num_results = 6;
    bytecode.num_registers = 12;
    for answer in every_answer(6) {
        let (ran, batches, end) = conditioned(&bytecode, answer);
        let (expected, _, expected_end) = unconditioned(&bytecode, answer);
        assert_eq!((ran, end), (expected, expected_end));
        assert_eq!(
            batches[0],
            "mz(qubit 0, result 0), mz(qubit 1, result 1), mz(qubit 2, result 2), \
             mz(qubit 3, result 3), mz(qubit 4, result 4), mz(qubit 5, result 5)",
            "the batch ends before the sixth result joins the value"
        );
    }
}

/// A deferred read of a result that comes back with no value traps at that
/// read, as under `resume`.
#[test]
fn a_deferred_read_of_a_missing_result_traps_at_the_read() {
    let mut bytecode = program(&[
        &[
            measure(OPID_MZ, 0, 0),
            read(OP_READ_RESULT, 1, 0),
            branch(1, 1, 2),
        ],
        &[gate(OPID_X, 1, 0), jump(2)],
        &[record_result(1), ret()],
    ]);
    bytecode.num_registers = 2;
    let missing = |_| QirReadout::Missing;
    let (_, _, end) = conditioned(&bytecode, missing);
    let (_, _, expected) = unconditioned(&bytecode, missing);
    assert_eq!(
        end,
        Err(QirTrap::MissingValue {
            instruction: 1,
            result: 0
        })
    );
    assert_eq!(end, expected);
}

/// `resume` after a conditioned batch runs on as the conditioned policy
/// alone would: it resolves the batch's deferred values.
#[test]
fn the_two_policies_mix_on_one_context() {
    let bytecode = decoder_round();
    let program = built(&bytecode);
    let machine = QirMachine::new(&program);
    for answer in every_answer(4) {
        let mut context = machine.start_shot();
        let first = match machine.resume_conditioned(&mut context, &[]) {
            Ok(QirConditionedStop::Batch { calls, .. }) => answers(calls, answer),
            other => panic!("a batch: {other:?}"),
        };
        let records = match machine.resume(&mut context, &first) {
            Ok(QirStop::Ended(records)) => records.to_vec(),
            other => panic!("the end: {other:?}"),
        };
        let (_, _, expected) = unconditioned(&bytecode, answer);
        assert_eq!(Ok(records), expected);
    }
}

/// A conditioned call is a gate: it writes no result.
#[test]
fn conditioned_calls_write_no_result() {
    let program = built(&decoder_round());
    let machine = QirMachine::new(&program);
    let mut context = machine.start_shot();
    let Ok(QirConditionedStop::Batch { calls, conditions }) =
        machine.resume_conditioned(&mut context, &[])
    else {
        panic!("a batch");
    };
    assert_ne!(conditions, []);
    for condition in conditions {
        assert!(matches!(calls[condition.call], QirCall::Gate { .. }));
    }
}
