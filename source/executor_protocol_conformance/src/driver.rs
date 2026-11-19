// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rules a driver keeps, which the protocol states but does not check
//! as it runs, each checked by a function of its own, except putting each
//! shot in one batch of an `execute`, which [`sends_only_its_shots`]
//! checks, within the limits the crate's `docs/limits.md` lists: the
//! driver gives one output per shot, in the order of the run's shots; it
//! sends only the run's shots, each in at most one batch of an `execute`,
//! with the runner's seed, stating each batch's readout slots as its
//! instructions count them (the protocol checks this only for a checked
//! batch); and it merges each `execute`'s conditions into the shot's
//! output, and no other shot's.
//!
//! Each function takes `make`, which builds the driver afresh, the same
//! each time, and the run's shots. The driver runs over the kit's
//! executor, which accepts what the driver sends, answers each batch with
//! values drawn from each shot's seed, raises the conditions the check
//! chooses, and logs each batch.
//!
//! # The trials
//!
//! Each check runs trial 0 on the shots it is given, then each drawn trial
//! on as many shots with new ids, in a new order, run with a seed of their
//! own ([`Trials`] says why). The check of conditions also draws, in each
//! drawn trial, the shots raised on and the kinds raised: one kind or more
//! for each `execute` and shot.

use std::collections::BTreeMap;
use std::fmt::Debug;

use executor_protocol::{Drive, Driver, Executor, RunError, ShotConditions, ShotId, ShotSet};

use crate::caller_faults::{misstates_slots, note_misstated, repeated_shots, sends_something};
use crate::chain::{RecordingExecutor, ScriptedDriver};
use crate::nonconformance::{Findings, Nonconformance, Place, failed_run, shot_list, uncomposed};
use crate::rule::Rule;
use crate::shared_run::{Raise, Raised, RunLog, RunSetup, SharedRun, every_kind, raised_in_turn};
use crate::trials::{TrialPatterns, Trials};

