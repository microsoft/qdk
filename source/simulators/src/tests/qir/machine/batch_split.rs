// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A batch handed over in pieces ([`SplitBatches`]): where a context
//! splits its batch, what each piece holds, that the pieces, in order,
//! are the batch a machine of whole batches yields, its outcomes asked
//! once, at its end, and that a trap comes after every call walked before
//! it.

use std::fmt;
use std::num::NonZeroUsize;

use super::super::{
    binary, branch, compare, correlated_noise, gate, jump, measure, op, program, read, record_int,
    record_result, ret, set,
};
use super::{answers, built, measure_into_register, run, run_on};
use crate::OutputRecord;
use crate::bytecode::encoding::{
    FLAG_AUX1_IMM, FLAG_SRC0_IMM, FLAG_SRC1_IMM, ICMP_SLT, OP_ADD, OP_ICMP, OP_LOAD,
    OP_QUANTUM_GATE, OP_READ_RESULT, OP_STORE, OP_UDIV, OP_WRITE_RESULT, OPID_CX, OPID_MZ, OPID_X,
};
use crate::bytecode::{AdaptiveProgram, Instruction};
use crate::qir::{
    QirCall, QirMachine, QirOutcome, QirReadout, QirShotContext, QirStop, QirTrap, SplitBatches,
};

/// The split of batches into pieces of at most `n` calls.
fn at_most(n: usize) -> SplitBatches {
    SplitBatches::at_most(NonZeroUsize::new(n).expect("a piece holds a call"))
}

/// A stop of a context whose batches are split, with its calls written
/// out: it displays as `split: ` or `batch: `, then its calls.
struct Piece {
    /// Whether the stop is a [`QirStop::Split`], else a [`QirStop::Batch`].
    split: bool,
    /// Its calls, each as it displays.
    calls: Vec<String>,
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = if self.split { "split" } else { "batch" };
        write!(f, "{kind}: {}", self.calls.join(", "))
    }
}

impl Piece {
    fn of(split: bool, calls: &[QirCall]) -> Self {
        Self {
            split,
            calls: calls.iter().map(QirCall::to_string).collect(),
        }
    }
}

/// What a new context of `bytecode` gives when its batches are split at
/// `n` calls, as [`split_run_on`] gives it.
fn split_run(
    bytecode: &AdaptiveProgram<u64>,
    n: usize,
    answer: impl Fn(u32) -> QirReadout,
) -> (Vec<Piece>, Result<Vec<OutputRecord>, QirTrap>) {
    let program = built(bytecode);
    let machine = QirMachine::new(&program).with_split(at_most(n));
    split_run_on(&machine, &mut machine.start_shot(), answer)
}

/// What the rest of `context` gives under `machine`, each result as
/// `answer` gives it: its stops, then its records or the trap it reaches.
/// A split is resumed with no outcomes; a batch with those of its calls
/// and of the splits before it. No test program stops 1,000 times: a
/// context that does fails rather than runs on without end.
fn split_run_on(
    machine: &QirMachine<'_, SplitBatches>,
    context: &mut QirShotContext,
    answer: impl Fn(u32) -> QirReadout,
) -> (Vec<Piece>, Result<Vec<OutputRecord>, QirTrap>) {
    let mut stops = Vec::new();
    let mut batch = Vec::new();
    let mut outcomes = Vec::new();
    loop {
        assert!(stops.len() < 1_000, "the context stops without end");
        match machine.resume(context, &outcomes) {
            Ok(QirStop::Split(piece)) => {
                stops.push(Piece::of(true, piece));
                batch.extend_from_slice(piece);
                outcomes.clear();
            }
            Ok(QirStop::Batch(piece)) => {
                stops.push(Piece::of(false, piece));
                batch.extend_from_slice(piece);
                outcomes = answers(&batch, &answer);
                batch.clear();
            }
            Ok(QirStop::Ended(records)) => return (stops, Ok(records.to_vec())),
            Err(trap) => return (stops, Err(trap)),
        }
    }
}

