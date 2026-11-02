// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [Guide: Running a program](../docs/guides.md#running-a-program): what a
//! user of a simulator writes. A simulator, built with its seed, is an
//! executor; composed with the crate's driver, it gives a chain, and a
//! program, with the shots it runs, is the chain's job: `chain.run`
//! checks one against the other, and the run it gives runs nothing until it
//! is waited for, then gives the program's output and the executor's report.
//!
//! The executor here is the toy simulator (`toy/simulator.rs`), and the
//! program flips a coin on each shot and reads it, so a test sees which
//! seed each shot drew from. A noise model written by hand, a readout
//! error (`simulator/readout_error.rs`), draws from seeds of its own.

#[path = "simulator/readout_error.rs"]
mod readout_error;
#[allow(dead_code, reason = "this guide uses only the toy's executor")]
#[path = "toy/simulator.rs"]
mod toy;

use std::error::Error;

use executor_protocol::{
    AdaptiveProgram, Executor, InstructionBatch, InstructionBatchRef, JobFaults, Need, Needs,
    Rejected, ShotContext, ShotOutcome, ShotSet, Value, compose, conformance, driver, wait,
};

use readout_error::{READOUT, ReadoutError, flips};
use toy::{Op, QUBITS, toy_simulator};

/// A program that flips a coin on qubit 0 of each of its shots, in one
/// batch, and reads it; it needs `qubits` qubits.
struct FlipACoin {
    qubits: u32,
}

impl AdaptiveProgram for FlipACoin {
    type Instruction = Op;
    type Output = Vec<ShotOutcome>;

    fn needs(&self) -> Needs {
        Needs::default().with_qubits(self.qubits)
    }

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

/// Shots 0 to 63: enough coins that two seeds almost never draw the same.
fn shots() -> ShotSet {
    ShotSet::range(0..=63).expect("0..=63 is not empty")
}

/// The coin each shot read, on the toy simulator built with `seed`.
fn coins(seed: u64) -> Vec<Value> {
    coins_of(shots(), seed)
        .into_iter()
        .map(|outcome| outcome.value)
        .collect()
}

/// The coins `shots` read, on the toy simulator built with `seed`.
fn coins_of(shots: ShotSet, seed: u64) -> Vec<ShotOutcome> {
    let program = FlipACoin { qubits: 1 };
    let chain = compose(driver(), toy_simulator(seed)).expect("nothing between to refuse");
    let run = chain.run(program, shots).expect("the toy has a qubit");
    run.wait().0
}

/// A user composes the crate's driver with an executor built with its
/// seed, gives the chain the program and its shots, and waits for the run:
/// it gives the program's output, here one outcome per shot, and the
/// executor's report, here of the one batch sent.
#[test]
fn a_program_runs_on_an_executor_when_given_to_its_chain_and_waited_for()
-> Result<(), Box<dyn Error>> {
    let (program, shots, seed) = (FlipACoin { qubits: 1 }, shots(), 7);
    let run = compose(driver(), toy_simulator(seed))?.run(program, shots)?;
    let (output, report) = run.wait();
    let ran: Vec<u32> = output.iter().map(|outcome| outcome.shot).collect();
    assert_eq!(ran, (0..=63).collect::<Vec<_>>());
    assert_eq!(report.calls, 1);
    Ok(())
}

/// An executor built with the same seed gives the same outcomes: a run is
/// reproduced by its seed.
#[test]
fn an_executor_built_with_the_same_seed_gives_the_same_outcomes() {
    assert_eq!(coins(7), coins(7));
}

/// An executor built with another seed gives other outcomes: each shot
/// draws from the executor's seed.
#[test]
fn an_executor_built_with_another_seed_gives_other_outcomes() {
    assert_ne!(coins(7), coins(8));
}

/// A shot draws the same outcomes whichever other shots run beside it:
/// the even shots and the odd shots, run apart on executors built with the
/// same seed, read the coins they read when all the shots run together.
#[test]
fn a_shots_outcomes_do_not_depend_on_the_shots_run_beside_it() {
    let all = shots();
    let even = all.subset(|j| j % 2 == 0).expect("shot 0 is even");
    let odd = all.subset(|j| j % 2 == 1).expect("shot 1 is odd");
    let mut apart = coins_of(even, 7);
    apart.extend(coins_of(odd, 7));
    apart.sort_by_key(|outcome| outcome.shot);
    assert_eq!(apart, coins_of(all, 7));
}

/// A user adds a readout error written by hand, which draws, between the
/// driver and the simulator, grouping the chain either way: each shot
/// reads the simulator's coin, flipped where the readout error's own seed
/// for the shot says so, the seed of its id in a run with the simulator's
/// seed, as `compose` gives it.
#[test]
fn a_noise_model_written_by_hand_draws_from_its_own_seeds_in_either_grouping() {
    let program = || FlipACoin { qubits: 1 };
    let coins = coins_of(shots(), 7);
    let expected: Vec<ShotOutcome> = coins
        .iter()
        .map(|coin| {
            let seed = ShotContext::new(7, coin.shot).rng_seed(READOUT);
            let mut read = *coin;
            read.value = match (coin.value, flips(seed, 0)) {
                (Value::Zero, true) => Value::One,
                (Value::One, true) => Value::Zero,
                (value, _) => value,
            };
            read
        })
        .collect();
    let left = (driver() >> ReadoutError::default() >> toy_simulator(7))
        .expect("the readout error passes the toy's instructions on")
        .run(program(), shots())
        .expect("the program needs a qubit")
        .wait()
        .0;
    let below = compose(ReadoutError::default(), toy_simulator(7))
        .expect("the readout error passes the toy's instructions on");
    let right = compose(driver(), below)
        .expect("nothing between to refuse")
        .run(program(), shots())
        .expect("the program needs a qubit")
        .wait()
        .0;
    for (grouping, outcomes) in [("left", left), ("right", right)] {
        assert_eq!(outcomes, expected, "grouped {grouping}");
    }
}

/// A program that reads a coin, sends a batch that flips it and then
/// names a qubit the toy does not have, and reads the coin again: it gives
/// the first readouts, whether the second batch was refused, and the
/// readouts after it.
struct FlipThenOutOfRange;

impl AdaptiveProgram for FlipThenOutOfRange {
    type Instruction = Op;
    type Output = (Vec<ShotOutcome>, bool, Vec<ShotOutcome>);

