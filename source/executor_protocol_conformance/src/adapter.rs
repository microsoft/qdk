// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rules an adapter keeps, which the protocol states but does not
//! check as it runs, each checked by a function of its own, except two
//! that every check reports (below), and putting each shot in one batch of
//! a call, which [`sends_only_shots_it_got`] checks. An adapter answers the
//! part above it as an executor does, so it keeps an executor's rules,
//! which the functions here of the same names as in [`executor`] check on
//! it: all an executor's, except that its `accepts` and `offers` stay as
//! composed, which the crate's `docs/limits.md` lists among what the kit
//! does not check. And it keeps those of a caller of the part below. It
//! sends only the shots it got, each in at most one batch of a call, with
//! the seed it got, stating each batch's readout slots as its instructions
//! count them (the protocol checks the count only for a checked batch). It
//! sends batches checked when the batch it got was. And it reports up,
//! besides its own, the conditions read below on each shot in the same
//! `execute`, and none of another shot's or of an earlier `execute` (the
//! crate's `docs/limits.md` lists what the checks do not see).
//!
//! Every check also reports, in each of its runs that ends (and in the run
//! that [`runs_as_new_after_a_failed_run`] fails on purpose), each batch
//! the adapter answered with readouts of another shape
//! ([`Rule::KeepsEachBatchsShape`]), and each `end_shots` that panicked
//! ending shots among which some the adapter did not hold
//! ([`Rule::IgnoresEndsOfShotsItDoesNotHold`], which says why a held
//! shot's panic there is reported too). Another run that fails ends the
//! check, as `docs/checks.md` says under "Reading a report".
//!
//! Each function takes `make`, which builds the adapter afresh, the same
//! each time; `needs`, what the kit's driver above it needs of it; and what
//! the kit's driver sends, as for an executor's checks. Each run is on a new
//! adapter, except in [`runs_as_new_after_a_failed_run`], whose rule is
//! about an adapter's later runs. Below the adapter,
//! the kit's executor answers each batch with values drawn from each
//! shot's seed, raises the conditions the check chooses, and logs each
//! batch.
//!
//! # The trials
//!
//! Each check runs trial 0 on the script it is given, then each drawn
//! trial on it redrawn, as for an executor's checks (the
//! [`executor`](crate::executor#the-trials) module says how). The checks of
//! conditions also draw, in each drawn trial, the shots raised on and the
//! kinds raised: one kind or more for each `execute`, call below and shot.
//!
//! An adapter whose offer above the kit's executor
//! ([`AdapterSetup::offers_above`](executor_protocol::AdapterSetup::offers_above))
//! is not replayable is checked as the
//! [`executor`](crate::executor#parts-that-are-not-replayable) module says
//! of such a part: no check compares the records of runs that send the
//! shots otherwise.

use std::collections::{BTreeMap, BTreeSet};

use executor_protocol::{Adapt, Instruction, Needs, ShotConditions, ShotId, ShotSet};

use crate::caller_faults::{misstates_slots, note_misstated, repeated_shots, sends_something};
use crate::chain::{AdapterMount, Chain, noted_run};
use crate::executor;
use crate::nonconformance::{Nonconformance, Place, shot_list};
use crate::rule::Rule;
use crate::script::Script;
use crate::shared_run::{RAISED_IN_TURN, Raise, Raised, RowRecord, RunLog, RunSetup, every_kind};
use crate::trial_patterns::{TrialPatterns, run_trials};
use crate::trials::Trials;

