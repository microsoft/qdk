// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Whether a shot of a [`QirProgram`] may continue after a batch, as its
//! build finds over every path of its bytecode: where a context may stop
//! before the program's end, and a quantum call may run after it.

use super::super::{
    branch, call, call_return, correlated_noise, gate, jump, measure, op, program, read,
    readout_noise, record_bool, record_int, record_result, ret, set, switch, write,
};
use crate::bytecode::encoding::{
    FLAG_AUX1_IMM, FLAG_AUX2_IMM, OP_MEASURE, OP_PHI, OP_READ_LOSS, OP_READ_RESULT, OPID_MZ,
    OPID_X, VOID_RETURN,
};
use crate::bytecode::{AdaptiveProgram, Function, Instruction, PhiNodeEntry, SwitchCase};
use crate::qir::{QirCall, QirMachine, QirOutcome, QirProgram, QirReadout, QirStop};

/// Whether a shot of `bytecode`, which has no fault, may continue after a
/// batch.
fn continues(bytecode: &AdaptiveProgram<u64>) -> bool {
    QirProgram::new(bytecode)
        .expect("the program has no fault")
        .continues_after_a_batch()
}

/// The `mz` of the constant `qubit` into the result held in register
/// `result`.
fn measure_into_register(qubit: u64, result: u64) -> Instruction<u64> {
    Instruction {
        aux0: OPID_MZ,
        aux1: qubit,
        aux2: result,
        ..op(OP_MEASURE, FLAG_AUX1_IMM)
    }
}

/// The `mz` of the qubit held in register `qubit` into the constant
/// `result`.
fn measure_from_register(qubit: u64, result: u64) -> Instruction<u64> {
    Instruction {
        aux0: OPID_MZ,
        aux1: qubit,
        aux2: result,
        ..op(OP_MEASURE, FLAG_AUX2_IMM)
    }
}

/// A shot that measures qubit 0, reads the result (or its loss) and
/// branches on it: it continues when the branch it may take, its `other`
/// one here, flips qubit 1 or measures the qubit a register holds, and
/// not when the shot only records after the read.
#[test]
fn a_read_of_a_result_continues_when_a_quantum_call_may_follow_it() {
    for opcode in [OP_READ_RESULT, OP_READ_LOSS] {
        let reads_then = |taken: &[Instruction<u64>], other: &[Instruction<u64>]| {
            continues(&program(&[
                &[measure(OPID_MZ, 0, 0), read(opcode, 0, 0), branch(0, 1, 2)],
                taken,
                other,
            ]))
        };
        assert!(
            reads_then(&[ret()], &[gate(OPID_X, 1, 0), ret()]),
            "a call after the read, opcode {opcode:#x}"
        );
        assert!(
            reads_then(&[ret()], &[set(1, 1), measure_from_register(1, 1), ret()]),
            "a call on a register's qubit after the read, opcode {opcode:#x}"
        );
        assert!(
            !reads_then(&[record_result(0), ret()], &[ret()]),
            "no call after the read, opcode {opcode:#x}"
        );
    }
}

/// A shot that records result 0 between two measurements, the second in
/// the block it jumps to, continues; a shot that records its results
/// after its last measurement does not, nor one that branches on a
/// register to one of two blocks, each measuring a qubit and recording its
/// result.
#[test]
fn a_record_of_a_result_continues_when_a_quantum_call_may_follow_it() {
    assert!(continues(&program(&[
        &[measure(OPID_MZ, 0, 0), record_result(0), jump(1)],
        &[measure(OPID_MZ, 1, 1), record_result(1), ret()],
    ])));
    assert!(!continues(&program(&[&[
        measure(OPID_MZ, 0, 0),
        measure(OPID_MZ, 1, 1),
        record_result(0),
        record_result(1),
        ret(),
    ]])));
    assert!(!continues(&program(&[
        &[set(0, 1), branch(0, 1, 2)],
        &[measure(OPID_MZ, 0, 0), record_result(0), ret()],
        &[measure(OPID_MZ, 1, 1), record_result(1), ret()],
    ])));
}

