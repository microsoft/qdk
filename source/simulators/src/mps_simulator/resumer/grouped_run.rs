// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The grouped run: how an [`MpsResumer`] runs the shots of one run, shots
//! that drew the same values sharing one state.
//!
//! **Shots share states while their histories agree.** A run starts with
//! one group holding every shot, one machine context and no state. At each
//! measurement and reset, every shot of the group draws a uniform from its
//! own stream, seeded from the resumer's seed and the shot's id, and reads
//! One when it is below the state's P(One). When the shots draw different
//! values, the group splits: the state is forked after P(One) is read (an
//! exact copy, so each copy is the state a shot alone would project) and
//! the context cloned, so each part goes on with its own value. Since a
//! shot draws one uniform per measurement and reset whatever group it is
//! in, and a group's state is the state each of its shots would hold
//! alone, a shot's records are those it gets alone: a run costs what its
//! distinct histories cost (one state for a program with no random draw,
//! one per shot at most).
//!
//! **How a group runs**, in the phases of the README, each a function or a
//! call of the same name. Once per run, [`GroupedRun::start`] chooses the
//! qubit order (`qubit_order.rs`) and the capacity. Then a group's context
//! is resumed to its next batch ([`QirMachine::resume`]), whose calls are
//! lowered to steps on sites (`lower`, in `lowering.rs`) and fused
//! ([`GateFusion::fuse`]). Each step runs on the group's state, which
//! `prepare` makes within the capacity at the first step: `apply` applies
//! an operator; at a measurement or reset, `read_bond` reads the largest
//! bond for the report, `draw` reads each shot's value, splitting the
//! group when they differ, and `settle` projects the state on the value.
//! The context is resumed with the outcomes, until it ends: then `end`
//! reads the state's retained weight.
//!
//! **Few states at once.** Where a group splits, the part with fewer shots
//! goes on (Zero's on a tie) and the other waits on a stack until the part
//! that went on ends: so each waiting part has at least as many shots as
//! all the groups that wait above it and the one that runs, and a run of N
//! shots holds at most ⌊log₂ N⌋ + 1 states at once, which bounds a GPU's
//! memory whatever the program draws.
//!
//! **Failures.** A shot fails with its group: its context traps, a call is
//! refused, or the engine fails. Where a group splits, the copy for the
//! part that waits is forked and projected after the shots are split, so
//! the copy's failure fails that part's shots only, and the part that goes on
//! runs on: it may end, or fail on its own. The run names the first
//! failing shot in the order of the shots, and the waiting groups whose
//! shots all come after it are dropped unrun.

use std::num::NonZeroUsize;
use std::rc::Rc;

use crate::OutputRecord;
use crate::mps_simulator::{MpsResumerError, MpsRunReport};
use crate::qir::{
    QirGate, QirMachine, QirOutcome, QirProgram, QirReadout, QirResumeError, QirShotContext,
    QirShotId, QirStop, QirTrap,
};
use tensornet::{MpsEngine, MpsShape};

#[cfg(doc)]
use super::MpsResumer;
use super::fusion::GateFusion;
use super::lowering::{Operator, Step, lower};
use super::qubit_order::QubitOrder;
use super::shot_stream::{SplitMix64, shot_stream};