/// The stops of a context of `bytecode` whose batches are split at `n`
/// calls, each result `One`, as they display, when it does not trap.
fn pieces(bytecode: &AdaptiveProgram<u64>, n: usize) -> Vec<String> {
    let (stops, end) = split_run(bytecode, n, |_| QirReadout::One);
    end.expect("the context does not trap");
    stops.iter().map(Piece::to_string).collect()
}

/// `stops`, each batch with the splits before it joined to it, written as
/// a machine of whole batches writes its batches; and the calls of the
/// splits after the last batch, if any.
fn joined(stops: &[Piece]) -> (Vec<String>, Vec<String>) {
    let mut batches = Vec::new();
    let mut calls = Vec::new();
    for stop in stops {
        calls.extend_from_slice(&stop.calls);
        if !stop.split {
            batches.push(calls.join(", "));
            calls.clear();
        }
    }
    (batches, calls)
}

/// Asserts that a context of `bytecode`, its batches split at each size of
/// [`SPLITS`], gives `whole`, what a machine of whole batches gives it,
/// each result as `answer` gives it: each batch, joined to the splits
/// before it, and the records or the trap. Only before a trap may splits
/// follow the last batch, and they hold the same calls at every size: that
/// they are the calls walked since it is checked by
/// `a_trap_comes_after_every_call_walked_before_it`, which names them.
pub(super) fn assert_split_alike(
    bytecode: &AdaptiveProgram<u64>,
    whole: &(Vec<String>, Result<Vec<OutputRecord>, QirTrap>),
    answer: impl Fn(u32) -> QirReadout,
) {
    let mut before_the_trap = None;
    for n in SPLITS {
        let (stops, end) = split_run(bytecode, n, &answer);
        let (batches, after) = joined(&stops);
        assert!(
            after.is_empty() || end.is_err(),
            "at most {n}: the splits end in a batch: {after:?}"
        );
        assert_eq!(
            &after,
            before_the_trap.get_or_insert_with(|| after.clone()),
            "at most {n}: the calls walked before the trap"
        );
        assert_eq!(&(batches, end), whole, "at most {n}");
    }
}

/// An `x` of the qubit register `register` holds: a call built from
/// registers.
fn x_from_register(register: u64) -> Instruction<u64> {
    Instruction {
        aux0: OPID_X,
        aux1: register,
        ..op(OP_QUANTUM_GATE, FLAG_SRC0_IMM)
    }
}

/// Programs that yield batches of each kind of call, named: runs of
/// prebuilt calls, calls built from registers, correlated noise, and a
/// loop of three rounds that measure results the round before measured;
/// and one that makes no quantum call.
fn programs() -> Vec<(&'static str, AdaptiveProgram<u64>)> {
    let prebuilt = program(&[&[
        gate(OPID_X, 0, 0),
        gate(OPID_CX, 0, 1),
        measure(OPID_MZ, 0, 0),
        measure(OPID_MZ, 1, 1),
        read(OP_READ_RESULT, 0, 0),
        gate(OPID_X, 2, 0),
        measure(OPID_MZ, 2, 2),
        record_result(0),
        record_result(1),
        record_result(2),
        ret(),
    ]]);
    let from_registers = program(&[&[
        set(3, 1),
        x_from_register(3),
        x_from_register(3),
        set(2, 2),
        measure_into_register(1, 2),
        x_from_register(3),
        measure_into_register(0, 2),
        record_result(2),
        ret(),
    ]]);
    let mut noise = program(&[&[
        set(0, 2),
        set(1, 3),
        gate(OPID_X, 0, 0),
        correlated_noise(2, 0),
        correlated_noise(1, 1),
        measure(OPID_MZ, 2, 0),
        correlated_noise(1, 0),
        record_result(0),
        ret(),
    ]]);
    noise.call_args = vec![0, 1];
    let less_than_3 = Instruction {
        dst: 2,
        src0: 0,
        src1: 3,
        opcode: compare(OP_ICMP, ICMP_SLT).opcode | FLAG_SRC1_IMM,
        ..Instruction::default()
    };
    let rounds = program(&[
        &[set(0, 0), jump(1)],
        &[
            gate(OPID_X, 0, 0),
            gate(OPID_X, 1, 0),
            measure(OPID_MZ, 0, 0),
            measure(OPID_MZ, 1, 1),
            binary(OP_ADD, FLAG_SRC1_IMM, 0, 0, 1),
            less_than_3,
            branch(2, 1, 2),
        ],
        &[record_result(0), record_result(1), ret()],
    ]);
    let classical = program(&[&[set(0, 7), record_int(0), ret()]]);
    vec![
        ("prebuilt", prebuilt),
        ("from registers", from_registers),
        ("correlated noise", noise),
        ("rounds", rounds),
        ("classical", classical),
    ]
}