/// Checks that the adapter sends below only shots of the batches it got in
/// the same `execute` ([`Rule::SendsOnlyShotsItGot`]), and each in at most
/// one batch of a call below ([`Rule::PutsEachShotInOneBatchPerCall`]).
///
/// # Why it matters
///
/// The part below holds a shot's state from its first batch to its end,
/// which the runner sends only for the run's shots. A shot the `execute` did
/// not name, sent below, is made there and never ended, or changes a shot
/// held for a later round; a shot in two batches of one call gets its
/// instructions in an order the part below picks.
///
/// # How it checks
///
/// Each trial runs its script once and reads each batch the adapter sends
/// below. Trial 0 runs `script`; each drawn trial runs it redrawn (the
/// module's [trials](crate::adapter#the-trials)).
///
/// # Example
///
/// An adapter that sends again, in an `execute`, shots of earlier ones is
/// reported as `execute 1, call 0 below, batch 1: holds shots 2, 3 that no
/// batch of the execute holds`; one that sends a batch below twice in one
/// call, as `execute 0, call 0 below, batch 1: holds shots 0, 1, 2, 3 that
/// an earlier batch of its call holds`.
///
/// # What it does not see
///
/// Shots the adapter adds only for instructions or sizes the script does
/// not hold.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each batch below that holds another
/// shot or a shot twice in its call. [`Nonconformance::CannotJudge`] when the
/// adapter sends nothing below for `script`, `script` does not suit it, or
/// the run fails.
pub fn sends_only_shots_it_got<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    run_trials(trials, |patterns| {
        let script = patterns.script(script);
        let mut findings = patterns.findings();
        let log = noted_run(&mut findings, &mut mount, RunSetup::of(script.clone()))?;
        sends_something(&mut findings, &log, "the adapter");
        for below in &log.below {
            let got: BTreeSet<_> = script
                .executes()
                .get(below.execute)
                .map_or_else(BTreeSet::new, |batches| {
                    batches.iter().flat_map(|(shots, _)| shots.iter()).collect()
                });
            let others: Vec<String> = below
                .shots
                .iter()
                .filter(|shot| !got.contains(shot))
                .map(ToString::to_string)
                .collect();
            if !others.is_empty() {
                findings.fault(
                    Rule::SendsOnlyShotsItGot,
                    below.place(),
                    format!(
                        "holds {} that no batch of the execute holds",
                        shot_list(&others)
                    ),
                );
            }
        }
        findings.faults(
            Rule::PutsEachShotInOneBatchPerCall,
            repeated_shots(&log.below),
        );
        findings.finish()
    })
}

/// Checks that the adapter passes the seed it got below unchanged
/// ([`Rule::PassesTheSeedOn`]).
///
/// # Why it matters
///
/// The part below draws a shot's random values from the seed and the shot's
/// id, so that a run can be replayed. An adapter that sends a seed of its
/// own, fixed or new for each `execute`, ties the records below to it: two
/// runs with one seed differ, or runs with different seeds agree.
///
/// # How it checks
///
/// Each trial runs its script once and compares the seed of each batch
/// below with the one its `execute` got. Trial 0 runs `script`; each drawn
/// trial runs it redrawn, with a seed of its own (the module's
/// [trials](crate::adapter#the-trials)).
///
/// # Example
///
/// An adapter that reseeds the part below is reported as `execute 0, call 0
/// below, batch 0: sent with another seed than its execute got`.
///
/// # What it does not see
///
/// How the adapter uses the seed itself: that its own draws depend only on
/// the seed and the shot is [`gives_each_shot_what_it_gives_alone`]'s rule.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each batch below sent with another
/// seed. [`Nonconformance::CannotJudge`] when the adapter sends nothing below
/// for `script`, `script` does not suit it, or the run fails.
pub fn passes_the_seed_on<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    run_trials(trials, |patterns| {
        let script = patterns.script(script);
        let mut findings = patterns.findings();
        let log = noted_run(&mut findings, &mut mount, RunSetup::of(script))?;
        sends_something(&mut findings, &log, "the adapter");
        for below in &log.below {
            // The runner gives every `execute` the run's seed, so the seeds
            // above are equal today; the lookup by `execute` keeps the check
            // right if a caller may give each its own.
            let above = log
                .seeds_above
                .iter()
                .find(|(execute, _)| *execute == below.execute)
                .map(|(_, seed)| *seed);
            if above != Some(below.seed) {
                findings.fault(
                    Rule::PassesTheSeedOn,
                    below.place(),
                    "sent with another seed than its execute got",
                );
            }
        }
        findings.finish()
    })
}

/// Checks that the adapter states each batch's readout slots below as its
/// instructions count them ([`Rule::StatesReadoutSlots`]).
///
/// # Why it matters
///
/// The protocol shapes the readouts below by the stated count, and checks
/// it only for a checked batch, refusing the run. Misstated in an unchecked
/// batch, the count gives readouts of another shape than the instructions
/// fill: the part below leaves slots unwritten, or has no slot for an
/// outcome, and the adapter reads values that belong to no instruction.
///
/// # How it checks
///
/// Two runs of the trial's script: one with unchecked batches, where the
/// kit's executor below compares each batch's stated count with its
/// instructions' (if the adapter sends them unchecked too); one with checked
/// batches, which the protocol refuses below if the adapter misstates a
/// checked batch's count. Trial 0 runs `script`; each drawn trial runs it
/// redrawn (the module's [trials](crate::adapter#the-trials)).
///
/// # Example
///
/// An adapter that states one slot more than its instructions count is
/// reported as `execute 0, call 0 below, batch 0: states 3 readout slots;
/// its instructions count 2`.
///
/// # What it does not see
///
/// A count misstated only for instructions the script does not hold.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each batch below whose count is
/// misstated, and each run refused for it. [`Nonconformance::CannotJudge`]
/// when the adapter sends nothing below for `script`, `script` does not
/// suit it, or a run fails otherwise.
pub fn states_readout_slots<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    run_trials(trials, |patterns| {
        let script = patterns.script(script);
        let mut findings = patterns.findings();
        let seed = findings.run_seed();
        for (run, unchecked) in [(1, true), (2, false)] {
            let setup = RunSetup {
                unchecked,
                ..RunSetup::of(script.clone())
            };
            let mut chain =
                Chain::new(&mut mount, setup.clone()).map_err(|why| findings.failed(why))?;
            let ran = chain.run(setup, seed);
            findings.note_run(&ran.log);
            if let Some(failure) = ran.failure {
                if !misstates_slots(&failure) {
                    return Err(findings.run_failed(&failure));
                }
                findings.fault(
                    Rule::StatesReadoutSlots,
                    Place::Run { run },
                    format!("refused below: {failure}"),
                );
                continue;
            }
            sends_something(&mut findings, &ran.log, "the adapter");
            note_misstated(&mut findings, &ran.log.below);
        }
        findings.finish()
    })
}

