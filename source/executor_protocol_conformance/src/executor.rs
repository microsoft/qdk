// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rules an executor keeps, which the protocol states but does not
//! check as it runs, each checked by a function of its own, except two
//! that every check reports (below): the executor writes
//! every readout value it answers, in its batch's shape; for a replayable
//! executor, a shot's records depend only on the seed and the shot's id; a
//! shot's state is kept from one `execute` to the next, made at its first
//! batch, and changed by no `execute` that does not name it; an `execute`
//! works only once polled; the executor's `accepts` and `offers` stay as
//! composed; and after a failed run it runs the same shots again as a new
//! executor would.
//!
//! Each function takes `make`, which builds the executor afresh, the same
//! each time; `needs`, what the kit's driver above it needs of it, which it
//! gets ready for at each run; what the kit's driver sends: a [`Script`],
//! or, for the rules on a shot's state, the shots and two lists of
//! instructions, `first` and `second`, which the check sends in several
//! orders; and the [`Trials`] to run. Each run is on a new executor, except
//! in [`runs_as_new_after_a_failed_run`], whose rule is about an executor's
//! later runs.
//!
//! Every check also reports, in each of its runs that ends (and in the run
//! that [`runs_as_new_after_a_failed_run`] fails on purpose), each batch
//! the executor answered with readouts of another shape
//! ([`Rule::KeepsEachBatchsShape`]), and each `end_shots` that panicked
//! ending shots among which some the executor did not hold
//! ([`Rule::IgnoresEndsOfShotsItDoesNotHold`], which says why a held
//! shot's panic there is reported too). Another run that fails ends the
//! check, as `docs/checks.md` says under "Reading a report".
//!
//! # The trials
//!
//! Each check runs trial 0 on what it is given, then each drawn trial on it
//! redrawn from the trial's seed, within what the rule leaves free ([`Trials`]
//! says why). A check given a script sends each shot the same instructions,
//! `execute` after `execute`, in other company: the shots get new ids, each
//! batch's shots are put in a new order and cut into batches, each
//! `execute`'s batches are put in a new order, and some `execute`s are split
//! in two. A check given shots gives them new ids, draws the groups where
//! it splits them in two, and cuts the shots into batches in a new order,
//! once per trial: every run it compares makes each shot in the same batch,
//! so that a part whose shots depend on the batch they are made in (seeded
//! by it, say) breaks only [`gives_each_shot_what_it_gives_alone`]'s rule,
//! and is reported under it alone. A later `execute` may send a shot in
//! another batch: [`continues_each_shot_across_executes`] sends `second` to
//! the two groups as two batches by design, so a part whose shots draw by
//! the batch they sit in at a later `execute` is reported under its rule
//! too. In [`runs_as_new_after_a_failed_run`], each drawn trial also draws
//! the `execute` after which its failed run fails. The runs of a drawn
//! trial get a seed of their own. A check's guard that the given
//! instructions can show a broken rule judges trial 0 only, whose patterns
//! are the caller's.
//!
//! # Parts that are not replayable
//!
//! A part that offers shots that are not replayable
//! ([`Offer::not_replayable`](executor_protocol::Offer::not_replayable))
//! may draw a run's shots from one stream, so a shot's records may depend
//! on the shots run beside it and on how the run puts them in batches and
//! `execute`s; only a run sent again as it was, with the same seed, must
//! give the same records. So, for such a part, no check compares the
//! records of runs that send the shots otherwise:
//! [`gives_each_shot_what_it_gives_alone`],
//! [`continues_each_shot_across_executes`] and
//! [`changes_no_shot_outside_its_batches`], whose every comparison is of
//! such runs, pass it on their rule after their first run. The other
//! checks compare only runs sent alike, but [`starts_each_shot_afresh`]
//! and [`does_nothing_until_polled`] skip their guard, which compares runs
//! sent otherwise. Each check that skips says so on stderr in trial 0,
//! and every run it makes still judges the rules every run judges.

use executor_protocol::{Execute, Instruction, Needs, ShotId, ShotSet};

use crate::chain::{Chain, ExecutorMount, Mount, noted_run, records};
use crate::nonconformance::{Nonconformance, Place};
use crate::readout_prefills::{MAX_RUNS, ReadoutPrefills};
use crate::rule::Rule;
use crate::script::Script;
use crate::shared_run::RunSetup;
use crate::shot_records::{ShotRecords, joined_fault, row_faults};
use crate::trial_patterns::run_trials;
use crate::trials::Trials;