/// Checks that the driver gives one output per shot, in the order of the
/// run's shots, each the one its shot gives alone
/// ([`Rule::GivesOneOutputPerShotInOrder`]).
///
/// # Why it matters
///
/// A caller pairs the outputs with its shots by position. A driver that
/// sorts its outputs, drops one, or gives two for a shot shifts the pairing,
/// and each later shot's result is reported, silently, as another shot's.
///
/// # How it checks
///
/// The kit runs the trial's shots in their order (run 1), then in the
/// reverse order (run 2), then each shot alone (runs 3 and on, in the order
/// of the shots). In runs 1 and 2, the output at each position must be the
/// one its shot gives alone. So the check asks that an output depend only
/// on its shot, as [`Seed`] asks of a shot's records; `Drive` does not
/// state that an output is one of its shot's records, so this asks more of
/// a driver than `Drive` states. Of the two orders, at most one is sorted,
/// so a driver that sorts its outputs (by shot id, say) shows whatever the
/// order of the shots. Trial 0 runs `shots` as given; each drawn trial
/// gives them new ids, in a new order (the module's
/// [trials](crate::driver#the-trials)).
///
/// [`Seed`]: executor_protocol::Seed
///
/// # Example
///
/// A driver that drops the first output when the shots do not come in the
/// order of their ids is reported as `run 2: 3 outputs for 4 shots, in the
/// reverse order`.
///
/// # What it does not see
///
/// An output that depends on the other shots only in a company the check
/// does not run: it runs the shots all together or each alone.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming a run with another number of outputs
/// (and nothing more, as its outputs cannot be paired with shots by
/// position), or each output that is not its shot's, with the order it
/// was given in.
/// [`Nonconformance::CannotJudge`] when `shots` holds one shot, before any
/// run, or when, in trial 0, every shot gives the same output (so that
/// outputs in another order would pass), or when a run fails.
pub fn gives_one_output_per_shot_in_order<D>(
    mut make: impl FnMut() -> D,
    shots: &ShotSet,
    trials: Trials,
) -> Result<(), Nonconformance>
where
    D: Drive,
    D::Output: PartialEq + Debug,
{
    let rule = Rule::GivesOneOutputPerShotInOrder;
    trials.run(|patterns| {
        if shots.len() < 2 {
            return Err(patterns.cannot_judge(
                "`shots` holds one shot; an order needs two or more, whose outputs differ",
            ));
        }
        let mut findings = patterns.findings();
        let forward: Vec<ShotId> = patterns.shots(shots).iter().collect();
        let reversed: Vec<ShotId> = forward.iter().rev().copied().collect();
        let mut ordered = Vec::new();
        for (number, (order, ids)) in [
            ("in the shots' order", &forward),
            ("in the reverse order", &reversed),
        ]
        .into_iter()
        .enumerate()
        {
            let set = ShotSet::ids(ids.iter().map(|shot| shot.get()));
            let ran = run(&mut make, &set, RunSetup::default(), &mut findings)?;
            let outputs = ran
                .outputs
                .map_err(|failure| findings.failed(failed_run(&failure)))?;
            if outputs.len() != ids.len() {
                findings.fault(
                    rule,
                    Place::Run { run: number + 1 },
                    format!("{} outputs for {} shots, {order}", outputs.len(), ids.len()),
                );
                return findings.finish();
            }
            ordered.push((order, ids, outputs));
        }
        let mut alone = BTreeMap::new();
        for (index, &shot) in forward.iter().enumerate() {
            let ran = run(
                &mut make,
                &ShotSet::ids([shot.get()]),
                RunSetup::default(),
                &mut findings,
            )?;
            let mut outputs = ran
                .outputs
                .map_err(|failure| findings.failed(failed_run(&failure)))?;
            if outputs.len() == 1 {
                alone.insert(shot, outputs.remove(0));
            } else {
                findings.fault(
                    rule,
                    Place::Run { run: index + 3 },
                    format!("{} outputs for one shot, {shot}", outputs.len()),
                );
            }
        }
        for (order, ids, outputs) in &ordered {
            for (index, (shot, output)) in ids.iter().zip(outputs).enumerate() {
                let Some(alone) = alone.get(shot) else {
                    continue;
                };
                if output != alone {
                    findings.fault(
                        rule,
                        Place::Output { index, shot: *shot },
                        format!("{output:?} {order}; {alone:?} alone"),
                    );
                }
            }
        }
        let outputs = &ordered[0].2;
        if outputs.iter().all(|output| *output == outputs[0]) {
            findings.undetectable(
                "every shot gives the same output, so outputs in another order would pass; give \
                 shots whose outputs differ",
            );
        }
        findings.finish()
    })
}

/// Checks that the driver states each batch's readout slots as its
/// instructions count them ([`Rule::StatesReadoutSlots`]).
///
/// # Why it matters
///
/// The protocol shapes the readouts below by the stated count, and checks
/// it only for a checked batch, refusing the run. Misstated in an unchecked
/// batch, the count gives readouts of another shape than the instructions
/// fill: the part below leaves slots unwritten, or has no slot for an
/// outcome, and the driver reads values that belong to no instruction.
///
/// # How it checks
///
/// The kit runs the trial's shots once and reads each batch the driver
/// sends; a run the protocol refuses for a misstated count is reported too.
/// Trial 0 runs `shots` as given; each drawn trial gives them new ids, in a
/// new order (the module's [trials](crate::driver#the-trials)).
///
/// # Example
///
/// A driver that states one slot fewer than its second `execute`'s batches
/// read is reported, with unchecked batches, at each such batch, as
/// `execute 1, call 0 below, batch 0: states 0 readout slots; its
/// instructions count 1`; with checked ones, at the run the protocol
/// refused.
///
/// # What it does not see
///
/// A count misstated only for shots or programs the check does not run.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each batch whose count is misstated,
/// or the run refused for it. [`Nonconformance::CannotJudge`] when the driver
/// sends nothing, or the run fails otherwise.
pub fn states_readout_slots<D: Drive>(
    mut make: impl FnMut() -> D,
    shots: &ShotSet,
    trials: Trials,
) -> Result<(), Nonconformance> {
    trials.run(|patterns| {
        let mut findings = patterns.findings();
        let shots = patterns.shots(shots);
        let ran = run(&mut make, &shots, RunSetup::default(), &mut findings)?;
        if let Err(failure) = ran.outputs {
            if !misstates_slots(&failure) {
                return Err(findings.failed(failed_run(&failure)));
            }
            findings.fault(
                Rule::StatesReadoutSlots,
                Place::Run { run: 1 },
                format!("refused below: {failure}"),
            );
        }
        sends_something(&mut findings, &ran.log, "the driver");
        note_misstated(&mut findings, &ran.log.below);
        findings.finish()
    })
}