/// A shot that measures qubit 0, writes result 0 and switches on a
/// register: it continues when the switch's default, or its case, goes to
/// a block that flips qubit 1, and not when the write follows its last
/// quantum call.
#[test]
fn a_write_of_a_result_continues_when_a_quantum_call_may_follow_it() {
    let writes_then = |default: u64, case: u64| {
        let mut bytecode = program(&[
            &[
                measure(OPID_MZ, 0, 0),
                write(1, 0),
                switch(0, default, 0, 1),
            ],
            &[gate(OPID_X, 1, 0), ret()],
            &[ret()],
        ]);
        bytecode.switch_cases = vec![SwitchCase::from_tuple((5, case))];
        continues(&bytecode)
    };
    assert!(writes_then(1, 2), "the default flips qubit 1");
    assert!(writes_then(2, 1), "the case flips qubit 1");
    assert!(!continues(&program(&[&[
        gate(OPID_X, 1, 0),
        measure(OPID_MZ, 0, 0),
        write(1, 0),
        record_result(0),
        ret(),
    ]])));
}

/// A noisy measurement as the Stim compiler writes it (`mz`, then readout
/// noise on its result), and a result measured twice, the second time of
/// a qubit held in a register: the second quantum call writing the result
/// starts
/// a second batch. Two measurements of distinct results do not, the second
/// of a qubit held in a register or not.
#[test]
fn two_quantum_calls_writing_one_result_continue() {
    let measures_then_records = |second: Instruction<u64>| {
        continues(&program(&[&[
            measure(OPID_MZ, 0, 0),
            second,
            record_result(0),
            record_result(1),
            ret(),
        ]]))
    };
    assert!(
        measures_then_records(readout_noise(0.01, 0)),
        "readout noise on the result"
    );
    assert!(
        measures_then_records(measure(OPID_MZ, 1, 0)),
        "a second measurement into the result"
    );
    assert!(
        measures_then_records(measure_from_register(1, 0)),
        "a second measurement into the result, of a qubit in a register"
    );
    assert!(
        !measures_then_records(measure(OPID_MZ, 1, 1)),
        "a measurement into another result"
    );
    assert!(
        !measures_then_records(measure_from_register(1, 1)),
        "a measurement into another result, of a qubit in a register"
    );
}

/// A shot that measures qubit 0 into result 0, then reads the result (or
/// its loss), records it or writes it, and adds correlated noise to qubit
/// 1 after that stop: the noise is a quantum call after the stop, so the
/// shot continues. With the noise before the stop, the shot does not.
#[test]
fn correlated_noise_after_a_stop_continues() {
    let continues_around = |before: &[Instruction<u64>], after: &[Instruction<u64>]| {
        let block = [
            &[set(1, 1), measure(OPID_MZ, 0, 0)],
            before,
            after,
            &[ret()],
        ]
        .concat();
        let mut bytecode = program(&[&block]);
        bytecode.call_args = vec![1];
        continues(&bytecode)
    };
    let noise = correlated_noise(1, 0);
    let stops = [
        ("a read", read(OP_READ_RESULT, 0, 0)),
        ("a read of its loss", read(OP_READ_LOSS, 0, 0)),
        ("a record", record_result(0)),
        ("a write", write(1, 0)),
    ];
    for (name, stop) in stops {
        assert!(
            continues_around(&[stop], &[noise]),
            "the noise after {name}"
        );
        assert!(
            !continues_around(&[noise], &[stop]),
            "the noise before {name}"
        );
    }
}

