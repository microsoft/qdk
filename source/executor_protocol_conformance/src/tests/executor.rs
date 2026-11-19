// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor's checks: each passes an executor that keeps every rule,
//! and reports an executor that breaks its rule at the places it breaks
//! it.

use std::cell::Cell;
use std::rc::Rc;

use executor_protocol::{ReadoutSlot, ShotId, ShotSet};

use super::fake_executor::{Breaks, Device, FakeExecutor};
use super::{
    NOT_JUDGED_IF_NOT_REPLAYABLE, faults_found, faults_of, first, gates, needs,
    not_judged_as_not_replayable, script, second, shots, shows_each, trials, why_cannot_judge,
};
use crate::executor::{
    changes_no_shot_outside_its_batches, continues_each_shot_across_executes,
    does_nothing_until_polled, gives_each_shot_what_it_gives_alone, keeps_its_accepts_and_offers,
    runs_as_new_after_a_failed_run, starts_each_shot_afresh, writes_every_readout,
};
use crate::nonconformance::take_unexercised;
use crate::{Nonconformance, Place, Rule, Script, Trials};

/// An executor that keeps every rule passes every check, each with what
/// it needs, whether it raises no condition, raises one on a shot by its
/// state, or draws a run's shots from one stream, offering shots that are
/// not replayable: the checks comparing runs that send the shots
/// otherwise then write on stderr that they did not judge it, and only
/// then.
#[test]
fn an_executor_that_keeps_every_rule_passes_every_check() {
    let not_replayable = &NOT_JUDGED_IF_NOT_REPLAYABLE[..];
    for (executor, make, not_judged) in [
        (
            "raising no condition",
            FakeExecutor::new as fn() -> FakeExecutor,
            &[][..],
        ),
        ("raising conditions", FakeExecutor::flagging, &[]),
        (
            "drawing shots together",
            FakeExecutor::drawing_shots_together,
            not_replayable,
        ),
    ] {
        take_unexercised();
        for (check, result) in every_check(make) {
            assert_eq!(result, Ok(()), "{check}, on an executor {executor}");
        }
        let notes = take_unexercised();
        assert_eq!(
            not_judged_as_not_replayable(&notes),
            not_judged,
            "{executor}"
        );
    }
}