/// The pieces' sizes the tests split at: one call, a few, and more than
/// any test program's batch holds.
const SPLITS: [usize; 4] = [1, 2, 3, 1024];

/// Result 0 `One` and the others `Zero`, so that a record shows which
/// outcome a result took.
fn one_for_result_0(result: u32) -> QirReadout {
    if result == 0 {
        QirReadout::One
    } else {
        QirReadout::Zero
    }
}

/// Each program, split at each size: its batches, each with the splits
/// before it, and its records, are those of whole batches; so its pieces
/// hold the batch's calls in order, and the batch's outcomes, asked at its
/// end, are the whole batch's. Every program the machine's tests run
/// through their `run` is checked so too.
#[test]
fn the_pieces_of_a_batch_in_order_are_the_whole_batch() {
    for (_, bytecode) in programs() {
        let program = built(&bytecode);
        let whole = run_on(
            &program,
            &mut QirMachine::new(&program).start_shot(),
            one_for_result_0,
        );
        assert_split_alike(&bytecode, &whole, one_for_result_0);
    }
}

/// Each program, none of which traps, split at each size: each split
/// holds `n` calls, as many as a piece may hold.
#[test]
fn a_split_of_a_shot_that_does_not_trap_holds_n_calls() {
    for (name, bytecode) in programs() {
        for n in SPLITS {
            let (stops, _) = split_run(&bytecode, n, one_for_result_0);
            for stop in stops.iter().filter(|stop| stop.split) {
                assert_eq!(stop.calls.len(), n, "{name}, at most {n}: {stop}");
            }
        }
    }
}

/// Each program, split at each size: each batch, after its splits, holds
/// at least one call and at most `n`.
#[test]
fn a_batch_holds_one_to_n_calls() {
    for (name, bytecode) in programs() {
        for n in SPLITS {
            let (stops, _) = split_run(&bytecode, n, one_for_result_0);
            for stop in stops.iter().filter(|stop| !stop.split) {
                assert!(
                    (1..=n).contains(&stop.calls.len()),
                    "{name}, at most {n}: {stop}"
                );
            }
        }
    }
}

