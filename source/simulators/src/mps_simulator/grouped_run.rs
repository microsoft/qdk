// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The grouped run: how an [`MpsResumer`] runs the shots of one run, shots
//! that drew the same values sharing one state.
//!
//! The phases of a group, each a function or a call of the same name here
//! or in the file its entry names, are in `src/mps_simulator/design.md`
//! ("The phases of a group"), with how and why, as is the bound on the
//! states a run holds; this header gives the mechanics that page does not.
//!
//! **The loop.** [`GroupedRun::start`] makes one group of every shot, with
//! one machine context and no state. `run` pops the groups off a stack, the
//! last split off first, until none waits; `run_group` runs one group:
//! its steps, then a bond read, then the context resumed with the
//! outcomes, whose next batch it lowers into the next steps, until the
//! context ends. `prepare` makes the group's state at the first
//! step that needs it, and a split pushes the group that waits, with a
//! cloned context and a forked state, onto the stack.
//!
//! **Failures.** `fail` keeps the failure of the first shot in the order of
//! the shots; a group's shots keep that order, so a group whose first shot
//! comes after the failing shot is dropped: `run` drops it unrun when it
//! waits, and `run_group` stops it after the step whose split leaves it
//! only such shots. Where the stop flag is read is
//! [`MpsResumer::with_stop_flag`]'s rule; `check_stop_flag`'s callers apply
//! it.

use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::OutputRecord;
use crate::qir::{
    QirGate, QirMachine, QirOutcome, QirProgram, QirReadout, QirResumeError, QirShotContext,
    QirShotId, QirStop, QirTrap,
};
use tensornet::{MpsEngine, MpsShape};

#[cfg(doc)]
use super::MpsResumer;
use super::lowering::{SiteGate, SiteStep, lower};
use super::mps_resumer_error::MpsResumerError;
use super::mps_run_report::MpsRunReport;
use super::qubit_order::QubitOrder;
use super::shot_stream::{SplitMix64, shot_stream};

/// One run of an [`MpsResumer`]: what every group of the run shares, the
/// groups that wait their turn, and the first failure.
pub(super) struct GroupedRun<'r, E: MpsEngine> {
    engine: &'r E,
    machine: &'r QirMachine<'r>,
    /// The run's shots, which a failure is named by.
    shots: &'r [QirShotId],
    qubit_order: QubitOrder,
    /// The capacity of each state; `None` for a program of no qubits, which
    /// needs no state.
    capacity: Option<MpsShape>,
    /// The flag that stops the run, if any.
    stop_flag: Option<&'r AtomicBool>,
    report: &'r mut MpsRunReport,
    /// The groups split off that wait their turn, the last to run next.
    waiting: Vec<ShotGroup<E::State>>,
    /// Each shot's records, at its place in the order of the run's shots,
    /// once its group ended.
    records: Vec<Option<Vec<OutputRecord>>>,
    /// The first failing shot's place in the order of the run's shots, and
    /// why it failed.
    failure: Option<(usize, GroupFailure<E::Error>)>,
}

impl<'r, E: MpsEngine> GroupedRun<'r, E> {
    /// The run of `shots` of `machine`'s program on `engine`, before any
    /// shot runs: the qubit order and the capacity, from the program, and
    /// one group of every shot, each drawing from its stream under `seed`,
    /// stopping once `stop_flag`, if any, is set
    /// ([`MpsResumer::with_stop_flag`]).
    /// `shots` is not empty: a run of no shots needs neither the qubit order
    /// nor the capacity, so [`MpsResumer`] does not start it.
    ///
    /// # Errors
    ///
    /// [`MpsResumerError::Capacity`], naming the first shot, when the
    /// program's qubits and `max_bond_dimension` make no chain.
    pub(super) fn start(
        engine: &'r E,
        machine: &'r QirMachine<'r>,
        shots: &'r [QirShotId],
        seed: u64,
        max_bond_dimension: NonZeroUsize,
        stop_flag: Option<&'r AtomicBool>,
        report: &'r mut MpsRunReport,
    ) -> Result<Self, QirResumeError<MpsResumerError<E::Error>>> {
        let program = machine.program();
        let &first_shot = shots
            .first()
            .expect("the resumer starts no run of no shots");
        let capacity =
            capacity_of(program, max_bond_dimension).map_err(|error| QirResumeError::Resumer {
                shot: first_shot,
                error,
            })?;
        let waiting = vec![ShotGroup {
            shots: shots
                .iter()
                .enumerate()
                .map(|(place, &id)| Shot {
                    place,
                    stream: shot_stream(seed, id),
                })
                .collect(),
            context: machine.start_shot(),
            state: None,
            outcomes: Vec::new(),
            steps: Rc::from([]),
            next: 0,
            bond_unread: false,
        }];
        Ok(Self {
            engine,
            machine,
            shots,
            qubit_order: QubitOrder::for_program(program),
            capacity,
            stop_flag,
            report,
            waiting,
            records: vec![None; shots.len()],
            failure: None,
        })
    }

