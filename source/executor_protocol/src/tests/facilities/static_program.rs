// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::tests::test_double::{Model, TestDoubleExecutor, TestDoubleModel};
use crate::tests::{
    Bits, BitsState, Count, YieldNow, bits_set, call, ends_soon, flip_and_toss, run_tasks, values,
};
use crate::{
    Action, CallInput, Capabilities, Close, Describe, Executor, ExecutorLifecycle, ImmediateReply,
    InstructionBatch, InstructionBatchRef, InstructionCall, InstructionSet, JobFaults, Need, Needs,
    OnePerShot, Opcode, Operand, OperandKind, Outcome, Outcomes, Pauli, Rejected, ShotContext,
    ShotOutcome, StaticProgram, StaticProgramError, Value, compose, driver, executor_fn,
};

/// [`flip_and_toss`] as a static program in [`bits_set`]'s numbering,
/// needing that set, and `qubits` when stated.
fn flip_and_toss_program(qubits: Option<u32>) -> StaticProgram<InstructionCall> {
    let set = bits_set();
    let needs = Needs {
        qubits,
        ..Needs::default().with_instruction_set(Some(Arc::new(set.clone())))
    };
    StaticProgram::new(needs, flip_and_toss(&set))
}

/// A program that flips bit 0 and reads it under tag 0, then tosses bit 1
/// and reads it under tag 1, run on four shots that are not a range: it
/// gives every shot's readout of the first batch, then every shot's of the
/// second, and each shot's bit 0 reads one, as flipped.
#[test]
fn a_static_program_gives_every_shots_outcomes_batch_after_batch() {
    let chain = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let outcomes = chain
        .run(flip_and_toss_program(None), [2, 5, 6, 11])
        .expect("the bits run the program's set")
        .wait()
        .0
        .expect("the bits neither refuse nor fail");
    let by_batch: Vec<Vec<(u32, u32)>> = outcomes
        .chunks(4)
        .map(|batch| {
            let mut keys: Vec<(u32, u32)> = batch.iter().map(|o| (o.shot, o.tag)).collect();
            keys.sort_unstable();
            keys
        })
        .collect();
    assert_eq!(
        by_batch,
        [
            [(2, 0), (5, 0), (6, 0), (11, 0)],
            [(2, 1), (5, 1), (6, 1), (11, 1)]
        ]
    );
    assert!(
        values(&outcomes[..4])
            .iter()
            .all(|&(_, _, v)| v == Value::One),
        "{outcomes:?}"
    );
}

/// The same program on shots given out of order, `[11, 2, 6]`, whose
/// bits deliver each batch's outcomes in the order the shots are given:
/// the program gives them in that order, batch after batch, not sorted by
/// shot.
#[test]
fn a_static_program_gives_each_batchs_outcomes_in_the_order_its_reply_delivers_them() {
    let chain = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let outcomes = chain
        .run(flip_and_toss_program(None), [11, 2, 6])
        .expect("the bits run the program's set")
        .wait()
        .0
        .expect("the bits neither refuse nor fail");
    let keys: Vec<(u32, u32)> = outcomes.iter().map(|o| (o.shot, o.tag)).collect();
    assert_eq!(keys, [(11, 0), (2, 0), (6, 0), (11, 1), (2, 1), (6, 1)]);
}

/// A program of three reads, the second of which (a `false`) the
/// executor's check refuses: the program stops there, naming that batch
/// and the refusal as the executor typed it, and the third batch is never
/// sent.
#[test]
fn a_static_program_stops_at_the_first_batch_the_chain_refuses() {
    let model = TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    };
    let seen = Rc::clone(&model.seen);
    let program = StaticProgram::new(Needs::default(), vec![vec![true], vec![false], vec![true]]);
    let chain = (driver() >> TestDoubleExecutor::with_model(model, 7))
        .expect("the double takes any batch of `bool`s");
    let (outcomes, _) = chain.run(program, [4]).expect("a new shot").wait();
    assert_eq!(
        outcomes,
        Err(StaticProgramError::Refused {
            batch: 1,
            rejected: Rejected::Instruction {
                at: 0,
                reason: "not a read".into()
            },
        })
    );
    assert_eq!(seen.borrow().calls[&4], 1, "only the first batch ran");
}

/// A program of a read, then a `false` the executor's check refuses at
/// its first instruction: the refusal reads as a fault the program's check
/// finds when the job is given to the chain, by the instruction, then the
/// batch.
#[test]
fn a_refusal_at_an_instruction_reads_as_the_programs_check_reads_it() {
    let model = TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    };
    let program = StaticProgram::new(Needs::default(), vec![vec![true], vec![false]]);
    let chain = (driver() >> TestDoubleExecutor::with_model(model, 7))
        .expect("the double takes any batch of `bool`s");
    let (outcomes, _) = chain.run(program, [4]).expect("a new shot").wait();
    assert_eq!(
        outcomes.expect_err("the `false` is refused").to_string(),
        "instruction 0: batch 1: not a read"
    );
}