/// Checks that the executor writes every readout value of each batch it
/// answers ([`Rule::WritesEveryReadout`]).
///
/// # Why it matters
///
/// The parts above take each value as a shot's outcome. A slot the executor
/// leaves unwritten reaches them as the protocol shaped it, `Missing`, with
/// no error: a decoder or a driver's output then holds a gap no part
/// reported.
///
/// # How it checks
///
/// Each trial runs its script on new executors, with one seed, writing a
/// value of its own into each slot before the executor runs, and reports
/// each slot that equals the kit's value in every run. The second run's
/// values are the first's complements, so an executor that writes every slot
/// and repeats its values for a seed passes in two runs; further runs, with
/// new values, run while a slot still equals them, up to a fixed number of
/// runs that makes the odds of reporting a slot written with values that
/// change from run to run negligible. Trial 0 runs `script`; each drawn trial
/// runs it redrawn, with a seed of its own (the module's
/// [trials](crate::executor#the-trials)).
///
/// # Example
///
/// An executor whose loop over a row's slots stops one short for shot 1 is
/// reported as `execute 0, batch 0, row 1 (shot 1), slot 1: holds the value
/// the kit wrote before the part ran, in each of 22 runs`.
///
/// # What it does not see
///
/// An executor that reads the kit's value before writing its own, and one
/// that answers with new readouts of the right shape.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each slot left unwritten.
/// [`Nonconformance::CannotJudge`] when `script` reads no readout, does not
/// suit the executor, or a run fails.
pub fn writes_every_readout<E: Execute>(
    make: impl FnMut() -> E,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    writes_every_readout_on(&mut ExecutorMount { make, needs }, script, trials)
}

/// Checks that a shot's records depend only on the seed and the shot's id
/// ([`Rule::GivesEachShotWhatItGivesAlone`]), not on the shots that share
/// its batches and `execute`s, nor on their order, for a part whose offer
/// is replayable.
///
/// # Why it matters
///
/// The parts above group, cut and reorder shots freely: an adapter cuts a
/// batch to fit a device, a driver sends each round only the shots that need
/// one. They rely on a shot reading the same in any company. An executor
/// that draws one random stream per batch, or rejects a batch as a whole,
/// gives a shot other records when it is sent otherwise: a run's statistics
/// then change with how a part above batches it, and a shot cannot be
/// replayed alone.
///
/// # How it checks
///
/// Each shot must read alone, in its own run, what it reads among the
/// others, and what it reads when the shots are split into two groups, each
/// sent in its own `execute` for each of the script's. Trial 0 runs `script`
/// and two splits: the shots at odd and even positions, and the halves, each
/// reversed, the second half first. Each drawn trial runs the script
/// redrawn, and two splits into groups drawn for it (the module's
/// [trials](crate::executor#the-trials)).
///
/// # Example
///
/// An executor that raises `REJECTED` on every row of a batch of more than
/// two shots, as a device rejecting a batch as a whole would, is reported as
/// `execute 0, batch 0, row 0 (shot 0): reads [01] ShotConditions(REJECTED)
/// among the others, [01] alone`.
///
/// # What it does not see
///
/// Whether the records depend on the seed at all: an executor that ignores
/// it is correct if it is deterministic. And a dependence on the company
/// that only shows on scripts larger than the one given.
///
/// Nor a part that offers shots that are not replayable
/// ([`Offer::not_replayable`](executor_protocol::Offer::not_replayable)):
/// it may draw a run's shots from one stream, so its records may depend on
/// the company. The check runs `script` once, and reports the faults that
/// run shows of the rules every run judges (a batch's shape, ends of shots
/// it does not hold); it passes the part on this rule, and in trial 0
/// notes on stderr that it did not judge it.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each row that reads otherwise than
/// alone. [`Nonconformance::CannotJudge`] when `script` holds fewer than three
/// shots, or when it does not suit the executor, or a run fails; and, for a
/// replayable part, when it reads no readout, or, in trial 0, gives every
/// shot the same records (so that an executor giving each shot another's
/// would pass).
pub fn gives_each_shot_what_it_gives_alone<E: Execute>(
    make: impl FnMut() -> E,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    gives_each_shot_what_it_gives_alone_on(&mut ExecutorMount { make, needs }, script, trials)
}

/// Checks that the executor keeps each shot's state from one `execute` to
/// the next ([`Rule::ContinuesEachShotAcrossExecutes`]).
///
/// # Why it matters
///
/// A driver runs a shot in rounds: it sends instructions, reads the
/// readouts, decides, and sends the next round in a later `execute`. It
/// relies on the shot's qubits being where the last round left them. An
/// executor that makes the shot again at each `execute`, or distrusts a
/// kept state, breaks every program with mid-circuit decisions.
///
/// # How it checks
///
/// `first` then `second` on `shots`, in two `execute`s, must read what one
/// batch of both reads. With two shots or more, also: `second` sent to two
/// groups of the shots as two batches, and `first` on one group alone, then
/// `second` on all (the other group's shots then new to the executor, which
/// must read as `second` alone). In trial 0 the groups are the shots at even
/// and odd positions, and each `execute` sends its shots in one batch. Each
/// drawn trial gives the shots new ids, draws the groups, and cuts the shots
/// into batches in a new order once, those of the group `first` runs on
/// alone first: every `execute` sends that cut, or that group's batches
/// alone, but the one sending `second` to the two groups (the module's
/// [trials](crate::executor#the-trials)).
///
/// `second` must read what `first` changes, so that a shot made again at
/// its second `execute` would show: on some shot, `second` alone must read
/// otherwise than after `first`.
///
/// # Example
///
/// An executor that raises `REJECTED` on each row of a shot it held before
/// the `execute`, as one distrusting a kept state would, is reported as
/// `shot 0: reads [01] ShotConditions(REJECTED) over two executes, [01] in
/// one batch`.
///
/// # What it does not see
///
/// State that `second` does not read: an executor that keeps the qubits
/// `second` measures but loses others passes.
///
/// Nor a part that offers shots that are not replayable, since every run
/// the check compares sends the shots otherwise (the module's
/// [parts that are not replayable](self#parts-that-are-not-replayable)):
/// the check runs `first` and `second` in one batch once, and reports the
/// faults that run shows of the rules every run judges; it passes the part
/// on this rule, and in trial 0 notes on stderr that it did not judge it.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each shot or row that reads otherwise.
/// [`Nonconformance::CannotJudge`] when the instructions do not suit the
/// executor, or when a run fails; and, for a replayable part, when, in
/// trial 0, `second` reads the same alone as after `first` on every shot.
pub fn continues_each_shot_across_executes<E: Execute>(
    make: impl FnMut() -> E,
    needs: Needs,
    shots: &ShotSet,
    first: &[Instruction],
    second: &[Instruction],
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = ExecutorMount { make, needs };
    continues_each_shot_across_executes_on(&mut mount, shots, first, second, trials)
}

