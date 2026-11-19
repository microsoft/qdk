// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of [`Trials`]: the drawn trials find a part fitted to the fixed
//! patterns, under the rule it breaks alone; a report of a drawn trial
//! names it and replays to the same report; and the trials a test's
//! environment sets are read as written.

use std::cell::Cell;
use std::rc::Rc;

use super::fake_adapter::{self, FakeAdapter};
use super::fake_driver::{self, FakeDriver, Record};
use super::fake_executor::{Breaks, FakeExecutor};
use super::{first, gates, needs, script, second, shots};
use crate::executor::{
    changes_no_shot_outside_its_batches, continues_each_shot_across_executes,
    gives_each_shot_what_it_gives_alone, runs_as_new_after_a_failed_run, starts_each_shot_afresh,
    writes_every_readout,
};
use crate::{Nonconformance, Rule, Script, Trial, Trials, adapter, driver};

/// A part built by iterating against the kit's fixed runs, all on shots 0
/// to 3, passes them; the drawn trials run other ids, and report it. They
/// also run the ids from 0 up, as most callers number their shots, so a
/// fault keyed to a small id shows in them too, whatever the seed: spread
/// ids alone would reach shot 1 with a few seeds only.
#[test]
fn drawn_trials_report_faults_on_one_small_id_or_beyond_the_fixed_ids() {
    let fitted = || FakeExecutor::breaking(Breaks::LeavesSlotsOfShotsAboveThreeUnwritten);
    writes_every_readout(
        fitted,
        needs(),
        &script(),
        Trials::new(Trials::DEFAULT_SEED, 0),
    )
    .expect("trial 0 runs shots 0 to 3 alone, which the part writes");
    for seed in [Trials::DEFAULT_SEED, 1, 2, 3, 4, 5, 6, 7] {
        for breaks in [
            Breaks::LeavesSlotsOfShotsAboveThreeUnwritten,
            Breaks::LeavesASlotUnwritten,
        ] {
            let drawn = Trials::drawn_only(seed, Trials::DEFAULT_COUNT);
            let found =
                writes_every_readout(|| FakeExecutor::breaking(breaks), needs(), &script(), drawn)
                    .expect_err("a drawn trial runs a shot the fault is keyed to");
            let Nonconformance::Broken { rules, .. } = &found else {
                panic!("expected broken rules for {breaks:?}, seed {seed}, got {found:?}");
            };
            assert_eq!(
                rules.iter().map(|rule| rule.rule).collect::<Vec<_>>(),
                [Rule::WritesEveryReadout],
                "{breaks:?}, seed {seed}"
            );
        }
    }
}

/// A part keeping its shots in a table sized for small ids, a `u64` bit
/// mask or an array of 4,096, or storing ids as `u16`, passes runs on ids
/// below that size, as most callers give; the drawn trials give large ids
/// too, and ids that agree on their low 16 bits, and report it, whatever
/// the seed.
#[test]
fn drawn_trials_report_parts_fitted_to_small_ids() {
    for breaks in [
        Breaks::RestartsShotsFrom(64),
        Breaks::RestartsShotsFrom(4096),
        Breaks::KeysShotsByTheirLow16Bits,
    ] {
        let fitted = || FakeExecutor::breaking(breaks);
        let run = |trials| {
            continues_each_shot_across_executes(
                fitted,
                needs(),
                &shots(0..4),
                &first(),
                &second(),
                trials,
            )
        };
        run(Trials::new(Trials::DEFAULT_SEED, 0)).expect("trial 0 runs shots 0 to 3");
        for seed in [Trials::DEFAULT_SEED, 1, 2, 3, 4, 5, 6, 7] {
            let found = run(Trials::drawn_only(seed, Trials::DEFAULT_COUNT))
                .expect_err("a drawn trial gives an id the part is not fitted to");
            let Nonconformance::Broken { rules, .. } = &found else {
                panic!("expected broken rules for {breaks:?}, seed {seed}: {found:?}");
            };
            assert_eq!(
                rules.iter().map(|rule| rule.rule).collect::<Vec<_>>(),
                [Rule::ContinuesEachShotAcrossExecutes],
                "{breaks:?}, seed {seed}"
            );
        }
    }
}

/// A check's run, on a part fitted to one of its fixed patterns, in the
/// trials given.
type FittedRun = Box<dyn Fn(Trials) -> Result<(), Nonconformance>>;