/// One run of an [`MpsResumer`]: what every group of the run shares, the
/// groups that wait their turn, and the first failure.
pub(super) struct GroupedRun<'r, E: MpsEngine> {
    engine: &'r E,
    machine: &'r QirMachine<'r>,
    /// The run's shots, which a failure is named by.
    shots: &'r [QirShotId],
    order: QubitOrder,
    /// The capacity of each state; `None` for a program of no qubits, which
    /// needs no state.
    capacity: Option<MpsShape>,
    fusion: GateFusion,
    report: &'r mut MpsRunReport,
    /// The groups split off that wait their turn, the last to run next.
    waiting: Vec<Group<E::State>>,
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
    /// one group of every shot, each drawing from its stream under `seed`.
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
        fusion: GateFusion,
        report: &'r mut MpsRunReport,
    ) -> Result<Self, QirResumeError<MpsResumerError<E::Error>>> {
        let program = machine.program();
        let mut waiting = Vec::new();
        let mut capacity = None;
        if let Some(&first_shot) = shots.first() {
            capacity = capacity_of(program, max_bond_dimension).map_err(|error| {
                QirResumeError::Resumer {
                    shot: first_shot,
                    error,
                }
            })?;
            waiting.push(Group {
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
            });
        }
        Ok(Self {
            engine,
            machine,
            shots,
            order: QubitOrder::for_program(program),
            capacity,
            fusion,
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
            // A group's shots are in the order of the run's: a split keeps it.
            if self
                .failure
                .as_ref()
                .is_some_and(|(place, _)| *place < group.shots[0].place)
            {
                continue;
            }
            match self.run_group(&mut group) {
                Ok(ended) => {
                    for shot in &group.shots {
                        self.records[shot.place] = Some(ended.clone());
                    }
                }
                // The group's shots now, which failed: a split may have
                // moved its first shots to a part that went on apart.
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

    /// Runs `group` until its context ends, giving its records; the groups
    /// that split off it wait on [`Self::waiting`].
    fn run_group(
        &mut self,
        group: &mut Group<E::State>,
    ) -> Result<Vec<OutputRecord>, GroupFailure<E::Error>> {
        loop {
            while let Some(step) = group.steps.get(group.next).cloned() {
                group.next += 1;
                self.run_step(&step, group)?;
            }
            self.read_bond(group)?;
            match self
                .machine
                .resume(&mut group.context, &group.outcomes)
                .map_err(GroupFailure::Trapped)?
            {
                QirStop::Ended(records) => {
                    self.end(group.state.as_ref())?;
                    return Ok(records.to_vec());
                }
                QirStop::Batch(calls) => {
                    group.steps = self.fusion.fuse(lower(calls, &self.order)?).into();
                    group.next = 0;
                    group.outcomes.clear();
                }
            }
        }
    }

    /// Runs `step` on `group`: an operator is applied, a measurement or a
    /// reset drawn.
    fn run_step(
        &mut self,
        step: &Step,
        group: &mut Group<E::State>,
    ) -> Result<(), GroupFailure<E::Error>> {
        match *step {
            Step::Apply(ref operator) => self.apply(group, operator),
            Step::Measure {
                site,
                result,
                reset,
            } => self.draw(group, site, reset, Some(result)),
            Step::Reset { site } => self.draw(group, site, true, None),
        }
    }

    /// Applies `operator` to `group`'s state, prepared at the group's first
    /// step.
    fn apply(
        &self,
        group: &mut Group<E::State>,
        operator: &Operator,
    ) -> Result<(), GroupFailure<E::Error>> {
        let state = self.prepare(&mut group.state)?;
        group.bond_unread = true;
        self.engine
            .apply(state, operator.sites(), operator.matrix.row_major())
            .map_err(engine_failure)
    }

    /// Each shot of `group` draws the value of `site`; when they drew both
    /// values, the group splits ([`Self::split`]). The state of the part
    /// that goes on as `group` keeps its value, and is returned to |0⟩ when
    /// `reset`; `result`, when given, is answered with the value.
    fn draw(
        &mut self,
        group: &mut Group<E::State>,
        site: usize,
        reset: bool,
        result: Option<u32>,
    ) -> Result<(), GroupFailure<E::Error>> {
        self.read_bond(group)?;
        let state = self.prepare(&mut group.state)?;
        let p_one = self
            .engine
            .probability_of_one(state, site)
            .map_err(engine_failure)?
            .clamp(0.0, 1.0);
        // A uniform is in [0, 1): One only when P(One) > 0, Zero only when
        // P(One) < 1.
        let draws: Vec<bool> = group
            .shots
            .iter_mut()
            .map(|shot| shot.stream.next_uniform() < p_one)
            .collect();
        let one = value_going_on(&draws);
        if draws.iter().any(|&drew| drew != one) {
            self.split(group, &draws, one, site, reset, result);
        }
        let state = group.state.as_mut().expect("prepared above");
        self.settle(state, site, one, reset)?;
        group.outcomes.extend(answer(result, one));
        Ok(())
    }

    /// Splits `group` by its shots' `draws` of `site`: the shots that drew
    /// `one` stay in `group`, and the others wait, with a fork of the state
    /// that keeps their value and a copy of the context. When the fork, or
    /// its projection, fails, the failure fails the waiting shots only
    /// (kept in the run's failure), and `group` goes on.
    fn split(
        &mut self,
        group: &mut Group<E::State>,
        draws: &[bool],
        one: bool,
        site: usize,
        reset: bool,
        result: Option<u32>,
    ) {
        let (mut going_on, mut waiting) = (Vec::new(), Vec::new());
        for (shot, &drew) in std::mem::take(&mut group.shots).into_iter().zip(draws) {
            if drew == one {
                going_on.push(shot);
            } else {
                waiting.push(shot);
            }
        }
        group.shots = going_on;
        let state = group
            .state
            .as_ref()
            .expect("a group that draws holds a state");
        match self.fork_keeping(state, site, !one, reset) {
            Ok(fork) => {
                let mut outcomes = group.outcomes.clone();
                outcomes.extend(answer(result, !one));
                self.waiting.push(Group {
                    shots: waiting,
                    context: group.context.clone(),
                    state: Some(fork),
                    outcomes,
                    steps: Rc::clone(&group.steps),
                    next: group.next,
                    bond_unread: false,
                });
            }
            // Only the shots that wait met the failure: those going on run
            // on, and may end, or fail first in the order.
            Err(failed) => self.fail(waiting[0].place, failed),
        }
    }

    /// A fork of `state` keeping the value One (`one` true) or Zero (`one`
    /// false) of `site`, as [`Self::settle`] keeps it. Forked before
    /// `state` is settled, after its P(One) is read: so the fork is the
    /// state a shot alone would project.
    fn fork_keeping(
        &self,
        state: &E::State,
        site: usize,
        one: bool,
        reset: bool,
    ) -> Result<E::State, GroupFailure<E::Error>> {
        let mut fork = self.engine.fork(state).map_err(engine_failure)?;
        self.settle(&mut fork, site, one, reset)?;
        Ok(fork)
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
    /// were applied since it was last read: so it is read where a run of
    /// gates ends, before the draws that may shrink it.
    fn read_bond(&mut self, group: &mut Group<E::State>) -> Result<(), GroupFailure<E::Error>> {
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

/// Shots with one history: their context, their state (`None` until the
/// first step needs it), the outcomes of the batch they are in, and its
/// steps, from `next` on still to run; `bond_unread` when gates were
/// applied since its largest bond was read.
struct Group<S> {
    shots: Vec<Shot>,
    context: QirShotContext,
    state: Option<S>,
    outcomes: Vec<QirOutcome>,
    steps: Rc<[Step]>,
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
/// value of the fewer, Zero on a tie. So a group that splits keeps at
/// most half its shots, and a run holds few states at once.
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