/// Checks that an `execute` changes no shot outside its batches
/// ([`Rule::ChangesNoShotOutsideItsBatches`]), whether the executor holds
/// that shot yet or not.
///
/// # Why it matters
///
/// A driver holds many shots and sends each `execute` only those whose next
/// round is decided. It relies on the others staying as they were. An
/// executor that runs a range of ids, from the lowest an `execute` names to
/// the highest, changes the held shots between, and their later rounds read
/// a state no instruction made.
///
/// # How it checks
///
/// The shots are split into two groups, `a` and `b`. Five runs:
///
/// 1. `first` on `a`, `first` on `b`, `second` on `a`, `second` on `b`,
///    each its own `execute`;
/// 2. the same without `first` on `b`;
/// 3. `first` twice on `a`, then `second`;
/// 4. `second` alone on `b`;
/// 5. `first` then `second` on `b`.
///
/// Each shot of `a` must read the same in runs 1 and 2 (an `execute` on `b`
/// left it as it was), and each shot of `b` the same in runs 2 and 4 (the
/// `execute`s on `a` left it new) and in runs 1 and 5 (an `execute` on `a`
/// left it as it was, held). In trial 0, `a` and `b` are the shots, sorted,
/// at even and odd positions, which lie between each other, and each
/// `execute` sends its shots in one batch. Each drawn trial gives the shots
/// new ids, draws the groups, and cuts each group into batches in a new
/// order once, sent in every `execute` of every run (the module's
/// [trials](crate::executor#the-trials)).
///
/// This shows when `first` changes a shot each time it runs (a flip, not a
/// set) and `second` reads that change: on some shot of `a`, `second` must
/// read otherwise in run 3 than in run 2, and on some shot of `b`, otherwise
/// in run 1 than in run 4.
///
/// # Example
///
/// An executor that flips a bit of every shot it holds whose id lies between
/// the ids an `execute` names, as a run over a range of ids would, is
/// reported as `execute 2, batch 0, row 1 (shot 2): reads [00] after an
/// execute on the other shots, [01] without it`.
///
/// # What it does not see
///
/// A change that `second` does not read, and a change to shots outside the
/// run's: the check holds no other shots.
///
/// Nor a part that offers shots that are not replayable, since every run
/// the check compares sends the shots otherwise (the module's
/// [parts that are not replayable](self#parts-that-are-not-replayable)):
/// the check makes run 1 only, and reports the faults it shows of the rules
/// every run judges; it passes the part on this rule, and in trial 0 notes
/// on stderr that it did not judge it.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each row that reads otherwise.
/// [`Nonconformance::CannotJudge`] when `shots` holds fewer than four shots
/// (so that, in trial 0, the shots at odd positions are not contiguous),
/// when the instructions do not suit the executor, or when a run fails;
/// and, for a replayable part, when, in trial 0, `first` and `second` do
/// not show a change as above.
pub fn changes_no_shot_outside_its_batches<E: Execute>(
    make: impl FnMut() -> E,
    needs: Needs,
    shots: &ShotSet,
    first: &[Instruction],
    second: &[Instruction],
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = ExecutorMount { make, needs };
    changes_no_shot_outside_its_batches_on(&mut mount, shots, first, second, trials)
}