/// A run of three prebuilt `x`s on qubits 0, 1 and 2, after none, one or
/// two calls built from registers, split at 2, so that the batch is cut
/// before its last, middle or first call; the run split at 1; and two
/// prebuilt calls before a call built from registers and before
/// correlated noise, split at 2.
#[test]
fn a_context_splits_before_the_call_that_would_overfill_its_piece() {
    let run = [gate(OPID_X, 0, 0), gate(OPID_X, 1, 0), gate(OPID_X, 2, 0)];
    let after = |calls: usize| {
        let mut instructions = vec![set(3, 3)];
        instructions.extend(std::iter::repeat_n(x_from_register(3), calls));
        instructions.extend_from_slice(&run);
        instructions.push(ret());
        program(&[&instructions])
    };
    let two_then = |call: Instruction<u64>| {
        let mut bytecode = program(&[&[
            gate(OPID_X, 0, 0),
            gate(OPID_X, 1, 0),
            set(0, 3),
            call,
            ret(),
        ]]);
        bytecode.call_args = vec![0];
        bytecode
    };
    let cases = [
        (
            "the run's last call",
            after(0),
            2,
            vec!["split: x(qubit 0), x(qubit 1)", "batch: x(qubit 2)"],
        ),
        (
            "the run's middle call",
            after(1),
            2,
            vec![
                "split: x(qubit 3), x(qubit 0)",
                "batch: x(qubit 1), x(qubit 2)",
            ],
        ),
        (
            "the run's first call",
            after(2),
            2,
            vec![
                "split: x(qubit 3), x(qubit 3)",
                "split: x(qubit 0), x(qubit 1)",
                "batch: x(qubit 2)",
            ],
        ),
        (
            "each call of the run",
            after(0),
            1,
            vec![
                "split: x(qubit 0)",
                "split: x(qubit 1)",
                "batch: x(qubit 2)",
            ],
        ),
        (
            "a call built from registers",
            two_then(x_from_register(0)),
            2,
            vec!["split: x(qubit 0), x(qubit 1)", "batch: x(qubit 3)"],
        ),
        (
            "correlated noise",
            two_then(correlated_noise(1, 0)),
            2,
            vec![
                "split: x(qubit 0), x(qubit 1)",
                "batch: correlated_noise(qubits [3], table 0)",
            ],
        ),
    ];
    for (name, bytecode, n, expected) in cases {
        assert_eq!(pieces(&bytecode, n), expected, "before {name}");
    }
}

/// A batch of exactly `n` calls, for `n` from 1 to 3: `x`s then a
/// measurement, read before a last `x`; and `x`s at the program's end.
#[test]
fn a_batch_of_n_calls_comes_whole() {
    for n in 1..=3 {
        let mut read_after = vec![gate(OPID_X, 0, 0); n - 1];
        read_after.extend([
            measure(OPID_MZ, 0, 0),
            read(OP_READ_RESULT, 0, 0),
            gate(OPID_X, 1, 0),
            ret(),
        ]);
        let mut at_the_end = vec![gate(OPID_X, 0, 0); n];
        at_the_end.push(ret());
        let mut measured = vec!["x(qubit 0)"; n - 1];
        measured.push("mz(qubit 0, result 0)");
        assert_eq!(
            pieces(&program(&[&read_after]), n),
            [
                format!("batch: {}", measured.join(", ")),
                "batch: x(qubit 1)".to_string()
            ],
            "read, at most {n}"
        );
        assert_eq!(
            pieces(&program(&[&at_the_end]), n),
            [format!("batch: {}", vec!["x(qubit 0)"; n].join(", "))],
            "at the end, at most {n}"
        );
    }
}

/// An `x` and a measurement into result 0, split at 2, then a second
/// measurement into result 0, prebuilt and built from registers: the
/// third call would overfill the piece, but it ends the batch, so the two
/// come as the batch. A prebuilt `x` before that measurement, by
/// contrast, joins the batch, so the two come as a split.
#[test]
fn a_full_piece_before_a_call_that_ends_its_batch_comes_as_the_batch() {
    let first = [set(3, 0), gate(OPID_X, 0, 0), measure(OPID_MZ, 0, 0)];
    let ends = |second: &[Instruction<u64>]| {
        let instructions = [&first[..], second, &[ret()]].concat();
        program(&[&instructions])
    };
    let cases = [
        (
            "prebuilt",
            ends(&[measure(OPID_MZ, 0, 0)]),
            vec![
                "batch: x(qubit 0), mz(qubit 0, result 0)",
                "batch: mz(qubit 0, result 0)",
            ],
        ),
        (
            "built from registers",
            ends(&[measure_into_register(0, 3)]),
            vec![
                "batch: x(qubit 0), mz(qubit 0, result 0)",
                "batch: mz(qubit 0, result 0)",
            ],
        ),
        (
            "after an x",
            ends(&[gate(OPID_X, 1, 0), measure(OPID_MZ, 0, 0)]),
            vec![
                "split: x(qubit 0), mz(qubit 0, result 0)",
                "batch: x(qubit 1)",
                "batch: mz(qubit 0, result 0)",
            ],
        ),
    ];
    for (name, bytecode, expected) in cases {
        assert_eq!(pieces(&bytecode, 2), expected, "{name}");
    }
}