/// A part built by iterating against the kit's trial 0 fits each pattern
/// trial 0 fixes: which shots the check splits, the seed of its runs,
/// which kinds it raises together, which `execute` a failed run fails
/// after, how it cuts batches, the order of an `execute`'s batches, which
/// ids it runs, how many `execute`s it sends, and the order of the shots.
/// Each part below passes trial 0 of the check named, and breaks its rule
/// in a pattern only the drawn trials run, which report it.
#[test]
fn drawn_trials_report_parts_fitted_to_each_fixed_pattern() {
    for (part, run, rule) in fitted_parts() {
        run(Trials::new(Trials::DEFAULT_SEED, 0))
            .unwrap_or_else(|found| panic!("{part}: trial 0 passes it, got {found}"));
        let found = run(Trials::default()).expect_err(part);
        let Nonconformance::Broken { rules, trial, .. } = &found else {
            panic!("{part}: expected broken rules, got {found:?}");
        };
        assert!(!trial.is_fixed(), "{part}: found in {trial}");
        assert_eq!(
            rules.iter().map(|broken| broken.rule).collect::<Vec<_>>(),
            [rule],
            "{part}"
        );
    }
}

/// The parts of `drawn_trials_report_parts_fitted_to_each_fixed_pattern`:
/// each one's fault and the pattern it is fitted to, its check's run, and
/// the rule it breaks.
fn fitted_parts() -> [(&'static str, FittedRun, Rule); 9] {
    let first_seed = || Rc::new(Cell::new(None));
    [
        (
            "an execute of one shot drops the others: groups of one shot",
            Box::new(|trials| {
                let make = || FakeExecutor::breaking(Breaks::DropsItsShotsOnAnExecuteOfOneShot);
                changes_no_shot_outside_its_batches(
                    make,
                    needs(),
                    &shots(0..4),
                    &first(),
                    &second(),
                    trials,
                )
            }),
            Rule::ChangesNoShotOutsideItsBatches,
        ),
        (
            "the seed of the first execute kept: another run seed",
            Box::new(move |trials| {
                let kept = first_seed();
                let make = || FakeAdapter::keeping_its_first_seed(&kept);
                adapter::passes_the_seed_on(make, needs(), &script(), trials)
            }),
            Rule::PassesTheSeedOn,
        ),
        (
            "two kinds read together, one kept: two kinds raised on a row",
            Box::new(|trials| {
                let make = || FakeAdapter::breaking(fake_adapter::Breaks::KeepsOneKindUnlessEvery);
                adapter::passes_each_shots_conditions_up(make, needs(), &script(), trials)
            }),
            Rule::PassesEachShotsConditionsUp,
        ),
        (
            "continued shots kept past their end: a run failing later",
            Box::new(|trials| {
                let make = || FakeExecutor::breaking(Breaks::KeepsContinuedShotsPastTheirEnd);
                runs_as_new_after_a_failed_run(make, needs(), &script(), trials)
            }),
            Rule::RunsAsNewAfterAFailedRun,
        ),
        (
            "later batches share their draws: an execute cut into batches",
            Box::new(|trials| {
                let make = || FakeExecutor::breaking(Breaks::SharesItsDrawsBeyondTheFirstBatch);
                let uncut = Script::new().execute([(
                    shots(0..4),
                    gates(&[("h", 0), ("x", 1), ("mz", 0), ("mz", 1)]),
                )]);
                gives_each_shot_what_it_gives_alone(make, needs(), &uncut, trials)
            }),
            Rule::GivesEachShotWhatItGivesAlone,
        ),
        (
            "rows sized by the first batch: an execute's batches in another order",
            Box::new(|trials| {
                let make = || FakeExecutor::breaking(Breaks::SizesItsRowsByTheFirstBatch);
                writes_every_readout(make, needs(), &wide_first(), trials)
            }),
            Rule::WritesEveryReadout,
        ),
        (
            "shots above 3 restarted: ids beyond the fixed ones",
            Box::new(|trials| {
                let make = || FakeExecutor::breaking(Breaks::RestartsShotsFrom(4));
                continues_each_shot_across_executes(
                    make,
                    needs(),
                    &shots(0..4),
                    &first(),
                    &second(),
                    trials,
                )
            }),
            Rule::ContinuesEachShotAcrossExecutes,
        ),
        (
            "conditions carried from the second execute on: an execute split in two",
            Box::new(|trials| {
                let make = || {
                    FakeAdapter::breaking(
                        fake_adapter::Breaks::CarriesConditionsFromItsSecondExecute,
                    )
                };
                adapter::reports_only_each_executes_conditions(make, needs(), &script(), trials)
            }),
            Rule::ReportsOnlyEachExecutesConditions,
        ),
        (
            "conditions merged in the order of the ids: shots out of the ids' order",
            Box::new(|trials| {
                let make = || FakeDriver::breaking(fake_driver::Breaks::MergesConditionsInIdOrder);
                let conditions_of = |record: &Record| record.conditions;
                driver::merges_each_executes_conditions(make, &shots(0..4), conditions_of, trials)
            }),
            Rule::MergesEachExecutesConditions,
        ),
    ]
}

/// A script of one `execute` whose first batch reads more slots than its
/// second, which a part sizing its rows by the first batch fits.
fn wide_first() -> Script {
    Script::new().execute([
        (shots(0..2), gates(&[("x", 0), ("mz", 0), ("mz", 1)])),
        (shots(2..4), gates(&[("x", 0), ("mz", 0)])),
    ])
}

/// A test given `Trials::from_env()` reads its environment: a seed copied
/// from a report or from the kit's source, in hexadecimal or in decimal,
/// spaces around the value and each `_` in it ignored, and a count of
/// trials; each unset one is the default's. A value it cannot read, a
/// signed one included, stops the test, naming the variable and the value
/// as set, rather than running other trials than the implementer asked.
#[test]
fn the_environments_trials_are_read_as_the_variables_write_them() {
    let from = |seed: Option<&str>, count: Option<&str>| {
        let (seed, count) = (seed.map(str::to_owned), count.map(str::to_owned));
        let read = std::panic::AssertUnwindSafe(move || {
            Trials::from_settings(|variable| match variable {
                Trials::SEED_VARIABLE => seed.clone(),
                Trials::COUNT_VARIABLE => count.clone(),
                other => panic!("reads {other}"),
            })
        });
        std::panic::catch_unwind(read).map_err(|panic| {
            panic
                .downcast_ref::<String>()
                .cloned()
                .expect("a formatted panic")
        })
    };
    let (seed, count) = (Trials::DEFAULT_SEED, Trials::DEFAULT_COUNT);
    let read = [
        (None, None, Trials::default()),
        (Some("42"), Some("7"), Trials::new(42, 7)),
        (Some("0x10"), None, Trials::new(16, count)),
        (Some("0X1f"), None, Trials::new(31, count)),
        (
            Some(" 0x0C0F_FEE5_EED0_0002 "),
            Some("1_000"),
            Trials::new(seed, 1000),
        ),
        (Some("_0x_1_0_"), None, Trials::new(16, count)),
    ];
    for (seed, count, expected) in read {
        assert_eq!(from(seed, count), Ok(expected), "{seed:?}, {count:?}");
    }
    let refused = [
        (
            Some("0xg"),
            None,
            "EXECUTOR_PROTOCOL_CONFORMANCE_SEED is `0xg`",
        ),
        (
            Some(" 1_x "),
            None,
            "EXECUTOR_PROTOCOL_CONFORMANCE_SEED is ` 1_x `",
        ),
        (
            Some("+5"),
            None,
            "EXECUTOR_PROTOCOL_CONFORMANCE_SEED is `+5`",
        ),
        (
            None,
            Some("0x+a"),
            "EXECUTOR_PROTOCOL_CONFORMANCE_TRIALS is `0x+a`",
        ),
        (
            None,
            Some("ten"),
            "EXECUTOR_PROTOCOL_CONFORMANCE_TRIALS is `ten`",
        ),
        (
            None,
            Some("0x1_0000_0000"),
            "EXECUTOR_PROTOCOL_CONFORMANCE_TRIALS is `0x1_0000_0000`, more trials than a u32",
        ),
    ];
    for (seed, count, expected) in refused {
        let why = from(seed, count).expect_err("an unreadable value");
        assert!(why.starts_with(expected), "{why}");
    }
}

/// An implementer's CI reports a part broken in a drawn trial; the
/// implementer reruns that trial alone, from the report, to debug it, and
/// sees the same faults at the same places: whether the check draws a
/// script, or groups and cuts of the shots it is given.
#[test]
fn a_drawn_trials_report_replays_to_the_same_report() {
    let rows: [(FittedRun, Rule); 2] = [
        (
            Box::new(|trials| {
                let make = || FakeExecutor::breaking(Breaks::LeavesASlotUnwritten);
                writes_every_readout(make, needs(), &script(), trials)
            }),
            Rule::WritesEveryReadout,
        ),
        (
            Box::new(|trials| {
                let make = || FakeExecutor::breaking(Breaks::RunsHeldShotsBetweenItsOwn);
                changes_no_shot_outside_its_batches(
                    make,
                    needs(),
                    &shots(0..4),
                    &first(),
                    &second(),
                    trials,
                )
            }),
            Rule::ChangesNoShotOutsideItsBatches,
        ),
    ];
    for (run, rule) in rows {
        let found = run(Trials::drawn_only(7, 4)).expect_err("a drawn trial finds the fault");
        let Nonconformance::Broken { trial, .. } = &found else {
            panic!("expected broken rules, got {found:?}");
        };
        let Trial { seed: 7, index } = *trial else {
            panic!("expected a trial of seed 7, got {trial:?}");
        };
        assert_ne!(index, 0, "only drawn trials ran");
        let report = found.to_string();
        let names = [
            format!("{trial}"),
            rule.to_string(),
            format!("Trials::replaying(Trial {{ seed: 0x7, index: {index} }})"),
            format!("{}=0x7", Trials::SEED_VARIABLE),
            format!("{}={index}", Trials::COUNT_VARIABLE),
        ];
        for name in names {
            assert!(
                report.contains(&name),
                "the report names its trial, its rule and how to replay it ({name}): {report}"
            );
        }
        let replayed = run(Trials::replaying(*trial)).expect_err("the replayed trial finds it");
        assert_eq!(replayed.to_string(), report);
    }
}

/// A part whose shots depend on the batch they are made in, as a device
/// seeding each of its queues apart would, breaks one rule: a shot's
/// records depend only on the seed and the shot's id. The drawn trials cut
/// the shots into batches; each check of a shot's state makes each shot in
/// the same batch in every run it compares, so it passes the part, and the
/// implementer is pointed at the rule the part breaks alone.
#[test]
fn a_part_whose_shots_depend_on_the_batch_they_are_made_in_is_reported_under_that_rule_alone() {
    let make = || FakeExecutor::breaking(Breaks::SeedsNewShotsByBatch);
    let found = gives_each_shot_what_it_gives_alone(make, needs(), &script(), Trials::default())
        .expect_err("a drawn trial cuts a batch the part seeds apart");
    let Nonconformance::Broken { rules, .. } = &found else {
        panic!("expected broken rules, got {found:?}");
    };
    assert_eq!(
        rules.iter().map(|broken| broken.rule).collect::<Vec<_>>(),
        [Rule::GivesEachShotWhatItGivesAlone]
    );
    // `second()` draws nothing; this one draws on each shot, so that a shot
    // made by `second` alone shows the batch it sits in.
    let drawing = gates(&[("mz", 1), ("h", 0), ("mz", 0)]);
    let (all, first) = (shots(0..4), first());
    for second in [second(), drawing] {
        let passed = [
            continues_each_shot_across_executes(
                make,
                needs(),
                &all,
                &first,
                &second,
                Trials::default(),
            ),
            changes_no_shot_outside_its_batches(
                make,
                needs(),
                &all,
                &first,
                &second,
                Trials::default(),
            ),
            starts_each_shot_afresh(make, needs(), &all, &first, &second, Trials::default()),
        ];
        for result in passed {
            assert!(result.is_ok(), "{result:?}");
        }
    }
}

/// A part whose shots draw by the batch they sit in at a later `execute`,
/// as one handling a lone batch apart would, gives a shot other draws when
/// its `execute` is cut otherwise: the check that each shot continues
/// across `execute`s sends `second` to two groups as two batches, by
/// design, and reports it under its rule. Trial 0 sends every other
/// `execute` in one batch, so only that run can show it there.
#[test]
fn a_part_whose_later_draws_depend_on_their_batch_is_reported_as_not_continuing_its_shots() {
    let make = || FakeExecutor::breaking(Breaks::SharesItsDrawsBeyondTheFirstBatch);
    let drawing = gates(&[("mz", 1), ("h", 0), ("mz", 0)]);
    let found = continues_each_shot_across_executes(
        make,
        needs(),
        &shots(0..8),
        &first(),
        &drawing,
        Trials::new(Trials::DEFAULT_SEED, 0),
    )
    .expect_err("trial 0 sends `second` to the odd shots in a batch of their own");
    let Nonconformance::Broken { rules, .. } = &found else {
        panic!("expected broken rules, got {found:?}");
    };
    assert_eq!(
        rules.iter().map(|broken| broken.rule).collect::<Vec<_>>(),
        [Rule::ContinuesEachShotAcrossExecutes]
    );
}