/// Checks that the adapter merges into each row the conditions read below
/// on its shot in the same `execute`, and no other shot's
/// ([`Rule::PassesEachShotsConditionsUp`]).
///
/// # Why it matters
///
/// The caller decides on a shot's conditions: it drops a `REJECTED` shot,
/// flags an `INCONSISTENT` one. A shot rejected below that reads as
/// accepted above passes a bad outcome as good; a shot accepted below that
/// reads as rejected, because the adapter gave it another shot's
/// conditions, loses a good outcome and biases the statistics.
///
/// # How it checks
///
/// The kit runs the trial's script once with no condition raised below,
/// then in runs that each raise conditions on half of the shots: for each
/// bit of a shot's position among them, the shots whose bit is set, then
/// the others (one run for one shot, two for two, four for three or
/// four, six for up to eight). So any two shots are apart in some run.
/// Each row of a shot raised on must hold every condition raised on it in
/// its `execute`'s calls below that sent it, and no condition beyond those
/// and the ones it reports in the first run. Each row of a shot not raised
/// on must report no condition it does not report in the first run, since
/// an adapter's own conditions on a shot depend on that shot alone (the
/// protocol's `docs/writing-a-part.md`, "The rules a part keeps"), and an
/// adapter raises its own conditions for what it finds in its readouts,
/// never because of a condition it read below ([`ShotConditions`]).
///
/// In trial 0, the kit's executor below raises on each raised shot, in
/// every call of every `execute`, one kind by the call: `REJECTED`,
/// `INCONSISTENT`, `UNRESOLVED`, then again, starting one kind later for
/// each next position; a run then raises every kind at once on every shot,
/// and a last one every kind on every shot in each `execute`'s first call
/// below only, so that its later calls read none. An adapter that sends
/// each `execute` below in one call is held to the same bounds there as in
/// the other runs. Each drawn trial runs the script redrawn, orders the
/// positions anew before taking the halves, and raises one kind or more
/// drawn for each `execute`, call and shot (the module's
/// [trials](crate::adapter#the-trials)). So an adapter that keeps only some
/// kinds, or some calls' conditions, or clears them when a call reads none,
/// gives a row another row's, in place of its own or added to them, or
/// raises a kind of its own because it read another below, shows. The
/// check reports each row once, in the order of the rows.
///
/// # Example
///
/// An adapter that sends a batch below in two calls and keeps only the last
/// call's conditions is reported as `execute 0, batch 0, row 0 (shot 0):
/// reports ShotConditions(INCONSISTENT), though ShotConditions(REJECTED |
/// INCONSISTENT) was read below in its execute's calls (raised on shots 0,
/// 2)`.
///
/// An adapter that flags as `INCONSISTENT` a row it read `REJECTED` below
/// is reported as `execute 0, batch 0, row 0 (shot 0): reports
/// ShotConditions(REJECTED | INCONSISTENT), though only
/// ShotConditions(REJECTED) was read below in its execute's calls (raised
/// on shots 0, 2); ShotConditions(NONE) when none were raised`.
///
/// # What it does not see
///
/// The adapter's own conditions are judged only against the first run's: a
/// condition it raises on a shot alike in every run passes. The run that
/// leaves later calls clean raises in each `execute`'s first call below
/// only, so an adapter that clears a shot's conditions at a clean call
/// only after several that read some, or whose first call does not send
/// the shot, is not seen clearing them.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each row that lacks a condition read
/// below on its shot, or reports another shot's or one raised because of
/// one read below. [`Nonconformance::CannotJudge`] when, in trial 0, the
/// adapter sends none of the shots raised on below, or when `script` does
/// not suit it, or a run fails.
pub fn passes_each_shots_conditions_up<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let rule = Rule::PassesEachShotsConditionsUp;
    let mut mount = AdapterMount { make, needs };
    run_trials(trials, |patterns| {
        let script = patterns.script(script);
        let shots = script.shots();
        let mut findings = patterns.findings();
        let clean = noted_run(&mut findings, &mut mount, RunSetup::of(script.clone()))?;
        // Each row's first fault, by the row's place in the run.
        let mut shown = BTreeMap::new();
        let mut raised_any = false;
        // The halves, raising kinds in turn (or drawn), then, in trial 0,
        // every shot, raising every kind at once in every call, then every
        // kind in each `execute`'s first call below only.
        let mut runs: Vec<(Vec<usize>, Option<Raised>)> = patterns
            .raising_halves(shots.len())
            .into_iter()
            .map(|half| (half, None))
            .collect();
        if patterns.is_fixed() {
            let every_shot: Vec<usize> = (0..shots.len()).collect();
            runs.push((every_shot.clone(), Some(Raised::InEveryCall(every_kind()))));
            runs.push((every_shot, Some(Raised::InFirstCall(every_kind()))));
        }
        for (half, fixed) in runs {
            let raised_on: Vec<ShotId> = half.iter().map(|&position| shots[position]).collect();
            let raising = RunSetup {
                raises: raises_in_every_execute(patterns, &script, &half, &raised_on, fixed),
                ..RunSetup::of(script.clone())
            };
            let log = noted_run(&mut findings, &mut mount, raising.clone())?;
            let mut on = raised_on
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            if let Some(Raised::InFirstCall(_)) = fixed {
                on.push_str(" in each execute's first call below only");
            }
            for row in &log.rows {
                let place = Place::Row {
                    execute: row.execute,
                    batch: row.batch,
                    row: row.row,
                    shot: row.shot,
                };
                let found = if raised_on.contains(&row.shot) {
                    let read_below = log
                        .below
                        .iter()
                        .filter(|below| {
                            below.execute == row.execute && below.shots.contains(&row.shot)
                        })
                        .fold(ShotConditions::NONE, |all, below| {
                            all | raising.raised(below.execute, below.call, row.shot)
                        });
                    raised_any |= !read_below.is_empty();
                    let bound = clean_conditions(&clean, row) | read_below;
                    if row.conditions.contains(read_below) {
                        (!bound.contains(row.conditions)).then(|| {
                            format!(
                                "reports {:?}, though only {read_below:?} was read below in \
                                 its execute's calls (raised on shots {on}); {:?} when none \
                                 were raised",
                                row.conditions,
                                clean_conditions(&clean, row)
                            )
                        })
                    } else {
                        Some(format!(
                            "reports {:?}, though {read_below:?} was read below in its \
                             execute's calls (raised on shots {on})",
                            row.conditions
                        ))
                    }
                } else {
                    let expected = clean_conditions(&clean, row);
                    (!expected.contains(row.conditions)).then(|| {
                        format!(
                            "reports {:?}, though conditions were raised below only on shots \
                             {on}, not on its own; {expected:?} when none were raised",
                            row.conditions
                        )
                    })
                };
                if let Some(found) = found {
                    shown
                        .entry((row.execute, row.batch, row.row))
                        .or_insert((place, found));
                }
            }
        }
        for (place, found) in shown.into_values() {
            findings.fault(rule, place, found);
        }
        if !raised_any {
            findings.undetectable(
                "the adapter sent below none of the shots the kit's executor raises conditions \
                 on",
            );
        }
        findings.finish()
    })
}

