// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [Guide: Observers: a profiler and a debugger](../docs/guides.md#observers-a-profiler-and-a-debugger):
//! two adapters that watch the calls of a chain, each written with
//! `adapter_fn` in a function of its own, and composed with `>>` between
//! the crate's driver and a simulator.
//!
//! - A profiler counts the instructions each shot sends below, and sends
//!   each call down as it is (`observers/profiler.rs`;
//!   `the_profiler_sends_each_call_down_as_the_program_sent_it`). The run's
//!   report gives its counts, folded over the shots, beside the report
//!   below.
//! - A debugger logs each call of each shot, with what the call read or
//!   why it failed or was refused below, and passes the readouts up as
//!   they came (`observers/debugger.rs`). It sends each shot's call down
//!   alone, so that it can read what it logs
//!   (`below_the_debugger_each_shots_batch_is_a_call_of_its_own`). The user
//!   keeps a handle on its log and reads it after the run; it reports
//!   nothing, so the run's report is the one below.
//!
//! Neither states an instruction set: each passes on the one below, so the
//! program's instructions reach the simulator as they are. The simulator is
//! the toy (`toy/simulator.rs`), taking its instructions as calls, a state
//! per shot, each shot's coins drawn from its own seed; it records what
//! each shot receives, for the tests that check what reaches it.
//!
//! Read `observers/profiler.rs` and `observers/debugger.rs`, then the
//! tests in their order in the file.

#[path = "observers/debugger.rs"]
mod debugger;
#[path = "observers/profiler.rs"]
mod profiler;
#[allow(dead_code, reason = "this guide runs the toy's instructions as calls")]
#[path = "toy/simulator.rs"]
mod toy;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::task::Waker;

use executor_protocol::{
    Below, CallInput, Capabilities, Close, Describe, Executor, ExecutorLogic, ExecutorReport,
    ImmediateReply, InstructionBatch, InstructionBatchRef, InstructionCall, MakeState, Needs,
    OnePerShot, OnePerShotExecutor, OpcodeTable, Operand, Outcomes, Rejected, Reply, ReplyStatus,
    ShotContext, ShotId, ShotOutcome, ShotSet, StaticProgram, StaticProgramError, Value,
    adapter_fn, check_unique_tags, compose, driver, executor_fn,
};

use debugger::{CallResult, DebugLog, LoggedCall, debugger};
use profiler::{Counts, profiler};
use toy::{Op, QUBITS, ToyState, toy_set};

/// The run's seed: each shot's coins are drawn from it and the shot.
const SEED: u64 = 0x5EED;

/// The shots of each run: not a plain range.
const SHOTS: [ShotId; 4] = [2, 5, 6, 11];

/// What each run here needs to start: the shots it names are distinct, and
/// the toy below meets the program's needs, its qubits and its set.
const THE_JOB_FITS: &str = "the shots are distinct, and the toy meets the program's needs";

/// The number of `call`'s operand `i`, a qubit or a tag.
fn number(call: &InstructionCall, i: usize) -> u32 {
    match call.operands[i] {
        Operand::Block(n) | Operand::Tag(n) => n,
        other => unreachable!("the toy takes a block or a tag, not {other:?}"),
    }
}

/// The toy instruction a call of [`toy_set`] stands for.
type ToOp = fn(&InstructionCall) -> Op;

/// What the simulator received: each shot's calls, in the order they came,
/// each its instructions. A handle: its clones share one record.
#[derive(Clone, Default)]
struct Received(Arc<Mutex<BTreeMap<ShotId, Vec<Vec<InstructionCall>>>>>);

impl Received {
    /// What shot `shot` received, in order: none if it never ran.
    fn of_shot(&self, shot: ShotId) -> Vec<Vec<InstructionCall>> {
        let shots = self.0.lock().expect("no logic panics holding the record");
        shots.get(&shot).cloned().unwrap_or_default()
    }

    /// Records `calls`, received by shot `shot`.
    fn record(&self, shot: ShotId, calls: &[InstructionCall]) {
        let mut shots = self.0.lock().expect("no logic panics holding the record");
        shots.entry(shot).or_default().push(calls.to_vec());
    }
}

/// One shot's state in the simulator: the shot, to record what it
/// received, and the toy's state.
type Shot = (ShotId, ToyState);

/// The toy instruction each call of [`toy_set`] stands for, by opcode.
fn toy_ops() -> OpcodeTable<ToOp> {
    let ops = toy_set().table(|mnemonic| {
        let to_op: ToOp = match mnemonic {
            "coin" => |call| Op::Coin(number(call, 0)),
            "x" => |call| Op::X(number(call, 0)),
            "cx" => |call| Op::Cx(number(call, 0), number(call, 1)),
            "measure" => |call| Op::Measure {
                qubit: number(call, 0),
                tag: number(call, 1),
            },
            "reset" => |call| Op::Reset(number(call, 0)),
            _ => return None,
        };
        Some(to_op)
    });
    ops.expect("the toy runs each instruction of its set")
}