/// A shot that measures qubit 0 into result 0 and records the result, then
/// reaches correlated noise on qubit 1 by a jump, a branch on register 0, a
/// switch on it, a function call of the function that adds it, or a return from
/// the function that measures and records: the noise is a quantum call after
/// the stop, so the shot continues. With the noise before the record, on the
/// same edges, the shot does not.
#[test]
fn correlated_noise_continues_a_shot_on_any_edge_after_a_stop() {
    let noise = correlated_noise(1, 0);
    let measures = measure(OPID_MZ, 0, 0);
    let records = record_result(0);
    let calls = call(0, 0, 0, VOID_RETURN);
    // The program of `blocks`, its switch going to block 1 on 0, and its
    // block 1 a function's when it `calls`.
    let continues_with = |blocks: &[&[Instruction<u64>]], calls: bool| {
        let mut bytecode = program(blocks);
        bytecode.switch_cases = vec![SwitchCase::from_tuple((0, 1))];
        if calls {
            bytecode.function_table = vec![Function::from_tuple((1, 0, 0))];
        }
        bytecode.call_args = vec![1];
        continues(&bytecode)
    };
    let shapes = |first: Instruction<u64>, second: Instruction<u64>| {
        let start = [set(1, 1), measures, first];
        let jumps = [&start[..], &[jump(1)]].concat();
        let branches = [&start[..], &[branch(0, 1, 2)]].concat();
        let switches = [&start[..], &[switch(0, 2, 0, 1)]].concat();
        [
            ("a jump", continues_with(&[&jumps, &[second, ret()]], false)),
            (
                "a branch",
                continues_with(&[&branches, &[second, ret()], &[ret()]], false),
            ),
            (
                "a switch",
                continues_with(&[&switches, &[second, ret()], &[ret()]], false),
            ),
            (
                "a return",
                continues_with(
                    &[
                        &[set(1, 1), calls, second, ret()],
                        &[measures, first, call_return(0)],
                    ],
                    true,
                ),
            ),
        ]
    };
    for (edge, continues) in shapes(records, noise) {
        assert!(continues, "the noise after the record, by {edge}");
    }
    for (edge, continues) in shapes(noise, records) {
        assert!(!continues, "the noise before the record, by {edge}");
    }
    let calls_noise = |first: Instruction<u64>, second: Instruction<u64>| {
        continues_with(
            &[
                &[set(1, 1), measures, first, second, jump(2)],
                &[noise, call_return(0)],
                &[ret()],
            ],
            true,
        )
    };
    assert!(
        calls_noise(records, calls),
        "the noise in a function called after the record"
    );
    assert!(
        !calls_noise(calls, records),
        "the noise in a function called before the record"
    );
}

/// A shot that records register 0 as an `Int` or a `Bool`, or switches on
/// it, before it measures qubit 0 and records the result: none of these
/// reads or writes a result, so the shot does not continue.
#[test]
fn a_record_of_a_value_or_a_switch_does_not_continue() {
    let then_measures = [measure(OPID_MZ, 0, 0), record_result(0), ret()];
    let first_then_measures = |first: Instruction<u64>| {
        let block = [&[set(0, 1), first][..], &then_measures].concat();
        continues(&program(&[&block]))
    };
    assert!(!first_then_measures(record_int(0)), "an Int record");
    assert!(!first_then_measures(record_bool(0)), "a Bool record");
    let mut switches = program(&[&[set(0, 1), switch(0, 1, 0, 1)], &then_measures]);
    switches.switch_cases = vec![SwitchCase::from_tuple((1, 1))];
    assert!(!continues(&switches), "a switch");
}

/// A shot that measures into the result register 1 holds, beside a
/// measurement into result 1, or another into the result register 1
/// holds: the register may hold result 1, so the two may write one
/// result. Alone, it does not continue.
#[test]
fn a_quantum_call_writing_a_result_held_in_a_register_may_write_any() {
    let measures_into_register_then = |also: &[Instruction<u64>]| {
        let block = [
            &[set(1, 0), measure_into_register(0, 1)],
            also,
            &[record_result(0), record_result(1), ret()],
        ]
        .concat();
        continues(&program(&[&block]))
    };
    assert!(
        measures_into_register_then(&[measure(OPID_MZ, 1, 1)]),
        "another measurement"
    );
    assert!(
        measures_into_register_then(&[measure_into_register(1, 1)]),
        "another measurement into a register's result"
    );
    assert!(!measures_into_register_then(&[]), "alone");
}