/// A program of two reads on a double whose device fails shot 4's batches
/// as they are called: the program stops at its first batch, naming it and
/// the reply's reason, and sends the executor no later batch.
#[test]
fn a_static_program_stops_at_the_first_batch_that_fails() {
    let model = TestDoubleModel {
        failing: Some(4),
        ..TestDoubleModel::default()
    };
    let seen = Rc::clone(&model.seen);
    let chain = (driver() >> TestDoubleExecutor::with_model(model, 7))
        .expect("the double takes any batch of `bool`s");
    let (outcomes, _) = chain
        .run(
            StaticProgram::new(Needs::default(), vec![vec![true], vec![true]]),
            [4],
        )
        .expect("a new shot")
        .wait();
    assert_eq!(
        outcomes,
        Err(StaticProgramError::Failed {
            batch: 0,
            reason: "lost".into(),
        })
    );
    assert_eq!(
        seen.borrow().calls.get(&4),
        Some(&1),
        "a batch after the failure was sent"
    );
}

/// A program of an empty batch, then a read, on two shots: an empty batch
/// is a call like any other, so each shot receives two calls, the empty
/// one first.
#[test]
fn every_batch_of_a_static_program_is_sent_to_every_shot_empty_ones_too() {
    let model = TestDoubleModel::default();
    let seen = Rc::clone(&model.seen);
    let chain = (driver() >> TestDoubleExecutor::with_model(model, 7))
        .expect("the double takes any batch of `bool`s");
    let (outcomes, _) = chain
        .run(
            StaticProgram::new(Needs::default(), vec![vec![], vec![true]]),
            [4, 9],
        )
        .expect("new shots")
        .wait();
    outcomes.expect("the double neither refuses nor fails");
    let calls: Vec<(u32, u64)> = seen.borrow().calls.iter().map(|(&s, &n)| (s, n)).collect();
    assert_eq!(calls, [(4, 2), (9, 2)], "an empty batch was not sent");
}

/// A program of two reads on a double whose calls for shot 4 are cancelled
/// as they are called: a cancelled reply is a failure of its batch, the
/// first ([`StaticProgramError::Failed`]), and the program stops there.
#[test]
fn a_static_program_stops_at_a_cancelled_batch_as_failed() {
    let model = TestDoubleModel {
        cancelling: Some(4),
        ..TestDoubleModel::default()
    };
    let chain = (driver() >> TestDoubleExecutor::with_model(model, 7))
        .expect("the double takes any batch of `bool`s");
    let (outcomes, _) = chain
        .run(
            StaticProgram::new(Needs::default(), vec![vec![true], vec![true]]),
            [4],
        )
        .expect("a new shot")
        .wait();
    let error = outcomes.expect_err("a cancelled batch stops the program");
    assert!(
        matches!(error, StaticProgramError::Failed { batch: 0, .. }),
        "{error:?}"
    );
}

/// A program of one read, run awaited beside another task, on a double
/// whose read's reply ends only when that task ends it: the run leaves its
/// thread to the task while the reply is pending, then gives the read.
#[test]
fn a_static_program_run_awaited_leaves_its_thread_to_other_tasks_while_a_reply_is_pending() {
    let log = ends_soon(|| {
        let model = TestDoubleModel {
            deferred: vec![0],
            ..TestDoubleModel::default()
        };
        let deferred = Rc::clone(&model.seen);
        let run = (driver() >> TestDoubleExecutor::with_model(model, 7))
            .expect("the double takes any batch of `bool`s")
            .run(StaticProgram::new(Needs::default(), vec![vec![true]]), [0])
            .expect("a new shot");
        let log = RefCell::new(Vec::new());
        let awaits = async {
            let (outcomes, _) = run.await;
            let outcomes = outcomes.expect("the double neither refuses nor fails");
            log.borrow_mut()
                .push(format!("run gave {}", outcomes.len()));
        };
        let ends_the_read = async {
            loop {
                let read = deferred.borrow_mut().replies.get_mut(&0).and_then(Vec::pop);
                if let Some(read) = read {
                    log.borrow_mut().push("read ended".to_string());
                    read.done();
                    return;
                }
                YieldNow(false).await;
            }
        };
        run_tasks(vec![Box::pin(awaits), Box::pin(ends_the_read)], &log);
        let mut log = log.into_inner();
        log.retain(|entry| entry != "park");
        log
    });
    assert_eq!(log, ["read ended", "run gave 1"]);
}