/// Split at 1, the resume after the first split given an outcome, for a
/// result the batch writes or for one it does not: after measurements into
/// results 0 and 1, the first a split; and after a measurement into
/// result 0 then a division by a register holding 0, whose split holds
/// the trap. The outcome, the resumer's fault, is reported before the
/// program's trap.
#[test]
fn an_outcome_given_after_a_split_is_a_trap() {
    let two_measurements = program(&[&[measure(OPID_MZ, 0, 0), measure(OPID_MZ, 1, 1), ret()]]);
    let holds_a_trap = program(&[&[
        measure(OPID_MZ, 0, 0),
        binary(OP_UDIV, FLAG_SRC0_IMM, 1, 5, 0),
        ret(),
    ]]);
    for (name, bytecode) in [
        ("a split", two_measurements),
        ("a split holding a trap", holds_a_trap),
    ] {
        let program = built(&bytecode);
        let machine = QirMachine::new(&program).with_split(at_most(1));
        for result in [0, 3] {
            let mut context = machine.start_shot();
            let stop = machine.resume(&mut context, &[]).expect("a context starts");
            assert!(matches!(stop, QirStop::Split([_])), "{name}: {stop:?}");
            let outcome = QirOutcome {
                result,
                value: QirReadout::One,
            };
            assert_eq!(
                machine.resume(&mut context, &[outcome]),
                Err(QirTrap::UnexpectedOutcome { result }),
                "{name}, result {result}"
            );
        }
    }
}

/// Measurements into results 0 and 1, split at 1, so the first comes as a
/// split and the second as the batch: the batch's resume needs the outcome
/// of the split's call too, asked first when both are missing.
#[test]
fn the_resume_after_a_batch_takes_the_outcomes_of_its_splits_calls() {
    let bytecode = program(&[&[measure(OPID_MZ, 0, 0), measure(OPID_MZ, 1, 1), ret()]]);
    let program = built(&bytecode);
    let machine = QirMachine::new(&program).with_split(at_most(1));
    let mut context = machine.start_shot();
    let stop = machine.resume(&mut context, &[]).expect("a context starts");
    assert!(matches!(stop, QirStop::Split([_])), "{stop:?}");
    let stop = machine
        .resume(&mut context, &[])
        .expect("a split takes no outcomes");
    assert!(matches!(stop, QirStop::Batch([_])), "{stop:?}");
    let [of_0, of_1] = [0, 1].map(|result| QirOutcome {
        result,
        value: QirReadout::One,
    });
    for given in [&[][..], &[of_1]] {
        assert_eq!(
            machine.resume(&mut context.clone(), given),
            Err(QirTrap::MissingOutcome { result: 0 }),
            "{given:?}"
        );
    }
    assert!(matches!(
        machine.resume(&mut context, &[of_1, of_0]),
        Ok(QirStop::Ended(_))
    ));
}

/// A program of `instructions` whose `call_args` table is `[0]`.
fn one_call_arg(instructions: &[Instruction<u64>]) -> AdaptiveProgram<u64> {
    let mut bytecode = program(&[instructions]);
    bytecode.call_args = vec![0];
    bytecode
}