/// Checks that each row the adapter answers reports only the conditions of
/// its own `execute` ([`Rule::ReportsOnlyEachExecutesConditions`]).
///
/// # Why it matters
///
/// A caller reads each `execute`'s conditions as that round's. A driver
/// that repeats a round read `REJECTED` sees, if the adapter carries the
/// condition on, every later round rejected too, and repeats it without
/// end; and an adapter that raises a condition anew because of one it read
/// below earlier reports it twice, though the driver already holds it.
///
/// # How it checks
///
/// The kit runs the trial's script once with no condition raised below,
/// then, for each `execute` but the last, runs where the kit's executor
/// below raises conditions on every shot in that `execute` only. Each row
/// of a later `execute` must report no condition beyond those it reports in
/// the first run; it may report fewer, as an adapter that stops decoding a
/// rejected shot raises no condition of its own on it later. So the check
/// sees the conditions read below carried on from any `execute`, and those
/// the adapter raises anew on a shot because of a condition it read below
/// earlier, which the protocol forbids: an adapter raises its own
/// conditions for what it finds in its readouts, never because of a
/// condition it read below ([`ShotConditions`]).
///
/// In trial 0, two runs for each `execute` raised in: every kind
/// (`REJECTED`, `INCONSISTENT` and `UNRESOLVED`) at once, then one kind per
/// shot and call, neighbouring shots reading different kinds, so that an
/// adapter that carries a kind only when it read it without another (say
/// `INCONSISTENT` without `REJECTED`) is reported too. Each drawn trial runs
/// the script redrawn, with one run for each `execute` raised in, raising
/// one kind or more drawn for each shot and call (the module's
/// [trials](crate::adapter#the-trials)). The check reports each row once,
/// in the order of the rows.
///
/// # Example
///
/// An adapter that keeps the conditions it read below in a shot's state,
/// and reports them in each later row, is reported as `execute 1, batch 0,
/// row 0 (shot 0): reports ShotConditions(REJECTED | INCONSISTENT |
/// UNRESOLVED) after conditions were raised in execute 0 only,
/// ShotConditions(NONE) when none were`.
///
/// # What it does not see
///
/// A condition carried on only into an `execute` the script does not send.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each later row that reports more.
/// [`Nonconformance::CannotJudge`] when `script` sends fewer than two
/// `execute`s, when, in trial 0, no condition raised reaches a row of the
/// `execute` it is raised in (a kind the row holds when none are raised
/// is the adapter's own, and does not count), or when `script` does not
/// suit the adapter, or a run fails.
pub fn reports_only_each_executes_conditions<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    run_trials(trials, |patterns| {
        if script.executes().len() < 2 {
            return Err(patterns.cannot_judge(
                "the script sends fewer than two executes; the check needs two or more, to see \
                 conditions carried from one to the next",
            ));
        }
        let script = patterns.script(script);
        let executes = script.executes().len();
        let mut findings = patterns.findings();
        let clean = noted_run(&mut findings, &mut mount, RunSetup::of(script.clone()))?;
        let mut reached = false;
        // Each later row's first fault, by the row's place in the run.
        let mut shown = BTreeMap::new();
        // For each `execute` raised in, in trial 0 a run raising every kind
        // at once, then one raising a kind in turn, so that each shot reads
        // some kind alone; in a drawn trial, one run raising kinds drawn
        // for each shot and call.
        let fixed = patterns.is_fixed();
        let runs = (0..executes - 1).flat_map(|raised_in| {
            let ways: &[Option<bool>] = if fixed {
                &[Some(true), Some(false)]
            } else {
                &[None]
            };
            ways.iter().map(move |&way| (raised_in, way))
        });
        for (raised_in, way) in runs.collect::<Vec<_>>() {
            let mut raises = Vec::new();
            for (position, shot) in script.shots().into_iter().enumerate() {
                let conditions = match way {
                    Some(true) => Raised::InEveryCall(every_kind()),
                    Some(false) => Raised::InTurn { position },
                    None => patterns.raised(Raised::InTurn { position }),
                };
                raises.push(Raise {
                    execute: raised_in,
                    shot,
                    raised: conditions,
                });
            }
            let raising = RunSetup {
                raises,
                ..RunSetup::of(script.clone())
            };
            let log = noted_run(&mut findings, &mut mount, raising)?;
            // A kind the row holds in the clean run too is the adapter's own.
            reached |= log.rows.iter().any(|row| {
                let own = clean_conditions(&clean, row);
                row.execute == raised_in
                    && RAISED_IN_TURN
                        .into_iter()
                        .any(|kind| row.conditions.contains(kind) && !own.contains(kind))
            });
            for row in log.rows.iter().filter(|row| row.execute > raised_in) {
                let expected = clean_conditions(&clean, row);
                if !expected.contains(row.conditions) {
                    let place = Place::Row {
                        execute: row.execute,
                        batch: row.batch,
                        row: row.row,
                        shot: row.shot,
                    };
                    let found = format!(
                        "reports {:?} after conditions were raised in execute {raised_in} \
                         only, {expected:?} when none were",
                        row.conditions
                    );
                    shown
                        .entry((row.execute, row.batch, row.row))
                        .or_insert((place, found));
                }
            }
        }
        for (place, found) in shown.into_values() {
            findings.fault(Rule::ReportsOnlyEachExecutesConditions, place, found);
        }
        if !reached {
            findings.undetectable(
                "no row reports a condition raised below in its own execute, so carrying it \
                 on would not show; check `passes_each_shots_conditions_up` first",
            );
        }
        findings.finish()
    })
}