/// Checks that the executor makes each shot afresh at its first batch,
/// whatever an earlier executor left ([`Rule::StartsEachShotAfresh`]).
///
/// # Why it matters
///
/// A run's records must depend only on the seed and the shots' ids, so that
/// it can be replayed, and every check of the kit that builds several
/// executors relies on it. An executor whose shots live on a device reset
/// only at some point, or in a store its instances share, starts a shot
/// where an earlier executor left it, and the same run gives other records
/// each time.
///
/// # How it checks
///
/// Each run on a new executor:
///
/// 1. `second` on `shots`;
/// 2. `first` on `shots`;
/// 3. `second` on `shots` again;
/// 4. `first` on `shots`, on an executor dropped with its shots never
///    ended;
/// 5. `second` on `shots` again;
/// 6. `first` and `second` in one batch on `shots`.
///
/// Each shot must read the same in runs 1, 3 and 5. Trial 0 sends `shots`
/// as given, in one batch; each drawn trial gives them new ids and cuts them
/// into batches in a new order once, the same in its six runs (the module's
/// [trials](crate::executor#the-trials)). Every run of a trial has the same
/// needs and seed, so the check does not see how the executor makes a shot
/// from them.
///
/// The runner ends every shot of a run, even after a panic, so in a chain
/// the protocol itself never leaves a shot unended; run 4 is there for what
/// lies outside it: an executor whose shots live on a device it shares with
/// other executors, or one left by a crash, must still make each shot
/// afresh. So run 4 asks the executor to tolerate being dropped with its
/// shots held; one that panics then is not judged.
///
/// This shows when `second` reads what `first` changes: on some shot,
/// `second` must read otherwise in run 6 than in run 1.
///
/// # Example
///
/// An executor that keeps each shot's noise in a store shared by all its
/// instances, so that a shot never ended is found again by the next
/// executor, is reported as ``execute 0, batch 0, row 0 (shot 0): reads [00]
/// after an earlier part ran `first` on it and never ended it, [10] before``.
///
/// # What it does not see
///
/// A leftover that `second` does not read, and one that shows only after
/// more runs than the check makes.
///
/// For a part that offers shots that are not replayable, whether `second`
/// reads what `first` changes: run 6 sends the shots otherwise than run 1
/// (the module's
/// [parts that are not replayable](self#parts-that-are-not-replayable)).
/// The check compares runs 1, 3 and 5, sent alike, but skips run 6; it
/// then passes a part whose leftovers `second` cannot show, and in trial 0
/// notes on stderr that it did not judge whether it could.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each row of runs 3 and 5 that reads
/// otherwise than in run 1. [`Nonconformance::CannotJudge`] when the
/// instructions do not suit the executor, when a run fails, or when the
/// executor panics as run 4 drops it, naming the panic; and, for a
/// replayable part, when, in trial 0, `second` reads the same after
/// `first` as alone on every shot.
pub fn starts_each_shot_afresh<E: Execute>(
    make: impl FnMut() -> E,
    needs: Needs,
    shots: &ShotSet,
    first: &[Instruction],
    second: &[Instruction],
    trials: Trials,
) -> Result<(), Nonconformance> {
    let mut mount = ExecutorMount { make, needs };
    starts_each_shot_afresh_on(&mut mount, shots, first, second, trials)
}

/// Checks that the executor does an `execute`'s work only once its future
/// is polled ([`Rule::DoesNothingUntilPolled`]).
///
/// # Why it matters
///
/// A caller may drop an `execute`'s future unpolled, when a run is
/// cancelled or a `select!` takes another branch, and call again. It relies
/// on the dropped call having done nothing. An executor that does the work
/// when called, and answers the next call with it, runs a shot's
/// instructions twice or answers with another call's readouts.
///
/// # How it checks
///
/// The kit calls the executor once before each `execute` it waits for, with
/// readouts of its own, and drops that call's future unpolled; the records
/// must be those of a run without the dropped calls. Trial 0 runs `script`;
/// each drawn trial runs it redrawn, with a seed of its own (the module's
/// [trials](crate::executor#the-trials)).
///
/// This shows when `script`'s `execute`s change a shot each time they run
/// and read the change: on some shot, a run sending each `execute` twice
/// must read otherwise, in the second, than a run sending it once.
///
/// # Example
///
/// An executor that does an `execute`'s work when called, before its future
/// is polled, is reported as `execute 0, batch 0, row 0 (shot 0): reads
/// [10] after a call dropped unpolled, [01] without`.
///
/// # What it does not see
///
/// Work done at the call that the next call's readouts do not show, such as
/// a log written twice.
///
/// For a part that offers shots that are not replayable, whether `script`
/// shows a change as above: the run sending each `execute` twice sends
/// the shots otherwise (the module's
/// [parts that are not replayable](self#parts-that-are-not-replayable)).
/// The check compares the runs with and without the dropped calls, sent
/// alike, but skips that run; it then passes a part whose dropped work
/// `script` cannot show, and in trial 0 notes on stderr that it did not
/// judge whether it could.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each row that reads otherwise after
/// the dropped calls. [`Nonconformance::CannotJudge`] when `script` does not
/// suit the executor, or a run fails; and, for a replayable part, when, in
/// trial 0, `script` does not show a change as above.
pub fn does_nothing_until_polled<E: Execute>(
    make: impl FnMut() -> E,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    does_nothing_until_polled_on(&mut ExecutorMount { make, needs }, script, trials)
}

/// Checks that the executor's `accepts` and `offers` give, once it is
/// composed, what they gave before ([`Rule::KeepsItsAcceptsAndOffers`]).
///
/// # Why it matters
///
/// `>>` fits each part to the one below on what `accepts` and `offers` give,
/// and admits the chain once. Nothing reads them again: an executor that
/// later accepts more, or offers less, gets what the chain was never
/// admitted for, and fails at a run, far from the cause.
///
/// # How it checks
///
/// The kit reads them before `>>`, once the executor got ready for the run
/// (its `ready` answered `Ok`), and at each `execute` of the run. Trial 0
/// runs `script`; each drawn trial runs it redrawn (the module's
/// [trials](crate::executor#the-trials)).
///
/// # Example
///
/// An executor that accepts a wider instruction set once ready is reported
/// as `` once ready: `accepts` gives another set than before `>>` ``.
///
/// # What it does not see
///
/// A change at a time the kit does not read them, such as between two
/// runs.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each time either reads otherwise.
/// [`Nonconformance::CannotJudge`] when `script` does not suit the executor,
/// or the run fails.
pub fn keeps_its_accepts_and_offers<E: Execute>(
    make: impl FnMut() -> E,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    keeps_its_accepts_and_offers_on(&mut ExecutorMount { make, needs }, script, trials)
}