/// A program that flips bit 0 in its first batch and reads it in its
/// second: its second call continues each shot where the first left it,
/// so on bits that keep no state between calls it is refused for
/// `resumable_shots` when given to the chain, and the executor receives no
/// call.
#[test]
fn a_call_before_a_static_programs_last_batch_needs_resumable_shots() {
    let set = bits_set();
    let program = StaticProgram::new(
        Needs::default().with_instruction_set(Some(Arc::new(set.clone()))),
        vec![
            vec![call(&set, "x", &[Operand::Block(0)])],
            vec![read(0, 0)],
        ],
    );
    let executor = StatelessBits::new(7);
    let received = Rc::clone(&executor.received);
    let chain = compose(driver(), executor).expect("the bits run calls by opcode");
    let Err(JobFaults(faults)) = chain.run(program, [4, 9]) else {
        panic!("a call before the last batch must need resumable shots");
    };
    assert!(is_unmet_resumable_shots(&faults), "{faults:?}");
    assert_eq!(received.get(), 0, "a refused job sends nothing");
}

/// A program whose only call before its last batch is in neither its first
/// earlier batch nor the one just before its last (`[], [x], [], [read]`):
/// the later batches still continue each shot where that call left it, so
/// on bits that keep no state between calls it is refused for `resumable_shots`
/// when given to the chain, and the executor receives no call.
#[test]
fn a_call_in_any_batch_before_a_static_programs_last_needs_resumable_shots() {
    let set = bits_set();
    let program = StaticProgram::new(
        Needs::default().with_instruction_set(Some(Arc::new(set.clone()))),
        vec![
            vec![],
            vec![call(&set, "x", &[Operand::Block(0)])],
            vec![],
            vec![read(0, 0)],
        ],
    );
    let executor = StatelessBits::new(7);
    let received = Rc::clone(&executor.received);
    let chain = compose(driver(), executor).expect("the bits run calls by opcode");
    let Err(JobFaults(faults)) = chain.run(program, [4, 9]) else {
        panic!("a call in a middle batch must need resumable shots");
    };
    assert!(is_unmet_resumable_shots(&faults), "{faults:?}");
    assert_eq!(received.get(), 0, "a refused job sends nothing");
}

/// The same for typed calls: a program of the test double's reads, one per
/// batch, is refused on an executor without resumable shots when given to the
/// chain.
#[test]
fn a_typed_call_before_a_static_programs_last_batch_needs_resumable_shots() {
    let model = TestDoubleModel {
        without_resumable_shots: true,
        ..TestDoubleModel::default()
    };
    let program = StaticProgram::new(Needs::default(), vec![vec![true], vec![true]]);
    let chain = (driver() >> TestDoubleExecutor::with_model(model, 7))
        .expect("the double takes any batch of `bool`s");
    let Err(JobFaults(faults)) = chain.run(program, [4, 9]) else {
        panic!("a typed call before the last batch must need resumable shots");
    };
    assert!(is_unmet_resumable_shots(&faults), "{faults:?}");
}

/// A program of one batch whose writer states `resumable_shots` anyway: its
/// batches add no need, but they take none of the needs it states away, so
/// on bits that keep no state between calls it is refused for `resumable_shots`
/// when given to the chain.
#[test]
fn a_static_programs_stated_resumable_shots_are_kept() {
    let set = bits_set();
    let program = StaticProgram::new(
        Needs::default()
            .with_instruction_set(Some(Arc::new(set.clone())))
            .with_resumable_shots(true),
        vec![vec![call(&set, "x", &[Operand::Block(0)]), read(0, 0)]],
    );
    let chain = compose(driver(), StatelessBits::new(7)).expect("the bits run calls by opcode");
    let Err(JobFaults(faults)) = chain.run(program, [4, 9]) else {
        panic!("a program that states resumable shots must need them");
    };
    assert!(is_unmet_resumable_shots(&faults), "{faults:?}");
}

/// A program of no batches, stating its set: it adds no need to those it
/// states, so it runs on bits that keep no state between calls, sends them
/// no call, and gives no outcomes.
#[test]
fn a_static_program_of_no_batches_adds_no_need_and_gives_no_outcomes() {
    let program = StaticProgram::new(
        Needs::default().with_instruction_set(Some(Arc::new(bits_set()))),
        Vec::<Vec<InstructionCall>>::new(),
    );
    let executor = StatelessBits::new(7);
    let received = Rc::clone(&executor.received);
    let chain = compose(driver(), executor).expect("the bits run calls by opcode");
    let (outcomes, _) = chain
        .run(program, [4, 9])
        .expect("a program of no batches needs no resumable shots")
        .wait();
    assert_eq!(outcomes, Ok(vec![]));
    assert_eq!(received.get(), 0, "a program of no batches sends no call");
}

/// A program that flips bit 0 and reads it in one batch: no call continues
/// a shot, so it runs on bits that keep no state between calls, each
/// shot's bit reading one.
#[test]
fn a_static_program_of_one_batch_runs_on_an_executor_without_resumable_shots() {
    let set = bits_set();
    let program = StaticProgram::new(
        Needs::default().with_instruction_set(Some(Arc::new(set.clone()))),
        vec![vec![call(&set, "x", &[Operand::Block(0)]), read(0, 0)]],
    );
    let chain = compose(driver(), StatelessBits::new(7)).expect("the bits run calls by opcode");
    let outcomes = chain
        .run(program, [4, 9])
        .expect("a program of one batch needs no resumable shots")
        .wait()
        .0
        .expect("the bits neither refuse nor fail");
    let mut read = values(&outcomes);
    read.sort_unstable_by_key(|&(shot, tag, _)| (shot, tag));
    assert_eq!(read, [(4, 0, Value::One), (9, 0, Value::One)]);
}