    /// Runs the groups one at a time, the last split off first, until none
    /// waits, and gives each shot's records, or the first failing shot.
    pub(super) fn run(
        mut self,
    ) -> Result<Vec<Vec<OutputRecord>>, QirResumeError<MpsResumerError<E::Error>>> {
        while let Some(mut group) = self.waiting.pop() {
            if self.comes_after_the_failure(&group) {
                continue;
            }
            match self.run_group(&mut group) {
                Ok(Some(ended)) => {
                    for shot in &group.shots {
                        self.records[shot.place] = Some(ended.clone());
                    }
                }
                Ok(None) => {}
                // The group's shots now, which failed: a split may have
                // moved its first shots to a group of their own.
                Err(failed) => self.fail(group.shots[0].place, failed),
            }
        }
        match self.failure {
            Some((place, failed)) => {
                let shot = self.shots[place];
                Err(match failed {
                    GroupFailure::Trapped(trap) => QirResumeError::Trapped { shot, trap },
                    GroupFailure::Resumer(error) => QirResumeError::Resumer { shot, error },
                })
            }
            None => Ok(self
                .records
                .into_iter()
                .map(|records| records.expect("every shot ended or the run failed"))
                .collect()),
        }
    }

    /// Keeps `failure` as the run's when the shot at `place` comes before
    /// every shot that failed so far.
    fn fail(&mut self, place: usize, failure: GroupFailure<E::Error>) {
        if self
            .failure
            .as_ref()
            .is_none_or(|(first, _)| place < *first)
        {
            self.failure = Some((place, failure));
        }
    }

    /// Whether a shot before every shot of `group` failed: the run's result
    /// is then that failure, whatever `group` does, so `group` is dropped.
    fn comes_after_the_failure(&self, group: &ShotGroup<E::State>) -> bool {
        // A group's shots are in the order of the run's: a split keeps it.
        self.failure
            .as_ref()
            .is_some_and(|(place, _)| *place < group.shots[0].place)
    }

    /// Runs `group` until its context ends, giving its records; the groups
    /// that split off it wait on [`Self::waiting`]. Gives `None`, and stops,
    /// after a step whose split leaves `group` only shots that come after
    /// the run's failure, whether settling the fork failed or not: the step
    /// has settled `group`'s own value, and no later step runs.
    fn run_group(
        &mut self,
        group: &mut ShotGroup<E::State>,
    ) -> Result<Option<Vec<OutputRecord>>, GroupFailure<E::Error>> {
        loop {
            self.check_stop_flag()?;
            while let Some(step) = group.steps.get(group.next).cloned() {
                group.next += 1;
                self.run_step(&step, group)?;
                if self.comes_after_the_failure(group) {
                    return Ok(None);
                }
            }
            self.read_bond(group)?;
            let stop = self.machine.resume(&mut group.context, &group.outcomes);
            // The stop flag is read again before the batch is lowered or a
            // trap reported; an ended context keeps its records.
            if !matches!(stop, Ok(QirStop::Ended(_))) {
                self.check_stop_flag()?;
            }
            match stop.map_err(GroupFailure::Trapped)? {
                QirStop::Ended(records) => {
                    self.end(group.state.as_ref())?;
                    return Ok(Some(records.to_vec()));
                }
                QirStop::Batch(calls) => {
                    group.steps = lower(calls, &self.qubit_order)?.into();
                    group.next = 0;
                    group.outcomes.clear();
                }
            }
        }
    }