/// A shot that flips qubit 1 and measures qubit 0 in a loop, until
/// register 0 clears, its qubit or its result held in register 1 or not,
/// or that measures qubit 0 in a block that branches back to itself once,
/// its `PHI` clearing register 1 on the way back (a loop of the
/// measurement and the branch alone): the measurement may run twice, so
/// the shot continues. A loop of flips alone, measured once after, does
/// not.
#[test]
fn a_quantum_call_writing_a_result_continues_when_it_may_run_twice() {
    let loops = |body: &[Instruction<u64>], after: &[Instruction<u64>]| {
        let body = [body, &[branch(0, 2, 3)]].concat();
        let after = [after, &[record_result(0), ret()]].concat();
        continues(&program(&[
            &[set(0, 1), set(1, 0), jump(1)],
            &body,
            &[set(0, 0), jump(1)],
            &after,
        ]))
    };
    assert!(
        loops(&[gate(OPID_X, 1, 0), measure(OPID_MZ, 0, 0)], &[]),
        "the measurement in the loop"
    );
    assert!(
        loops(&[gate(OPID_X, 1, 0), measure_into_register(0, 1)], &[]),
        "the measurement in the loop, into a register's result"
    );
    assert!(
        loops(&[gate(OPID_X, 1, 0), measure_from_register(1, 0)], &[]),
        "the measurement in the loop, of a register's qubit"
    );
    assert!(
        !loops(&[gate(OPID_X, 1, 0)], &[measure(OPID_MZ, 0, 0)]),
        "the measurement after the loop"
    );
    assert!(
        !loops(&[gate(OPID_X, 1, 0)], &[measure_into_register(0, 1)]),
        "the measurement after the loop, into a register's result"
    );
    assert!(
        !loops(&[gate(OPID_X, 1, 0)], &[measure_from_register(1, 0)]),
        "the measurement after the loop, of a register's qubit"
    );
    let phi = Instruction {
        dst: 1,
        aux0: 0,
        aux1: 2,
        ..op(OP_PHI, 0)
    };
    let mut self_loop = program(&[
        &[set(1, 1), set(2, 0), jump(1)],
        &[phi, measure(OPID_MZ, 0, 0), branch(1, 1, 2)],
        &[record_result(0), ret()],
    ]);
    self_loop.phi_entries = vec![
        PhiNodeEntry::from_tuple((0, 1)),
        PhiNodeEntry::from_tuple((1, 2)),
    ];
    assert!(
        continues(&self_loop),
        "a block measuring and branching back to itself"
    );
}

/// A function that measures qubit 0 into result 0, its qubit or its
/// result held in register 1 or not, its block between the entry block
/// and the block that records: called twice, its measurement runs twice
/// in the shot; called once, it does not continue.
#[test]
fn a_quantum_call_writing_a_result_continues_when_its_function_may_run_twice() {
    let calls = |count: usize, measurement: Instruction<u64>| {
        let entry = [vec![call(0, 0, 0, VOID_RETURN); count], vec![jump(2)]].concat();
        let mut bytecode = program(&[
            &entry,
            &[set(1, 0), measurement, call_return(0)],
            &[record_result(0), ret()],
        ]);
        bytecode.function_table = vec![Function::from_tuple((1, 0, 0))];
        continues(&bytecode)
    };
    let measurements = [
        ("constant", measure(OPID_MZ, 0, 0)),
        ("into a register's result", measure_into_register(0, 1)),
        ("of a register's qubit", measure_from_register(1, 0)),
    ];
    for (operands, measurement) in measurements {
        assert!(calls(2, measurement), "called twice, {operands}");
        assert!(!calls(1, measurement), "called once, {operands}");
    }
}

/// The number of batches a shot of `program` yields, each result read as
/// `value`.
fn batches_of_a_shot(program: &QirProgram, value: QirReadout) -> usize {
    let machine = QirMachine::new(program);
    let mut context = machine.start_shot();
    let mut outcomes = Vec::new();
    let mut batches = 0;
    loop {
        match machine
            .resume(&mut context, &outcomes)
            .expect("the shot does not trap")
        {
            QirStop::Batch(batch) => {
                batches += 1;
                // No program here yields this many: one that does runs
                // on without end.
                assert!(batches < 100, "the shot runs on without end");
                outcomes = batch
                    .iter()
                    .filter_map(QirCall::result)
                    .map(|result| QirOutcome { result, value })
                    .collect();
            }
            QirStop::Ended(_) => return batches,
        }
    }
}

