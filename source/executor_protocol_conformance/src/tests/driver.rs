// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The driver's checks: each passes a driver that keeps every rule, and
//! reports a driver that breaks its rule at the places it breaks it.

use executor_protocol::{ShotConditions, ShotId};

use super::fake_driver::{Breaks, FakeDriver, Record};
use super::{faults_found, faults_of, shots, trials, why_cannot_judge};
use crate::driver::{
    gives_one_output_per_shot_in_order, merges_each_executes_conditions, passes_the_seed_on,
    sends_only_its_shots, states_readout_slots,
};
use crate::{Nonconformance, Place, Rule, Trial, Trials};

/// Where the kit's executor logs the `batch`th batch of `execute`.
fn batch(execute: usize, batch: usize) -> Place {
    Place::BatchBelow {
        execute,
        call: 0,
        batch,
    }
}

/// The output at `index`, which is `shot`'s.
fn output(index: usize, shot: u32) -> Place {
    Place::Output {
        index,
        shot: ShotId::new(shot),
    }
}

/// A driver that keeps every rule passes every check, also one that
/// marks a shot's output with a condition of its own, one that repeats
/// until success: in some drawn trials it sends no shot twice, which only
/// trial 0 may report as leaving the check of merged conditions blind, and
/// one that skips rejected shots: in the run that rejects each shot in its
/// first `execute` only, it sends no shot again.
#[test]
fn a_driver_that_keeps_every_rule_passes_every_check() {
    for (driver, make) in [
        ("plain", FakeDriver::new as fn() -> FakeDriver),
        ("marking shot 1", || FakeDriver::marking(1)),
        (
            "repeating until success",
            FakeDriver::repeating_until_success,
        ),
        (
            "skipping rejected shots",
            FakeDriver::skipping_rejected_shots,
        ),
    ] {
        passes_every_check(driver, make);
    }
}

/// Asserts that every check of a driver passes the driver `make` builds,
/// naming `driver` and the check that did not.
fn passes_every_check(driver: &str, make: fn() -> FakeDriver) {
    let shots = shots([3, 1, 2, 0]);
    let checks: [(&str, Result<(), Nonconformance>); 5] = [
        (
            "gives_one_output_per_shot_in_order",
            gives_one_output_per_shot_in_order(make, &shots, trials()),
        ),
        (
            "states_readout_slots",
            states_readout_slots(make, &shots, trials()),
        ),
        (
            "sends_only_its_shots",
            sends_only_its_shots(make, &shots, trials()),
        ),
        (
            "passes_the_seed_on",
            passes_the_seed_on(make, &shots, trials()),
        ),
        (
            "merges_each_executes_conditions",
            merges_each_executes_conditions(
                make,
                &shots,
                |record: &Record| record.conditions,
                trials(),
            ),
        ),
    ];
    for (check, result) in checks {
        assert_eq!(result, Ok(()), "{driver}: {check}");
    }
}

/// A driver that gives its outputs in the order of the shots' ids, not
/// the run's, is reported at each output that is not its shot's, in the
/// shots' order and then in the reverse order: so also when the caller's
/// shots are sorted.
#[test]
fn outputs_in_another_order_than_the_shots_are_reported() {
    let make = || FakeDriver::breaking(Breaks::OutputsInSortedOrder);
    let cases = [
        (
            shots([3, 1, 2, 0]),
            vec![output(0, 3), output(3, 0), output(1, 2), output(2, 1)],
        ),
        (
            shots([0, 1, 2, 3]),
            vec![output(0, 3), output(1, 2), output(2, 1), output(3, 0)],
        ),
    ];
    for (shots, expected) in cases {
        assert_eq!(
            faults_of(
                Rule::GivesOneOutputPerShotInOrder,
                gives_one_output_per_shot_in_order(make, &shots, trials()),
            ),
            expected,
            "{shots:?}"
        );
    }
}

/// A driver that gives another number of outputs than shots is reported
/// at each such run: one that drops its last output, at the first run;
/// one that drops its first output when the shots come in another order,
/// at the run in reversed order only, not at the outputs that then sit one
/// position early; one that gives two outputs for a shot run alone, at
/// each run of a shot alone; and one that gives an output more than its
/// shots in its runs of several, at the first run, whose report ends the
/// check.
#[test]
fn another_number_of_outputs_is_reported_at_its_run() {
    let run = |run| Place::Run { run };
    let cases = [
        (Breaks::DropsTheLastOutput, vec![run(1)]),
        (Breaks::DropsTheFirstOutputUnlessInOrder, vec![run(2)]),
        (
            Breaks::GivesTwoOutputsForOneShot,
            vec![run(3), run(4), run(5), run(6)],
        ),
        (Breaks::GivesAnExtraOutputForSeveralShots, vec![run(1)]),
    ];
    for (breaks, expected) in cases {
        assert_eq!(
            faults_of(
                Rule::GivesOneOutputPerShotInOrder,
                gives_one_output_per_shot_in_order(
                    || FakeDriver::breaking(breaks),
                    &shots(0..4),
                    trials()
                ),
            ),
            expected,
            "{breaks:?}"
        );
    }
}