/// Checks that the driver sends only the run's shots
/// ([`Rule::SendsOnlyItsShots`]), each in at most one batch of an `execute`
/// ([`Rule::PutsEachShotInOneBatchPerCall`]).
///
/// # Why it matters
///
/// The part below holds a shot's state from its first batch to its end,
/// which the runner sends only for the run's shots. A shot not the run's is
/// made below and never ended, or changes another run's shot; a shot in two
/// batches of one `execute` gets its instructions in an order the part
/// below picks.
///
/// # How it checks
///
/// The kit runs the trial's shots once and reads each batch the driver
/// sends. Trial 0 runs `shots` as given; each drawn trial gives them new
/// ids, in a new order (the module's [trials](crate::driver#the-trials)).
///
/// # Example
///
/// A driver that sends, in its second `execute`, a shot not the run's is
/// reported as `execute 1, call 0 below, batch 2: holds shot 999, not the
/// run's`; one that puts a shot in two batches of an `execute`, as
/// `execute 1, call 0 below, batch 1: holds shot 0 that an earlier batch
/// of its call holds`.
///
/// # What it does not see
///
/// Shots the driver adds only for programs or shots the check does not
/// run.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each batch that holds another shot or
/// a shot twice in its `execute`. [`Nonconformance::CannotJudge`] when the
/// driver sends nothing, or the run fails.
pub fn sends_only_its_shots<D: Drive>(
    mut make: impl FnMut() -> D,
    shots: &ShotSet,
    trials: Trials,
) -> Result<(), Nonconformance> {
    trials.run(|patterns| {
        let mut findings = patterns.findings();
        let shots = patterns.shots(shots);
        let ran = run(&mut make, &shots, RunSetup::default(), &mut findings)?;
        if let Err(failure) = ran.outputs {
            return Err(findings.failed(failed_run(&failure)));
        }
        sends_something(&mut findings, &ran.log, "the driver");
        for below in &ran.log.below {
            let others: Vec<String> = below
                .shots
                .iter()
                .filter(|shot| !shots.iter().any(|own| own == **shot))
                .map(ToString::to_string)
                .collect();
            if !others.is_empty() {
                findings.fault(
                    Rule::SendsOnlyItsShots,
                    below.place(),
                    format!("holds {}, not the run's", shot_list(&others)),
                );
            }
        }
        findings.faults(
            Rule::PutsEachShotInOneBatchPerCall,
            repeated_shots(&ran.log.below),
        );
        findings.finish()
    })
}

/// Checks that the driver passes the runner's seed below unchanged
/// ([`Rule::PassesTheSeedOn`]).
///
/// # Why it matters
///
/// The parts below draw a shot's random values from the seed and the
/// shot's id, so that a run can be replayed. A driver that sends a seed of
/// its own, fixed or new for each `execute`, ties the records below to it:
/// two runs with one seed differ, or runs with different seeds agree.
///
/// # How it checks
///
/// The kit's own driver, run first on the trial's shots with the same seed,
/// shows the seed the runner gives; then the kit runs the driver and
/// compares each batch's seed with it. Trial 0 runs `shots` as given; each
/// drawn trial gives them new ids, in a new order, with a seed of its own
/// (the module's [trials](crate::driver#the-trials)).
///
/// # Example
///
/// A driver that sends its second `execute` with a seed derived from the
/// run's is reported as `execute 1, call 0 below, batch 0: sent with
/// another seed than the runner gave`.
///
/// # What it does not see
///
/// How the driver uses the seed itself, for its own draws.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each batch sent with another seed.
/// [`Nonconformance::CannotJudge`] when the driver sends nothing, or a run
/// fails.
pub fn passes_the_seed_on<D: Drive>(
    mut make: impl FnMut() -> D,
    shots: &ShotSet,
    trials: Trials,
) -> Result<(), Nonconformance> {
    trials.run(|patterns| {
        let mut findings = patterns.findings();
        let shots = patterns.shots(shots);
        let given = seed_from_runner(&mut make, &shots, &mut findings)?;
        let ran = run(&mut make, &shots, RunSetup::default(), &mut findings)?;
        if let Err(failure) = ran.outputs {
            return Err(findings.failed(failed_run(&failure)));
        }
        sends_something(&mut findings, &ran.log, "the driver");
        for below in &ran.log.below {
            if below.seed != given {
                findings.fault(
                    Rule::PassesTheSeedOn,
                    below.place(),
                    "sent with another seed than the runner gave",
                );
            }
        }
        findings.finish()
    })
}