    async fn drive<E: Executor<InstructionBatch<Op>>>(
        self,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Self::Output {
        let mut read = |batch: &[Op]| {
            let reply = executor.call(InstructionBatchRef {
                shots,
                instructions: batch,
            })?;
            let mut outcomes = Vec::new();
            wait(reply, &mut outcomes).expect("the toy never fails a batch");
            Ok::<_, Rejected>(outcomes)
        };
        let before = read(&[Op::Coin(0), Op::Measure { qubit: 0, tag: 0 }])
            .expect("the toy takes a coin and a measurement");
        let refused = read(&[Op::X(0), Op::X(QUBITS)]).is_err();
        let after = read(&[Op::Measure { qubit: 0, tag: 0 }]).expect("the toy reads qubit 0");
        (before, refused, after)
    }
}

/// A refused call runs nothing: after a batch refused for its last
/// instruction, every shot still reads the coin it read before, unflipped
/// by the batch's first instruction.
#[test]
fn a_refused_batch_changes_no_shot() {
    let run = compose(driver(), toy_simulator(7))
        .expect("nothing between to refuse")
        .run(FlipThenOutOfRange, shots())
        .expect("the program needs nothing");
    let ((before, refused, after), _) = run.wait();
    assert!(refused, "qubit {QUBITS} is out of the toy's range");
    assert_eq!(after, before);
}

/// A batch naming a qubit the toy does not have is refused, and the toy
/// then runs the corrected batch: the conformance kit's check that a
/// refusal leaves the executor usable.
#[test]
fn the_toy_accepts_calls_after_a_refusal() {
    let shots = ShotSet::one(0);
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<Op>, _>(
        &mut toy_simulator(7),
        InstructionBatchRef {
            shots: &shots,
            instructions: &[Op::X(0), Op::X(QUBITS)],
        },
        InstructionBatchRef {
            shots: &shots,
            instructions: &[Op::X(0)],
        },
    );
}

/// Once closed, the toy refuses every call: the conformance kit's check.
#[test]
fn the_toy_rejects_calls_once_closed() {
    conformance::rejects_calls_once_closed::<InstructionBatch<Op>, _>(
        &mut toy_simulator(7),
        InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::X(0)],
        },
    );
}

/// A toy that ran a call reports once: the conformance kit's check.
#[test]
fn the_toy_reports_once() {
    let mut toy = toy_simulator(7);
    toss_on_shot_0(&mut toy);
    conformance::reports_once(&mut toy);
}