/// Programs that trap, whole and split: by a call built from registers on
/// qubit 99, past the program's (instruction 3), after a full piece of
/// `n` = 1 and a piece of `n` = 3 holding one or two calls; by correlated
/// noise on three qubits past the one entry of the `call_args` table,
/// after a full piece; by a division by a register holding 0 after calls,
/// split at 1 and 3, and after none, at the start and after a batch; by a
/// call that would overfill a full piece; after a split inside a run of
/// prebuilt calls; and, a piece of two calls pending, split at 3, by each
/// other trap the walk can reach with calls pending: a measurement into a
/// result past the program's, correlated noise naming a register past the
/// program's, a write of 2 to a result, a load and a store past the
/// context's memory, and a read of result 3, answered with no value in an
/// earlier batch (every other result is answered `One`). Whole, the
/// trapping batch is not handed over: the trap comes at once. Split,
/// every call walked before the trap is, those since the last stop as a
/// last split, and the trap comes on the next resume; with none since the
/// last stop, it comes at once.
#[test]
#[allow(clippy::too_many_lines)]
fn a_trap_comes_after_every_call_walked_before_it() {
    let past_the_qubits = QirTrap::NoSuchQubit {
        instruction: 3,
        qubit: 99,
        qubits: 4,
    };
    let two_x_then = |trapping: &[Instruction<u64>]| {
        let instructions = [
            &[gate(OPID_X, 0, 0), gate(OPID_X, 1, 0)][..],
            trapping,
            &[ret()],
        ];
        one_call_arg(&instructions.concat())
    };
    let one_x_then_past_the_qubits =
        program(&[&[gate(OPID_X, 0, 0), set(3, 99), x_from_register(3), ret()]]);
    let mut noise_count_from_register_0 = correlated_noise(0, 0);
    noise_count_from_register_0.opcode &= !FLAG_AUX1_IMM;
    let by_zero = binary(OP_UDIV, FLAG_SRC0_IMM, 1, 5, 0);
    let after_a_batch = program(&[&[
        measure(OPID_MZ, 0, 0),
        read(OP_READ_RESULT, 1, 0),
        by_zero,
        ret(),
    ]]);
    let mut past_the_registers = two_x_then(&[set(0, 1), noise_count_from_register_0]);
    past_the_registers.call_args = vec![4];
    let no_value_after_a_batch = program(&[&[
        measure(OPID_MZ, 0, 3),
        measure(OPID_MZ, 1, 1),
        read(OP_READ_RESULT, 2, 1),
        measure(OPID_MZ, 2, 2),
        read(OP_READ_RESULT, 3, 3),
        ret(),
    ]]);
    let one_but_result_3_with_no_value = |result| {
        if result == 3 {
            QirReadout::Missing
        } else {
            QirReadout::One
        }
    };
    let cases = [
        (
            "a call built from registers, a full piece pending",
            two_x_then(&[set(3, 99), x_from_register(3)]),
            1,
            vec!["split: x(qubit 0)", "split: x(qubit 1)"],
            vec![],
            past_the_qubits.clone(),
        ),
        (
            "a call built from registers, a piece of one call pending",
            one_x_then_past_the_qubits,
            3,
            vec!["split: x(qubit 0)"],
            vec![],
            QirTrap::NoSuchQubit {
                instruction: 2,
                qubit: 99,
                qubits: 4,
            },
        ),
        (
            "a call built from registers, a piece of two calls pending",
            two_x_then(&[set(3, 99), x_from_register(3)]),
            3,
            vec!["split: x(qubit 0), x(qubit 1)"],
            vec![],
            past_the_qubits.clone(),
        ),
        (
            "a call that would overfill a full piece",
            two_x_then(&[set(3, 99), x_from_register(3)]),
            2,
            vec!["split: x(qubit 0), x(qubit 1)"],
            vec![],
            past_the_qubits,
        ),
        (
            "correlated noise past the call args, a full piece pending",
            two_x_then(&[set(0, 3), noise_count_from_register_0]),
            1,
            vec!["split: x(qubit 0)", "split: x(qubit 1)"],
            vec![],
            QirTrap::PastCallArgs {
                instruction: 3,
                offset: 0,
                count: 3,
                len: 1,
            },
        ),
        (
            "a division, a full piece pending",
            two_x_then(&[by_zero]),
            1,
            vec!["split: x(qubit 0)", "split: x(qubit 1)"],
            vec![],
            QirTrap::DivisionByZero { instruction: 2 },
        ),
        (
            "a division, a piece of two calls pending",
            two_x_then(&[by_zero]),
            3,
            vec!["split: x(qubit 0), x(qubit 1)"],
            vec![],
            QirTrap::DivisionByZero { instruction: 2 },
        ),
        (
            "a division after a split inside a run of prebuilt calls",
            two_x_then(&[gate(OPID_X, 2, 0), by_zero]),
            2,
            vec!["split: x(qubit 0), x(qubit 1)", "split: x(qubit 2)"],
            vec![],
            QirTrap::DivisionByZero { instruction: 3 },
        ),
        (
            "a division with no call",
            program(&[&[by_zero, ret()]]),
            1,
            vec![],
            vec![],
            QirTrap::DivisionByZero { instruction: 0 },
        ),
        (
            "a division right after a batch",
            after_a_batch,
            1,
            vec!["batch: mz(qubit 0, result 0)"],
            vec!["mz(qubit 0, result 0)"],
            QirTrap::DivisionByZero { instruction: 2 },
        ),
        (
            "a measurement into a result past the program's",
            two_x_then(&[set(3, 4), measure_into_register(2, 3)]),
            3,
            vec!["split: x(qubit 0), x(qubit 1)"],
            vec![],
            QirTrap::NoSuchResult {
                instruction: 3,
                result: 4,
                results: 4,
            },
        ),
        (
            "correlated noise naming a register past the program's",
            past_the_registers,
            3,
            vec!["split: x(qubit 0), x(qubit 1)"],
            vec![],
            QirTrap::NoSuchRegister {
                instruction: 3,
                register: 4,
                registers: 4,
            },
        ),
        (
            "a write of 2 to a result",
            two_x_then(&[set(3, 2), binary(OP_WRITE_RESULT, FLAG_SRC1_IMM, 0, 3, 1)]),
            3,
            vec!["split: x(qubit 0), x(qubit 1)"],
            vec![],
            QirTrap::NotABit {
                instruction: 3,
                value: 2,
            },
        ),
        (
            "a load past memory",
            two_x_then(&[set(3, 5), binary(OP_LOAD, 0, 1, 3, 0)]),
            3,
            vec!["split: x(qubit 0), x(qubit 1)"],
            vec![],
            QirTrap::LoadPastMemory {
                instruction: 3,
                address: 5,
                words: 0,
            },
        ),
        (
            "a store past memory",
            two_x_then(&[set(3, 5), binary(OP_STORE, 0, 0, 1, 3)]),
            3,
            vec!["split: x(qubit 0), x(qubit 1)"],
            vec![],
            QirTrap::StorePastMemory {
                instruction: 3,
                address: 5,
                words: 0,
            },
        ),
        (
            "a read of a result with no value",
            no_value_after_a_batch,
            3,
            vec![
                "batch: mz(qubit 0, result 3), mz(qubit 1, result 1)",
                "split: mz(qubit 2, result 2)",
            ],
            vec!["mz(qubit 0, result 3), mz(qubit 1, result 1)"],
            QirTrap::MissingValue {
                instruction: 4,
                result: 3,
            },
        ),
    ];
    for (name, bytecode, n, split, whole, trap) in cases {
        let whole: Vec<String> = whole.into_iter().map(String::from).collect();
        assert_eq!(
            run(&bytecode, one_but_result_3_with_no_value),
            (whole, Err(trap.clone())),
            "{name}, whole"
        );
        let split: Vec<String> = split.into_iter().map(String::from).collect();
        let (stops, end) = split_run(&bytecode, n, one_but_result_3_with_no_value);
        let stops: Vec<String> = stops.iter().map(Piece::to_string).collect();
        assert_eq!((stops, end), (split, Err(trap)), "{name}, at most {n}");
    }
}

