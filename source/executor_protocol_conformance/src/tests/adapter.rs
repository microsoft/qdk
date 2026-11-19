// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The adapter's checks: each passes an adapter that keeps every rule, and
//! reports an adapter that breaks its rule at the places it breaks it.

use std::collections::BTreeSet;

use executor_protocol::{ReadoutSlot, ShotConditions, ShotId};

use super::fake_adapter::{Breaks, FakeAdapter, Noise};
use super::{
    NOT_JUDGED_IF_NOT_REPLAYABLE, faults_found, faults_of, first, gates, needs,
    not_judged_as_not_replayable, numbers_named, script, second, shots, shows_each, trials,
    why_cannot_judge,
};
use crate::adapter::{
    carries_checked_batches_below, changes_no_shot_outside_its_batches,
    continues_each_shot_across_executes, does_nothing_until_polled,
    gives_each_shot_what_it_gives_alone, passes_each_shots_conditions_up, passes_the_seed_on,
    reports_only_each_executes_conditions, runs_as_new_after_a_failed_run, sends_only_shots_it_got,
    starts_each_shot_afresh, states_readout_slots, writes_every_readout,
};
use crate::findings::take_unexercised;
use crate::{Nonconformance, Place, Rule, Script};

/// Where the kit's executor logs the `batch`th batch of the first call
/// below in `execute`.
fn below(execute: usize, batch: usize) -> Place {
    Place::BatchBelow {
        execute,
        call: 0,
        batch,
    }
}

/// The row at `row` of `batch` in `execute`, which holds `shot`.
fn row(execute: usize, batch: usize, row: usize, shot: u32) -> Place {
    Place::Row {
        execute,
        batch,
        row,
        shot: ShotId::new(shot),
    }
}

/// An adapter that keeps every rule passes every check, each with what it
/// needs: with noise or without, sending each `execute` below in one call,
/// two or three, or each batch in a call of its own, marking a shot of the
/// first batch of each `execute` or of a later batch, laying out the
/// qubits the needs above it state, at each run, so that a check must get
/// it ready for the needs it is given, in each run, or drawing a run's
/// noise from one stream, offering shots that are not replayable: the
/// checks comparing runs that send the shots otherwise then write on
/// stderr that they did not judge it, and only then.
#[test]
fn an_adapter_that_keeps_every_rule_passes_every_check() {
    for (adapter, make) in [
        ("without noise", FakeAdapter::new as fn() -> FakeAdapter),
        ("noisy", FakeAdapter::noisy),
        ("in two calls below", FakeAdapter::splitting),
        ("in three calls below", FakeAdapter::in_three_calls),
        (
            "sending batches apart below",
            FakeAdapter::sending_batches_apart,
        ),
        ("marking a shot of its own", || FakeAdapter::marking(1)),
        ("marking a shot of a later batch", || {
            FakeAdapter::marking(3)
        }),
        ("marking a shot until it is rejected", || {
            FakeAdapter::marking_until_rejected(1)
        }),
        (
            "laying out the qubits its needs state",
            FakeAdapter::laying_out_its_needs,
        ),
    ] {
        passes_every_check(adapter, make, &[]);
    }
    passes_every_check(
        "drawing noise together",
        FakeAdapter::noisy_drawing_shots_together,
        &NOT_JUDGED_IF_NOT_REPLAYABLE,
    );
}

/// Runs every check on the adapters `make` makes, and asserts each passes,
/// and that what they write they did not judge, as the part is not
/// replayable, is about the rules `not_judged`.
fn passes_every_check(adapter: &str, make: fn() -> FakeAdapter, not_judged: &[Rule]) {
    take_unexercised();
    for (check, result) in every_check(make) {
        assert_eq!(result, Ok(()), "{check}, {adapter}");
    }
    let notes = take_unexercised();
    assert_eq!(
        not_judged_as_not_replayable(&notes),
        not_judged,
        "{adapter}"
    );
}