/// Refuses a batch of calls the toy cannot run, before any shot runs it,
/// as the toy's own check does for its instructions: a call on a qubit the
/// toy does not have, or a readout that repeats a tag. Each of the toy's
/// blocks is a qubit, and each tag a readout.
fn check_calls(input: &CallInput<'_, InstructionCall>) -> Result<(), Rejected> {
    let calls = input.instructions();
    let beyond = |operand: &Operand| matches!(operand, Operand::Block(q) if *q >= QUBITS);
    let on_a_missing_qubit = |call: &InstructionCall| call.operands.iter().any(beyond);
    if let Some(at) = calls.iter().position(on_a_missing_qubit) {
        let reason = format!("the toy has {QUBITS} qubits");
        return Err(Rejected::Instruction { at, reason });
    }
    let tags = calls.iter().enumerate().flat_map(|(at, call)| {
        (call.operands.iter()).filter_map(move |operand| match operand {
            Operand::Tag(tag) => Some((at, *tag)),
            _ => None,
        })
    });
    check_unique_tags(tags)
}

/// What the simulator promises: the toy's qubits, a state per shot kept
/// from one call to the next on its own seeds, resets, and its set.
fn the_toys_promise() -> Capabilities {
    Capabilities::default()
        .with_qubits(QUBITS)
        .with_resumable_shots(true)
        .with_replayable(true)
        .with_resets(true)
        .with_instruction_set(Some(Arc::new(toy_set())))
}

/// The simulator below the observers, recording what it receives into
/// `received`, a handle the test keeps a clone of: the toy, taking its
/// instructions as calls of its set, by opcode, checked first
/// ([`check_calls`]), a state per shot, its shots' coins drawn from
/// [`SEED`]. Its check, a function pointer, has the type
/// `NoCheck<InstructionCall>`, the default of that type parameter, so the
/// return type leaves it out.
fn simulator_recording(
    received: Received,
) -> OnePerShotExecutor<
    InstructionCall,
    Shot,
    impl MakeState<Shot>,
    impl ExecutorLogic<InstructionCall, Shot>,
> {
    let ops = toy_ops();
    let start = |context: ShotContext| Ok((context.shot(), ToyState::start(context)?));
    let logic = move |(shot, toy): &mut Shot,
                      input: &CallInput<'_, InstructionCall>,
                      outcomes: &mut Outcomes<'_>| {
        received.record(*shot, input.instructions());
        for call in input.instructions() {
            toy.apply(ops[call.opcode](call), outcomes);
        }
        Ok(())
    };
    let check: fn(&CallInput<'_, InstructionCall>) -> Result<(), Rejected> = check_calls;
    executor_fn(the_toys_promise(), start, logic)
        .check(check)
        .seed(SEED)
        .shots(OnePerShot)
}

/// The simulator below the observers, as [`simulator_recording`], its
/// record dropped.
fn simulator() -> OnePerShotExecutor<
    InstructionCall,
    Shot,
    impl MakeState<Shot>,
    impl ExecutorLogic<InstructionCall, Shot>,
> {
    simulator_recording(Received::default())
}

/// The simulator, its device stopping at each reset: the call fails with
/// what `stop` gives, unless `stop` panics.
fn simulator_stopping_at_reset(
    stop: fn() -> String,
) -> OnePerShotExecutor<
    InstructionCall,
    ToyState,
    impl MakeState<ToyState>,
    impl ExecutorLogic<InstructionCall, ToyState>,
> {
    let ops = toy_ops();
    let reset = toy_set().opcode("reset").expect("the toy's set has reset");
    let logic = move |toy: &mut ToyState,
                      input: &CallInput<'_, InstructionCall>,
                      outcomes: &mut Outcomes<'_>| {
        for call in input.instructions() {
            if call.opcode == reset {
                return Err(stop());
            }
            toy.apply(ops[call.opcode](call), outcomes);
        }
        Ok(())
    };
    executor_fn(the_toys_promise(), ToyState::start, logic)
        .seed(SEED)
        .shots(OnePerShot)
}

/// The simulator, the device of shot `down` unavailable: it refuses a
/// call on that shot, before the shot's state is made, and runs the
/// others.
fn simulator_refusing_shot(
    down: ShotId,
) -> OnePerShotExecutor<
    InstructionCall,
    ToyState,
    impl MakeState<ToyState>,
    impl ExecutorLogic<InstructionCall, ToyState>,
> {
    let ops = toy_ops();
    let start = move |context: ShotContext| {
        if context.shot() == down {
            let reason = format!("the device of shot {down} is down");
            return Err(Rejected::Unavailable(reason));
        }
        ToyState::start(context)
    };
    let logic = move |toy: &mut ToyState,
                      input: &CallInput<'_, InstructionCall>,
                      outcomes: &mut Outcomes<'_>| {
        for call in input.instructions() {
            toy.apply(ops[call.opcode](call), outcomes);
        }
        Ok(())
    };
    executor_fn(the_toys_promise(), start, logic)
        .seed(SEED)
        .shots(OnePerShot)
}

/// The program's first batch: toss a coin on qubit 0, and read it.
fn toss() -> Vec<InstructionCall> {
    let set = toy_set();
    let call = |mnemonic, operands| {
        set.call(mnemonic, operands)
            .expect("the toy's set has each instruction, on operands of these kinds")
    };
    vec![
        call("coin", vec![Operand::Block(0)]),
        call("measure", vec![Operand::Block(0), Operand::Tag(0)]),
    ]
}

/// The program's second batch: copy qubit 0 onto qubit 1, read both, and
/// reset qubit 0.
fn copy() -> Vec<InstructionCall> {
    let set = toy_set();
    let call = |mnemonic, operands| {
        set.call(mnemonic, operands)
            .expect("the toy's set has each instruction, on operands of these kinds")
    };
    vec![
        call("cx", vec![Operand::Block(0), Operand::Block(1)]),
        call("measure", vec![Operand::Block(1), Operand::Tag(1)]),
        call("measure", vec![Operand::Block(0), Operand::Tag(2)]),
        call("reset", vec![Operand::Block(0)]),
    ]
}