    /// Fails with [`MpsResumerError::Stopped`] once the stop flag is set.
    fn check_stop_flag(&self) -> Result<(), GroupFailure<E::Error>> {
        if self
            .stop_flag
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err(GroupFailure::Resumer(MpsResumerError::Stopped));
        }
        Ok(())
    }

    /// Runs `step` on `group`: a gate is applied, a measurement or a reset
    /// drawn. A draw reads the largest bond first ([`Self::read_bond`]), as
    /// projecting may shrink it; a gate does not.
    fn run_step(
        &mut self,
        step: &SiteStep,
        group: &mut ShotGroup<E::State>,
    ) -> Result<(), GroupFailure<E::Error>> {
        match *step {
            SiteStep::Apply(ref gate) => self.apply(group, gate),
            SiteStep::Measure {
                site,
                result,
                reset,
            } => self.draw(group, site, reset, Some(result)),
            SiteStep::Reset { site } => self.draw(group, site, true, None),
        }
    }

    /// Applies `gate` to `group`'s state, prepared at the group's first
    /// step: one engine call per gate; an engine that gains by fusing gates
    /// fuses them itself.
    fn apply(
        &self,
        group: &mut ShotGroup<E::State>,
        gate: &SiteGate,
    ) -> Result<(), GroupFailure<E::Error>> {
        let state = self.prepare(&mut group.state)?;
        group.bond_unread = true;
        self.engine
            .apply(state, gate.sites(), gate.matrix.row_major())
            .map_err(engine_failure)
    }

    /// Each shot of `group` draws the value of `site`; when they drew both
    /// values, the group splits ([`Self::split`]). The state of the group that
    /// goes on, still `group`, keeps its value, and is returned to |0⟩ when
    /// `reset`; `result`, when given, is answered with the value.
    fn draw(
        &mut self,
        group: &mut ShotGroup<E::State>,
        site: usize,
        reset: bool,
        result: Option<u32>,
    ) -> Result<(), GroupFailure<E::Error>> {
        self.read_bond(group)?;
        let state = self.prepare(&mut group.state)?;
        let p_one = self
            .engine
            .probability_of_one(state, site)
            .map_err(engine_failure)?;
        // A uniform is in [0, 1): One only when P(One) > 0, Zero only when
        // P(One) < 1; a P(One) rounded past 0 or 1 draws as a P(One) of 0
        // or 1 would. An impossible value that rounding gives a tiny
        // probability needs no threshold: a shot draws it with that tiny
        // probability only, as a measurement draws and does not post-select.
        let draws: Vec<bool> = group
            .shots
            .iter_mut()
            .map(|shot| shot.stream.next_uniform() < p_one)
            .collect();
        let one = value_going_on(&draws);
        if draws.iter().any(|&drew| drew != one) {
            self.split(group, &draws, one, site, reset, result)?;
        }
        let state = group.state.as_mut().expect("prepared above");
        self.settle(state, site, one, reset)?;
        group.outcomes.extend(answer(result, one));
        Ok(())
    }

    /// Splits `group` by its shots' `draws` of `site`: the shots that drew
    /// `one` stay in `group`, and the others wait, with a fork of the state
    /// that keeps their value and a copy of the context. The fork is made
    /// before `group`'s state is settled, after its P(One) is read: so it
    /// copies the state a shot alone would project.
    ///
    /// When settling the fork ([`Self::settle`]: the projection, and a
    /// reset's X) fails, `group`'s state was not touched, so the failure
    /// fails the waiting shots only (kept in the run's failure), and
    /// `group` goes on, unless its shots all come after them.
    ///
    /// # Errors
    ///
    /// The engine's failure to fork, which fails all of `group`'s shots,
    /// before they split: the engine leaves the state it failed on
    /// unspecified (the `MpsEngine` contract), so no group may go on with
    /// it, and `run` drops it with `group`.
    fn split(
        &mut self,
        group: &mut ShotGroup<E::State>,
        draws: &[bool],
        one: bool,
        site: usize,
        reset: bool,
        result: Option<u32>,
    ) -> Result<(), GroupFailure<E::Error>> {
        let state = group
            .state
            .as_ref()
            .expect("a group that draws holds a state");
        let mut fork = self.engine.fork(state).map_err(engine_failure)?;
        let (mut going_on, mut waiting) = (Vec::new(), Vec::new());
        for (shot, &drew) in std::mem::take(&mut group.shots).into_iter().zip(draws) {
            if drew == one {
                going_on.push(shot);
            } else {
                waiting.push(shot);
            }
        }
        group.shots = going_on;
        match self.settle(&mut fork, site, !one, reset) {
            Ok(()) => {
                let mut outcomes = group.outcomes.clone();
                outcomes.extend(answer(result, !one));
                self.waiting.push(ShotGroup {
                    shots: waiting,
                    context: group.context.clone(),
                    state: Some(fork),
                    outcomes,
                    steps: Rc::clone(&group.steps),
                    next: group.next,
                    bond_unread: false,
                });
            }
            // Only the shots that wait met the failure, on the fork, which
            // is dropped: those going on run on, unless they all come after
            // it (`run_group` then stops them), and may end, or fail first
            // in the order.
            Err(failed) => self.fail(waiting[0].place, failed),
        }
        Ok(())
    }

    /// Keeps the value One (`one` true) or Zero (`one` false) of `site` in
    /// `state`, projecting on it; then, when `reset` and the value is One,
    /// flips `site` back to |0⟩.
    fn settle(
        &self,
        state: &mut E::State,
        site: usize,
        one: bool,
        reset: bool,
    ) -> Result<(), GroupFailure<E::Error>> {
        self.engine
            .project(state, site, one)
            .map_err(engine_failure)?;
        if reset && one {
            let x = QirGate::X.matrix_at(None);
            self.engine
                .apply(state, &[site], x.row_major())
                .map_err(engine_failure)?;
        }
        Ok(())
    }

    /// Keeps the largest bond of `group`'s state in the report, when gates
    /// were applied since it was last read: so it is read where a sequence
    /// of gates ends, before the draws that may shrink it.
    fn read_bond(&mut self, group: &mut ShotGroup<E::State>) -> Result<(), GroupFailure<E::Error>> {
        if let (true, Some(state)) = (group.bond_unread, &group.state) {
            let bond = self
                .engine
                .shape(state)
                .map_err(engine_failure)?
                .largest_bond();
            self.report.largest_bond = self.report.largest_bond.max(bond);
            group.bond_unread = false;
        }
        Ok(())
    }

    /// The group's state, prepared within the capacity at the group's first
    /// step, which needs it.
    fn prepare<'s>(
        &self,
        state: &'s mut Option<E::State>,
    ) -> Result<&'s mut E::State, GroupFailure<E::Error>> {
        if state.is_none() {
            let capacity = self
                .capacity
                .as_ref()
                .expect("a call names a qubit, so the program has qubits");
            *state = Some(self.engine.prepare(capacity).map_err(engine_failure)?);
        }
        Ok(state.as_mut().expect("prepared above"))
    }

    /// Keeps the retained weight of the state of a group whose context
    /// ended, if it has one, in the report, when it is the least so far.
    fn end(&mut self, state: Option<&E::State>) -> Result<(), GroupFailure<E::Error>> {
        if let Some(state) = state {
            let weight = self.engine.retained_weight(state).map_err(engine_failure)?;
            let least = &mut self.report.least_retained_weight;
            *least = least.min(weight);
        }
        Ok(())
    }
}

