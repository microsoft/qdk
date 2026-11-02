// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [Guide: A debugger: holding and completing
//! work](../docs/guides.md#a-debugger-holding-and-completing-work):
//! a debugger, an adapter whose executors keep the instruction set (a
//! probe), between a driver and the executor it drives.
//!
//! The debugger records every batch that goes through it and every outcome
//! and end that come back (`debugger/log.rs`), and it can pause: while
//! paused, it holds each batch, and the caller's reply stays pending until
//! the batch is stepped through from its handle (`debugger/debugger.rs`).
//! It changes nothing else: a run with the debugger gives the outcomes,
//! drawn from the same seeds, that the run without it gives. As an adapter
//! (`debugger/debuggers.rs`), it puts a debugger on the executor of a
//! chain, and hands out its handle.
//!
//! Read `debugger/log.rs`, `debugger/debugger.rs` and
//! `debugger/debuggers.rs`, then the tests in their order in the file.

#[path = "debugger/debugger.rs"]
mod debugger;
#[path = "debugger/debuggers.rs"]
mod debuggers;
#[path = "debugger/log.rs"]
mod log;
#[path = "toy/simulator.rs"]
mod toy;

use std::task::Waker;
use std::thread;
use std::time::{Duration, Instant};

use executor_protocol::{
    AdaptiveProgram, Close, Describe, Executor, InstructionBatch, InstructionBatchRef, Rejected,
    Reply, ReplyStatus, ShotOutcome, ShotSet, Value, conformance, driver, wait,
};

use debugger::debugger;
use debuggers::Debuggers;
use log::Entry;
use toy::{Op, coin_then_feedback, toy_simulator};

/// Between a driver and an executor, the debugger leaves every outcome
/// as it was, for every seed and shot, so each shot drew from its own
/// seeds as without it.
#[test]
fn the_debugger_changes_no_outcome_and_no_seed() {
    let mut ones = 0;
    for seed in 0..32 {
        for j in [0, 5] {
            let without = coin_then_feedback(&mut toy_simulator(seed), j);
            let (mut with, _) = debugger(toy_simulator(seed));
            assert_eq!(
                coin_then_feedback(&mut with, j),
                without,
                "seed {seed}, shot {j}"
            );
            ones += usize::from(without[0].value == Value::One);
        }
    }
    assert!(0 < ones && ones < 64, "the coins fell both ways: {ones}");
}

/// The debugger offers what the executor it wraps offers: it draws nothing
/// and changes no instruction, so a chain checks a caller's needs as if it
/// were not there.
#[test]
fn the_debugger_offers_what_it_wraps() {
    let (with, _) = debugger(toy_simulator(0));
    assert_eq!(with.capabilities(), toy_simulator(0).capabilities());
}

/// The log is the run as the executor saw it: each batch, then its outcomes
/// and its end, in order. The second batch depends on the first's outcome,
/// and the log shows which one ran.
#[test]
fn the_log_holds_each_batch_then_its_outcomes() {
    let (mut with, handle) = debugger(toy_simulator(3));
    let outcomes = coin_then_feedback(&mut with, 0);

    let log = handle.log();
    let Entry::Batch(second) = &log[3] else {
        panic!("{log:?}");
    };
    // The program flips qubit 1 only after a coin that shows 1.
    assert_eq!(second[0] == Op::X(1), outcomes[0].value == Value::One);
    assert_eq!(
        log,
        vec![
            Entry::Batch(vec![Op::Coin(0), Op::Measure { qubit: 0, tag: 0 }]),
            Entry::Outcome(outcomes[0]),
            Entry::End(ReplyStatus::Done),
            Entry::Batch(second.clone()),
            Entry::Outcome(outcomes[1]),
            Entry::Outcome(outcomes[2]),
            Entry::End(ReplyStatus::Done),
        ]
    );
}

/// Paused, the debugger accepts a batch and holds it: the reply is pending
/// and nothing reached the executor. One step sends it on and completes the
/// reply; with nothing held, a step does nothing.
#[test]
fn a_paused_debugger_holds_each_batch_until_it_is_stepped() {
    let (mut with, handle) = debugger(toy_simulator(0));
    handle.pause();
    let mut reply = with
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::X(0), Op::Measure { qubit: 0, tag: 0 }],
        })
        .expect("paused, the debugger holds the batch, so it accepts it");
    let mut outcomes = Vec::new();
    assert_eq!(
        reply.poll_events(&mut outcomes, Waker::noop()),
        ReplyStatus::Pending
    );
    assert_eq!((handle.held(), handle.log().len()), (1, 0));

    assert!(handle.step());
    assert_eq!(
        reply.poll_events(&mut outcomes, Waker::noop()),
        ReplyStatus::Done
    );
    let one = ShotOutcome::new(0, 0, Value::One);
    assert_eq!(outcomes, vec![one]);
    assert!(!handle.step());
}