/// Checks that the adapter sends checked batches below for checked batches
/// ([`Rule::CarriesCheckedBatchesBelow`]).
///
/// # Why it matters
///
/// A caller that checks its batches relies on the protocol checking, below
/// too, each batch's stated readout slots against its instructions, and
/// refusing the run before the part below runs it. An adapter that sends
/// unchecked batches below drops that check, and a misstated count goes
/// unrefused.
///
/// # How it checks
///
/// Each trial runs its script, in checked batches, once, and reads each
/// batch the adapter sends below. Trial 0 runs `script`; each drawn trial
/// runs it redrawn (the module's [trials](crate::adapter#the-trials)).
///
/// # Example
///
/// An adapter that always sends unchecked batches is reported as `execute
/// 0, call 0 below, batch 0: sent unchecked, for checked batches`.
///
/// # What it does not see
///
/// Checked batches sent for unchecked ones: the protocol allows it.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each batch below sent unchecked.
/// [`Nonconformance::CannotJudge`] when the adapter sends nothing below for
/// `script`, `script` does not suit it, or the run fails.
pub fn carries_checked_batches_below<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    run_trials(trials, |patterns| {
        let script = patterns.script(script);
        let mut findings = patterns.findings();
        let log = noted_run(&mut findings, &mut mount, RunSetup::of(script))?;
        sends_something(&mut findings, &log, "the adapter");
        for below in log.below.iter().filter(|below| !below.checked) {
            findings.fault(
                Rule::CarriesCheckedBatchesBelow,
                below.place(),
                "sent unchecked, for checked batches",
            );
        }
        findings.finish()
    })
}