/// A program whose batches before its last are empty: they continue no
/// shot, so it runs on bits that keep no state between calls, each shot's
/// bit reading one.
#[test]
fn empty_batches_before_a_static_programs_last_add_no_need() {
    let set = bits_set();
    let program = StaticProgram::new(
        Needs::default().with_instruction_set(Some(Arc::new(set.clone()))),
        vec![
            vec![],
            vec![],
            vec![call(&set, "x", &[Operand::Block(0)]), read(0, 0)],
        ],
    );
    let chain = compose(driver(), StatelessBits::new(7)).expect("the bits run calls by opcode");
    let outcomes = chain
        .run(program, [4, 9])
        .expect("empty earlier batches continue no shot")
        .wait()
        .0
        .expect("the bits neither refuse nor fail");
    let mut read = values(&outcomes);
    read.sort_unstable_by_key(|&(shot, tag, _)| (shot, tag));
    assert_eq!(read, [(4, 0, Value::One), (9, 0, Value::One)]);
}

/// A program that flips bit 0 and reads it in its first batch, then sends
/// an empty batch: an empty batch is a call like any other, so the second
/// continues each shot, and on bits that keep no state between calls the
/// job is refused for `resumable_shots` when given to the chain, the executor
/// receiving no call.
#[test]
fn an_empty_last_batch_after_calls_needs_resumable_shots() {
    let set = bits_set();
    let program = StaticProgram::new(
        Needs::default().with_instruction_set(Some(Arc::new(set.clone()))),
        vec![
            vec![call(&set, "x", &[Operand::Block(0)]), read(0, 0)],
            vec![],
        ],
    );
    let executor = StatelessBits::new(7);
    let received = Rc::clone(&executor.received);
    let chain = compose(driver(), executor).expect("the bits run calls by opcode");
    let Err(JobFaults(faults)) = chain.run(program, [4, 9]) else {
        panic!("an empty batch after calls must need resumable shots");
    };
    assert!(is_unmet_resumable_shots(&faults), "{faults:?}");
    assert_eq!(received.get(), 0, "a refused job sends nothing");
}

/// Whether `faults` is one fault, an unmet `resumable_shots`.
fn is_unmet_resumable_shots(faults: &[Rejected]) -> bool {
    matches!(faults, [Rejected::Unmet(unmet)] if unmet.need == Need::ResumableShots)
}

/// A static program needs what it states: one that needs three qubits is
/// refused on two bits when given to the chain. A refused job gives no
/// [`Run`](crate::Run), so nothing of it runs.
#[test]
fn a_static_programs_needs_are_checked_when_given_to_a_chain() {
    let chain = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let Err(JobFaults(faults)) = chain.run(flip_and_toss_program(Some(3)), 0..2) else {
        panic!("three qubits on two bits must be refused");
    };
    let [fault] = faults.as_slice() else {
        panic!("one fault: {faults:?}");
    };
    assert!(
        matches!(fault, Rejected::Unmet(unmet) if unmet.need == Need::Qubits),
        "{faults:?}"
    );
}

/// A program that states no qubits flips bit 0 and reads it, then flips
/// bit 2, given to a device of two bits whose check refuses a call on a bit
/// it lacks. Its calls' qubits are not read when the job is given, so the
/// chain accepts it; the bit it lacks is found as its batch runs, by the
/// device's check, and the program stops there, the first batch's
/// outcomes dropped.
#[test]
fn a_qubit_a_static_program_does_not_state_is_found_only_by_the_check_at_its_batch() {
    let set = bits_set();
    let program = StaticProgram::new(
        Needs::default().with_instruction_set(Some(Arc::new(set.clone()))),
        vec![
            vec![
                call(&set, "x", &[Operand::Block(0)]),
                call(&set, "measure", &[Operand::Block(0), Operand::Tag(0)]),
            ],
            vec![call(&set, "x", &[Operand::Block(2)])],
        ],
    );
    let two_bits = Capabilities::default()
        .with_qubits(2)
        .with_resumable_shots(true)
        .with_instruction_set(Some(Arc::new(set.clone())));
    let device = executor_fn(
        two_bits,
        |_: ShotContext| Ok([false; 2]),
        move |bits: &mut [bool; 2],
              input: &CallInput<'_, InstructionCall>,
              outcomes: &mut Outcomes<'_>| {
            for call in input.instructions() {
                let operand = |i: usize| match call.operands[i] {
                    Operand::Block(n) | Operand::Tag(n) => n,
                    _ => unreachable!("the set takes blocks and tags"),
                };
                let bit = &mut bits[operand(0) as usize];
                if set.mnemonic(call.opcode) == Some("x") {
                    *bit = !*bit;
                } else {
                    outcomes.push(Outcome::new(operand(1), (*bit).into()));
                }
            }
            Ok(())
        },
    )
    .check(|input: &CallInput<'_, InstructionCall>| {
        let lacked = input
            .instructions()
            .iter()
            .position(|call| matches!(call.operands[0], Operand::Block(bit) if bit >= 2));
        match lacked {
            Some(at) => Err(Rejected::Instruction {
                at,
                reason: "no such bit".into(),
            }),
            None => Ok(()),
        }
    })
    .shots(OnePerShot);
    let chain = (driver() >> device).expect("the device runs calls by opcode");
    let (outcomes, _) = chain
        .run(program, [3])
        .expect("the job states no qubits, so the chain accepts it")
        .wait();
    assert_eq!(
        outcomes,
        Err(StaticProgramError::Refused {
            batch: 1,
            rejected: Rejected::Instruction {
                at: 0,
                reason: "no such bit".into()
            },
        })
    );
}