/// Checks that the driver merges each `execute`'s conditions into its
/// shot's output, and no other shot's, and raises none because of one read
/// below ([`Rule::MergesEachExecutesConditions`]).
///
/// # Why it matters
///
/// A part below reports in each row only that `execute`'s conditions, so
/// the driver is where a shot's conditions over the run are joined, and the
/// caller decides on them: it drops a `REJECTED` shot. A driver that keeps
/// only the latest round's conditions loses a rejection read earlier, and a
/// bad outcome is taken as good; one that merges another shot's conditions
/// rejects good outcomes.
///
/// # How it checks
///
/// The first run raises no condition, and shows which `execute`s send each
/// shot. Then the kit runs the driver in runs that each raise conditions on
/// half of the shots: for each bit of a shot's position, the shots whose
/// bit is set, then the others (one run for one shot, two for two, four
/// for three or four, six for up to eight). So any two shots are
/// apart in some run. The output of a shot raised on, read by
/// `conditions_of`, must hold every kind raised on it, and no condition
/// beyond those and the ones its output holds in the first run, since a
/// part raises no condition because of one it read below
/// ([`ShotConditions`]). The output of a shot not raised on must hold no
/// condition its output in the first run does not.
///
/// In trial 0, the kit's executor raises one kind on each raised shot in
/// each `execute` that sends it, in turn: `REJECTED`, `INCONSISTENT`,
/// `UNRESOLVED`, then again, starting one kind later for each next
/// position; and a last run raises every kind on each shot the first run
/// sends in two `execute`s or more, in the first of them only, so that the
/// later ones read none. A driver that sends such a shot in no later
/// `execute` in that run, as one that stops running a rejected shot does,
/// is held to the same bounds there as in the other runs. Each drawn trial
/// gives the shots new ids, in a new order, orders the positions anew
/// before taking the halves, and raises one kind or more drawn for each
/// `execute` and shot (the module's [trials](crate::driver#the-trials)).
/// So a driver that keeps only some `execute`s' conditions, or clears them
/// when an `execute` reads none, or gives an output another shot's, in
/// place of its own or added to them, shows. The check reports each output
/// once, in the order of the shots.
///
/// # Example
///
/// A driver that keeps only the latest conditions read on a shot is
/// reported as `output 0 (shot 0): holds ShotConditions(INCONSISTENT); the
/// executes that sent it read ShotConditions(REJECTED | INCONSISTENT) (in
/// executes [0, 1], raised on shots 0, 2)`.
///
/// A driver that flags as `INCONSISTENT` a shot it read `REJECTED` below is
/// reported as `output 2 (shot 2): holds ShotConditions(REJECTED |
/// INCONSISTENT | UNRESOLVED), though the executes that sent it read only
/// ShotConditions(REJECTED | UNRESOLVED) (in executes [0, 1], raised on
/// shots 0, 2)`.
///
/// # What it does not see
///
/// A driver that drops outputs, as one that post-selects does: the check
/// pairs outputs with shots by position, so it cannot judge it. And the
/// driver's own conditions, judged only against the first run's. The run
/// that leaves later `execute`s clean raises on a shot's first `execute`
/// only, so a driver that clears a shot's conditions at a clean `execute`
/// only after several that read some is not seen.
///
/// # Errors
///
/// [`Nonconformance::Broken`] naming each output that lacks some, or
/// holds another shot's or one raised because of one read below.
/// [`Nonconformance::CannotJudge`] when, in trial 0, no shot is sent in two
/// `execute`s or more (so that dropping earlier conditions would not show),
/// or when the driver gives another number of outputs than shots, or a run
/// fails.
pub fn merges_each_executes_conditions<D: Drive>(
    mut make: impl FnMut() -> D,
    shots: &ShotSet,
    conditions_of: impl Fn(&D::Output) -> ShotConditions,
    trials: Trials,
) -> Result<(), Nonconformance> {
    let rule = Rule::MergesEachExecutesConditions;
    trials.run(|patterns| {
        let mut findings = patterns.findings();
        let shots = patterns.shots(shots);
        let plain = run(&mut make, &shots, RunSetup::default(), &mut findings)?;
        let plain_outputs = plain
            .outputs
            .map_err(|failure| findings.failed(failed_run(&failure)))?;
        let ids: Vec<ShotId> = shots.iter().collect();
        if plain_outputs.len() != ids.len() {
            return Err(findings.failed(unpaired(plain_outputs.len(), ids.len())));
        }
        let in_two = ids.iter().any(|&shot| sent_in(&plain.log, shot).len() > 1);
        // Each output's first fault, by its index.
        let mut shown = BTreeMap::new();
        for RaisingRun {
            raised_on,
            raises,
            only,
        } in raising_runs(patterns, &plain.log, &ids)
        {
            let setup = RunSetup {
                raises,
                ..RunSetup::default()
            };
            let raising = run(&mut make, &shots, setup.clone(), &mut findings)?;
            let outputs = raising
                .outputs
                .map_err(|failure| findings.failed(failed_run(&failure)))?;
            if outputs.len() != ids.len() {
                return Err(findings.failed(unpaired(outputs.len(), ids.len())));
            }
            let on = raised_on
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
                + only;
            for (index, ((&shot, output), plain)) in
                ids.iter().zip(&outputs).zip(&plain_outputs).enumerate()
            {
                let place = Place::Output { index, shot };
                let found = conditions_of(output);
                let fault = if raised_on.contains(&shot) {
                    let executes = sent_in(&raising.log, shot);
                    // Below a driver, each of its `execute`s is one call.
                    let expected = executes.iter().fold(ShotConditions::NONE, |all, &execute| {
                        all | setup.raised(execute, 0, shot)
                    });
                    let bound = conditions_of(plain) | expected;
                    if found.contains(expected) {
                        (!bound.contains(found)).then(|| {
                            format!(
                                "holds {found:?}, though the executes that sent it read only \
                                 {expected:?} (in executes {executes:?}, raised on shots {on})"
                            )
                        })
                    } else {
                        Some(format!(
                            "holds {found:?}; the executes that sent it read {expected:?} (in \
                             executes {executes:?}, raised on shots {on})"
                        ))
                    }
                } else {
                    let expected = conditions_of(plain);
                    (!expected.contains(found)).then(|| {
                        format!(
                            "holds {found:?}, though conditions were raised only on shots \
                             {on}, not on its own; {expected:?} when none were raised"
                        )
                    })
                };
                if let Some(fault) = fault {
                    shown.entry(index).or_insert((place, fault));
                }
            }
        }
        for (place, fault) in shown.into_values() {
            findings.fault(rule, place, fault);
        }
        if !in_two {
            findings.undetectable(
                "the driver sends no shot in two executes or more, so dropping earlier \
                 conditions would not show",
            );
        }
        findings.finish()
    })
}