/// Checks that the adapter writes every readout value of each batch it
/// answers, as an executor does ([`Rule::WritesEveryReadout`]; the rule's
/// mechanics are [`executor::writes_every_readout`]'s).
///
/// # Why it matters
///
/// The parts above read an adapter's readouts as they would an executor's,
/// slot by slot. An adapter that writes slots of its own, such as one that
/// decodes a value from those read below, and leaves one unwritten hands
/// up whatever the buffer held, read above as a measurement.
///
/// # How it checks
///
/// As for an executor, with the kit's executor below, which writes every
/// slot it answers: a slot above that still holds the kit's value in every
/// run is one the adapter left.
///
/// # Example
///
/// An adapter that copies up what it read below but leaves the last slot
/// of a shot's rows unwritten, as one that forgets its own decoded value
/// would, is reported as `execute 0, batch 0, row 1 (shot 1), slot 1:
/// holds the value the kit wrote before the part ran, in each of 22 runs`.
///
/// # What it does not see
///
/// A slot the part below leaves unwritten and the adapter copies up: the
/// kit's executor writes every slot. And what the
/// [executor's check](executor::writes_every_readout#what-it-does-not-see)
/// does not see.
///
/// # Errors
///
/// As [`executor::writes_every_readout`].
pub fn writes_every_readout<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    executor::writes_every_readout_on(&mut mount, script, trials)
}

/// Checks that a shot's records, through the adapter, depend only on the
/// seed and the shot's id, as an executor's do
/// ([`Rule::GivesEachShotWhatItGivesAlone`]; the rule's mechanics are
/// [`executor::gives_each_shot_what_it_gives_alone`]'s), for an adapter
/// whose offer above the kit's executor is replayable.
///
/// # Why it matters
///
/// An adapter's own work on a shot, such as a noise adapter's draws, must
/// depend on the seed and the shot alone, and it must hand each row the
/// readouts of its own shot. One that draws a shot's noise from one stream
/// per `execute`, after the shots before it, or that pairs rows with shots
/// by position once it reordered them, gives a shot other records when a
/// part above batches it otherwise.
///
/// # How it checks
///
/// As for an executor, with the kit's executor below, which gives each shot
/// the same values in any company: a shot that reads otherwise among the
/// others than alone does so because of the adapter.
///
/// # Example
///
/// An adapter that answers each batch's rows in reverse order, as one that
/// collects the answers below on a stack would, is reported as `execute 0,
/// batch 0, row 0 (shot 0): reads [01] among the others, [00] alone`.
///
/// # What it does not see
///
/// What the
/// [executor's check](executor::gives_each_shot_what_it_gives_alone#what-it-does-not-see)
/// does not see, the adapter's draws included: an adapter that ignores the
/// seed passes if it draws the same each time. Whether the adapter is
/// replayable is read from its offer above the kit's executor
/// ([`AdapterSetup::offers_above`](executor_protocol::AdapterSetup::offers_above)),
/// which offers replayable shots.
///
/// # Errors
///
/// As [`executor::gives_each_shot_what_it_gives_alone`].
pub fn gives_each_shot_what_it_gives_alone<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    executor::gives_each_shot_what_it_gives_alone_on(&mut mount, script, trials)
}