/// A program of calls by opcode that states no set: its opcodes are in no
/// numbering the chain can map, so it is refused for the instructions it
/// needs when given to the chain, rather than run in the executor's own
/// numbering. A refused job gives no [`Run`](crate::Run), so nothing of it
/// runs.
#[test]
fn a_static_program_by_opcode_that_states_no_set_is_refused() {
    let chain = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let program = StaticProgram::new(Needs::default(), flip_and_toss(&bits_set()));
    let Err(JobFaults(faults)) = chain.run(program, 0..2) else {
        panic!("calls by opcode in no stated set must be refused");
    };
    let [Rejected::Unmet(unmet)] = faults.as_slice() else {
        panic!("one unmet need: {faults:?}");
    };
    assert_eq!(
        (unmet.need, unmet.needed, unmet.offered),
        (Need::InstructionSet, 1, 0)
    );
}

/// A program of calls by opcode with no batches yet, which states no set:
/// sending nothing does not excuse it, so it is refused for the
/// instructions it needs when given to the chain, as one with calls is,
/// and the executor receives no call.
#[test]
fn a_static_program_by_opcode_of_no_batches_that_states_no_set_is_refused() {
    let executor = StatelessBits::new(7);
    let received = Rc::clone(&executor.received);
    let chain = compose(driver(), executor).expect("the bits run calls by opcode");
    let program = StaticProgram::new(Needs::default(), Vec::<Vec<InstructionCall>>::new());
    let Err(JobFaults(faults)) = chain.run(program, [4, 9]) else {
        panic!("calls by opcode in no stated set must be refused");
    };
    let [Rejected::Unmet(unmet)] = faults.as_slice() else {
        panic!("one unmet need: {faults:?}");
    };
    assert_eq!(unmet.need, Need::InstructionSet, "{faults:?}");
    assert_eq!(received.get(), 0, "a refused job sends nothing");
}

/// A call of opcode 3, which the bits' set lacks.
fn outside_the_bits_set() -> InstructionCall {
    InstructionCall::new(Opcode::new(3), [Operand::Block(0)])
}

/// [`flip_and_toss_program`] with [`outside_the_bits_set`] at each
/// `(batch, index)` of `outside`, in order, needing `qubits` when stated.
fn with_calls_outside_its_set(
    qubits: Option<u32>,
    outside: &[(usize, usize)],
) -> StaticProgram<InstructionCall> {
    let set = bits_set();
    let mut batches = flip_and_toss(&set);
    for &(batch, at) in outside {
        batches[batch].insert(at, outside_the_bits_set());
    }
    let needs = Needs {
        qubits,
        ..Needs::default().with_instruction_set(Some(Arc::new(set)))
    };
    StaticProgram::new(needs, batches)
}

/// Whether `fault` is the instruction fault of a call at `at` in batch
/// `batch`.
fn is_a_call_fault_at(fault: &Rejected, batch: usize, at: usize) -> bool {
    matches!(
        fault,
        Rejected::Instruction { at: index, reason }
            if *index == at && reason.contains(&format!("batch {batch}:"))
    )
}

/// A program whose second batch ends with a call its set lacks is refused
/// when given to the chain, naming the call by its index and batch
/// (`instruction 2: batch 1: …`): before
/// its first batch runs, rather than at its second, with the first run. A
/// refused job gives no [`Run`](crate::Run), so nothing of it runs.
#[test]
fn a_call_outside_the_programs_set_is_refused() {
    let chain = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let Err(JobFaults(faults)) = chain.run(with_calls_outside_its_set(None, &[(1, 2)]), 0..2)
    else {
        panic!("a call outside the program's set must be refused");
    };
    let [fault] = faults.as_slice() else {
        panic!("one fault: {faults:?}");
    };
    assert!(is_a_call_fault_at(fault, 1, 2), "{faults:?}");
    assert!(
        fault.to_string().starts_with("instruction 2: batch 1: "),
        "{fault}"
    );
}