/// Checks that, after a failed run, the executor runs the run's shots again
/// as a new executor would ([`Rule::RunsAsNewAfterAFailedRun`]).
///
/// # Why it matters
///
/// A chain is often kept after a failed run, and the run tried again. The
/// protocol asks a part to take calls again after a failed run, whether it
/// failed itself, by an error or a panic, or a part above it did, and a
/// shot's records depend only on the seed and its id. An executor that
/// stops working after a failure, or keeps a shot's state past its
/// `end_shots`, fails the retry or gives it other records.
///
/// # How it checks
///
/// On one executor, two runs of the script, on the same shots:
///
/// 1. the kit's driver fails the run, by an error, once the executor
///    answered an `execute`;
/// 2. the whole script.
///
/// Run 2 must give the records a new executor gives. Trial 0 runs `script`
/// and fails run 1 after the first `execute`; each drawn trial runs it
/// redrawn, with a seed of its own, and fails run 1 after an `execute` drawn
/// for it (the module's [trials](crate::executor#the-trials)).
///
/// A run failed by a panic above the executor is not run apart: the runner
/// catches the panic and ends the run's shots as after an error, so the
/// executor gets the same calls either way. The runner ends every shot of a
/// run, failed or not, so this also asks that the executor drop a shot's
/// state at `end_shots`: a shot kept past its end starts run 2 where run 1
/// left it.
///
/// # Example
///
/// An executor that refuses every `execute` once it ended a shot is
/// reported, on a script whose shots all end at its last `execute`, as
/// `run 2: fails: run failed: shot 0 failed: an execute was refused: shot
/// 0 is not held: it ended, or never started`.
///
/// # What it does not see
///
/// Recovery from the executor's own failure: the kit cannot make it fail,
/// so the check fails the run above it only, and an executor that recovers
/// from a failure above it but not from its own passes.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming run 2 if it fails after it started (a
/// refusal is `CannotJudge`, below), or each row of it that reads otherwise
/// than on a new executor.
/// [`Nonconformance::CannotJudge`] when, in trial 0, the `execute`s up to the
/// failing one read no readout, or when `script` does not suit the executor,
/// a run on a new executor fails, or run 1 or run 2 is refused before it
/// runs, as a busy executor may refuse one.
pub fn runs_as_new_after_a_failed_run<E: Execute>(
    make: impl FnMut() -> E,
    needs: Needs,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    runs_as_new_after_a_failed_run_on(&mut ExecutorMount { make, needs }, script, trials)
}

// ------------------------------------------------- the checks, on a mount

/// [`writes_every_readout`], on the part `mount` builds.
pub(crate) fn writes_every_readout_on(
    mount: &mut impl Mount,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    run_trials(trials, |patterns| {
        let script = patterns.script(script);
        let mut findings = patterns.findings();
        let mut prefills = ReadoutPrefills::default();
        let mut reads_anything = false;
        while prefills.needs_another_run() {
            let setup = RunSetup {
                prefill_run: Some(prefills.runs()),
                ..RunSetup::of(script.clone())
            };
            let log = noted_run(&mut findings, mount, setup)?;
            reads_anything |= log.rows.iter().any(|row| !row.values.is_empty());
            prefills.record(&log);
        }
        if !reads_anything {
            findings.cannot_judge("the script reads no readout, so there is no value to write");
        }
        let runs = prefills.runs();
        for place in prefills.unwritten() {
            findings.fault(
                Rule::WritesEveryReadout,
                place,
                format!(
                    "holds the value the kit wrote before the part ran, in each of {runs} runs"
                ),
            );
        }
        debug_assert!(runs <= MAX_RUNS);
        findings.finish()
    })
}