/// The batches of the program: [`toss`], then [`copy`].
fn batches() -> Vec<Vec<InstructionCall>> {
    vec![toss(), copy()]
}

/// The program each run runs on every shot: its [`batches`], on two
/// qubits, of the toy's set.
fn program() -> StaticProgram<InstructionCall> {
    program_of(batches())
}

/// A program of `batches`, on two qubits, of the toy's set.
fn program_of(batches: Vec<Vec<InstructionCall>>) -> StaticProgram<InstructionCall> {
    let needs = Needs::default()
        .with_qubits(2)
        .with_instruction_set(Some(Arc::new(toy_set())));
    StaticProgram::new(needs, batches)
}

/// `outcomes`, by shot, each shot's in the order it read them, so that
/// runs compare whatever order their replies interleaved the shots in.
fn by_shot(outcomes: Result<Vec<ShotOutcome>, impl std::fmt::Debug>) -> Vec<ShotOutcome> {
    let mut outcomes = outcomes.expect("the run succeeds");
    outcomes.sort_by_key(|outcome| outcome.shot);
    outcomes
}

/// The run of the program on [`SHOTS`] with no observer: its outcomes, and
/// the simulator's report. A precondition: each shot reads its coin at
/// tag 0, and again at tags 1 and 2, since the copy copies it, so that the
/// observed runs are compared with the program run as the toy means it.
fn run_alone() -> (Vec<ShotOutcome>, ExecutorReport<()>) {
    let chain = (driver() >> simulator()).expect("the program's calls fit the toy");
    let (outcomes, report) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    let outcomes = by_shot(outcomes);
    for shot in SHOTS {
        let read: Vec<_> = outcomes.iter().filter(|o| o.shot == shot).collect();
        let [coin, copy, again] = read[..] else {
            panic!("shot {shot} reads tags 0, 1 and 2 once each: {read:?}");
        };
        assert_eq!(
            [coin.tag, copy.tag, again.tag],
            [0, 1, 2],
            "shot {shot} reads tags 0, 1 and 2"
        );
        assert!(
            copy.value == coin.value && again.value == coin.value,
            "shot {shot} reads its coin at each tag: {read:?}"
        );
    }
    (outcomes, report)
}

/// What the shots send below, counted: one toss, one copy and one reset,
/// and three readouts, on each of the four shots.
fn the_runs_counts() -> Counts {
    let set = toy_set();
    let opcode = |mnemonic| {
        set.opcode(mnemonic)
            .expect("the toy's set has each one counted")
    };
    Counts::from([
        (opcode("coin"), 4),
        (opcode("cx"), 4),
        (opcode("measure"), 12),
        (opcode("reset"), 4),
    ])
}

/// A user profiles a program on a simulator: `run.wait()` gives, beside
/// the simulator's report, how many calls of each instruction the run's
/// shots sent below, folded over the shots.
#[test]
fn the_profilers_report_counts_each_instruction_over_the_shots() {
    let chain = (driver() >> profiler() >> simulator()).expect("the profiler fits");
    let (_, (counts, _)) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    assert_eq!(counts, the_runs_counts());
}