// ------------------------------------------------------------- the steps

/// What a run of a driver gave: its outputs or its failure, and what the
/// kit's executor saw.
struct DriverRun<O> {
    outputs: Result<Vec<O>, RunError>,
    log: RunLog,
}

/// The driver's `execute`s that sent `shot` in the run `log` shows, in
/// order.
fn sent_in(log: &RunLog, shot: ShotId) -> Vec<usize> {
    let mut executes: Vec<usize> = log
        .below
        .iter()
        .filter(|below| below.shots.contains(&shot))
        .map(|below| below.execute)
        .collect();
    executes.dedup();
    executes
}

/// A run of [`merges_each_executes_conditions`] that raises conditions:
/// the shots it raises on, its raises, and how it raises, for the report.
struct RaisingRun {
    raised_on: Vec<ShotId>,
    raises: Vec<Raise>,
    /// Added to the list of shots raised on in a report.
    only: &'static str,
}

/// The runs of [`merges_each_executes_conditions`] that raise conditions
/// on the shots `ids`, given `plain`, the log of a run that raised none:
/// for each half, each raised shot raised on in every `execute` that sends
/// it, kinds in turn (or drawn); then, in trial 0, every shot sent in two
/// `execute`s or more raised on with every kind in the first of them only,
/// so that a later one reads none.
fn raising_runs(patterns: &mut TrialPatterns, plain: &RunLog, ids: &[ShotId]) -> Vec<RaisingRun> {
    let mut runs = Vec::new();
    for half in patterns.raising_halves(ids.len()) {
        let raised_on: Vec<ShotId> = half.iter().map(|&position| ids[position]).collect();
        let mut raises = Vec::new();
        for &position in &half {
            let shot = ids[position];
            for (turn, execute) in sent_in(plain, shot).into_iter().enumerate() {
                let fixed = Raised::InEveryCall(raised_in_turn(turn, position));
                raises.push(Raise {
                    execute,
                    shot,
                    raised: patterns.raised(fixed),
                });
            }
        }
        runs.push(RaisingRun {
            raised_on,
            raises,
            only: "",
        });
    }
    if patterns.is_fixed() {
        let mut raised_on = Vec::new();
        let mut raises = Vec::new();
        for &shot in ids {
            if let [first, _, ..] = sent_in(plain, shot)[..] {
                raised_on.push(shot);
                raises.push(Raise {
                    execute: first,
                    shot,
                    raised: Raised::InEveryCall(every_kind()),
                });
            }
        }
        if !raises.is_empty() {
            runs.push(RaisingRun {
                raised_on,
                raises,
                only: " in their first execute only",
            });
        }
    }
    runs
}