/// [`gives_each_shot_what_it_gives_alone`], on the part `mount` builds.
pub(crate) fn gives_each_shot_what_it_gives_alone_on(
    mount: &mut impl Mount,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    run_trials(trials, |patterns| {
        let count = script.shots().len();
        if count < 3 {
            return Err(patterns.cannot_judge(format!(
                "the script holds {count} shots; the check needs three or more, to split them"
            )));
        }
        let script = patterns.script(script);
        let shots = script.shots();
        let mut findings = patterns.findings();
        let rule = Rule::GivesEachShotWhatItGivesAlone;
        let among = noted_run(&mut findings, mount, RunSetup::of(script.clone()))?;
        if among.not_replayable {
            findings.not_judged(rule, NOT_REPLAYABLE);
            return findings.finish();
        }
        let among = ShotRecords::of(&among);
        if !among.reads_anything() {
            findings.cannot_judge("the script reads no readout, so no record shows a shot");
            return findings.finish();
        }
        let alone: Vec<ShotRecords> = shots
            .iter()
            .map(|&shot| records(&mut findings, mount, script.alone(shot)))
            .collect::<Result<_, _>>()?;
        let splits = if patterns.is_fixed() {
            let half = shots.len() / 2;
            let mut reversed = shots.clone();
            reversed.reverse();
            vec![
                (
                    patterns.two_groups(&shots),
                    "with the shots split at odd and even positions",
                ),
                (
                    [
                        reversed[..shots.len() - half].to_vec(),
                        reversed[shots.len() - half..].to_vec(),
                    ],
                    "with the shots split in halves, each reversed",
                ),
            ]
        } else {
            let label = "with the shots split in two groups drawn for the trial";
            vec![
                (patterns.two_groups(&shots), label),
                (patterns.two_groups(&shots), label),
            ]
        };
        for (shot, alone) in shots.iter().zip(&alone) {
            findings.faults(
                rule,
                row_faults(*shot, (&among, "among the others"), (alone, "alone")),
            );
        }
        for (groups, label) in splits {
            let split = records(&mut findings, mount, script.split(&groups))?;
            for (shot, alone) in shots.iter().zip(&alone) {
                findings.faults(rule, row_faults(*shot, (&split, label), (alone, "alone")));
            }
        }
        let differ = shots.iter().any(|&shot| among.differ(shot, shots[0]));
        if !differ {
            findings.undetectable(
                "every shot reads the same, so a part that gave each shot another's records \
                 would pass; choose instructions whose readouts differ between shots",
            );
        }
        findings.finish()
    })
}

/// [`continues_each_shot_across_executes`], on the part `mount` builds.
pub(crate) fn continues_each_shot_across_executes_on(
    mount: &mut impl Mount,
    shots: &ShotSet,
    first: &[Instruction],
    second: &[Instruction],
    trials: Trials,
) -> Result<(), Nonconformance> {
    let rule = Rule::ContinuesEachShotAcrossExecutes;
    let both: Vec<Instruction> = first.iter().chain(second).cloned().collect();
    run_trials(trials, |patterns| {
        let shots = patterns.shots(shots);
        let all: Vec<ShotId> = shots.iter().collect();
        let mut findings = patterns.findings();
        // One cut per trial: the held group's, and the whole's, which starts
        // with it, so that each shot sits in the same batch in every run.
        let groups = (all.len() > 1).then(|| {
            let [held, new] = patterns.two_groups(&all);
            (set(&held), set(&new))
        });
        let held = groups.as_ref().map_or(&shots, |(held, _)| held);
        let new = groups.as_ref().map(|(_, new)| new);
        let [held_cut, whole] = patterns.held_and_all_cuts(held, new, &shots);
        let in_one = Script::new().execute(whole.batches(&both));
        let over_two = Script::new()
            .execute(whole.batches(first))
            .execute(whole.batches(second));
        let in_one = noted_run(&mut findings, mount, RunSetup::of(in_one))?;
        if in_one.not_replayable {
            findings.not_judged(rule, NOT_REPLAYABLE);
            return findings.finish();
        }
        let joined = ShotRecords::of(&in_one);
        let parted = records(&mut findings, mount, over_two)?;
        let label = "over two executes";
        for &shot in &all {
            findings.faults(
                rule,
                joined_fault(shot, (&parted, label), (&joined, "in one batch")),
            );
        }
        let second_alone = Script::new().execute(whole.batches(second));
        let second_alone = records(&mut findings, mount, second_alone)?;
        if let Some((held, new)) = &groups {
            let split_second = Script::new().execute(whole.batches(first)).execute([
                (held.clone(), second.to_vec()),
                (new.clone(), second.to_vec()),
            ]);
            let split_second = records(&mut findings, mount, split_second)?;
            let label = "with `second` in two batches";
            for &shot in &all {
                findings.faults(
                    rule,
                    row_faults(
                        shot,
                        (&split_second, label),
                        (&parted, "with `second` in one batch"),
                    ),
                );
            }
            let joining = Script::new()
                .execute(held_cut.batches(first))
                .execute(whole.batches(second));
            let joining = records(&mut findings, mount, joining)?;
            let label = "with shots new to the part in the second execute";
            for shot in held.iter() {
                findings.faults(
                    rule,
                    row_faults(shot, (&joining, label), (&parted, "without")),
                );
            }
            for shot in new.iter() {
                let alone = "with `second` alone";
                findings.faults(
                    rule,
                    row_faults(shot, (&joining, label), (&second_alone, alone)),
                );
            }
        }
        let detectable = all.iter().any(|&shot| {
            let after_first = parted.rows(shot).last().map(|row| &row.values);
            let alone = second_alone.rows(shot).last().map(|row| &row.values);
            after_first != alone
        });
        if !detectable {
            findings.undetectable(
                "`second` reads the same alone as after `first` on every shot, so a part that \
                 made each shot again at each execute would pass; choose a `second` that reads \
                 what `first` changes",
            );
        }
        findings.finish()
    })
}