/// A program with calls its set lacks between the flip and its read and
/// after the read in its first batch, and first in its second: all three
/// are reported, in the program's order (by batch, then index, not by
/// index alone), each by its batch and its index in it, so that all are
/// fixed in one go.
#[test]
fn every_call_outside_a_static_programs_set_is_reported() {
    let chain = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let program = with_calls_outside_its_set(None, &[(0, 1), (0, 3), (1, 0)]);
    let Err(JobFaults(faults)) = chain.run(program, 0..2) else {
        panic!("calls outside the program's set must be refused");
    };
    let [first, second, third] = faults.as_slice() else {
        panic!("three faults: {faults:?}");
    };
    assert!(is_a_call_fault_at(first, 0, 1), "{faults:?}");
    assert!(is_a_call_fault_at(second, 0, 3), "{faults:?}");
    assert!(is_a_call_fault_at(third, 1, 0), "{faults:?}");
}

/// A program that needs three qubits of the two bits and calls outside its
/// set: both faults are reported at once, the unmet need first, so that
/// both are fixed in one go.
#[test]
fn a_programs_call_faults_are_reported_with_its_unmet_needs() {
    let chain = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let program = with_calls_outside_its_set(Some(3), &[(1, 2)]);
    let Err(JobFaults(faults)) = chain.run(program, 0..2) else {
        panic!("three qubits and a call outside the set must be refused");
    };
    let [qubits, call] = faults.as_slice() else {
        panic!("two faults: {faults:?}");
    };
    assert!(
        matches!(qubits, Rejected::Unmet(unmet) if unmet.need == Need::Qubits),
        "{faults:?}"
    );
    assert!(is_a_call_fault_at(call, 1, 2), "{faults:?}");
}

/// A read of `bit` under `tag`, in [`bits_set`]'s numbering.
fn read(bit: u32, tag: u32) -> InstructionCall {
    call(
        &bits_set(),
        "measure",
        &[Operand::Block(bit), Operand::Tag(tag)],
    )
}

/// [`flip_and_toss`] in [`bits_set`]'s numbering, changed by `edit`, as a
/// static program needing that set.
fn flip_and_toss_edited(
    edit: impl FnOnce(&mut Vec<Vec<InstructionCall>>),
) -> StaticProgram<InstructionCall> {
    let set = bits_set();
    let mut batches = flip_and_toss(&set);
    edit(&mut batches);
    StaticProgram::new(
        Needs::default().with_instruction_set(Some(Arc::new(set))),
        batches,
    )
}

/// The faults of `program` given to a chain on the bits, which offer
/// `set`.
fn faults_on(set: InstructionSet, program: StaticProgram<InstructionCall>) -> Vec<Rejected> {
    let chain = (driver() >> TestDoubleExecutor::with_model(BitsOffering(set), 7))
        .expect("the bits run calls by opcode");
    let Err(JobFaults(faults)) = chain.run(program, [2, 5]) else {
        panic!("the program must be refused");
    };
    faults
}

/// The bits, offering a set of their own; they run only [`bits_set`]'s
/// instructions, but a refused job runs none.
struct BitsOffering(InstructionSet);

impl Describe for BitsOffering {
    fn capabilities(&self) -> Capabilities {
        (Bits::default().capabilities()).with_instruction_set(Some(Arc::new(self.0.clone())))
    }
}

impl Model for BitsOffering {
    type Instruction = InstructionCall;
    type State = BitsState;

    fn state(&self, ctx: &ShotContext) -> Result<BitsState, String> {
        Bits::default().state(ctx)
    }
}

/// Bits that keep no state between calls: each call runs every shot it
/// names on fresh bits, made from the seed and the shot, so they offer no
/// resumable shots. They keep only whether they are closed, on their
/// [`ExecutorLifecycle`].
struct StatelessBits {
    seed: u64,
    lifecycle: ExecutorLifecycle,
    /// The calls it received, refused ones too, shared with the test.
    received: Rc<Cell<u64>>,
}

impl StatelessBits {
    fn new(seed: u64) -> Self {
        Self {
            seed,
            lifecycle: ExecutorLifecycle::default(),
            received: Rc::default(),
        }
    }
}

impl Describe for StatelessBits {
    fn capabilities(&self) -> Capabilities {
        Bits::default().capabilities().with_resumable_shots(false)
    }
}

impl Executor<InstructionBatch<InstructionCall>> for StatelessBits {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        self.received.set(self.received.get() + 1);
        self.lifecycle.admit()?;
        let mut outcomes = Vec::new();
        for &shot in batch.shots {
            let mut bits = Bits::default()
                .state(&ShotContext::last_executor(self.seed, shot))
                .expect("bits always make a state");
            outcomes.extend(
                (bits.run(batch.instructions)?)
                    .into_iter()
                    .map(|(tag, value)| ShotOutcome::new(shot, tag, value)),
            );
        }
        Ok(ImmediateReply::done(outcomes))
    }
}