/// Checks that the adapter keeps its own state of each shot from one
/// `execute` to the next, as an executor does
/// ([`Rule::ContinuesEachShotAcrossExecutes`]; the rule's mechanics are
/// [`executor::continues_each_shot_across_executes`]'s).
///
/// # Why it matters
///
/// A driver sends a shot's rounds in later `execute`s, through every
/// adapter of the chain. An adapter that holds state per shot, such as a
/// noise adapter's stream or a decoder's syndrome history, must pick it up
/// where the last round left it; one that makes it again at each `execute`
/// gives a shot split over rounds other records than the same shot sent in
/// one.
///
/// # How it checks
///
/// As for an executor, with the kit's executor below, which keeps each
/// shot's state across `execute`s: a shot that reads otherwise over two
/// `execute`s than in one does so because of the adapter.
///
/// # Example
///
/// A noise adapter that makes each shot's noise again at each `execute` is
/// reported as `shot 0: reads [11] over two executes, [01] in one batch`.
///
/// # What it does not see
///
/// State of the adapter's that `second` does not read through it: a noise
/// adapter that keeps the noise of the qubits `second` measures and loses
/// the rest passes. And what the
/// [executor's check](executor::continues_each_shot_across_executes#what-it-does-not-see)
/// does not see.
///
/// # Errors
///
/// As [`executor::continues_each_shot_across_executes`].
pub fn continues_each_shot_across_executes<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    shots: &ShotSet,
    first: &[Instruction],
    second: &[Instruction],
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    executor::continues_each_shot_across_executes_on(&mut mount, shots, first, second, trials)
}

/// Checks that an `execute` changes, through the adapter, no shot outside
/// its batches, as an executor's does
/// ([`Rule::ChangesNoShotOutsideItsBatches`]; the rule's mechanics are
/// [`executor::changes_no_shot_outside_its_batches`]'s).
///
/// # Why it matters
///
/// A driver sends each `execute` only the shots whose next round is
/// decided, and relies on the others staying as they were, in every part
/// of the chain. An adapter that, at each `execute`, advances its state of
/// every shot it holds, such as a noise adapter drawing noise for each,
/// named or not, gives the shots left out a state no instruction made.
///
/// # How it checks
///
/// As for an executor, with the kit's executor below, which changes only
/// the shots of its batches: a held shot that reads otherwise after an
/// `execute` on other shots does so because of the adapter.
///
/// # Example
///
/// A noise adapter that, at each `execute`, draws the noise of every shot
/// it holds that the `execute` does not name is reported as `execute 2,
/// batch 0, row 0 (shot 0): reads [11] after an execute on the other
/// shots, [01] without it`.
///
/// # What it does not see
///
/// A shot sent below with instructions that leave it as it was:
/// [`sends_only_shots_it_got`] reports every shot sent below that the
/// `execute` does not hold. And what the
/// [executor's check](executor::changes_no_shot_outside_its_batches#what-it-does-not-see)
/// does not see.
///
/// # Errors
///
/// As [`executor::changes_no_shot_outside_its_batches`].
pub fn changes_no_shot_outside_its_batches<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    shots: &ShotSet,
    first: &[Instruction],
    second: &[Instruction],
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    executor::changes_no_shot_outside_its_batches_on(&mut mount, shots, first, second, trials)
}

/// Checks that the adapter makes its own state of each shot afresh at the
/// shot's first batch, whatever an earlier adapter left, as an executor
/// does ([`Rule::StartsEachShotAfresh`]; the rule's mechanics are
/// [`executor::starts_each_shot_afresh`]'s).
///
/// # Why it matters
///
/// A run's records must depend only on the seed and the shots' ids, so
/// that it can be replayed. An adapter whose per-shot state outlives the
/// adapter, such as noise kept in a store its instances share, starts a
/// shot where an earlier adapter left it, and the same run gives other
/// records each time.
///
/// # How it checks
///
/// As for an executor, on a new adapter for each run, with a new kit's
/// executor below, which starts each shot afresh: a shot that reads
/// otherwise after an earlier run does so because of the adapter.
///
/// # Example
///
/// A noise adapter that keeps each shot's noise in a store shared by all
/// its instances is reported as ``execute 0, batch 0, row 0 (shot 0):
/// reads [00] after an earlier part ran `first` on it and never ended it,
/// [10] before``.
///
/// # What it does not see
///
/// What the
/// [executor's check](executor::starts_each_shot_afresh#what-it-does-not-see)
/// does not see, in the adapter's state: a leftover that `second` does not
/// read through it.
///
/// # Errors
///
/// As [`executor::starts_each_shot_afresh`].
pub fn starts_each_shot_afresh<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    shots: &ShotSet,
    first: &[Instruction],
    second: &[Instruction],
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    executor::starts_each_shot_afresh_on(&mut mount, shots, first, second, trials)
}