/// One shot gives nothing to put in another order, so the check says so
/// before it runs the driver: whether it runs trial 0, or a drawn trial
/// alone, replayed from a report.
#[test]
fn one_shot_cannot_judge_the_outputs_order() {
    let replayed = Trials::replaying(Trial {
        seed: Trials::DEFAULT_SEED,
        index: 1,
    });
    for trials in [trials(), replayed] {
        let mut drivers_made = 0;
        let make = || {
            drivers_made += 1;
            FakeDriver::new()
        };
        let why = why_cannot_judge(gives_one_output_per_shot_in_order(
            make,
            &shots([0]),
            trials,
        ));
        assert!(
            why.contains("an order needs two or more"),
            "{trials:?}: {why}"
        );
        assert_eq!(drivers_made, 0, "{trials:?}: drivers made before judging");
    }
}

/// A driver that understates a checked batch's slots is refused by the
/// protocol: reported at the run.
#[test]
fn checked_batches_with_misstated_slots_are_reported_at_the_refused_run() {
    let make = || FakeDriver::breaking(Breaks::UnderstatesSlots);
    assert_eq!(
        faults_of(
            Rule::StatesReadoutSlots,
            states_readout_slots(make, &shots(0..4), trials())
        ),
        [Place::Run { run: 1 }]
    );
}

/// A driver that understates an unchecked batch's slots is reported at
/// each such batch, which the protocol does not check.
#[test]
fn unchecked_batches_with_misstated_slots_are_reported_at_each_batch() {
    let make = || FakeDriver::unchecked_breaking(Breaks::UnderstatesSlots);
    assert_eq!(
        faults_of(
            Rule::StatesReadoutSlots,
            states_readout_slots(make, &shots(0..4), trials())
        ),
        [batch(1, 0), batch(1, 1)]
    );
}

/// A driver refused below for another fault than its slots, here qubits
/// past its needs, cannot be judged on its slots: the refusal is not
/// reported as a misstated count.
#[test]
fn a_run_refused_for_another_fault_cannot_judge_stated_slots() {
    let make = || FakeDriver::breaking(Breaks::SendsQubitsBeyondNeeds);
    let why = why_cannot_judge(states_readout_slots(make, &shots(0..4), trials()));
    assert!(why.contains("beyond"), "{why}");
}

/// A driver that sends a shot not the run's is reported at its batch,
/// naming the shot.
#[test]
fn a_shot_not_the_runs_is_reported_at_its_batch() {
    let make = || FakeDriver::breaking(Breaks::SendsAShotNotItsOwn);
    assert_eq!(
        faults_found(
            Rule::SendsOnlyItsShots,
            sends_only_its_shots(make, &shots(0..4), trials())
        ),
        [(batch(1, 2), "holds shot 999, not the run's".to_owned())]
    );
}

/// A driver that puts a shot in two batches of one `execute` is reported
/// at the second.
#[test]
fn a_shot_in_two_batches_of_an_execute_is_reported_at_the_second() {
    let make = || FakeDriver::breaking(Breaks::PutsAShotInTwoBatches);
    assert_eq!(
        faults_of(
            Rule::PutsEachShotInOneBatchPerCall,
            sends_only_its_shots(make, &shots(0..4), trials())
        ),
        [batch(1, 1)]
    );
}

/// A driver that sends its second `execute` with another seed is
/// reported at each batch of it, and only there.
#[test]
fn a_seed_changed_below_is_reported() {
    let make = || FakeDriver::breaking(Breaks::ReseedsItsSecondExecute);
    assert_eq!(
        faults_of(
            Rule::PassesTheSeedOn,
            passes_the_seed_on(make, &shots(0..4), trials())
        ),
        [batch(1, 0), batch(1, 1)]
    );
}