/// Tosses a coin on shot 0 of `toy` and reads it, to the reply's end.
fn toss_on_shot_0(toy: &mut impl Executor<InstructionBatch<Op>>) {
    let reply = toy
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::Coin(0), Op::Measure { qubit: 0, tag: 0 }],
        })
        .expect("the toy takes a coin and a measurement");
    wait(reply, &mut Vec::new()).expect("the toy never fails a batch");
}

/// Each of shots 0 to 7 tosses alone, and with the shots split into calls
/// in another order, the coin it tosses among the others: the conformance
/// kit's check of `replayable`. Unlike
/// `a_shots_outcomes_do_not_depend_on_the_shots_run_beside_it`, which runs
/// a program through a driver, it calls the executor itself, as any
/// implementer's test can.
#[test]
fn the_toy_gives_each_shot_alone_what_it_gives_among_others() {
    conformance::gives_each_shot_alone_what_it_gives_among_others(
        || toy_simulator(7),
        &ShotSet::range(0..=7).expect("0..=7 is not empty"),
        &[&[Op::Coin(0), Op::Measure { qubit: 0, tag: 0 }]],
    );
}

/// A second call reads the flip of the first, on each of shots 0 and 1:
/// the conformance kit's check of `resumable_shots`.
#[test]
fn the_toy_continues_a_shot_from_one_call_to_the_next() {
    conformance::continues_a_shot_from_one_call_to_the_next(
        || toy_simulator(7),
        &ShotSet::range(0..=1).expect("0..=1 is not empty"),
        &[Op::X(0)],
        &[Op::Measure { qubit: 0, tag: 0 }],
    );
}

/// A flip on shots 1 and 3 leaves shots 0 and 2, flipped before, as they
/// were: the conformance kit's check that a call changes no shot it does
/// not name.
#[test]
fn the_toy_changes_no_shot_a_call_does_not_name() {
    conformance::changes_no_shot_a_call_does_not_name(
        || toy_simulator(7),
        &ShotSet::range(0..=3).expect("0..=3 is not empty"),
        &[Op::X(0)],
        &[Op::Measure { qubit: 0, tag: 0 }],
    );
}

/// After a toy flipped shots 0 to 3 and was closed, or dropped unclosed,
/// the next one reads them as made: the conformance kit's check that a new
/// executor starts its shots as made.
#[test]
fn the_toy_starts_its_shots_as_made_whatever_an_earlier_one_left() {
    conformance::starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || toy_simulator(7),
        &ShotSet::range(0..=3).expect("0..=3 is not empty"),
        &[Op::X(0)],
        &[Op::Measure { qubit: 0, tag: 0 }],
    );
}

/// A qubit flipped then reset reads zero: `reset` prepares `|0⟩`, as the
/// toy's set states.
#[test]
fn the_toy_reads_a_reset_qubit_as_zero() {
    let mut toy = toy_simulator(7);
    let reply = toy
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions: &[Op::X(0), Op::Reset(0), Op::Measure { qubit: 0, tag: 0 }],
        })
        .expect("the toy takes a flip, a reset and a measurement");
    let mut outcomes = Vec::new();
    wait(reply, &mut outcomes).expect("the toy never fails a batch");
    let read: Vec<Value> = outcomes.iter().map(|outcome| outcome.value).collect();
    assert_eq!(read, [Value::Zero]);
}

/// A batch with two instructions on qubits the toy does not have is
/// refused naming the first of them.
#[test]
fn a_batch_out_of_the_toys_range_is_refused_at_its_first_bad_instruction() {
    let mut toy = toy_simulator(7);
    let refused = toy.call(InstructionBatchRef {
        shots: &ShotSet::one(0),
        instructions: &[Op::X(0), Op::X(QUBITS), Op::X(QUBITS + 1)],
    });
    let reason = format!("the toy has {QUBITS} qubits");
    assert_eq!(refused.err(), Some(Rejected::Instruction { at: 1, reason }));
}

/// A program that needs more qubits than the simulator has is refused
/// when given to the chain, with what it needs and what the simulator
/// offers, before any shot runs.
#[test]
fn a_program_needing_more_than_the_executor_offers_is_refused_when_given_to_the_chain() {
    let program = FlipACoin { qubits: QUBITS + 1 };
    let chain = compose(driver(), toy_simulator(7)).expect("nothing between to refuse");
    let Err(JobFaults(faults)) = chain.run(program, shots()) else {
        panic!("given to a chain of a toy of {QUBITS} qubits")
    };
    let [Rejected::Unmet(unmet)] = faults.as_slice() else {
        panic!("{faults:?}")
    };
    assert_eq!(unmet.need, Need::Qubits);
}