/// The shots of one group: their context, their state (`None` until the
/// first step needs it), the outcomes of the batch they are in, and its
/// steps, from `next` on still to run; `bond_unread` when gates were
/// applied since its largest bond was read.
struct ShotGroup<S> {
    shots: Vec<Shot>,
    context: QirShotContext,
    state: Option<S>,
    outcomes: Vec<QirOutcome>,
    steps: Rc<[SiteStep]>,
    next: usize,
    bond_unread: bool,
}

/// A shot of a group: its place among the run's shots and its stream.
struct Shot {
    place: usize,
    stream: SplitMix64,
}

/// Why a group failed: its context trapped, or the resumer failed it.
enum GroupFailure<E> {
    Trapped(QirTrap),
    Resumer(MpsResumerError<E>),
}

impl<E> From<MpsResumerError<E>> for GroupFailure<E> {
    fn from(error: MpsResumerError<E>) -> Self {
        Self::Resumer(error)
    }
}

/// The capacity of each state of a run of `program`: a chain of one site
/// per qubit and bonds of at most `max_bond_dimension`; `None` for a
/// program of no qubits, which needs no state.
fn capacity_of<E>(
    program: &QirProgram,
    max_bond_dimension: NonZeroUsize,
) -> Result<Option<MpsShape>, MpsResumerError<E>> {
    let qubits = program.qubits();
    if qubits == 0 {
        return Ok(None);
    }
    let sites = usize::try_from(qubits).expect("a u32 fits a usize");
    MpsShape::qubit_chain(sites, max_bond_dimension.get())
        .map(Some)
        .map_err(|source| MpsResumerError::Capacity {
            qubits,
            max_bond_dimension: max_bond_dimension.get(),
            source,
        })
}

/// The value of the shots that go on as the group, given each shot's draw
/// (One when true): the value they all drew, or, where they differ, the
/// value of the fewer. So a group that splits keeps at most half its
/// shots, and a run holds few states at once. A tie goes to Zero, so which
/// group goes on depends only on the draws: no record and no bound depends
/// on it, but which group runs first does, and with it which groups still
/// run after a failure, and which shots a failure settling the fork fails
/// (see `split`).
fn value_going_on(draws: &[bool]) -> bool {
    let ones = draws.iter().filter(|&&one| one).count();
    if ones == 0 || ones == draws.len() {
        ones > 0
    } else {
        2 * ones < draws.len()
    }
}

/// The outcome that answers `result`, when one is given, with One (`one`
/// true) or Zero (`one` false).
fn answer(result: Option<u32>, one: bool) -> Option<QirOutcome> {
    result.map(|result| QirOutcome {
        result,
        value: if one {
            QirReadout::One
        } else {
            QirReadout::Zero
        },
    })
}

fn engine_failure<E>(error: E) -> GroupFailure<E> {
    GroupFailure::Resumer(MpsResumerError::Engine(error))
}