/// Programs of each kind of stop, read at each value a result can hold:
/// a read, a record or a write of a result before a quantum call; readout
/// noise
/// after a measurement; a measurement in a loop or in a function called
/// twice; correlated noise after a record; and programs that do not
/// continue. A shot of one the build says does not continue yields its
/// quantum calls in one batch, or none; the others, which a shot runs past a
/// stop, yield more.
#[test]
fn a_shot_of_a_program_that_does_not_continue_yields_one_batch_at_most() {
    let mut writes = program(&[
        &[measure(OPID_MZ, 0, 0), write(1, 0), switch(0, 1, 0, 1)],
        &[gate(OPID_X, 1, 0), ret()],
    ]);
    writes.switch_cases = vec![SwitchCase::from_tuple((5, 1))];
    let mut called_twice = program(&[
        &[
            call(0, 0, 0, VOID_RETURN),
            call(0, 0, 0, VOID_RETURN),
            jump(2),
        ],
        &[measure(OPID_MZ, 0, 0), call_return(0)],
        &[record_result(0), ret()],
    ]);
    called_twice.function_table = vec![Function::from_tuple((1, 0, 0))];
    let mut noise_after_a_record = program(&[&[
        set(1, 1),
        measure(OPID_MZ, 0, 0),
        record_result(0),
        correlated_noise(1, 0),
        ret(),
    ]]);
    noise_after_a_record.call_args = vec![1];
    let continuing = [
        program(&[
            &[
                measure(OPID_MZ, 0, 0),
                read(OP_READ_RESULT, 0, 0),
                branch(0, 1, 2),
            ],
            &[gate(OPID_X, 1, 0), ret()],
            &[gate(OPID_X, 1, 0), ret()],
        ]),
        program(&[&[
            measure(OPID_MZ, 0, 0),
            read(OP_READ_LOSS, 0, 0),
            gate(OPID_X, 1, 0),
            ret(),
        ]]),
        program(&[
            &[measure(OPID_MZ, 0, 0), record_result(0), jump(1)],
            &[measure(OPID_MZ, 1, 1), record_result(1), ret()],
        ]),
        writes,
        program(&[&[
            measure(OPID_MZ, 0, 0),
            readout_noise(0.01, 0),
            record_result(0),
            ret(),
        ]]),
        program(&[
            &[set(0, 1), jump(1)],
            &[measure(OPID_MZ, 0, 0), branch(0, 2, 3)],
            &[set(0, 0), jump(1)],
            &[record_result(0), ret()],
        ]),
        called_twice,
        noise_after_a_record,
    ];
    let not_continuing = [
        program(&[&[
            gate(OPID_X, 1, 0),
            measure(OPID_MZ, 0, 0),
            measure(OPID_MZ, 1, 1),
            record_result(0),
            record_result(1),
            ret(),
        ]]),
        program(&[
            &[set(0, 1), branch(0, 1, 2)],
            &[measure(OPID_MZ, 0, 0), record_result(0), ret()],
            &[measure(OPID_MZ, 1, 1), record_result(1), ret()],
        ]),
        program(&[&[set(0, 1), record_int(0), record_bool(0), ret()]]),
    ];
    let values = [QirReadout::Zero, QirReadout::One, QirReadout::Loss];
    for (index, bytecode) in continuing.iter().enumerate() {
        let program = QirProgram::new(bytecode).expect("the program has no fault");
        assert!(
            values
                .iter()
                .any(|&value| batches_of_a_shot(&program, value) > 1),
            "continuing program {index} runs past a stop"
        );
    }
    for (index, bytecode) in continuing.iter().chain(&not_continuing).enumerate() {
        let program = QirProgram::new(bytecode).expect("the program has no fault");
        for value in values {
            let batches = batches_of_a_shot(&program, value);
            assert!(
                program.continues_after_a_batch() || batches <= 1,
                "program {index}, results {value:?}: {batches} batches"
            );
        }
    }
}