/// A context split at 1 that holds a trap (an `x`, then a division by a
/// register holding 0), restarted by the machine of its program and of
/// another: it runs as a started one, the trap gone.
#[test]
fn a_restarted_context_holds_no_trap() {
    let traps = program(&[&[
        gate(OPID_X, 0, 0),
        binary(OP_UDIV, FLAG_SRC0_IMM, 1, 5, 0),
        ret(),
    ]]);
    let traps = built(&traps);
    let held = QirMachine::new(&traps).with_split(at_most(1));
    let (_, another) = programs().swap_remove(0);
    for (name, bytecode) in [("its program", None), ("another", Some(&another))] {
        let mut context = held.start_shot();
        let stop = held.resume(&mut context, &[]).expect("a context starts");
        assert!(matches!(stop, QirStop::Split([_])), "{stop:?}");
        let other = bytecode.map(built);
        let restarting = other.as_ref().unwrap_or(&traps);
        let machine = QirMachine::new(restarting).with_split(at_most(1));
        machine.restart_shot(&mut context);
        let restarted = split_run_on(&machine, &mut context, |_| QirReadout::One);
        let started = split_run_on(&machine, &mut machine.start_shot(), |_| QirReadout::One);
        let shown = |(stops, end): (Vec<Piece>, _)| {
            (stops.iter().map(Piece::to_string).collect::<Vec<_>>(), end)
        };
        assert_eq!(shown(restarted), shown(started), "{name}");
    }
}

