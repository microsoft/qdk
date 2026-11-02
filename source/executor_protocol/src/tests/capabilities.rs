// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    Capabilities, Describe, JobFaults, Need, Needs, OnePerShot, Outcome, Outcomes, ProgramInput,
    Rejected, StaticProgram, driver, executor_fn,
};

/// A component that states nothing states no limit, any number of qubits
/// and shots at once, and offers nothing it does not state: no resumable
/// shots, resets, replayable shots or log probability.
#[test]
fn default_capabilities_state_no_limit_and_offer_nothing_unstated() {
    let offered = Capabilities::default();
    assert_eq!(
        (
            offered.qubits,
            offered.max_shots,
            offered.resumable_shots,
            offered.resets,
            offered.replayable,
            offered.log_probability
        ),
        (None, None, false, false, false, false)
    );
}

/// A simulator with a state per shot whose promise forgets to state
/// resumable shots, given a program that reads a shot, then reads it again
/// in a later call: the job is refused, naming the need, rather than run
/// on an executor that does not promise to continue the shot.
#[test]
fn an_executor_that_states_no_resumable_shots_is_refused_a_job_that_continues_a_shot() {
    let simulator = executor_fn(
        Capabilities::default(),
        |_| Ok(()),
        |(): &mut (), input: &ProgramInput<'_, bool>, outcomes: &mut Outcomes<'_>| {
            for _ in input.instructions() {
                outcomes.push(Outcome::new(0, false.into()));
            }
            Ok(())
        },
    )
    .shots(OnePerShot);
    let chain = (driver() >> simulator).expect("the simulator takes bools");
    let program = StaticProgram::new(Needs::default(), vec![vec![true], vec![true]]);
    let Err(JobFaults(faults)) = chain.run(program, 0..2) else {
        panic!("a job that continues a shot must be refused");
    };
    let [fault] = faults.as_slice() else {
        panic!("one fault: {faults:?}");
    };
    assert!(
        matches!(fault, Rejected::Unmet(unmet) if unmet.need == Need::ResumableShots),
        "{faults:?}"
    );
}

/// Two runs of one simulator, given other seeds: the run's seed is not a
/// capability, so they offer the same, and show the same.
#[test]
fn the_runs_seed_is_not_a_capability() {
    let simulator = |seed| {
        executor_fn(
            Capabilities::default().with_qubits(4),
            |_| Ok(()),
            |(): &mut (), _: &ProgramInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
        )
        .seed(seed)
        .capabilities()
    };
    let (first, second) = (simulator(1), simulator(2));
    assert_eq!(first, second);
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
}