/// [`changes_no_shot_outside_its_batches`], on the part `mount` builds.
pub(crate) fn changes_no_shot_outside_its_batches_on(
    mount: &mut impl Mount,
    shots: &ShotSet,
    first: &[Instruction],
    second: &[Instruction],
    trials: Trials,
) -> Result<(), Nonconformance> {
    let rule = Rule::ChangesNoShotOutsideItsBatches;
    run_trials(trials, |patterns| {
        if shots.len() < 4 {
            return Err(patterns.cannot_judge(format!(
                "`shots` holds {} shots; the check needs four or more, so that the shots at \
                 odd positions are not contiguous",
                shots.len()
            )));
        }
        let mut sorted: Vec<ShotId> = patterns.shots(shots).iter().collect();
        if patterns.is_fixed() {
            sorted.sort_unstable();
        }
        let [even, odd] = patterns.two_groups(&sorted);
        let (even, odd) = (set(&even), set(&odd));
        let mut findings = patterns.findings();
        // One cut per group and trial, sent in every run.
        let (even_cut, odd_cut) = (patterns.cut(&even), patterns.cut(&odd));
        let with = Script::new()
            .execute(even_cut.batches(first))
            .execute(odd_cut.batches(first))
            .execute(even_cut.batches(second))
            .execute(odd_cut.batches(second));
        let without = Script::new()
            .execute(even_cut.batches(first))
            .execute(even_cut.batches(second))
            .execute(odd_cut.batches(second));
        let twice = Script::new()
            .execute(even_cut.batches(first))
            .execute(even_cut.batches(first))
            .execute(even_cut.batches(second));
        let alone = Script::new().execute(odd_cut.batches(second));
        let odd_only = Script::new()
            .execute(odd_cut.batches(first))
            .execute(odd_cut.batches(second));
        let with = noted_run(&mut findings, mount, RunSetup::of(with))?;
        if with.not_replayable {
            findings.not_judged(rule, NOT_REPLAYABLE);
            return findings.finish();
        }
        let with = ShotRecords::of(&with);
        let without = records(&mut findings, mount, without)?;
        let alone = records(&mut findings, mount, alone)?;
        let twice = records(&mut findings, mount, twice)?;
        let odd_only = records(&mut findings, mount, odd_only)?;
        let label = "after an execute on the other shots";
        for shot in even.iter() {
            findings.faults(
                rule,
                row_faults(shot, (&with, label), (&without, "without it")),
            );
        }
        let label = "after executes on the other shots";
        for shot in odd.iter() {
            findings.faults(rule, row_faults(shot, (&without, label), (&alone, "alone")));
        }
        let label = "with executes on the other shots between";
        for shot in odd.iter() {
            findings.faults(
                rule,
                row_faults(shot, (&with, label), (&odd_only, "without them")),
            );
        }
        let last =
            |records: &ShotRecords, shot| records.rows(shot).last().map(|row| row.values.clone());
        let even_shows = even
            .iter()
            .any(|shot| last(&twice, shot) != last(&without, shot));
        let odd_shows = odd
            .iter()
            .any(|shot| last(&with, shot) != last(&alone, shot));
        if !(even_shows && odd_shows) {
            findings.undetectable(
                "`second` does not read what `first` changes on some shot, each time it runs, \
                 so an execute changing a shot outside its batches would not show; choose a \
                 `first` that flips a shot's state and a `second` that reads it",
            );
        }
        findings.finish()
    })
}

/// [`starts_each_shot_afresh`], on the part `mount` builds.
pub(crate) fn starts_each_shot_afresh_on(
    mount: &mut impl Mount,
    shots: &ShotSet,
    first: &[Instruction],
    second: &[Instruction],
    trials: Trials,
) -> Result<(), Nonconformance> {
    let rule = Rule::StartsEachShotAfresh;
    let both: Vec<Instruction> = first.iter().chain(second).cloned().collect();
    run_trials(trials, |patterns| {
        let shots = patterns.shots(shots);
        let all: Vec<ShotId> = shots.iter().collect();
        // One cut per trial, sent in every run.
        let cut = patterns.cut(&shots);
        let on = |instructions: &[Instruction]| Script::new().execute(cut.batches(instructions));
        let (second_on, first_on) = (on(second), on(first));
        let (second_again, unended_first, second_last, both_on) =
            (on(second), on(first), on(second), on(&both));
        let mut findings = patterns.findings();
        let before = noted_run(&mut findings, mount, RunSetup::of(second_on))?;
        let not_replayable = before.not_replayable;
        let before = ShotRecords::of(&before);
        records(&mut findings, mount, first_on)?;
        let after = records(&mut findings, mount, second_again)?;
        let unended = RunSetup {
            withholds_end_shots: true,
            ..RunSetup::of(unended_first)
        };
        noted_run(&mut findings, mount, unended)?;
        let after_unended = records(&mut findings, mount, second_last)?;
        for &shot in &all {
            let label = "after an earlier part ran `first` on it";
            findings.faults(rule, row_faults(shot, (&after, label), (&before, "before")));
            let label = "after an earlier part ran `first` on it and never ended it";
            findings.faults(
                rule,
                row_faults(shot, (&after_unended, label), (&before, "before")),
            );
        }
        if not_replayable {
            findings.detectability_not_judged(rule, NOT_REPLAYABLE);
            return findings.finish();
        }
        let joined = records(&mut findings, mount, both_on)?;
        let detectable = all.iter().any(|&shot| {
            let alone = before.values(shot);
            let joined = joined.values(shot);
            joined[joined.len().saturating_sub(alone.len())..] != alone[..]
        });
        if !detectable {
            findings.undetectable(
                "`second` reads the same after `first` as alone on every shot, so a shot left \
                 changed by an earlier part would not show; choose a `second` that reads what \
                 `first` changes",
            );
        }
        findings.finish()
    })
}