/// A loop of `x`s that never ends its batch, split at 2: it comes in
/// pieces of two `x`s, a thousand of them and on.
#[test]
fn a_batch_without_end_comes_in_pieces_without_end() {
    let bytecode = program(&[&[jump(1)], &[gate(OPID_X, 0, 0), jump(1)]]);
    let program = built(&bytecode);
    let machine = QirMachine::new(&program).with_split(at_most(2));
    let mut context = machine.start_shot();
    for _ in 0..1_000 {
        let stop = machine
            .resume(&mut context, &[])
            .expect("a split takes no outcomes");
        let QirStop::Split(piece) = stop else {
            panic!("the batch never ends: {stop:?}");
        };
        assert_eq!(
            Piece::of(true, piece).to_string(),
            "split: x(qubit 0), x(qubit 0)"
        );
    }
}

/// An `x`, another and a measurement, split at 1 up to the first split,
/// then resumed by a machine of whole batches of the same program: the
/// batch's other calls come whole, and its outcome ends the shot. And an
/// `x` then a division by a register holding 0, split at 1 up to the
/// split holding the trap: the machine of whole batches gives the trap.
#[test]
fn a_context_split_by_one_machine_runs_on_under_a_machine_of_whole_batches() {
    let traps = built(&program(&[&[
        gate(OPID_X, 0, 0),
        binary(OP_UDIV, FLAG_SRC0_IMM, 1, 5, 0),
        ret(),
    ]]));
    let whole = QirMachine::new(&traps);
    let split = whole.with_split(at_most(1));
    let mut context = split.start_shot();
    let stop = split.resume(&mut context, &[]).expect("a context starts");
    assert!(matches!(stop, QirStop::Split([_])), "{stop:?}");
    assert_eq!(
        whole.resume(&mut context, &[]),
        Err(QirTrap::DivisionByZero { instruction: 1 }),
        "the held trap"
    );
    let bytecode = program(&[&[
        gate(OPID_X, 0, 0),
        gate(OPID_X, 1, 0),
        measure(OPID_MZ, 0, 0),
        ret(),
    ]]);
    let program = built(&bytecode);
    let whole = QirMachine::new(&program);
    let split = whole.with_split(at_most(1));
    let mut context = split.start_shot();
    let stop = split.resume(&mut context, &[]).expect("a context starts");
    assert!(matches!(stop, QirStop::Split([_])), "{stop:?}");
    let stop = whole
        .resume(&mut context, &[])
        .expect("a split takes no outcomes");
    let QirStop::Batch(batch) = stop else {
        panic!("the batch's other calls come whole: {stop:?}");
    };
    assert_eq!(
        Piece::of(false, batch).to_string(),
        "batch: x(qubit 1), mz(qubit 0, result 0)"
    );
    let outcome = QirOutcome {
        result: 0,
        value: QirReadout::One,
    };
    assert!(matches!(
        whole.resume(&mut context, &[outcome]),
        Ok(QirStop::Ended(_))
    ));
}