/// Every check of an adapter, on the adapters `make` makes, each with what
/// it needs.
fn every_check(make: fn() -> FakeAdapter) -> [(&'static str, Result<(), Nonconformance>); 13] {
    let (all, first, second) = (shots(0..4), first(), second());
    [
        (
            "writes_every_readout",
            writes_every_readout(make, needs(), &script(), trials()),
        ),
        (
            "sends_only_shots_it_got",
            sends_only_shots_it_got(make, needs(), &script(), trials()),
        ),
        (
            "passes_the_seed_on",
            passes_the_seed_on(make, needs(), &script(), trials()),
        ),
        (
            "states_readout_slots",
            states_readout_slots(make, needs(), &script(), trials()),
        ),
        (
            "passes_each_shots_conditions_up",
            passes_each_shots_conditions_up(make, needs(), &script(), trials()),
        ),
        (
            "reports_only_each_executes_conditions",
            reports_only_each_executes_conditions(make, needs(), &script(), trials()),
        ),
        (
            "carries_checked_batches_below",
            carries_checked_batches_below(make, needs(), &script(), trials()),
        ),
        (
            "gives_each_shot_what_it_gives_alone",
            gives_each_shot_what_it_gives_alone(make, needs(), &script(), trials()),
        ),
        (
            "continues_each_shot_across_executes",
            continues_each_shot_across_executes(make, needs(), &all, &first, &second, trials()),
        ),
        (
            "changes_no_shot_outside_its_batches",
            changes_no_shot_outside_its_batches(make, needs(), &all, &first, &second, trials()),
        ),
        (
            "starts_each_shot_afresh",
            starts_each_shot_afresh(make, needs(), &all, &first, &second, trials()),
        ),
        (
            "does_nothing_until_polled",
            does_nothing_until_polled(make, needs(), &script(), trials()),
        ),
        (
            "runs_as_new_after_a_failed_run",
            runs_as_new_after_a_failed_run(make, needs(), &script(), trials()),
        ),
    ]
}

/// An adapter that panics in `end_shots` on a shot it no longer holds, as the
/// runner ends every shot of a run that the driver ended after its last
/// `execute`, is reported at that end, naming the shots it ended and
/// quoting its panic.
#[test]
fn an_end_of_shots_no_longer_held_that_panics_is_reported() {
    let make = || FakeAdapter::breaking(Breaks::PanicsEndingShotsItDoesNotHold);
    let (place, found) = faults_found(
        Rule::IgnoresEndsOfShotsItDoesNotHold,
        writes_every_readout(make, needs(), &script(), trials()),
    )
    .swap_remove(0);
    assert_eq!(place, Place::EndShots { after: Some(1) });
    assert_eq!(numbers_named(&found), [0, 1, 2, 3, 57].into(), "{found}");
    assert!(found.contains("shot 0 is not held"), "the panic: {found}");
}

/// An adapter whose `end_shots` panics on shots it holds is not reported as
/// ending shots it does not hold: the panic fails the run, as the
/// protocol says, and the check cannot judge it.
#[test]
fn an_end_of_held_shots_that_panics_fails_the_run() {
    let result = writes_every_readout(
        FakeAdapter::panicking_ending_shots,
        needs(),
        &script(),
        trials(),
    );
    assert!(
        matches!(&result, Err(Nonconformance::CannotJudge { why, .. })
            if why.contains("panicked: the shots' release failed")),
        "{result:?}"
    );
}

/// An adapter that is not seedable, as one drawing its noise from a
/// physical source is not, cannot be judged by any check, though the
/// kit's executor below is seedable: every run of a check fixes its seed,
/// and the chain refuses such a run. Each check says so, naming the
/// adapter and that it is not seedable.
#[test]
fn an_adapter_that_is_not_seedable_cannot_be_judged_by_any_check() {
    for (check, result) in every_check(FakeAdapter::not_seedable) {
        let why = why_cannot_judge(result);
        assert!(
            why.contains("FakeAdapter") && why.contains("is not seedable"),
            "{check}: {why}"
        );
    }
}

/// A noisy adapter that breaks a rule on a shot's state, or on its calls, is
/// reported by that rule's check alone, comparing the runs an executor's
/// check would: the checks an adapter shares with an executor run through
/// the adapter's own wrapper.
#[test]
fn an_adapter_breaking_an_executors_rule_is_reported() {
    // A rule, the check's result, and the comparisons it must show.
    type Case = (
        Rule,
        Result<(), Nonconformance>,
        &'static [(&'static str, &'static str)],
    );
    let noise = Noise::default();
    let (all, first, second) = (shots(0..4), first(), second());
    let cases: [Case; 6] = [
        (
            Rule::ContinuesEachShotAcrossExecutes,
            continues_each_shot_across_executes(
                || FakeAdapter::noisy_breaking(Breaks::RestartsItsNoise),
                needs(),
                &all,
                &first,
                &second,
                trials(),
            ),
            &[("over two executes", "in one batch")],
        ),
        (
            Rule::ChangesNoShotOutsideItsBatches,
            changes_no_shot_outside_its_batches(
                || FakeAdapter::noisy_breaking(Breaks::DrawsNoiseOfOtherShots),
                needs(),
                &all,
                &first,
                &second,
                trials(),
            ),
            &[("after an execute on the other shots", "without it")],
        ),
        (
            Rule::StartsEachShotAfresh,
            starts_each_shot_afresh(
                || FakeAdapter::sharing_noise(&noise),
                needs(),
                &all,
                &first,
                &second,
                trials(),
            ),
            &[(
                "after an earlier part ran `first` on it and never ended it",
                "before",
            )],
        ),
        (
            Rule::DoesNothingUntilPolled,
            does_nothing_until_polled(
                || FakeAdapter::noisy_breaking(Breaks::DrawsNoiseWhenCalled),
                needs(),
                &script(),
                trials(),
            ),
            &[("after a call dropped unpolled", "without")],
        ),
        (
            Rule::DoesNothingUntilPolled,
            does_nothing_until_polled(
                || FakeAdapter::noisy_breaking(Breaks::RelaysWhenCalledAndReuses),
                needs(),
                &script(),
                trials(),
            ),
            &[("after a call dropped unpolled", "without")],
        ),
        (
            Rule::RunsAsNewAfterAFailedRun,
            runs_as_new_after_a_failed_run(
                || FakeAdapter::breaking(Breaks::RefusesAfterEndingShots),
                needs(),
                &script(),
                trials(),
            ),
            &[],
        ),
    ];
    for (rule, result, comparisons) in cases {
        shows_each(rule, result, comparisons);
    }
}

/// A noisy adapter that keeps a shot's noise past its end starts the shot,
/// in a later run on the same ids, where the earlier run left it.
#[test]
fn noise_kept_past_a_shots_end_is_reported_at_the_rows_of_the_run_after() {
    let make = || FakeAdapter::noisy_breaking(Breaks::KeepsEndedNoise);
    shows_each(
        Rule::RunsAsNewAfterAFailedRun,
        runs_as_new_after_a_failed_run(make, needs(), &script(), trials()),
        &[("after a failed run", "on a new part")],
    );
}

/// An adapter that leaves a slot unwritten above is reported at each such
/// slot, as an executor is.
#[test]
fn an_unwritten_slot_above_is_reported_at_its_place() {
    let make = || FakeAdapter::breaking(Breaks::LeavesASlotUnwritten);
    let slot = |execute, row, slot| Place::Slot {
        execute,
        batch: 0,
        row,
        shot: ShotId::new(1),
        slot: ReadoutSlot::new(slot),
    };
    assert_eq!(
        faults_of(
            Rule::WritesEveryReadout,
            writes_every_readout(make, needs(), &script(), trials())
        ),
        [slot(0, 1, 1), slot(1, 1, 0)]
    );
}

/// An adapter that gives a shot, among others, other records than alone is
/// reported: one answering a batch's rows in another order, which gives a
/// shot another shot's records, and a noisy one whose shots share a
/// stream. Unless it offers shots that are not replayable, as a noise
/// model drawing a run's shots from one stream does: the rule binds a
/// replayable part only, so the check passes it, and says in trial 0 that
/// it did not judge it.
#[test]
fn a_shot_reading_otherwise_among_others_is_reported_if_replayable() {
    let cases: [(fn() -> FakeAdapter, bool); 3] = [
        (|| FakeAdapter::breaking(Breaks::SwapsRows), true),
        (|| FakeAdapter::noisy_breaking(Breaks::SharesItsNoise), true),
        (FakeAdapter::noisy_drawing_shots_together, false),
    ];
    for (make, reported) in cases {
        take_unexercised();
        let result = gives_each_shot_what_it_gives_alone(make, needs(), &script(), trials());
        if reported {
            assert_ne!(
                faults_of(Rule::GivesEachShotWhatItGivesAlone, result),
                [] as [Place; 0]
            );
        } else {
            assert_eq!(result, Ok(()));
            let notes = take_unexercised();
            assert!(
                notes.iter().any(|note| note.contains("not replayable")),
                "{notes:?}"
            );
        }
    }
}

/// An adapter that sends below a shot no batch of the `execute` holds, one it
/// never got or one it got in an earlier `execute` only, and holds still,
/// is reported at each batch below holding it, naming the shot, or the
/// shots.
#[test]
fn a_shot_sent_below_that_the_adapter_did_not_get_is_reported() {
    let fewer_later = Script::new()
        .execute([(shots(0..4), gates(&[("h", 0), ("mz", 0)]))])
        .execute([(shots([0, 1]), gates(&[("x", 0), ("mz", 0)]))])
        .execute([(shots([2, 3]), gates(&[("x", 0), ("mz", 0)]))]);
    for (breaks, script, expected) in [
        (
            Breaks::SendsAShotItDidNotGet,
            script(),
            vec![(below(0, 1), [999].into()), (below(1, 2), [999].into())],
        ),
        (
            Breaks::SendsShotsOfEarlierExecutes,
            fewer_later,
            vec![(below(1, 1), [2, 3].into())],
        ),
    ] {
        let make = || FakeAdapter::breaking(breaks);
        let found: Vec<(Place, BTreeSet<u32>)> = faults_found(
            Rule::SendsOnlyShotsItGot,
            sends_only_shots_it_got(make, needs(), &script, trials()),
        )
        .into_iter()
        .map(|(place, found)| (place, numbers_named(&found)))
        .collect();
        assert_eq!(found, expected, "{breaks:?}");
    }
}

/// An adapter that sends a shot in two batches of one call below is reported
/// at the second.
#[test]
fn a_shot_sent_in_two_batches_of_a_call_below_is_reported() {
    let make = || FakeAdapter::breaking(Breaks::PutsAShotInTwoBatchesBelow);
    assert_eq!(
        faults_of(
            Rule::PutsEachShotInOneBatchPerCall,
            sends_only_shots_it_got(make, needs(), &script(), trials())
        ),
        [below(0, 1), below(1, 2)]
    );
}

/// An adapter that derives a seed of its own for the part below is reported
/// at each batch below.
#[test]
fn a_seed_changed_below_is_reported() {
    let make = || FakeAdapter::breaking(Breaks::ReseedsBelow);
    assert_eq!(
        faults_of(
            Rule::PassesTheSeedOn,
            passes_the_seed_on(make, needs(), &script(), trials())
        ),
        [below(0, 0), below(1, 0), below(1, 1)]
    );
}

/// An adapter that states one slot too many below is reported at each batch
/// below when its batches are unchecked, and at the run the protocol
/// refuses when they are checked.
#[test]
fn readout_slots_misstated_below_are_reported_checked_or_not() {
    let make = || FakeAdapter::breaking(Breaks::OverstatesSlotsBelow);
    assert_eq!(
        faults_of(
            Rule::StatesReadoutSlots,
            states_readout_slots(make, needs(), &script(), trials())
        ),
        [below(0, 0), below(1, 0), below(1, 1), Place::Run { run: 2 }]
    );
}

/// An adapter refused below for another fault than its slots, here qubits
/// past the shot's needs, cannot be judged on its slots: the refusal is
/// not reported as a misstated count. Nor can one that sends below an
/// instruction the part below does not accept, whose slots no one counts:
/// the check names that instruction, not the refusal that follows.
#[test]
fn a_run_refused_below_for_another_fault_cannot_judge_stated_slots() {
    for (breaks, named) in [
        (Breaks::SendsQubitsBeyondNeeds, "beyond"),
        (
            Breaks::SendsAnInstructionNotAcceptedBelow,
            "does not accept",
        ),
    ] {
        let make = || FakeAdapter::breaking(breaks);
        let why = why_cannot_judge(states_readout_slots(make, needs(), &script(), trials()));
        assert!(why.contains(named), "{breaks:?}: {why}");
    }
}

/// An adapter whose every checked run is refused below, for slots it
/// misstates, fails after the failed runs as on a new adapter: the check of
/// running again cannot judge it, and does not blame its third run.
#[test]
fn a_part_whose_every_run_fails_cannot_judge_running_again() {
    let why = why_cannot_judge(runs_as_new_after_a_failed_run(
        || FakeAdapter::breaking(Breaks::OverstatesSlotsBelow),
        needs(),
        &script(),
        trials(),
    ));
    assert!(why.contains("a run"), "{why}");
}

/// A run the adapter refuses as busy in `ready`, as a decoder shared with
/// other users may, which the protocol allows, ran nothing: a check cannot
/// judge the adapter, names the refusal, and says to run the check again
/// once the adapter is ready, whether the adapter refuses its first run,
/// or the second run of the adapter that `runs_as_new_after_a_failed_run`
/// runs twice.
#[test]
fn a_run_the_adapter_refuses_as_busy_cannot_judge_it() {
    for (refused, result) in [
        (
            "its first run",
            writes_every_readout(|| FakeAdapter::busy_at_run(1), needs(), &script(), trials()),
        ),
        (
            "the second run of the adapter run twice",
            runs_as_new_after_a_failed_run(
                || FakeAdapter::busy_at_run(2),
                needs(),
                &script(),
                trials(),
            ),
        ),
    ] {
        let why = why_cannot_judge(result);
        assert!(
            why.contains("FakeAdapter")
                && why.contains("is not ready: busy")
                && why.contains("run the check again"),
            "{refused}: {why}"
        );
    }
}

/// An adapter that drops the conditions read below, keeps of an `execute`'s
/// calls below only the last call's, only the first's or only the first
/// two's, clears them when a call reads none, or keeps only `REJECTED`, or
/// only the first kind of those a call reads, lets a shot raised on below
/// read as clear: reported at each row that lacks one, in some run raising
/// on its shot. Each shot reads another kind in each call, and shot 0
/// starts with `REJECTED`, shot 1 with `INCONSISTENT`, shot 2 with
/// `UNRESOLVED`, shot 3 with `REJECTED` again; a run raises every kind in
/// every call, and the last every kind in each `execute`'s first call
/// only. Only the first `execute` sends its batch in three calls below,
/// its second's having two instructions each.
#[test]
fn conditions_dropped_on_the_way_up_are_reported() {
    let every_row = vec![
        row(0, 0, 0, 0),
        row(0, 0, 1, 1),
        row(0, 0, 2, 2),
        row(0, 0, 3, 3),
        row(1, 0, 0, 0),
        row(1, 0, 1, 1),
        row(1, 1, 0, 2),
        row(1, 1, 1, 3),
    ];
    for (adapter, breaks, expected) in [
        (
            FakeAdapter::breaking as fn(Breaks) -> FakeAdapter,
            Breaks::DropsConditions,
            every_row.clone(),
        ),
        (
            FakeAdapter::splitting_breaking,
            Breaks::KeepsTheLastCallsConditions,
            every_row.clone(),
        ),
        (
            FakeAdapter::splitting_breaking,
            Breaks::KeepsTheFirstCallsConditions,
            every_row.clone(),
        ),
        (
            FakeAdapter::in_three_calls_breaking,
            Breaks::KeepsTheFirstTwoCallsConditions,
            every_row[..4].to_vec(),
        ),
        (
            FakeAdapter::splitting_breaking,
            Breaks::ClearsWhenACallReadsNone,
            every_row.clone(),
        ),
        (
            FakeAdapter::breaking,
            Breaks::KeepsOnlyRejected,
            every_row.clone(),
        ),
        (FakeAdapter::breaking, Breaks::KeepsTheWorstKind, every_row),
    ] {
        let make = || adapter(breaks);
        assert_eq!(
            faults_of(
                Rule::PassesEachShotsConditionsUp,
                passes_each_shots_conditions_up(make, needs(), &script(), trials())
            ),
            expected,
            "{breaks:?}"
        );
    }
}

/// An adapter that reports in a row the conditions read on another row of its
/// batch, in place of its own or beside them, or one kind of them, or those
/// of every row, makes a shot read as rejected for another's fault:
/// reported at each row that lacks its own, or that reports a condition in
/// a run raising none on its shot. Only the first `execute`'s batch has
/// rows two apart, and the next row's kinds differ by row and by run, so
/// each one-kind leak shows on rows of its own.
#[test]
fn conditions_of_another_row_are_reported() {
    let first_execute = vec![
        row(0, 0, 0, 0),
        row(0, 0, 1, 1),
        row(0, 0, 2, 2),
        row(0, 0, 3, 3),
    ];
    let every_row = [
        first_execute.clone(),
        vec![
            row(1, 0, 0, 0),
            row(1, 0, 1, 1),
            row(1, 1, 0, 2),
            row(1, 1, 1, 3),
        ],
    ]
    .concat();
    for (adapter, breaks, expected) in [
        (
            FakeAdapter::breaking as fn(Breaks) -> FakeAdapter,
            Breaks::AddsTheConditionsOfTheRowTwoFurther,
            first_execute.clone(),
        ),
        (
            FakeAdapter::splitting_breaking,
            Breaks::AddsTheConditionsOfTheRowTwoFurther,
            first_execute.clone(),
        ),
        (
            FakeAdapter::breaking,
            Breaks::AddsOneKindOfTheNextRow(ShotConditions::REJECTED),
            vec![
                row(0, 0, 2, 2),
                row(0, 0, 3, 3),
                row(1, 0, 1, 1),
                row(1, 1, 0, 2),
            ],
        ),
        (
            FakeAdapter::breaking,
            Breaks::AddsOneKindOfTheNextRow(ShotConditions::INCONSISTENT),
            vec![row(0, 0, 0, 0), row(1, 0, 0, 0)],
        ),
        (
            FakeAdapter::breaking,
            Breaks::AddsOneKindOfTheNextRow(ShotConditions::UNRESOLVED),
            vec![row(0, 0, 1, 1), row(1, 1, 1, 3)],
        ),
        (
            FakeAdapter::breaking,
            Breaks::GivesAnotherRowsConditions,
            first_execute.clone(),
        ),
        (
            FakeAdapter::breaking,
            Breaks::MergesIntoRaisedRows,
            every_row.clone(),
        ),
        (
            FakeAdapter::splitting_breaking,
            Breaks::MergesIntoRaisedRows,
            every_row.clone(),
        ),
        (
            FakeAdapter::breaking,
            Breaks::AddsTwoFurtherIfRaised,
            first_execute,
        ),
        (
            FakeAdapter::breaking,
            Breaks::MergesItsBatchsConditions,
            every_row.clone(),
        ),
    ] {
        let make = || adapter(breaks);
        assert_eq!(
            faults_of(
                Rule::PassesEachShotsConditionsUp,
                passes_each_shots_conditions_up(make, needs(), &script(), trials())
            ),
            expected,
            "{breaks:?}"
        );
    }
}

/// An adapter that raises a condition of its own on a row because of one it
/// read below on its shot, `INCONSISTENT` where it read `REJECTED`, is
/// reported at the rows that read `REJECTED` without `INCONSISTENT`: the
/// protocol asks that a part raise its own conditions for what it finds in
/// its readouts only.
#[test]
fn a_condition_raised_because_of_one_read_below_is_reported() {
    let make = || FakeAdapter::breaking(Breaks::FlagsARejectedShot);
    assert_eq!(
        faults_of(
            Rule::PassesEachShotsConditionsUp,
            passes_each_shots_conditions_up(make, needs(), &script(), trials())
        ),
        [
            row(0, 0, 0, 0),
            row(0, 0, 3, 3),
            row(1, 0, 0, 0),
            row(1, 1, 1, 3)
        ]
    );
}

/// An adapter that answers a shot's `execute` itself, without sending it
/// below, read no condition on it there, so its rows need hold none; one
/// that sends no shot below cannot be judged.
#[test]
fn conditions_are_asked_only_of_rows_of_shots_sent_below() {
    assert_eq!(
        passes_each_shots_conditions_up(
            || FakeAdapter::answering_alone(1),
            needs(),
            &script(),
            trials()
        ),
        Ok(())
    );
    let make = || FakeAdapter::answering_alone(usize::MAX);
    let why = why_cannot_judge(passes_each_shots_conditions_up(
        make,
        needs(),
        &script(),
        trials(),
    ));
    assert!(why.contains("sent below none"), "{why}");
}

/// An adapter that answers every `execute` itself, sending nothing below,
/// cannot be judged on what it sends: its shots, its seed, its readout
/// slots or its checked batches.
#[test]
fn an_adapter_that_sends_nothing_below_cannot_be_judged_on_what_it_sends() {
    let make = || FakeAdapter::answering_alone(usize::MAX);
    for (check, result) in [
        (
            "sends_only_shots_it_got",
            sends_only_shots_it_got(make, needs(), &script(), trials()),
        ),
        (
            "passes_the_seed_on",
            passes_the_seed_on(make, needs(), &script(), trials()),
        ),
        (
            "states_readout_slots",
            states_readout_slots(make, needs(), &script(), trials()),
        ),
        (
            "carries_checked_batches_below",
            carries_checked_batches_below(make, needs(), &script(), trials()),
        ),
    ] {
        let why = why_cannot_judge(result);
        assert!(why.contains("sent nothing below"), "{check}: {why}");
    }
}

/// An adapter that drops the conditions read below reports none in the first
/// `execute`, so a condition carried on would not show: the check cannot
/// judge it, also when the adapter raises a kind of its own on a shot in
/// every `execute`, which the clean run shows. One that reports some kinds
/// only (`REJECTED`) is judged, and carries nothing on.
#[test]
fn conditions_carried_over_are_judged_only_when_the_first_execute_reports_some() {
    for make in [
        (|| FakeAdapter::breaking(Breaks::DropsConditions)) as fn() -> FakeAdapter,
        || FakeAdapter::marking_breaking(0, Breaks::DropsConditions),
    ] {
        let why = why_cannot_judge(reports_only_each_executes_conditions(
            make,
            needs(),
            &script(),
            trials(),
        ));
        assert!(
            why.contains("check `passes_each_shots_conditions_up` first"),
            "{why}"
        );
    }
    let make = || FakeAdapter::breaking(Breaks::KeepsOnlyRejected);
    assert_eq!(
        reports_only_each_executes_conditions(make, needs(), &script(), trials()),
        Ok(())
    );
}

/// An adapter that reports in each row every condition read on its shot so
/// far, or every `INCONSISTENT` and `UNRESOLVED` only, in one call below
/// or two, or one kind only, is reported at each row of the later
/// `execute`; so is one that raises its own `INCONSISTENT` on a shot it
/// read `REJECTED` on earlier, which the protocol forbids too: a part
/// raises no condition because of one it read below.
#[test]
fn conditions_carried_into_a_later_execute_are_reported() {
    for (adapter, breaks) in [
        (
            FakeAdapter::breaking as fn(Breaks) -> FakeAdapter,
            Breaks::CarriesConditionsAcrossExecutes,
        ),
        (
            FakeAdapter::breaking,
            Breaks::CarriesInconsistentAndUnresolved,
        ),
        (
            FakeAdapter::splitting_breaking,
            Breaks::CarriesInconsistentAndUnresolved,
        ),
        (
            FakeAdapter::breaking,
            Breaks::CarriesOneKind(ShotConditions::REJECTED),
        ),
        (
            FakeAdapter::breaking,
            Breaks::CarriesOneKind(ShotConditions::INCONSISTENT),
        ),
        (
            FakeAdapter::breaking,
            Breaks::CarriesOneKind(ShotConditions::UNRESOLVED),
        ),
        (
            FakeAdapter::breaking,
            Breaks::FlagsLaterRoundsOfARejectedShot,
        ),
    ] {
        let make = || adapter(breaks);
        assert_eq!(
            faults_of(
                Rule::ReportsOnlyEachExecutesConditions,
                reports_only_each_executes_conditions(make, needs(), &script(), trials()),
            ),
            [
                row(1, 0, 0, 0),
                row(1, 0, 1, 1),
                row(1, 1, 0, 2),
                row(1, 1, 1, 3)
            ],
            "{breaks:?}"
        );
    }
}

/// An adapter that carries a kind on only when it read it without another is
/// reported at the later rows of the shots that read it so: raised one
/// kind per shot, shot 1 reads `INCONSISTENT` alone.
#[test]
fn a_kind_carried_on_when_read_alone_is_reported() {
    let make = || FakeAdapter::breaking(Breaks::CarriesInconsistentUnlessRejected);
    assert_eq!(
        faults_of(
            Rule::ReportsOnlyEachExecutesConditions,
            reports_only_each_executes_conditions(make, needs(), &script(), trials()),
        ),
        [row(1, 0, 1, 1)]
    );
}

/// An adapter that carries on the conditions of one `execute` only is
/// reported at each later row, whichever `execute` that is: conditions are
/// raised in every `execute` but the last, one at a time. One carries
/// them from its second `execute` on, as a decoder clearing its flags
/// after the first round alone would; one carries its first `execute`'s
/// only, as a decoder keeping its first round's flags would.
#[test]
fn conditions_carried_from_one_execute_only_are_reported() {
    let script = script().execute([(shots(0..4), gates(&[("mz", 0)]))]);
    let third = vec![
        row(2, 0, 0, 0),
        row(2, 0, 1, 1),
        row(2, 0, 2, 2),
        row(2, 0, 3, 3),
    ];
    let second_and_third = [
        vec![
            row(1, 0, 0, 0),
            row(1, 0, 1, 1),
            row(1, 1, 0, 2),
            row(1, 1, 1, 3),
        ],
        third.clone(),
    ]
    .concat();
    for (breaks, expected) in [
        (Breaks::CarriesConditionsFromItsSecondExecute, third),
        (Breaks::CarriesConditionsOfItsFirstExecute, second_and_third),
    ] {
        let make = || FakeAdapter::breaking(breaks);
        assert_eq!(
            faults_of(
                Rule::ReportsOnlyEachExecutesConditions,
                reports_only_each_executes_conditions(make, needs(), &script, trials()),
            ),
            expected,
            "{breaks:?}"
        );
    }
}

/// A script of one `execute` has no later row to carry conditions into.
#[test]
fn one_execute_cannot_judge_conditions_carried_over() {
    let script = Script::new().execute([(shots(0..2), gates(&[("h", 0), ("mz", 0)]))]);
    let why = why_cannot_judge(reports_only_each_executes_conditions(
        FakeAdapter::new,
        needs(),
        &script,
        trials(),
    ));
    assert!(why.contains("two or more"), "{why}");
}

/// An adapter that sends checked batches below unchecked is reported at each
/// batch below.
#[test]
fn checked_batches_sent_below_unchecked_are_reported() {
    let make = || FakeAdapter::breaking(Breaks::UnchecksBelow);
    assert_eq!(
        faults_of(
            Rule::CarriesCheckedBatchesBelow,
            carries_checked_batches_below(make, needs(), &script(), trials()),
        ),
        [below(0, 0), below(1, 0), below(1, 1)]
    );
}