/// It holds no state to count, so it reports nothing.
impl Close for StatelessBits {
    type Report = Count;

    fn close(&mut self) -> Count {
        self.lifecycle.close();
        Count::default()
    }
}

/// The reason given for a readout of tag 0 at batch `batch` that a read
/// at index 1 of batch 0 has already.
fn tag_zero_read_again_in(batch: usize) -> String {
    format!("batch {batch}: tag 0 already names a readout of instruction 1 of batch 0")
}

/// The index and the reason of `fault`, an instruction fault.
fn at_and_reason(fault: &Rejected) -> (usize, &str) {
    match fault {
        Rejected::Instruction { at, reason } => (*at, reason),
        other => panic!("an instruction fault: {other:?}"),
    }
}

/// A program whose second batch reads its coin under tag 0, which its
/// first batch's read has already: its outcomes are one list, told apart
/// by shot and tag, so it is refused when given to the chain, at the
/// second read, naming the first. A refused job gives no
/// [`Run`](crate::Run), so nothing of it runs.
#[test]
fn a_static_program_that_reads_a_tag_again_in_a_later_batch_is_refused() {
    let program = flip_and_toss_edited(|batches| batches[1][1] = read(1, 0));
    let faults = faults_on(bits_set(), program);
    let [fault] = faults.as_slice() else {
        panic!("one fault: {faults:?}");
    };
    let (at, reason) = at_and_reason(fault);
    assert_eq!(at, 1, "{faults:?}");
    assert!(reason.starts_with(&tag_zero_read_again_in(1)), "{faults:?}");
}

/// A program whose first batch reads bit 0, then bit 1, both under tag 0:
/// it is refused when given to the chain, at the second read, naming the
/// first, rather than by the executor at that batch, once the batches
/// before it have run.
#[test]
fn a_tag_a_static_program_reads_twice_in_one_batch_is_refused() {
    let program = flip_and_toss_edited(|batches| batches[0].push(read(1, 0)));
    let faults = faults_on(bits_set(), program);
    let [fault] = faults.as_slice() else {
        panic!("one fault: {faults:?}");
    };
    let (at, reason) = at_and_reason(fault);
    assert_eq!(at, 2, "{faults:?}");
    assert!(reason.starts_with(&tag_zero_read_again_in(0)), "{faults:?}");
}

/// A program that reads tag 0 again twice in its second batch: each
/// repeat is reported, in order, each naming the first read, so that all
/// are fixed in one go.
#[test]
fn every_repeat_of_a_tag_in_a_static_program_is_reported() {
    let program = flip_and_toss_edited(|batches| {
        batches[1][1] = read(1, 0);
        batches[1].push(read(0, 0));
    });
    let faults = faults_on(bits_set(), program);
    let repeats: Vec<_> = (faults.iter().map(at_and_reason))
        .map(|(at, reason)| (at, reason.starts_with(&tag_zero_read_again_in(1))))
        .collect();
    assert_eq!(repeats, [(1, true), (2, true)], "{faults:?}");
}

/// A program whose second batch reads tag 0 again, then calls outside its
/// set: both are reported in the program's order, the repeat first, as
/// they would be met if run.
#[test]
fn a_static_programs_tag_and_call_faults_are_reported_in_its_order() {
    let program = flip_and_toss_edited(|batches| {
        batches[1][1] = read(1, 0);
        batches[1].push(outside_the_bits_set());
    });
    let faults = faults_on(bits_set(), program);
    let [repeat, outside] = faults.as_slice() else {
        panic!("two faults: {faults:?}");
    };
    let (at, reason) = at_and_reason(repeat);
    assert!(
        at == 1 && reason.starts_with(&tag_zero_read_again_in(1)),
        "{faults:?}"
    );
    assert!(is_a_call_fault_at(outside, 1, 2), "{faults:?}");
}

/// A program whose second batch reads its coin with a call its set lacks,
/// under tag 0, which its first batch's read has already: the call's tags
/// are read all the same, so both faults are reported at that call, its
/// call fault then the repeat, and both are fixed in one go.
#[test]
fn a_call_outside_a_static_programs_set_that_repeats_a_tag_is_reported_for_both() {
    let program = flip_and_toss_edited(|batches| {
        batches[1][1] = InstructionCall::new(Opcode::new(3), [Operand::Block(1), Operand::Tag(0)]);
    });
    let faults = faults_on(bits_set(), program);
    let [outside, repeat] = faults.as_slice() else {
        panic!("two faults: {faults:?}");
    };
    assert!(is_a_call_fault_at(outside, 1, 1), "{faults:?}");
    let (at, reason) = at_and_reason(repeat);
    assert!(
        at == 1 && reason.starts_with(&tag_zero_read_again_in(1)),
        "{faults:?}"
    );
}