/// Why [`merges_each_executes_conditions`] cannot judge a driver that gave
/// `outputs` outputs for `shots` shots.
fn unpaired(outputs: usize, shots: usize) -> String {
    format!(
        "{outputs} outputs for {shots} shots, so the check cannot pair them; a driver that \
         drops rejected shots cannot be judged, and any other should check \
         `gives_one_output_per_shot_in_order` first"
    )
}

/// `seed.shot(0)` of the seed the runner gives a run of `shots`: the
/// kit's driver, in the driver's place, notes it, and sends nothing.
fn seed_from_runner<D: Drive>(
    make: &mut impl FnMut() -> D,
    shots: &ShotSet,
    findings: &mut Findings,
) -> Result<u64, Nonconformance> {
    let shared_run = SharedRun::new(RunSetup::default());
    let driver = make();
    let kits_driver =
        ScriptedDriver::new(shared_run.clone(), driver.sends().clone(), driver.needs());
    over_kits_executor(kits_driver, &shared_run, shots, findings)?
        .map_err(|failure| findings.failed(failed_run(&failure)))?;
    Ok(shared_run
        .log()
        .seed_from_runner
        .expect("the kit's driver notes the seed it got"))
}

/// Runs a new driver over the kit's executor on `shots`, set up as
/// `setup`, and notes the driver's name in `findings`; or, if the chain
/// does not compose, gives the check's result.
fn run<D: Drive>(
    make: &mut impl FnMut() -> D,
    shots: &ShotSet,
    setup: RunSetup,
    findings: &mut Findings,
) -> Result<DriverRun<D::Output>, Nonconformance> {
    let shared_run = SharedRun::new(setup);
    let driver = make();
    findings.note_part_name(driver.name());
    let outputs = over_kits_executor(driver, &shared_run, shots, findings)?;
    Ok(DriverRun {
        outputs,
        log: shared_run.log(),
    })
}

/// Runs `top` over the kit's executor, which shares `run`, on `shots`:
/// its outputs or its failure; or, if the chain does not compose, the
/// check's result.
fn over_kits_executor<T: Drive>(
    top: T,
    run: &SharedRun,
    shots: &ShotSet,
    findings: &mut Findings,
) -> Result<Result<Vec<T::Output>, RunError>, Nonconformance> {
    let below = RecordingExecutor::below_a_driver(run.clone(), top.sends().clone());
    let mut runner = (Driver::new(top) >> Executor::new(below))
        .map_err(|faults| findings.failed(uncomposed(&faults)))?;
    Ok(runner.run(shots, findings.run_seed()))
}