/// [`does_nothing_until_polled`], on the part `mount` builds.
pub(crate) fn does_nothing_until_polled_on(
    mount: &mut impl Mount,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let rule = Rule::DoesNothingUntilPolled;
    run_trials(trials, |patterns| {
        let script = patterns.script(script);
        let mut findings = patterns.findings();
        let clean = records(&mut findings, mount, script.clone())?;
        let dropping = RunSetup {
            drops_a_call_unpolled: true,
            ..RunSetup::of(script.clone())
        };
        let log = noted_run(&mut findings, mount, dropping)?;
        let dropping = ShotRecords::of(&log);
        let label = "after a call dropped unpolled";
        for shot in script.shots() {
            findings.faults(
                rule,
                row_faults(shot, (&dropping, label), (&clean, "without")),
            );
        }
        if log.not_replayable {
            findings.detectability_not_judged(rule, NOT_REPLAYABLE);
            return findings.finish();
        }
        let doubled = records(&mut findings, mount, script.doubled())?;
        let detectable = script.shots().into_iter().any(|shot| {
            let again = doubled
                .rows(shot)
                .iter()
                .skip(1)
                .step_by(2)
                .map(|row| &row.values);
            !again.eq(clean.rows(shot).iter().map(|row| &row.values))
        });
        if !detectable {
            findings.undetectable(
                "each execute of the script reads the same run twice as once, so work done by \
                 a dropped call would not show; choose instructions that flip a shot's state \
                 and read it",
            );
        }
        findings.finish()
    })
}

/// [`keeps_its_accepts_and_offers`], on the part `mount` builds.
pub(crate) fn keeps_its_accepts_and_offers_on(
    mount: &mut impl Mount,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    run_trials(trials, |patterns| {
        let script = patterns.script(script);
        let mut findings = patterns.findings();
        let log = noted_run(&mut findings, mount, RunSetup::of(script))?;
        findings.faults(
            Rule::KeepsItsAcceptsAndOffers,
            log.accepts_and_offers_faults,
        );
        findings.finish()
    })
}

/// [`runs_as_new_after_a_failed_run`], on the part `mount` builds.
pub(crate) fn runs_as_new_after_a_failed_run_on(
    mount: &mut impl Mount,
    script: &Script,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let rule = Rule::RunsAsNewAfterAFailedRun;
    run_trials(trials, |patterns| {
        let script = patterns.script(script);
        let fails_after = patterns.failing_execute(script.executes().len());
        let mut findings = patterns.findings();
        let seed = findings.run_seed();
        // On a new part first, so that a part whose every run fails is not
        // blamed for its second.
        let fresh = records(&mut findings, mount, script.clone())?;
        let mut chain =
            Chain::new(mount, RunSetup::of(script.clone())).map_err(|why| findings.failed(why))?;
        findings.note_part_name(&chain.part_name());
        let failing = RunSetup {
            fails_after: Some(fails_after),
            ..RunSetup::of(script.clone())
        };
        let first = chain.run(failing, seed);
        findings.note_run(&first.log);
        assert!(first.failure.is_some(), "the kit's driver fails the run");
        if let Some(why) = first.why_refused() {
            return Err(findings.failed(why));
        }
        if first.log.rows.iter().all(|row| row.values.is_empty()) {
            findings.undetectable(
                "the script's executes up to the one the run fails after read no readout, so \
                 the failed run shows nothing of the part's state",
            );
        }
        let second = chain.run(RunSetup::of(script.clone()), seed);
        findings.note_run(&second.log);
        if let Some(why) = second.why_refused() {
            return Err(findings.failed(why));
        }
        if let Some(failure) = second.failure {
            findings.fault(rule, Place::Run { run: 2 }, format!("fails: {failure}"));
            return findings.finish();
        }
        let again = ShotRecords::of(&second.log);
        let label = "after a failed run";
        for shot in script.shots() {
            findings.faults(
                rule,
                row_faults(shot, (&again, label), (&fresh, "on a new part")),
            );
        }
        findings.finish()
    })
}

// ------------------------------------------------------------- the steps

/// Why a check compares no records of runs that send the shots otherwise,
/// for a part whose offer is not replayable (the module's
/// [parts that are not replayable](self#parts-that-are-not-replayable)).
const NOT_REPLAYABLE: &str = "the part offers shots that are not replayable \
     (`Offer::not_replayable`), whose records may depend on the shots run beside them \
     and on how the run puts them in batches and executes";

/// `shots`, as a set, in their order.
fn set(shots: &[ShotId]) -> ShotSet {
    ShotSet::ids(shots.iter().map(|shot| shot.get()))
}