/// A driver on its own thread runs the program through a paused debugger,
/// and waits, parked, on each reply; this thread steps each batch when it
/// arrives, and each step's sender wakes the driver. The outcomes are
/// those of the run without the debugger.
#[test]
fn a_driver_thread_waits_while_another_thread_steps() {
    let (mut with, handle) = debugger(toy_simulator(9));
    handle.pause();
    let driver = thread::spawn(move || coin_then_feedback(&mut with, 2));

    let deadline = Instant::now() + Duration::from_secs(10);
    for _ in 0..2 {
        while handle.held() == 0 {
            assert!(Instant::now() < deadline, "the driver sent no batch");
            thread::sleep(Duration::from_millis(1));
        }
        assert!(handle.step());
    }
    let outcomes = driver.join().expect("the driver thread did not panic");
    assert_eq!(outcomes, coin_then_feedback(&mut toy_simulator(9), 2));
    assert_eq!(handle.held(), 0);
}

/// A held batch that the executor refuses when stepped: the caller's call
/// returned `Ok` long ago, so its reply ends as a failure, and the log
/// shows the refusal.
#[test]
fn a_held_batch_refused_when_stepped_fails_its_reply() {
    let (mut with, handle) = debugger(toy_simulator(0));
    handle.pause();
    let batch = InstructionBatchRef {
        shots: &ShotSet::one(0),
        instructions: &[Op::X(99)],
    };
    let reply = with.call(batch).expect("held, so accepted");
    assert!(handle.step());

    let mut outcomes = Vec::new();
    let reason =
        wait(reply, &mut outcomes).expect_err("the toy refuses qubit 99 when the batch is stepped");
    assert!(reason.starts_with("refused when stepped"), "{reason}");
    assert!(matches!(
        handle.log()[1],
        Entry::Rejected(Rejected::Instruction { at: 0, .. })
    ));
}

/// Closing the debugger closes the executor it wraps: the executor
/// refuses a later call.
#[test]
fn closing_the_debugger_closes_what_it_wraps() {
    let (mut with, _handle) = debugger(toy_simulator(0));
    let batch = InstructionBatchRef {
        shots: &ShotSet::one(0),
        instructions: &[Op::X(0)],
    };
    conformance::rejects_calls_once_closed::<InstructionBatch<Op>, _>(&mut with, batch);
}

/// A batch still held when the debugger is closed reaches a closed
/// executor when stepped: the executor refuses it, and the caller's reply,
/// accepted before the close, ends as a failure.
#[test]
fn a_batch_held_at_close_fails_its_reply_when_stepped() {
    let (mut with, handle) = debugger(toy_simulator(0));
    handle.pause();
    let batch = InstructionBatchRef {
        shots: &ShotSet::one(0),
        instructions: &[Op::X(0)],
    };
    let reply = with.call(batch).expect("held, so accepted");
    with.close();
    assert!(handle.step());

    let mut outcomes = Vec::new();
    let reason = wait(reply, &mut outcomes).expect_err("the closed executor refuses the batch");
    assert!(reason.starts_with("refused when stepped"), "{reason}");
    assert_eq!(handle.log()[1], Entry::Rejected(Rejected::Closed));
}

/// A program that flips a coin on every shot of its run, in one batch,
/// and reads it.
struct FlipACoin;

impl AdaptiveProgram for FlipACoin {
    type Instruction = Op;
    type Output = Vec<ShotOutcome>;

    async fn drive<E: Executor<InstructionBatch<Op>>>(
        self,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<ShotOutcome> {
        let batch = [Op::Coin(0), Op::Measure { qubit: 0, tag: 0 }];
        let reply = executor
            .call(InstructionBatchRef {
                shots,
                instructions: &batch,
            })
            .expect("the toy takes a coin and a measurement");
        let mut outcomes = Vec::new();
        wait(reply, &mut outcomes).expect("the toy never fails a batch");
        outcomes
    }
}

/// Composed into a chain, the adapter puts a paused debugger on the
/// executor, whose handle steps the run's batch, on both shots, from
/// another thread while the run waits on its own: the run gives the
/// outcomes of the run without it.
#[test]
fn a_debugger_composed_into_a_chain_hands_out_its_handle() {
    let shots = ShotSet::range(0..=1).expect("0..=1 is not empty");
    let debuggers = Debuggers::paused();
    let run = (driver() >> debuggers.clone() >> toy_simulator(9))
        .expect("the debugger changes nothing the toy checks")
        .run(FlipACoin, shots.clone())
        .expect("the program needs nothing");
    let run_thread = thread::spawn(move || run.wait());

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "the run sent no batch");
        if let Some(handle) = debuggers.handle()
            && handle.step()
        {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    let (with, _) = run_thread.join().expect("the run's thread did not panic");
    let (without, _) = (driver() >> toy_simulator(9))
        .expect("nothing between to refuse")
        .run(FlipACoin, shots)
        .expect("the program needs nothing")
        .wait();
    assert_eq!(with, without);
}