/// Checks that the adapter does an `execute`'s work, its own and its calls
/// below, only once its future is polled, as an executor does
/// ([`Rule::DoesNothingUntilPolled`]; the rule's mechanics are
/// [`executor::does_nothing_until_polled`]'s).
///
/// # Why it matters
///
/// A caller may drop an `execute`'s future unpolled and call again, and
/// relies on the dropped call having done nothing in any part of the
/// chain. An adapter that draws its noise, or calls the part below, when
/// called advances a shot's state for a call that never ran, and the next
/// call reads it.
///
/// # How it checks
///
/// As for an executor, with the kit's executor below, which does nothing
/// until polled: a row that reads otherwise after a dropped call does so
/// because of the adapter.
///
/// # Example
///
/// A noise adapter that draws an `execute`'s noise when called, before its
/// future is polled, is reported as `execute 0, batch 0, row 0 (shot 0):
/// reads [10] after a call dropped unpolled, [01] without`.
///
/// # What it does not see
///
/// A call below made at the call and never polled: the kit's executor
/// below then does nothing, so the records do not show it. And what the
/// [executor's check](executor::does_nothing_until_polled#what-it-does-not-see)
/// does not see.
///
/// # Errors
///
/// As [`executor::does_nothing_until_polled`].
pub fn does_nothing_until_polled<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    executor::does_nothing_until_polled_on(&mut mount, script, trials)
}

/// Checks that, after a failed run, the adapter runs the run's shots again
/// as a new adapter would, as an executor does
/// ([`Rule::RunsAsNewAfterAFailedRun`]; the rule's mechanics are
/// [`executor::runs_as_new_after_a_failed_run`]'s).
///
/// # Why it matters
///
/// A chain is often kept after a failed run, and the run tried again. An
/// adapter that keeps its own state of a shot past `end_shots`, such as a
/// noise adapter's stream, starts the retry where the failed run left it;
/// one that stops taking calls after a failure fails the retry.
///
/// # How it checks
///
/// As for an executor, on one adapter above the kit's executor, which
/// drops a shot's state at `end_shots`: a retry that reads otherwise than
/// on a new adapter does so because of the adapter.
///
/// # Example
///
/// A noise adapter that ignores `end_shots`, keeping each shot's noise past
/// its end, is reported as `execute 0, batch 0, row 0 (shot 0): reads [10]
/// after a failed run, [01] on a new part`.
///
/// # What it does not see
///
/// Recovery from the adapter's own failure, or from one below it: the check
/// fails the run above it only.
///
/// # Errors
///
/// As [`executor::runs_as_new_after_a_failed_run`].
pub fn runs_as_new_after_a_failed_run<A: Adapt>(
    make: impl FnMut() -> A,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = AdapterMount { make, needs };
    executor::runs_as_new_after_a_failed_run_on(&mut mount, script, trials)
}

// ------------------------------------------------------------- the steps

/// For [`passes_each_shots_conditions_up`]: in each of `script`'s
/// `execute`s, the conditions raised on the shots `raised_on`, at the
/// positions `half`: `fixed`, or, with none given, kinds in turn by
/// position (drawn in a drawn trial).
fn raises_in_every_execute(
    patterns: &mut TrialPatterns,
    script: &Script,
    half: &[usize],
    raised_on: &[ShotId],
    fixed: Option<Raised>,
) -> Vec<Raise> {
    let mut raises = Vec::new();
    for execute in 0..script.executes().len() {
        for (&position, &shot) in half.iter().zip(raised_on) {
            let conditions = match fixed {
                Some(fixed) => fixed,
                None => patterns.raised(Raised::InTurn { position }),
            };
            raises.push(Raise {
                execute,
                shot,
                raised: conditions,
            });
        }
    }
    raises
}

/// The conditions that `clean`, a run raising none below, reports in the
/// row at `row`'s place.
fn clean_conditions(clean: &RunLog, row: &RowRecord) -> ShotConditions {
    clean
        .rows
        .iter()
        .find(|clean| (clean.execute, clean.batch, clean.row) == (row.execute, row.batch, row.row))
        .map_or(ShotConditions::NONE, |clean| clean.conditions)
}
