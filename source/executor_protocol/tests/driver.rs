// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [Writing a driver](../docs/getting-started.md#writing-a-driver): a
//! program with a branch on a readout, as data (`driver/if_one.rs`,
//! `IfOne`), and the driver that runs it (`IfOneDriver`), at the head of a
//! chain, `driver >> adapters >> executor`, here with no adapter. The program
//! states what its instructions need; the driver adds what its way of
//! sending needs (a shot continued in a later call), and the chain checks
//! both. The executor is the toy simulator (`toy/simulator.rs`).

#[path = "driver/if_one.rs"]
mod if_one;
#[allow(dead_code, reason = "this guide runs the toy's simulator as it is")]
#[path = "toy/simulator.rs"]
mod toy;

use executor_protocol::{
    Capabilities, DriverExt, JobFaults, Need, Needs, OnePerShot, Rejected, ShotSet, Value,
    executor_fn,
};

use if_one::{IfOne, IfOneDriver};
use toy::{Op, QUBITS, ToyState, toy_simulator};

/// Toss a coin on every shot; on the shots whose coin showed 1, toss a
/// second coin and read the first again.
fn toss_again_on_one() -> IfOne {
    IfOne {
        needs: Needs::default().with_qubits(2),
        first: vec![Op::Coin(0), Op::Measure { qubit: 0, tag: 0 }],
        tag: 0,
        then: vec![Op::Coin(1), Op::Measure { qubit: 0, tag: 1 }],
    }
}

/// A program that tosses a coin on eight shots, then tosses again and
/// reads the first coin again on the shots that read 1: only those shots
/// run the second batch, each where its first left it.
#[test]
fn a_driver_runs_a_branch_on_the_shots_whose_readout_is_one() {
    let shots = ShotSet::range(0..=7).expect("not empty");
    let chain = (IfOneDriver.into_chain() >> toy_simulator(3)).expect("nothing between to refuse");
    let (outcomes, _) = chain
        .run(toss_again_on_one(), shots)
        .expect("the toy has the qubits and resumable shots")
        .wait();
    let outcomes = outcomes.expect("the toy neither refuses nor fails a call");
    let ones: Vec<_> = (outcomes.iter())
        .filter(|o| o.tag == 0 && o.value == Value::One)
        .map(|o| o.shot)
        .collect();
    assert!(!ones.is_empty() && ones.len() < 8, "both sides: {ones:?}");
    let again: Vec<_> = outcomes.iter().filter(|o| o.tag == 1).collect();
    assert_eq!(again.iter().map(|o| o.shot).collect::<Vec<_>>(), ones);
    assert!(again.iter().all(|o| o.value == Value::One), "{again:?}");
}

/// The same program, on a toy that does not keep a shot's state between
/// calls: the job is refused before any call, for the need its driver adds
/// (`resumable_shots`), which the program does not state.
#[test]
fn a_job_is_refused_for_the_need_its_driver_adds() {
    let forgetful = Capabilities::default().with_qubits(QUBITS);
    let toy = executor_fn(forgetful, ToyState::start, ToyState::run)
        .seed(3)
        .shots(OnePerShot);
    let chain = (IfOneDriver.into_chain() >> toy).expect("nothing between to refuse");
    let Err(JobFaults(faults)) = chain.run(toss_again_on_one(), 0..=7) else {
        panic!("a toy without resumable shots must refuse the job");
    };
    let [Rejected::Unmet(unmet)] = faults.as_slice() else {
        panic!("the one fault is the driver's need: {faults:?}");
    };
    assert_eq!(unmet.need, Need::ResumableShots, "{unmet}");
}