/// A program profiled reads what it reads without the profiler: the
/// profiler passes each readout up as it came.
#[test]
fn the_profiler_changes_no_outcome() {
    let chain = (driver() >> profiler() >> simulator()).expect("the profiler fits");
    let (outcomes, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    assert_eq!(by_shot(outcomes), run_alone().0);
}

/// The profiler sends each call on without waiting for it (`Below::send`),
/// so the shots' equal calls still go down as one: the simulator below
/// reports what it reports alone, a call per batch.
#[test]
fn below_the_profiler_the_shots_equal_calls_go_as_one() {
    let chain = (driver() >> profiler() >> simulator()).expect("the profiler fits");
    let (_, (_, report)) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    let alone = run_alone().1;
    assert_eq!(
        alone.calls,
        batches().len() as u64,
        "alone, a call per batch"
    );
    assert_eq!(report, alone);
}

/// A user profiles a program to learn what it runs: the simulator below
/// the profiler receives, on each shot, each call as the program sent it,
/// none dropped, changed or added.
#[test]
fn the_profiler_sends_each_call_down_as_the_program_sent_it() {
    let received = Received::default();
    let chain = driver() >> profiler() >> simulator_recording(received.clone());
    let chain = chain.expect("the profiler fits");
    let (outcomes, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    outcomes.expect("the run succeeds");
    for shot in SHOTS {
        assert_eq!(received.of_shot(shot), batches(), "shot {shot}");
    }
}

/// The calls the debugger logs for shot `shot` of a run that read
/// `outcomes` ([`by_shot`]): each batch of the program, in order, with
/// what the shot read in it.
fn the_shots_log(shot: ShotId, outcomes: &[ShotOutcome]) -> Vec<LoggedCall> {
    let read = |tags: &[u32]| {
        let read = outcomes
            .iter()
            .filter(|o| o.shot == shot && tags.contains(&o.tag));
        Some(Ok(read.copied().collect()))
    };
    let logged = |instructions, result| LoggedCall {
        instructions,
        result,
    };
    vec![logged(toss(), read(&[0])), logged(copy(), read(&[1, 2]))]
}

/// A user debugs a program whose shots toss different coins: after the
/// run, the log of each shot holds each of its calls, in order, with what
/// the shot read in it.
#[test]
fn the_debugger_logs_each_shots_calls_with_what_they_read() {
    let (outcomes, _) = run_alone();
    let coins = |value| outcomes.iter().any(|o| o.value == value);
    assert!(
        coins(Value::Zero) && coins(Value::One),
        "the shots toss different coins"
    );
    let log = DebugLog::default();
    let chain = (driver() >> debugger(log.clone()) >> simulator()).expect("the debugger fits");
    let _ = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    for shot in SHOTS {
        assert_eq!(
            log.of_shot(shot),
            the_shots_log(shot, &outcomes),
            "shot {shot}"
        );
    }
}

/// A user debugs a shot whose device fails at its reset, in the
/// program's second call of three: the shot's log holds each call up to
/// the one that failed, that one included, with why it failed, and ends
/// there.
#[test]
fn the_debugger_logs_a_call_that_fails_below_with_its_reason() {
    let (alone, _) = run_alone();
    let set = toy_set();
    let read_again = set.call("measure", vec![Operand::Block(1), Operand::Tag(3)]);
    let read_again = read_again.expect("the toy's set reads a qubit under a tag");
    let mut calls = batches();
    calls.push(vec![read_again]);
    let failing = simulator_stopping_at_reset(|| "the device failed at its reset".to_string());
    let log = DebugLog::default();
    let chain = (driver() >> debugger(log.clone()) >> failing).expect("the debugger fits");
    let program = program_of(calls);
    let (outcomes, _) = chain.run(program, [6]).expect(THE_JOB_FITS).wait();
    outcomes.expect_err("the device fails the run");
    let logged = log.of_shot(6);
    let [
        tossed,
        LoggedCall {
            instructions: copied,
            result: failed,
        },
    ] = &logged[..]
    else {
        panic!("the log holds the toss and the copy, and ends there: {logged:?}");
    };
    assert_eq!(tossed, &the_shots_log(6, &alone)[0], "the toss is logged");
    assert_eq!(copied, &copy(), "the failing call is logged");
    let failed = failed.as_ref().expect("the copy returned");
    let why = failed.as_ref().expect_err("the copy failed");
    assert!(
        why.message().contains("the device failed at its reset"),
        "{why}"
    );
}

/// A user debugs a shot whose device panics at its reset: the panic
/// reaches the user, and the shot's log holds the toss, then the call that
/// panicked, with no outcome.
#[test]
fn the_debugger_logs_a_call_that_panics_below_without_an_outcome() {
    let (alone, _) = run_alone();
    let crashing = simulator_stopping_at_reset(|| panic!("the device crashed at its reset"));
    let log = DebugLog::default();
    let chain = (driver() >> debugger(log.clone()) >> crashing).expect("the debugger fits");
    let run = || chain.run(program(), [6]).expect(THE_JOB_FITS).wait();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).is_err();
    assert!(panicked, "the device's panic reaches the user");
    let tossed = the_shots_log(6, &alone)[0].clone();
    let panicked = LoggedCall {
        instructions: copy(),
        result: None,
    };
    assert_eq!(log.of_shot(6), [tossed, panicked]);
}

/// A user logs each shot's calls above and below an adapter in one place:
/// a debugger on each side of an adapter that splits each call in two, both
/// sharing one log. Each lower call returns while the upper one waits, and
/// each call the log holds has its own result, what the shot read in it.
#[test]
fn each_logged_call_holds_its_own_result_when_debuggers_share_a_log() {
    let (alone, _) = run_alone();
    let splitting = adapter_fn(
        |_| Ok(()),
        |(): &mut (),
         input: &CallInput<'_, InstructionCall>,
         below: &mut Below<'_, InstructionCall>,
         _: &mut Outcomes<'_>| {
            let (first, rest) = input.instructions().split_at(1);
            below.send(first)?;
            below.send(rest)
        },
    );
    let log = DebugLog::default();
    let chain =
        driver() >> debugger(log.clone()) >> splitting >> debugger(log.clone()) >> simulator();
    let chain = chain.expect("the debuggers and the splitting adapter fit");
    let (outcomes, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    outcomes.expect("the run succeeds");
    for shot in SHOTS {
        let logged = log.of_shot(shot);
        let above_and_in_two_below = 3 * batches().len();
        assert_eq!(
            logged.len(),
            above_and_in_two_below,
            "shot {shot}: each batch is logged above the splitting adapter, and in two parts below"
        );
        for LoggedCall {
            instructions,
            result,
        } in &logged
        {
            let tags: Vec<u32> = instructions
                .iter()
                .flat_map(|call| &call.operands)
                .filter_map(|operand| match operand {
                    Operand::Tag(tag) => Some(*tag),
                    _ => None,
                })
                .collect();
            let read = alone
                .iter()
                .filter(|o| o.shot == shot && tags.contains(&o.tag));
            let its_own: CallResult = Ok(read.copied().collect());
            assert_eq!(
                result.as_ref(),
                Some(&its_own),
                "shot {shot}: {instructions:?}"
            );
        }
    }
}

/// A program debugged over the simulator, which offers `replayable`, reads
/// what it reads without the debugger: the debugger passes each readout up
/// as it came, and a call per shot below changes no shot's coins.
#[test]
fn the_debugger_changes_no_outcome() {
    let replayable = simulator().capabilities().replayable;
    assert!(replayable, "the simulator offers replayable shots");
    let log = DebugLog::default();
    let chain = (driver() >> debugger(log.clone()) >> simulator()).expect("the debugger fits");
    let (outcomes, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    assert_eq!(by_shot(outcomes), run_alone().0);
}

/// A user debugs a program to see what it runs: the simulator below the
/// debugger receives, on each shot, each call as the program sent it, none
/// dropped, changed or added.
#[test]
fn the_debugger_runs_each_call_below_as_the_program_sent_it() {
    let received = Received::default();
    let log = DebugLog::default();
    let chain = driver() >> debugger(log.clone()) >> simulator_recording(received.clone());
    let chain = chain.expect("the debugger fits");
    let (outcomes, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    outcomes.expect("the run succeeds");
    for shot in SHOTS {
        assert_eq!(received.of_shot(shot), batches(), "shot {shot}");
    }
}

/// The debugger waits for each shot's readouts (`Below::run`), so each
/// shot's batch goes down in a call of its own: the run's report, the
/// simulator's, with no part for the debugger, counts a call per shot and
/// batch, where the run alone made one per batch.
#[test]
fn below_the_debugger_each_shots_batch_is_a_call_of_its_own() {
    let log = DebugLog::default();
    let chain = (driver() >> debugger(log.clone()) >> simulator()).expect("the debugger fits");
    let (_, report): (_, ExecutorReport<()>) =
        chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    let batches = batches().len() as u64;
    assert_eq!(run_alone().1.calls, batches, "alone, a call per batch");
    assert_eq!(report.calls, SHOTS.len() as u64 * batches);
}

/// A batch that reads its tags out of order: toss a coin on qubit 0, then
/// read tag 5, tag 3 and tag 4.
fn read_out_of_order() -> Vec<InstructionCall> {
    let set = toy_set();
    let call = |mnemonic, operands| {
        set.call(mnemonic, operands)
            .expect("the toy's set has each instruction, on operands of these kinds")
    };
    let read = |qubit, tag| call("measure", vec![Operand::Block(qubit), Operand::Tag(tag)]);
    vec![
        call("coin", vec![Operand::Block(0)]),
        read(0, 5),
        read(1, 3),
        read(0, 4),
    ]
}

/// A program whose batch reads its tags out of order, 5, then 3, then 4,
/// over the simulator, whose reply keeps each shot's read order: each
/// shot's outcomes come up in the order it read them, alone and under each
/// observer.
#[test]
fn the_observers_pass_each_shots_readouts_up_in_the_order_read() {
    let program = || program_of(vec![read_out_of_order()]);
    let chain = (driver() >> simulator()).expect("the program fits");
    let (alone, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    let alone = by_shot(alone);
    let tags: Vec<_> = alone.iter().map(|o| (o.shot, o.tag)).collect();
    let in_read_order = SHOTS
        .iter()
        .flat_map(|&shot| [(shot, 5), (shot, 3), (shot, 4)]);
    assert!(tags.into_iter().eq(in_read_order), "alone, in read order");
    let chain = (driver() >> profiler() >> simulator()).expect("the profiler fits");
    let (profiled, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    assert_eq!(by_shot(profiled), alone, "under the profiler");
    let log = DebugLog::default();
    let chain = (driver() >> debugger(log.clone()) >> simulator()).expect("the debugger fits");
    let (debugged, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    assert_eq!(by_shot(debugged), alone, "under the debugger");
}

/// A user debugs a batch that reads its tags out of order, 5, then 3,
/// then 4: the shot's log holds what the batch read in the order it read
/// it, as the run alone gives it.
#[test]
fn the_debugger_logs_each_calls_readouts_in_the_order_read() {
    let program = || program_of(vec![read_out_of_order()]);
    let chain = (driver() >> simulator()).expect("the program fits");
    let (alone, _) = chain.run(program(), [6]).expect(THE_JOB_FITS).wait();
    let alone = alone.expect("the run succeeds");
    let tags: Vec<_> = alone.iter().map(|o| o.tag).collect();
    assert_eq!(tags, [5, 3, 4], "alone, in read order");
    let log = DebugLog::default();
    let chain = (driver() >> debugger(log.clone()) >> simulator()).expect("the debugger fits");
    let (debugged, _) = chain.run(program(), [6]).expect(THE_JOB_FITS).wait();
    debugged.expect("the run succeeds");
    let read_in_order = LoggedCall {
        instructions: read_out_of_order(),
        result: Some(Ok(alone)),
    };
    assert_eq!(log.of_shot(6), [read_in_order]);
}

/// The batch the simulator's check refuses whole, one on a qubit the toy
/// does not have, is refused alike alone and under each observer: an
/// observer turns no such refusal into a failure.
#[test]
fn a_batch_the_simulators_check_refuses_is_refused_alike_under_each_observer() {
    let beyond = toy_set().call("measure", vec![Operand::Block(QUBITS), Operand::Tag(1)]);
    let beyond = beyond.expect("the toy's set reads a qubit under a tag");
    let program = || program_of(vec![toss(), vec![beyond.clone()]]);
    let chain = (driver() >> simulator()).expect("the program fits");
    let (alone, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    let alone = alone.expect_err("the simulator refuses the batch");
    let chain = (driver() >> profiler() >> simulator()).expect("the profiler fits");
    let (profiled, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    assert_eq!(
        profiled.expect_err("the simulator refuses the batch under the profiler"),
        alone,
        "under the profiler"
    );
    let log = DebugLog::default();
    let chain = (driver() >> debugger(log.clone()) >> simulator()).expect("the debugger fits");
    let (debugged, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    assert_eq!(
        debugged.expect_err("the simulator refuses the batch under the debugger"),
        alone,
        "under the debugger"
    );
}

/// A program whose second batch reads tag 1 twice is refused when it is
/// given to the chain, alike alone and under each observer: the static
/// program's own check refuses it before any batch is sent, so the
/// simulator's check never sees it, and no observer changes the refusal.
#[test]
fn a_program_that_reads_a_tag_twice_is_refused_alike_under_each_observer() {
    let set = toy_set();
    let read = |qubit| set.call("measure", vec![Operand::Block(qubit), Operand::Tag(1)]);
    let read = |qubit| read(qubit).expect("the toy's set reads a qubit under a tag");
    let program = || program_of(vec![toss(), vec![read(0), read(1)]]);
    let chain = (driver() >> simulator()).expect("the program fits");
    let Err(alone) = chain.run(program(), SHOTS) else {
        panic!("a static program refuses a tag read twice");
    };
    let chain = (driver() >> profiler() >> simulator()).expect("the profiler fits");
    let Err(profiled) = chain.run(program(), SHOTS) else {
        panic!("a static program refuses a tag read twice under the profiler");
    };
    assert_eq!(profiled, alone, "under the profiler");
    let log = DebugLog::default();
    let chain = (driver() >> debugger(log.clone()) >> simulator()).expect("the debugger fits");
    let Err(debugged) = chain.run(program(), SHOTS) else {
        panic!("a static program refuses a tag read twice under the debugger");
    };
    assert_eq!(debugged, alone, "under the debugger");
    assert!(
        log.of_shot(SHOTS[0]).is_empty(),
        "no call reached the debugger"
    );
}

/// A user debugs a program on shots 2, 5, 6 and 11 whose device is down
/// for one shot, which the simulator refuses. Below the debugger the shots
/// run one by one, in the call's order: a refusal of the first shot
/// refuses the run, as alone, but one of shot 5 fails it, since shot 2
/// already ran.
#[test]
fn under_the_debugger_a_shot_refused_after_another_ran_fails_the_run() {
    let program = || program_of(vec![toss()]);
    for down in [SHOTS[0], SHOTS[1]] {
        let chain = (driver() >> simulator_refusing_shot(down)).expect("the program fits");
        let (alone, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
        let alone = alone.expect_err("the simulator refuses the shot");
        assert!(
            matches!(alone, StaticProgramError::Refused { batch: 0, .. }),
            "alone, shot {down} refuses the run: {alone:?}"
        );
        let log = DebugLog::default();
        let chain = driver() >> debugger(log.clone()) >> simulator_refusing_shot(down);
        let chain = chain.expect("the debugger fits");
        let (debugged, _) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
        let debugged = debugged.expect_err("the simulator refuses the shot");
        if down == SHOTS[0] {
            assert_eq!(debugged, alone, "the first shot refused");
        } else {
            let StaticProgramError::Failed { batch: 0, reason } = debugged else {
                panic!("shot {down}'s refusal fails the run: {debugged:?}");
            };
            let down = format!("the device of shot {down} is down");
            assert!(reason.contains(&down), "{reason}");
        }
    }
}

/// A user debugs a program whose second batch, on a qubit the toy does not
/// have, the simulator's check refuses, on shots 2, 5, 6 and 11, and again
/// on the same shots in another order, 11 first: the run is refused, and
/// the refused call is logged, with its reason, in the call's first shot
/// alone, the first the call names, since below the debugger the refusal
/// ends the call before the other shots send it.
#[test]
fn the_debugger_logs_a_refused_call_in_the_calls_first_shot_alone() {
    let (alone, _) = run_alone();
    let set = toy_set();
    let refused = set.call("measure", vec![Operand::Block(QUBITS), Operand::Tag(1)]);
    let refused = vec![refused.expect("the toy's set reads a qubit under a tag")];
    let tossed = |shot| the_shots_log(shot, &alone)[0].clone();
    for shots in [SHOTS, [11, 2, 6, 5]] {
        let log = DebugLog::default();
        let chain = (driver() >> debugger(log.clone()) >> simulator()).expect("the debugger fits");
        let program = program_of(vec![toss(), refused.clone()]);
        let (outcomes, _) = chain.run(program, shots).expect(THE_JOB_FITS).wait();
        let outcomes = outcomes.expect_err("the simulator refuses the batch");
        assert!(
            matches!(outcomes, StaticProgramError::Refused { batch: 1, .. }),
            "the run is refused: {outcomes:?}"
        );
        let [first, others @ ..] = shots;
        let logged = log.of_shot(first);
        let [
            toss_logged,
            LoggedCall {
                instructions: call,
                result: Some(Err(why)),
            },
        ] = &logged[..]
        else {
            panic!("shot {first} logs the toss, then the refusal: {logged:?}");
        };
        assert_eq!(toss_logged, &tossed(first), "the toss is logged");
        assert_eq!(call, &refused, "the refused call is logged");
        assert!(
            why.message()
                .contains(&format!("the toy has {QUBITS} qubits")),
            "{why}"
        );
        for shot in others {
            assert_eq!(
                log.of_shot(shot),
                [tossed(shot)],
                "shot {shot} of {shots:?}"
            );
        }
    }
}

/// A device below that cancels each call before it runs, as one whose
/// time is taken back does: each reply ends cancelled, reading nothing,
/// and the device serves the next call.
struct CancellingEachCall;

/// The reply of [`CancellingEachCall`]: cancelled.
struct CancelledReply;

impl Reply for CancelledReply {
    type Item = ShotOutcome;

    fn poll_events(&mut self, _: &mut Vec<ShotOutcome>, _: &Waker) -> ReplyStatus {
        ReplyStatus::Cancelled
    }
}

impl Executor<InstructionBatch<InstructionCall>> for CancellingEachCall {
    type Reply = CancelledReply;

    fn call(
        &mut self,
        _: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<CancelledReply, Rejected> {
        Ok(CancelledReply)
    }
}

impl Describe for CancellingEachCall {
    fn capabilities(&self) -> Capabilities {
        the_toys_promise()
    }
}

impl Close for CancellingEachCall {
    type Report = ();

    fn close(&mut self) {}
}

/// How a call of the program's [`toss`] on shot 2 ends at `executor`: its
/// refusal, or the status its reply ends with, at the first poll, since
/// nothing below it is pending.
fn end_of_a_toss(
    executor: &mut impl Executor<InstructionBatch<InstructionCall>>,
) -> Result<ReplyStatus, Rejected> {
    let calls = toss();
    let batch = InstructionBatchRef {
        shots: &ShotSet::one(2),
        instructions: &calls,
    };
    let mut reply = executor.call(batch)?;
    Ok(reply.poll_events(&mut Vec::new(), Waker::noop()))
}

/// A user debugs a batch on a device that cancels it before it runs. Alone,
/// the call ends cancelled, and the device serves the next call. Under the
/// debugger, which waits for the call's readouts and cannot read a reply
/// that was cancelled, the call ends failed, and the debugger, failed,
/// refuses the next call.
#[test]
fn under_the_debugger_a_call_cancelled_below_fails_the_call_and_the_debugger() {
    let mut alone = CancellingEachCall;
    let first = end_of_a_toss(&mut alone).expect("the device takes the call");
    assert_eq!(
        first,
        ReplyStatus::Cancelled,
        "alone, the call is cancelled"
    );
    let next = end_of_a_toss(&mut alone).expect("alone, the device serves on");
    assert_eq!(next, ReplyStatus::Cancelled, "alone, the next call too");
    let log = DebugLog::default();
    let mut debugged = compose(debugger(log), CancellingEachCall).expect("the debugger fits");
    let first = end_of_a_toss(&mut debugged).expect("the device takes the call");
    assert!(
        matches!(first, ReplyStatus::Failed(_)),
        "under the debugger, the call fails: {first:?}"
    );
    let next = end_of_a_toss(&mut debugged).expect_err("the debugger failed");
    assert!(
        matches!(next, Rejected::EarlierFailure(_)),
        "the next call is refused: {next:?}"
    );
}

/// A device below whose reply is not whole: it reads what it was not
/// asked to read, as a faulty driver may.
#[derive(Clone, Copy)]
enum NotWhole {
    /// Reads tag 0 of each shot it is given twice, 1 then 0.
    ReadingTagZeroTwice,
    /// Reads tag 0 of shot 99, which no call gives it, and nothing else.
    ReadingShot99,
}

impl Executor<InstructionBatch<InstructionCall>> for NotWhole {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<ImmediateReply<ShotOutcome>, Rejected> {
        let read = |shot, value| ShotOutcome::new(shot, 0, value);
        let mut reply = ImmediateReply::default();
        match self {
            NotWhole::ReadingTagZeroTwice => {
                for &shot in batch.shots.as_slice() {
                    reply.push(read(shot, Value::One));
                    reply.push(read(shot, Value::Zero));
                }
            }
            NotWhole::ReadingShot99 => reply.push(read(99, Value::One)),
        }
        Ok(reply)
    }
}

impl Describe for NotWhole {
    fn capabilities(&self) -> Capabilities {
        the_toys_promise()
    }
}

impl Close for NotWhole {
    type Report = ();

    fn close(&mut self) {}
}

/// What a run of a program gave: its outcomes, or why it stopped.
type RunResult = Result<Vec<ShotOutcome>, StaticProgramError>;

/// A run of [`toss`] on [`SHOTS`] over `device`, alone, then under a
/// debugger: what each run gave, and the debugger's log.
fn alone_and_debugged(device: NotWhole) -> (RunResult, RunResult, DebugLog) {
    let program = || program_of(vec![toss()]);
    let chain = compose(driver(), device).expect("the program fits");
    let (alone, ()) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    let log = DebugLog::default();
    let chain = compose(driver() >> debugger(log.clone()), device);
    let chain = chain.expect("the debugger fits");
    let (debugged, ()) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    (alone, debugged, log)
}

/// A user debugs a toss on a device whose driver reads each shot's coin
/// twice. Alone, its readouts pass up as they came, two for each shot.
/// Under the debugger, which waits for each shot's readouts and checks
/// that they are whole, the first shot's call fails the run, and the
/// shot's log holds why.
#[test]
fn under_the_debugger_a_reply_that_reads_a_tag_twice_fails_the_run() {
    let (alone, debugged, log) = alone_and_debugged(NotWhole::ReadingTagZeroTwice);
    let alone = alone.expect("alone, the driver passes the readouts up");
    assert_eq!(alone.len(), 2 * SHOTS.len(), "alone, two readouts a shot");
    let debugged = debugged.expect_err("the reply below reads a tag twice");
    let StaticProgramError::Failed { batch: 0, reason } = debugged else {
        panic!("the reply fails the run: {debugged:?}");
    };
    assert!(reason.contains("tag 0 of shot 2 twice"), "{reason}");
    let logged = log.of_shot(2);
    let [
        LoggedCall {
            result: Some(Err(why)),
            ..
        },
    ] = &logged[..]
    else {
        panic!("shot 2 logs its toss, failed: {logged:?}");
    };
    assert!(why.message().contains("tag 0 of shot 2 twice"), "{why}");
}

/// A user debugs a toss on a device whose driver reads a shot no call
/// gave it. Alone, that readout passes up as it came. Under the debugger,
/// which waits for each shot's readouts and checks that they are whole,
/// the first shot's call fails the run, and the shot's log holds why.
#[test]
fn under_the_debugger_a_reply_that_reads_a_shot_not_given_fails_the_run() {
    let (alone, debugged, log) = alone_and_debugged(NotWhole::ReadingShot99);
    let alone = alone.expect("alone, the driver passes the readouts up");
    assert_eq!(
        alone,
        [ShotOutcome::new(99, 0, Value::One)],
        "alone, the stray readout passes up"
    );
    let debugged = debugged.expect_err("the reply below reads a shot not given");
    let StaticProgramError::Failed { batch: 0, reason } = debugged else {
        panic!("the reply fails the run: {debugged:?}");
    };
    assert!(
        reason.contains("shot 99, which it was not given"),
        "{reason}"
    );
    let logged = log.of_shot(2);
    let [
        LoggedCall {
            result: Some(Err(why)),
            ..
        },
    ] = &logged[..]
    else {
        panic!("shot 2 logs its toss, failed: {logged:?}");
    };
    assert!(
        why.message().contains("shot 99, which it was not given"),
        "{why}"
    );
}

/// The profiler over the debugger, as a target chain stacks them: each
/// observer sees what it sees alone, the profiler its counts, the debugger
/// each shot's log.
#[test]
fn stacked_observers_each_see_what_they_see_alone() {
    let (alone, _) = run_alone();
    let log = DebugLog::default();
    let chain = driver() >> profiler() >> debugger(log.clone()) >> simulator();
    let chain = chain.expect("the observers fit");
    let (outcomes, (counts, _)) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    outcomes.expect("the stacked run succeeds");
    assert_eq!(counts, the_runs_counts());
    for shot in SHOTS {
        assert_eq!(
            log.of_shot(shot),
            the_shots_log(shot, &alone),
            "shot {shot}"
        );
    }
}

/// The profiler and the debugger between the driver and the
/// simulator, grouped each of the five ways `>>` allows four components:
/// the same outcomes, the same report, and the same log.
#[test]
fn the_observers_grouped_any_way_give_the_same_run_and_log() {
    let logs = [(); 5].map(|()| DebugLog::default());
    let fit = "the observers fit";
    let chain = ((driver() >> profiler()) >> debugger(logs[0].clone())) >> simulator();
    let first = chain
        .expect(fit)
        .run(program(), SHOTS)
        .expect(THE_JOB_FITS)
        .wait();
    let chain = (driver() >> (profiler() >> debugger(logs[1].clone()))) >> simulator();
    let second = chain
        .expect(fit)
        .run(program(), SHOTS)
        .expect(THE_JOB_FITS)
        .wait();
    let below = (debugger(logs[2].clone()) >> simulator()).expect("the debugger fits");
    let chain = (driver() >> profiler()) >> below;
    let third = chain
        .expect(fit)
        .run(program(), SHOTS)
        .expect(THE_JOB_FITS)
        .wait();
    let below = ((profiler() >> debugger(logs[3].clone())) >> simulator()).expect(fit);
    let fourth = (driver() >> below).expect("the program fits");
    let fourth = fourth.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    let below = (debugger(logs[4].clone()) >> simulator()).expect("the debugger fits");
    let below = (profiler() >> below).expect("the profiler fits");
    let fifth = (driver() >> below).expect("the program fits");
    let fifth = fifth.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    let runs = [first, second, third, fourth, fifth];
    let [first, rest @ ..] = runs.map(|(outcomes, report)| (by_shot(outcomes), report));
    let groupings = [
        "(i >> (p >> d)) >> s",
        "(i >> p) >> (d >> s)",
        "i >> ((p >> d) >> s)",
        "i >> (p >> (d >> s))",
    ];
    for (run, grouping) in rest.iter().zip(groupings) {
        assert_eq!(run, &first, "{grouping}");
    }
    for shot in SHOTS {
        let [first, rest @ ..] = logs.each_ref().map(|log| log.of_shot(shot));
        assert!(!first.is_empty(), "the debugger logs shot {shot}");
        for (logged, grouping) in rest.iter().zip(groupings) {
            assert_eq!(logged, &first, "{grouping}, shot {shot}");
        }
    }
}

/// A user hands a chain of observers to another thread to run: the
/// debugger's log, kept by the user on this thread, holds what the shots
/// ran there.
#[test]
fn a_chain_of_observers_runs_on_another_thread() {
    let (alone, _) = run_alone();
    let log = DebugLog::default();
    let chain = driver() >> profiler() >> debugger(log.clone()) >> simulator();
    let chain = chain.expect("the observers fit");
    let run = std::thread::spawn(move || chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait());
    let (outcomes, _) = run.join().expect("the run's thread ends");
    outcomes.expect("the run succeeds");
    for shot in SHOTS {
        assert_eq!(
            log.of_shot(shot),
            the_shots_log(shot, &alone),
            "shot {shot}"
        );
    }
}