/// A program of calls by opcode whose writer forgot to state its set, and
/// whose second batch reads its coin under tag 0, which its first batch's
/// read has already: it is refused when given to the chain for the set it
/// needs, and the repeat is reported with that unmet need, since with no
/// set to check its calls against its tags are still read, so that both
/// are fixed in one go.
#[test]
fn the_tags_of_a_static_program_that_states_no_set_are_still_checked() {
    let mut batches = flip_and_toss(&bits_set());
    batches[1][1] = read(1, 0);
    let program = StaticProgram::new(Needs::default(), batches);
    let chain = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let Err(JobFaults(faults)) = chain.run(program, [2, 5]) else {
        panic!("calls by opcode in no stated set must be refused");
    };
    let [Rejected::Unmet(unmet), repeat] = faults.as_slice() else {
        panic!("an unmet need, then the repeat: {faults:?}");
    };
    assert_eq!(unmet.need, Need::InstructionSet, "{faults:?}");
    let (at, reason) = at_and_reason(repeat);
    assert!(
        at == 1 && reason.starts_with(&tag_zero_read_again_in(1)),
        "{faults:?}"
    );
}

/// [`bits_set`] with `measure both`, which reads two bits, each under a
/// tag of its own.
fn reading_both_set() -> InstructionSet {
    use OperandKind::{Block, Tag};
    let reads_both = Action::new([1, 1], []).observe([Pauli::z(0), Pauli::z(1)]);
    bits_set().with_action("measure both", [Block, Tag, Block, Tag], reads_both)
}

/// A read of bits 0 and 1 under `tags`, in [`reading_both_set`]'s
/// numbering.
fn read_both(tags: [u32; 2]) -> InstructionCall {
    let [first, second] = tags.map(Operand::Tag);
    let operands = [Operand::Block(0), first, Operand::Block(1), second];
    call(&reading_both_set(), "measure both", &operands)
}

/// A program in [`reading_both_set`] whose first batch reads bit 0 under
/// tag 0, and whose second reads both bits under tags 1 and 0: each tag of
/// a call is read, its second too, so the call is refused at its index,
/// naming the first read.
#[test]
fn a_tag_that_a_later_operand_of_a_call_reads_again_is_refused() {
    let set = reading_both_set();
    let mut batches = flip_and_toss(&set);
    batches[1][1] = read_both([1, 0]);
    let needs = Needs::default().with_instruction_set(Some(Arc::new(set.clone())));
    let faults = faults_on(set, StaticProgram::new(needs, batches));
    let [fault] = faults.as_slice() else {
        panic!("one fault: {faults:?}");
    };
    let (at, reason) = at_and_reason(fault);
    assert_eq!(at, 1, "{faults:?}");
    assert!(reason.starts_with(&tag_zero_read_again_in(1)), "{faults:?}");
}

/// A program in [`reading_both_set`] whose first batch reads both bits
/// under tag 0: the call is refused at its index, as naming another
/// readout of itself, as a batch's own check words it
/// ([`check_unique_tags`](crate::check_unique_tags)).
#[test]
fn a_call_that_reads_one_tag_twice_is_refused_as_its_own_repeat() {
    let set = reading_both_set();
    let mut batches = flip_and_toss(&set);
    batches[0][1] = read_both([0, 0]);
    let needs = Needs::default().with_instruction_set(Some(Arc::new(set.clone())));
    let faults = faults_on(set, StaticProgram::new(needs, batches));
    let [fault] = faults.as_slice() else {
        panic!("one fault: {faults:?}");
    };
    let (at, reason) = at_and_reason(fault);
    assert_eq!(at, 1, "{faults:?}");
    assert!(
        reason.starts_with("batch 0: tag 0 already names another readout of this instruction"),
        "{faults:?}"
    );
}

/// [`flip_and_toss_program`], whose tags are unique, one per read: it
/// runs, and each shot's two readouts are told apart by tag, its bit 0
/// read one, as flipped.
#[test]
fn a_static_program_whose_tags_are_unique_runs() {
    let chain = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let outcomes = chain
        .run(flip_and_toss_program(None), [2, 5])
        .expect("tags unique in the program")
        .wait()
        .0
        .expect("the bits neither refuse nor fail");
    let by_key: BTreeMap<(u32, u32), Value> = outcomes
        .iter()
        .map(|outcome| ((outcome.shot, outcome.tag), outcome.value))
        .collect();
    assert_eq!(
        by_key.len(),
        outcomes.len(),
        "a key read twice: {outcomes:?}"
    );
    assert_eq!(
        by_key.keys().copied().collect::<Vec<_>>(),
        [(2, 0), (2, 1), (5, 0), (5, 1)]
    );
    assert_eq!((by_key[&(2, 0)], by_key[&(5, 0)]), (Value::One, Value::One));
}