/// A driver that merges into each output its batch's first row's
/// conditions, in place of its own or beside them, or one kind of them,
/// gives a shot those of another: reported at each output that lacks its
/// own, or holds a condition in a run raising none on its shot. The first
/// row's kinds differ by `execute` and by run, so each one-kind leak shows
/// on outputs of its own.
#[test]
fn conditions_of_another_row_merged_are_reported() {
    for (breaks, expected) in [
        (
            Breaks::TakesTheFirstRowsConditions,
            vec![output(1, 1), output(2, 2), output(3, 3)],
        ),
        (
            Breaks::AddsTheFirstRowsConditions,
            vec![output(1, 1), output(2, 2), output(3, 3)],
        ),
        (
            Breaks::AddsOneKindOfTheFirstRow(ShotConditions::REJECTED),
            vec![output(1, 1), output(2, 2), output(3, 3)],
        ),
        (
            Breaks::AddsOneKindOfTheFirstRow(ShotConditions::INCONSISTENT),
            vec![output(2, 2)],
        ),
        (
            Breaks::AddsOneKindOfTheFirstRow(ShotConditions::UNRESOLVED),
            vec![output(3, 3)],
        ),
        (
            Breaks::MergesIntoRaisedOutputs,
            vec![output(0, 0), output(1, 1), output(2, 2), output(3, 3)],
        ),
    ] {
        let make = || FakeDriver::breaking(breaks);
        let faults = faults_of(
            Rule::MergesEachExecutesConditions,
            merges_each_executes_conditions(
                make,
                &shots(0..4),
                |record: &Record| record.conditions,
                trials(),
            ),
        );
        assert_eq!(faults, expected, "{breaks:?}");
    }
}

/// A driver that raises a condition of its own on a shot because of one it
/// read below, `INCONSISTENT` where it read `REJECTED`, is reported at the
/// outputs that read `REJECTED` without `INCONSISTENT`: the protocol asks
/// that a part raise its own conditions for what it finds in its readouts
/// only.
#[test]
fn a_condition_raised_because_of_one_read_below_is_reported() {
    let make = || FakeDriver::breaking(Breaks::FlagsARejectedShot);
    assert_eq!(
        faults_of(
            Rule::MergesEachExecutesConditions,
            merges_each_executes_conditions(
                make,
                &shots(0..4),
                |record: &Record| record.conditions,
                trials(),
            ),
        ),
        [output(2, 2)]
    );
}

/// A driver that sends nothing cannot be judged on what it sends: its
/// readout slots, its shots or its seed.
#[test]
fn a_driver_that_sends_nothing_cannot_be_judged_on_what_it_sends() {
    let make = FakeDriver::sending_nothing;
    for (check, result) in [
        (
            "states_readout_slots",
            states_readout_slots(make, &shots(0..4), trials()),
        ),
        (
            "sends_only_its_shots",
            sends_only_its_shots(make, &shots(0..4), trials()),
        ),
        (
            "passes_the_seed_on",
            passes_the_seed_on(make, &shots(0..4), trials()),
        ),
    ] {
        let why = why_cannot_judge(result);
        assert!(why.contains("sent nothing below"), "{check}: {why}");
    }
}

/// Outputs the check cannot pair with their shots, in every run, only in
/// those that raise conditions (a driver that drops rejected shots), or
/// only in the run that raises none, or a driver that sends no shot in
/// two `execute`s, cannot be judged on merging conditions.
#[test]
fn merging_conditions_cannot_be_judged_without_paired_outputs_over_two_executes() {
    let conditions = |record: &Record| record.conditions;
    for breaks in [
        Breaks::DropsTheLastOutput,
        Breaks::DropsRejectedShots,
        Breaks::DropsTheLastOutputWhenNoneRaised,
    ] {
        let make = || FakeDriver::breaking(breaks);
        let why = why_cannot_judge(merges_each_executes_conditions(
            make,
            &shots(0..4),
            conditions,
            trials(),
        ));
        assert!(why.contains("cannot pair"), "{breaks:?}: {why}");
    }
    let make = FakeDriver::of_one_execute;
    let why = why_cannot_judge(merges_each_executes_conditions(
        make,
        &shots(0..4),
        conditions,
        trials(),
    ));
    assert!(why.contains("no shot in two executes"), "{why}");
}

/// A driver that keeps only its last `execute`'s conditions, only the
/// latest conditions read on each shot, or clears them when a row reads
/// none, loses the earlier ones: reported at each output, also when a shot
/// skips an `execute`.
#[test]
fn conditions_of_an_earlier_execute_dropped_are_reported() {
    let every_output = [output(0, 0), output(1, 1), output(2, 2), output(3, 3)];
    let drivers = [
        (
            "of two executes",
            FakeDriver::breaking as fn(Breaks) -> FakeDriver,
        ),
        (
            "with a shot skipping an execute",
            FakeDriver::of_three_executes_breaking,
        ),
    ];
    for (driver, breaking) in drivers {
        for breaks in [
            Breaks::ForgetsEarlierConditions,
            Breaks::KeepsTheLatestConditions,
            Breaks::ClearsWhenARowReadsNone,
        ] {
            assert_eq!(
                faults_of(
                    Rule::MergesEachExecutesConditions,
                    merges_each_executes_conditions(
                        || breaking(breaks),
                        &shots(0..4),
                        |record: &Record| record.conditions,
                        trials()
                    ),
                ),
                every_output,
                "{breaks:?}, {driver}"
            );
        }
    }
}