/// Every check of an executor, on the executors `make` makes, each with
/// what it needs.
fn every_check(make: fn() -> FakeExecutor) -> [(&'static str, Result<(), Nonconformance>); 8] {
    let (all, first, second) = (shots(0..4), first(), second());
    [
        (
            "writes_every_readout",
            writes_every_readout(make, needs(), &script(), trials()),
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
            "keeps_its_accepts_and_offers",
            keeps_its_accepts_and_offers(make, needs(), &script(), trials()),
        ),
        (
            "runs_as_new_after_a_failed_run",
            runs_as_new_after_a_failed_run(make, needs(), &script(), trials()),
        ),
    ]
}

/// An executor that panics in `end_shots` on a shot it does not hold is
/// reported by every check, in trial 0, at its ends: whether it panics on
/// a shot it no longer holds, as the runner ends every shot of a run that
/// the driver ended after its last `execute`, or only on an id beyond the
/// table its shots fill, as the runner ends the shot of the run never
/// sent. The failed run's end counts too: it holds shots held and not.
/// A drawn trial's runs end such shots as well.
#[test]
fn an_end_of_shots_not_held_that_panics_is_reported_by_every_check() {
    for (part, make, panic) in [
        (
            "panicking on a shot not held",
            (|| FakeExecutor::breaking(Breaks::PanicsEndingShotsItDoesNotHold))
                as fn() -> FakeExecutor,
            "shot 0 is not held",
        ),
        (
            "panicking on an id beyond its table",
            || FakeExecutor::breaking(Breaks::PanicsEndingShotsBeyondItsTable),
            "shot 57 is beyond its table",
        ),
    ] {
        for (check, result) in every_check(make) {
            let Err(Nonconformance::Broken { trial, rules, .. }) = &result else {
                panic!("{part}, {check}: expected broken rules, got {result:?}");
            };
            assert!(trial.is_fixed(), "{part}, {check}: found in {trial}");
            let faults = faults_found(Rule::IgnoresEndsOfShotsItDoesNotHold, result.clone());
            assert!(!faults.is_empty(), "{part}, {check}");
            for (place, found) in &faults {
                assert!(
                    matches!(place, Place::EndShots { .. }),
                    "{part}, {check}: {place}"
                );
                let (unheld, panicked_on) = unheld_and_panicked_on(found);
                assert!(unheld.contains(&panicked_on), "{part}, {check}: {found}");
            }
            if check == "runs_as_new_after_a_failed_run" {
                assert!(
                    faults
                        .iter()
                        .any(|(place, _)| *place == Place::EndShots { after: Some(0) }),
                    "{part}: the failed run's end, after execute 0: {rules:?}"
                );
            }
        }
        let (place, found) = faults_found(
            Rule::IgnoresEndsOfShotsItDoesNotHold,
            writes_every_readout(make, needs(), &script(), trials()),
        )
        .swap_remove(0);
        assert_eq!(place, Place::EndShots { after: Some(1) }, "{part}: {found}");
        let (unheld, _) = unheld_and_panicked_on(&found);
        assert_eq!(unheld, ["0", "1", "2", "3", "57"], "{part}: {found}");
        assert!(
            found.contains(panic),
            "{part}: the fault names the part's panic: {found}"
        );
        let drawn = Trials::drawn_only(Trials::DEFAULT_SEED, 1);
        assert!(
            !faults_found(
                Rule::IgnoresEndsOfShotsItDoesNotHold,
                writes_every_readout(make, needs(), &script(), drawn)
            )
            .is_empty(),
            "{part}: a drawn trial's runs end a shot never sent too"
        );
    }
}

/// The shots an end fault's report names as not held, and the shot the
/// part's panic names.
fn unheld_and_panicked_on(found: &str) -> (Vec<String>, String) {
    let (_, after) = found
        .split_once("among which ")
        .expect("the shots not held");
    let (unheld, panic) = after
        .split_once(" it did not hold")
        .expect("the shots not held");
    let unheld = unheld
        .trim_start_matches("shots ")
        .trim_start_matches("shot ")
        .split(", ")
        .map(str::to_owned)
        .collect();
    let (_, panicked) = panic.split_once(": shot ").expect("the part's panic");
    let panicked_on = panicked.split(' ').next().expect("a shot id").to_owned();
    (unheld, panicked_on)
}

/// An executor whose `end_shots` panics on shots it holds is not reported
/// as ending shots it does not hold: the panic fails the run, as the
/// protocol says, and the check cannot judge it, whether the shots it ends
/// sat in one batch of an `execute` or in several.
#[test]
fn an_end_of_held_shots_that_panics_fails_the_run() {
    let in_two_batches = Script::new().execute([
        (shots([0, 1]), gates(&[("h", 0), ("mz", 0)])),
        (shots([2, 3]), gates(&[("x", 1), ("mz", 1)])),
    ]);
    for script in [script(), in_two_batches] {
        let result = writes_every_readout(
            FakeExecutor::panicking_ending_shots,
            needs(),
            &script,
            trials(),
        );
        assert!(
            matches!(&result, Err(Nonconformance::CannotJudge { why, .. })
                if why.contains("panicked: the shots' release failed")),
            "{script:?}: {result:?}"
        );
    }
}

/// A run holds a shot its script never names unless it cannot: the chain
/// refuses a run of one shot more (an executor offering runs of four
/// shots at most, under a script of four), or
/// the script names shot `u32::MAX`, with no id above it. Such a run is
/// judged on the named shots alone, and trial 0 notes it; a script of
/// three shots, or whose largest id is `u32::MAX - 1`, gets the shot, and
/// nothing is noted. A run refused otherwise, by a part busy at its first
/// run, is not run again: the check cannot judge the part. Every executor
/// check judges the four-shot part this way too, each noting it once.
#[test]
fn a_run_goes_without_the_shot_never_sent_only_where_it_cannot_hold_one() {
    let four_shots = || FakeExecutor::offering_four_shots_at_most();
    let beyond_its_table = || FakeExecutor::breaking(Breaks::PanicsEndingShotsBeyondItsTable);
    let busy = || FakeExecutor::busy_at_run(1);
    let on =
        |ids: &[u32]| Script::new().execute([(shots(ids.to_vec()), gates(&[("h", 0), ("mz", 0)]))]);
    let trial_0 = Trials::new(Trials::DEFAULT_SEED, 0);
    let passes = |result: &Result<(), Nonconformance>| *result == Ok(());
    let ends_unsent = |result: &Result<(), Nonconformance>| {
        matches!(result, Err(Nonconformance::Broken { trial, .. }) if trial.is_fixed())
            && faults_found(Rule::IgnoresEndsOfShotsItDoesNotHold, result.clone())
                .iter()
                .any(|(_, found)| found.contains("shot 4294967295 is beyond its table"))
    };
    let refused = |result: &Result<(), Nonconformance>| {
        why_cannot_judge(result.clone()).contains("is not ready: busy")
    };
    let rows: [WithoutUnsentRow; 5] = [
        (
            "room for four, four shots",
            four_shots,
            script(),
            trials(),
            passes,
            Some((2, 2)),
        ),
        (
            "room for four, three shots",
            four_shots,
            on(&[0, 1, 2]),
            trials(),
            passes,
            None,
        ),
        (
            "largest id u32::MAX - 1",
            beyond_its_table,
            on(&[u32::MAX - 1]),
            trial_0,
            ends_unsent,
            None,
        ),
        (
            "largest id u32::MAX",
            beyond_its_table,
            on(&[u32::MAX]),
            trial_0,
            passes,
            Some((1, 1)),
        ),
        (
            "busy at its first run",
            busy,
            script(),
            trials(),
            refused,
            None,
        ),
    ];
    for (row, make, script, trials, judged, noted) in rows {
        take_unexercised();
        let result = writes_every_readout(make, needs(), &script, trials);
        assert!(judged(&result), "{row}: {result:?}");
        let notes = take_unexercised();
        match noted {
            Some((without, runs)) => {
                let [note] = &notes[..] else {
                    panic!("{row}: one note, got {notes:?}");
                };
                assert_note_of_runs_without_unsent(note, without, runs);
            }
            None => assert!(notes.is_empty(), "{row}: {notes:?}"),
        }
    }
    let results = every_check(four_shots);
    let noted = take_unexercised();
    for (check, result) in &results {
        assert_eq!(*result, Ok(()), "{check}");
    }
    assert_eq!(noted.len(), results.len(), "one note per check: {noted:?}");
}

/// A row of the table of runs without the shot never sent: its name, the
/// part, the script, the trials, whether the check's result is as
/// expected, and the runs without the shot and the runs of trial 0 its
/// note names, if trial 0 writes one.
type WithoutUnsentRow = (
    &'static str,
    fn() -> FakeExecutor,
    Script,
    Trials,
    fn(&Result<(), Nonconformance>) -> bool,
    Option<(usize, usize)>,
);

/// Checks that `note`, written by trial 0 on [`FakeExecutor`] when
/// `without` of its `runs` held only the shots the check was given, names
/// the part, the rule its runs did not exercise, both counts, and what to
/// change: the chain's `max_shots`, and ids below `u32::MAX`.
fn assert_note_of_runs_without_unsent(note: &str, without: usize, runs: usize) {
    let names = [
        "tests::fake_executor::FakeExecutor".to_owned(),
        format!("{:?}", Rule::IgnoresEndsOfShotsItDoesNotHold),
        format!("{without} of the {runs} runs"),
        "`max_shots`".to_owned(),
        u32::MAX.to_string(),
    ];
    for name in names {
        assert!(note.contains(&name), "the note names {name}: {note}");
    }
}

/// An executor that leaves a slot unwritten, so that the parts above read
/// the `Missing` the protocol shaped as if it were the shot's, is reported
/// at each such slot.
#[test]
fn an_unwritten_slot_is_reported_at_its_place() {
    let make = || FakeExecutor::breaking(Breaks::LeavesASlotUnwritten);
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

/// An executor that writes every slot and repeats its values for a seed
/// passes each trial in two runs: the second run's prefills are the
/// first's complements, so no slot can equal both.
#[test]
fn an_executor_that_writes_every_value_passes_in_two_runs() {
    let built = Cell::new(0);
    let make = || {
        built.set(built.get() + 1);
        FakeExecutor::new()
    };
    assert_eq!(
        writes_every_readout(make, needs(), &script(), trials()),
        Ok(())
    );
    let trials = usize::try_from(Trials::DEFAULT_COUNT).expect("a count fits in usize") + 1;
    assert_eq!(built.get(), 2 * trials);
}

/// An executor whose values change from run to run, as a GPU's sums in
/// another order do, but that writes every value, is not reported.
#[test]
fn values_that_change_from_run_to_run_are_not_reported_unwritten() {
    let executes = Rc::new(Cell::new(0));
    let make = || FakeExecutor::drifting(&executes);
    assert_eq!(
        writes_every_readout(make, needs(), &script(), trials()),
        Ok(())
    );
}

/// A script that reads nothing gives no value to check, so the check says
/// it cannot judge the part rather than pass it.
#[test]
fn a_script_that_reads_nothing_cannot_judge_writing_every_readout() {
    let script = Script::new().execute([(shots(0..2), gates(&[("h", 0)]))]);
    let why = why_cannot_judge(writes_every_readout(
        FakeExecutor::new,
        needs(),
        &script,
        trials(),
    ));
    assert!(why.contains("reads no readout"), "{why}");
}

/// An executor that answers a batch with readouts of another shape, one
/// more slot or one more row, is reported at that batch, by any check that
/// runs it, once however many runs show it.
#[test]
fn readouts_of_another_shape_are_reported_at_their_batch() {
    let batch = |execute| Place::Batch { execute, batch: 0 };
    for breaks in [Breaks::ReplacesItsReadouts, Breaks::AddsARowToItsReadouts] {
        let make = || FakeExecutor::breaking(breaks);
        for (check, result) in [
            (
                "keeps_its_accepts_and_offers",
                keeps_its_accepts_and_offers(make, needs(), &script(), trials()),
            ),
            (
                "writes_every_readout, over several runs",
                writes_every_readout(make, needs(), &script(), trials()),
            ),
        ] {
            assert_eq!(
                faults_of(Rule::KeepsEachBatchsShape, result),
                [batch(0), batch(1)],
                "{breaks:?}, {check}"
            );
        }
    }
}

/// An executor that gives a shot other records among other shots than
/// alone is reported by each comparison that shows it: one whose shots
/// share a stream, by every split; one rejecting a batch of many as a
/// whole, though it reads the same values, among the others alone; and one
/// answering a batch's rows in the order of its sorted ids, only once a
/// batch's shots come in another order, by the split in reversed halves.
/// Unless it offers shots that are not replayable, as a sampler drawing a
/// run's shots from one stream does: the rule binds a replayable part
/// only, so the check passes it, and says in trial 0 that it did not judge
/// it.
#[test]
fn a_shot_reading_otherwise_among_others_is_reported_if_replayable() {
    type Comparisons<'a> = &'a [(&'a str, &'a str)];
    let among = ("among the others", "alone");
    let interleaved = ("with the shots split at odd and even positions", "alone");
    let reversed = ("with the shots split in halves, each reversed", "alone");
    let cases: [(fn() -> FakeExecutor, Comparisons<'_>); 4] = [
        (
            || FakeExecutor::breaking(Breaks::SharesItsDraws),
            &[among, interleaved, reversed],
        ),
        (
            || FakeExecutor::breaking(Breaks::RejectsBatchesOfMany),
            &[among],
        ),
        (|| FakeExecutor::breaking(Breaks::SortsItsRows), &[reversed]),
        (FakeExecutor::drawing_shots_together, &[]),
    ];
    for (make, comparisons) in cases {
        take_unexercised();
        let result = gives_each_shot_what_it_gives_alone(make, needs(), &script(), trials());
        if comparisons.is_empty() {
            assert_eq!(result, Ok(()));
            let notes = take_unexercised();
            assert!(
                notes.iter().any(|note| note.contains("not replayable")),
                "{notes:?}"
            );
        } else {
            shows_each(Rule::GivesEachShotWhatItGivesAlone, result, comparisons);
        }
    }
}

/// A script on which an executor giving each shot another's records could
/// pass cannot judge the part: one of two shots, which can be split only
/// one way; one reading nothing, so no record shows a shot; and one whose
/// shots read the same.
#[test]
fn a_script_unable_to_show_another_shots_records_cannot_judge_records_alone() {
    let cases = [
        (shots(0..2), gates(&[("h", 0), ("mz", 0)]), "three or more"),
        (shots(0..3), gates(&[("h", 0)]), "reads no readout"),
        (
            shots(0..3),
            gates(&[("x", 0), ("mz", 0)]),
            "every shot reads the same",
        ),
    ];
    for (shots, instructions, reason) in cases {
        let script = Script::new().execute([(shots, instructions)]);
        let why = why_cannot_judge(gives_each_shot_what_it_gives_alone(
            FakeExecutor::new,
            needs(),
            &script,
            trials(),
        ));
        assert!(why.contains(reason), "{why}");
    }
}

/// Shots that read the same values but raise different conditions differ,
/// so the check judges the part on them.
#[test]
fn shots_differing_in_conditions_alone_can_judge_records_alone() {
    let script = Script::new().execute([(shots(0..4), gates(&[("h", 1), ("mz", 0)]))]);
    assert_eq!(
        gives_each_shot_what_it_gives_alone(FakeExecutor::flagging, needs(), &script, trials()),
        Ok(())
    );
}

/// A shot over two `execute`s must read what one batch of both reads: an
/// executor that makes each shot again at each `execute` reads, after
/// `first` then `second`, what `second` reads alone, and one that rejects
/// a shot it continues, though it reads the same values, raises a
/// condition one batch does not.
#[test]
fn a_shot_reading_otherwise_over_two_executes_than_in_one_is_reported() {
    for breaks in [Breaks::RestartsItsShots, Breaks::FlagsContinuedShots] {
        let make = || FakeExecutor::breaking(breaks);
        let faults = faults_of(
            Rule::ContinuesEachShotAcrossExecutes,
            continues_each_shot_across_executes(
                make,
                needs(),
                &shots(0..4),
                &first(),
                &second(),
                trials(),
            ),
        );
        assert!(
            faults.contains(&Place::Shot {
                shot: ShotId::new(0)
            }),
            "{breaks:?}: {faults:?}"
        );
    }
}

/// An executor that keeps each shot's state at its row in the batch, not
/// at its id, continues a shot from another's state once the shots are
/// batched otherwise in the second `execute`, or are new to it there.
#[test]
fn a_shot_continued_from_anothers_state_is_reported() {
    let make = || FakeExecutor::breaking(Breaks::KeysShotsByRow);
    shows_each(
        Rule::ContinuesEachShotAcrossExecutes,
        continues_each_shot_across_executes(
            make,
            needs(),
            &shots(0..4),
            &first(),
            &second(),
            trials(),
        ),
        &[
            ("with `second` in two batches", "with `second` in one batch"),
            (
                "with shots new to the part in the second execute",
                "without",
            ),
            (
                "with shots new to the part in the second execute",
                "with `second` alone",
            ),
        ],
    );
}

/// A `second` that does not read what `first` changes would pass an
/// executor that makes each shot again, so the check cannot judge it.
#[test]
fn a_second_not_reading_first_cannot_judge_continuing_shots() {
    let (first, second) = (gates(&[("x", 2)]), gates(&[("mz", 0)]));
    let result = continues_each_shot_across_executes(
        FakeExecutor::new,
        needs(),
        &shots(0..4),
        &first,
        &second,
        trials(),
    );
    assert!(why_cannot_judge(result).contains("reads the same alone as after `first`"));
}

/// An executor whose `execute` flips the shots it holds, not named,
/// between those it names, above them, or made before or after them, is
/// reported at the rows that read the flip, in the run that compares them,
/// in the same report whatever the order of the shots given: trial 0
/// splits them sorted.
#[test]
fn an_execute_changing_a_shot_outside_its_batches_is_reported() {
    let rows: [(Breaks, &[(&str, &str)]); 4] = [
        (Breaks::RunsHeldShotsBetweenItsOwn, &[]),
        (Breaks::RunsHeldShotsAboveItsOwn, &[]),
        (
            Breaks::RunsHeldShotsMadeBeforeItsOwn,
            &[("after an execute on the other shots", "without it")],
        ),
        (
            Breaks::RunsHeldShotsMadeAfterItsOwn,
            &[("with executes on the other shots between", "without them")],
        ),
    ];
    for (breaks, comparisons) in rows {
        let check = |shots: ShotSet| {
            let make = || FakeExecutor::breaking(breaks);
            changes_no_shot_outside_its_batches(
                make,
                needs(),
                &shots,
                &first(),
                &second(),
                trials(),
            )
        };
        let (sorted, unsorted) = (check(shots(0..4)), check(shots([0, 2, 1, 3])));
        assert_eq!(unsorted, sorted, "{breaks:?}");
        shows_each(Rule::ChangesNoShotOutsideItsBatches, sorted, comparisons);
    }
}

/// An executor that keeps each shot's state at its row in the batch gives
/// a shot the state of another, run in an earlier `execute`, at the same
/// row.
#[test]
fn a_shot_given_the_state_of_one_in_another_execute_is_reported() {
    let make = || FakeExecutor::breaking(Breaks::KeysShotsByRow);
    shows_each(
        Rule::ChangesNoShotOutsideItsBatches,
        changes_no_shot_outside_its_batches(
            make,
            needs(),
            &shots(0..4),
            &first(),
            &second(),
            trials(),
        ),
        &[("after executes on the other shots", "alone")],
    );
}

/// With three shots, the shots at odd positions are one shot, so an
/// `execute` changing the shots between its own would not show.
#[test]
fn fewer_than_four_shots_cannot_judge_changes_outside_batches() {
    let result = changes_no_shot_outside_its_batches(
        FakeExecutor::new,
        needs(),
        &shots(0..3),
        &first(),
        &second(),
        trials(),
    );
    assert!(why_cannot_judge(result).contains("four or more"));
}

/// A `first` that tosses a bit shows a change outside the batches as its
/// tosses fall: on shots 8 to 11, with the kit's seed, on one side of the
/// split and not on the other. A change on the other side would not show,
/// so the check cannot judge the part.
#[test]
fn a_change_shown_on_one_side_only_cannot_judge_changes_outside_batches() {
    let (first, second) = (gates(&[("h", 0)]), gates(&[("mz", 0)]));
    let result = changes_no_shot_outside_its_batches(
        FakeExecutor::new,
        needs(),
        &shots(8..12),
        &first,
        &second,
        trials(),
    );
    assert!(why_cannot_judge(result).contains("on some shot"));
}

/// Executors sharing a device that keep the shots they ended give a shot,
/// at its first batch on a later executor, the state the earlier one left.
#[test]
fn a_shot_kept_past_its_end_by_an_earlier_executor_is_reported() {
    let device = Device::default();
    let make = || FakeExecutor::on_device_breaking(&device, Breaks::KeepsEndedShots);
    shows_each(
        Rule::StartsEachShotAfresh,
        starts_each_shot_afresh(make, needs(), &shots(0..4), &first(), &second(), trials()),
        &[("after an earlier part ran `first` on it", "before")],
    );
}

/// A `second` that reads nothing `first` changes would not show a shot
/// left changed, so the check cannot judge a replayable part: whether
/// `first` reads nothing, or reads what it changes, which the run of both
/// reads first. On a part that is not replayable, that run sends the
/// shots otherwise, so the check does not judge whether `second` shows a
/// shot left changed: it passes the part, and says so on stderr.
#[test]
fn a_second_not_reading_first_cannot_judge_starting_shots_if_replayable() {
    let second = gates(&[("mz", 0)]);
    let unjudged: &[&str] = &["whether its patterns can show StartsEachShotAfresh broken"];
    for first in [gates(&[("x", 2)]), gates(&[("x", 2), ("mz", 2)])] {
        for (make, not_judged) in [
            (FakeExecutor::new as fn() -> FakeExecutor, &[][..]),
            (FakeExecutor::drawing_shots_together, unjudged),
        ] {
            take_unexercised();
            let result =
                starts_each_shot_afresh(make, needs(), &shots(0..4), &first, &second, trials());
            if not_judged.is_empty() {
                assert!(
                    why_cannot_judge(result).contains("reads the same after `first` as alone"),
                    "{first:?}"
                );
            } else {
                assert_eq!(result, Ok(()), "{first:?}");
            }
            let notes = take_unexercised();
            assert_eq!(
                not_judged_as_not_replayable(&notes),
                not_judged,
                "{first:?}"
            );
        }
    }
}

/// Executors sharing a device that keeps the shots an executor never
/// ended give a shot, at its first batch on a later executor, the state
/// the earlier one left: reported at each row of the run after it.
#[test]
fn a_shot_left_unended_by_an_earlier_executor_is_reported() {
    let device = Device::default();
    let make = || FakeExecutor::on_device(&device);
    let row = |shot| Place::Row {
        execute: 0,
        batch: 0,
        row: shot as usize,
        shot: ShotId::new(shot),
    };
    assert_eq!(
        faults_of(
            Rule::StartsEachShotAfresh,
            starts_each_shot_afresh(make, needs(), &shots(0..4), &first(), &second(), trials()),
        ),
        [row(0), row(1), row(2), row(3)]
    );
}

/// An executor that works when called, before its future is polled, has
/// done the work of a call its caller dropped: whether it then does the
/// next call's work too, reported first as the check's example says, or
/// answers it with the dropped call's.
#[test]
fn work_done_before_the_future_is_polled_is_reported() {
    for (breaks, first_fault) in [
        (
            Breaks::WorksWhenCalled,
            Some((
                Place::Row {
                    execute: 0,
                    batch: 0,
                    row: 0,
                    shot: ShotId::new(0),
                },
                ["[10]", "[01]"],
            )),
        ),
        (Breaks::WorksWhenCalledAndReuses, None),
    ] {
        let make = || FakeExecutor::breaking(breaks);
        let faults = faults_found(
            Rule::DoesNothingUntilPolled,
            does_nothing_until_polled(make, needs(), &script(), trials()),
        );
        let (place, found) = faults.first().expect("a fault");
        if let Some((expected, readings)) = first_fault {
            assert_eq!(*place, expected, "{breaks:?}: {found}");
            for reading in readings {
                assert!(
                    found.contains(reading),
                    "{breaks:?}: the fault names both readings: {found}"
                );
            }
        }
    }
}

/// The check can judge polling only with a script whose `execute`s, each
/// sent twice, read otherwise the second time: one that only reads a
/// fresh qubit reads the same, so it would pass an executor that works
/// when called, and the check cannot judge a replayable part; one that
/// flips the qubit before reading it can, and a conforming executor
/// passes. On a part that is not replayable, the run sending each
/// `execute` twice sends the shots otherwise, so the check does not judge
/// whether the script can show dropped work: it passes the part, and says
/// so on stderr.
#[test]
fn a_script_judges_polling_only_if_an_execute_sent_twice_reads_otherwise_if_replayable() {
    let unjudged: &[&str] = &["whether its patterns can show DoesNothingUntilPolled broken"];
    let cases = [
        (
            gates(&[("mz", 0)]),
            FakeExecutor::new as fn() -> FakeExecutor,
            false,
            &[][..],
        ),
        (gates(&[("x", 0), ("mz", 0)]), FakeExecutor::new, true, &[]),
        (
            gates(&[("mz", 0)]),
            FakeExecutor::drawing_shots_together,
            true,
            unjudged,
        ),
    ];
    for (instructions, make, judges, not_judged) in cases {
        let script = Script::new().execute([(shots(0..2), instructions)]);
        take_unexercised();
        let result = does_nothing_until_polled(make, needs(), &script, trials());
        if judges {
            assert_eq!(result, Ok(()), "{script:?}");
        } else {
            let why = why_cannot_judge(result);
            assert!(why.contains("reads the same run twice as once"), "{why}");
        }
        let notes = take_unexercised();
        assert_eq!(
            not_judged_as_not_replayable(&notes),
            not_judged,
            "{script:?}"
        );
    }
}

/// An executor that offers fewer qubits, or accepts more instructions,
/// once ready is reported once ready and at each `execute`.
#[test]
fn an_offer_or_set_changed_once_composed_is_reported_each_time_it_is_read() {
    for breaks in [Breaks::ChangesItsOffer, Breaks::ChangesWhatItAccepts] {
        assert_eq!(
            faults_of(
                Rule::KeepsItsAcceptsAndOffers,
                keeps_its_accepts_and_offers(
                    || FakeExecutor::breaking(breaks),
                    needs(),
                    &script(),
                    trials()
                )
            ),
            [
                Place::Ready,
                Place::Execute { execute: 0 },
                Place::Execute { execute: 1 },
            ],
            "{breaks:?}"
        );
    }
}

/// An executor that refuses every `execute` once it ended a shot fails
/// the run after the failed one, which the report says.
#[test]
fn an_executor_failing_after_a_failed_run_is_reported_at_that_run() {
    let make = || FakeExecutor::breaking(Breaks::RefusesAfterEndingShots);
    let faults = faults_found(
        Rule::RunsAsNewAfterAFailedRun,
        runs_as_new_after_a_failed_run(make, needs(), &script(), trials()),
    );
    assert!(
        matches!(
            faults.as_slice(),
            [(Place::Run { run: 2 }, found)]
                if found.starts_with("fails: run failed:") && found.contains("shot 0 is not held")
        ),
        "{faults:?}"
    );
}

/// A run a part refused as busy, as a device shared with other users may,
/// which the protocol allows, ran nothing: the check cannot judge the
/// part, names the refusal, and says to run the check again once the part
/// is ready, whether the part made for the two runs refuses run 1,
/// the one the kit's driver fails, or run 2.
#[test]
fn a_refused_run_cannot_judge_running_again_after_a_failed_run() {
    let built = Cell::new(0);
    let busy_after_the_first_part = || {
        built.set(built.get() + 1);
        if built.get() == 1 {
            FakeExecutor::new()
        } else {
            FakeExecutor::busy_at_run(1)
        }
    };
    let busy_at_the_second = || FakeExecutor::busy_at_run(2);
    for (refused, result) in [
        (
            "run 1",
            runs_as_new_after_a_failed_run(busy_after_the_first_part, needs(), &script(), trials()),
        ),
        (
            "run 2",
            runs_as_new_after_a_failed_run(busy_at_the_second, needs(), &script(), trials()),
        ),
    ] {
        let why = why_cannot_judge(result);
        assert!(
            why.contains("FakeExecutor")
                && why.contains("is not ready: busy")
                && why.ends_with("run the check again when the part is ready"),
            "{refused}: {why}"
        );
    }
}

/// An executor that keeps a shot's state past its end starts the shot,
/// in a later run on the same ids, where the earlier run left it.
#[test]
fn shots_kept_past_their_end_are_reported_at_the_rows_of_the_run_after() {
    let make = || FakeExecutor::breaking(Breaks::KeepsEndedShots);
    let result = runs_as_new_after_a_failed_run(make, needs(), &script(), trials());
    shows_each(
        Rule::RunsAsNewAfterAFailedRun,
        result.clone(),
        &[("after a failed run", "on a new part")],
    );
    let faults = faults_of(Rule::RunsAsNewAfterAFailedRun, result);
    assert!(
        faults
            .iter()
            .all(|place| matches!(place, Place::Row { .. })),
        "{faults:?}"
    );
}

/// A failed run whose `execute`s up to the failure read nothing shows
/// nothing of what the failure left, so the check cannot judge the part:
/// trial 0 fails the run after the first `execute`.
#[test]
fn a_first_execute_reading_nothing_cannot_judge_running_again() {
    let script = Script::new()
        .execute([(shots(0..2), gates(&[("h", 0)]))])
        .execute([(shots(0..2), gates(&[("mz", 0)]))]);
    let why = why_cannot_judge(runs_as_new_after_a_failed_run(
        FakeExecutor::new,
        needs(),
        &script,
        trials(),
    ));
    assert!(
        why.contains("up to the one the run fails after read no readout"),
        "{why}"
    );
}

/// An executor that panics when dropped holding shots keeps the protocol,
/// whose runner ends every shot it runs; the check that drops it holding
/// shots, as a crash leaves it, cannot judge it, and says why.
#[test]
fn a_part_that_panics_when_dropped_holding_shots_cannot_judge_starting_shots() {
    let result = starts_each_shot_afresh(
        FakeExecutor::strict,
        needs(),
        &shots(0..4),
        &first(),
        &second(),
        trials(),
    );
    let why = why_cannot_judge(result);
    assert!(why.contains("panicked when dropped"), "{why}");
    assert!(why.contains("shots never ended"), "{why}");
}

/// A rule found broken before a run fails is reported, not lost behind
/// the failed run: the report says what was found broken on the way.
#[test]
fn a_rule_broken_before_a_failed_run_is_reported() {
    let built = Cell::new(0);
    let make = || {
        built.set(built.get() + 1);
        if built.get() == 1 {
            FakeExecutor::breaking(Breaks::ReplacesItsReadouts)
        } else {
            FakeExecutor::refusing_every_execute()
        }
    };
    assert_eq!(
        faults_of(
            Rule::KeepsEachBatchsShape,
            gives_each_shot_what_it_gives_alone(make, needs(), &script(), trials())
        ),
        [
            Place::Batch {
                execute: 0,
                batch: 0
            },
            Place::Batch {
                execute: 1,
                batch: 0
            },
        ]
    );
}
