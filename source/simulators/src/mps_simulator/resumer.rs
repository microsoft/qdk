// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsResumer`]: the [`QirResumer`] that runs each shot's quantum calls on
//! a matrix product state an [`MpsEngine`] holds.
//!
//! A user runs a QIR program on an MPS through it: a CPU or a GPU engine
//! beneath, the max bond dimension χ and the gate fusion as settings, and
//! records that do not depend on which shots run together, as the
//! [`QirResumer::run`] contract requires. What a run cost, and whether it
//! lost enough weight to truncation to warn, is in its report
//! ([`MpsResumer::last_run`]).
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
//! **How a group runs.** Its context is resumed to its next batch; the
//! batch's calls are lowered to steps on sites (`resumer/lowering.rs`,
//! through the program's qubit order, `resumer/order.rs`) and fused
//! (`fusion.rs`); the steps run on the group's state, prepared within the
//! capacity at the first step; the context is resumed with the outcomes.
//!
//! **Few states at once.** Where a group splits, the part with fewer shots
//! goes on (Zero's on a tie) and the other waits on a stack until the part
//! that went on ends: so each waiting part has at least as many shots as
//! all the groups that wait above it and the one that runs, and a run of N
//! shots holds at most ⌊log₂ N⌋ + 1 states at once, which bounds a GPU's
//! memory whatever the program draws.
//!
//! **Failures.** A shot fails with its group: its context traps, a call is
//! refused, or the engine fails. The run names the first failing shot in
//! the order of the shots, and groups whose shots all come after it are
//! dropped unrun.

use std::num::NonZeroUsize;
use std::rc::Rc;

use crate::OutputRecord;
use crate::qir::{
    QirGate, QirMachine, QirOutcome, QirReadout, QirResumeError, QirResumer, QirShotContext,
    QirShotId, QirStop, QirTrap,
};
use tensornet::{MpsEngine, MpsShape};

pub(crate) mod lowering;
pub(crate) mod order;
pub(crate) mod splitmix64;

use super::{GateFusion, MpsResumerError, MpsRunReport};
use lowering::{Step, lower};
use order::QubitOrder;
use splitmix64::SplitMix64;

/// The max bond dimension χ by default: 128. H₂'s phase estimation, run on
/// a CPU engine during development, kept every bond it needed within it
/// (its largest was 24).
pub const DEFAULT_MAX_BOND_DIMENSION: NonZeroUsize =
    NonZeroUsize::new(128).expect("128 is not zero");

/// Runs a QIR program's shots on matrix product states held by the engine
/// `E`, shots that drew the same values sharing one state (see the module
/// documentation).
///
/// Each shot draws from its own stream, derived from the resumer's seed and
/// the shot's id as the executor protocol (planned) will derive the seed
/// of a chain's only participant that draws. So a shot's records depend
/// only on the program, the seed, the shot's id and the settings: not on
/// the shots beside it, nor on its place among them, nor on earlier runs.
/// The qubit order, chosen from the program's two-qubit gates, changes the
/// bonds and the time, not the records (to rounding, and within the
/// truncation error).
///
/// The cutoff below which an engine drops singular values is the engine's
/// own setting: the engine contract takes only a capacity (here
/// `MpsShape::qubit_chain(n, χ)`).
pub struct MpsResumer<E: MpsEngine> {
    engine: E,
    seed: u64,
    max_bond_dimension: NonZeroUsize,
    fusion: GateFusion,
    last_run: MpsRunReport,
}

impl<E: MpsEngine> MpsResumer<E> {
    /// The resumer running shots on `engine`, drawing from streams derived
    /// from `seed`, with bonds of at most [`DEFAULT_MAX_BOND_DIMENSION`] and
    /// gates fused in pairs ([`GateFusion::Pairs`]).
    pub fn new(engine: E, seed: u64) -> Self {
        Self {
            engine,
            seed,
            max_bond_dimension: DEFAULT_MAX_BOND_DIMENSION,
            fusion: GateFusion::default(),
            last_run: MpsRunReport::new(DEFAULT_MAX_BOND_DIMENSION.get()),
        }
    }

    /// The resumer preparing each state with bonds of at most
    /// `max_bond_dimension` (χ): an engine that truncates keeps its chain
    /// within it, losing the weight its report gives.
    #[must_use]
    pub fn with_max_bond_dimension(mut self, max_bond_dimension: NonZeroUsize) -> Self {
        self.max_bond_dimension = max_bond_dimension;
        self
    }

    /// The resumer grouping each run of gates into operators as `fusion`
    /// says.
    #[must_use]
    pub fn with_gate_fusion(mut self, fusion: GateFusion) -> Self {
        self.fusion = fusion;
        self
    }

    /// What the last run cost: the largest bond any of its states held
    /// where a run of gates ended, and the least weight kept by a shot that
    /// ended, with the warning they call for
    /// ([`MpsRunReport::truncation_warning`]). After a failed run, what the
    /// run read before it failed. Before any run, and after a run of no
    /// shot, that of a run that prepared no state.
    #[must_use]
    pub fn last_run(&self) -> &MpsRunReport {
        &self.last_run
    }

    /// The engine, for the tests that observe it.
    #[cfg(test)]
    pub(crate) fn engine(&self) -> &E {
        &self.engine
    }
}

impl<E: MpsEngine> QirResumer for MpsResumer<E> {
    type Error = MpsResumerError<E::Error>;

    /// Runs `shots` as [`QirResumer::run`] requires, shots that drew the
    /// same values sharing one state. It never waits: its future is ready
    /// when first polled.
    ///
    /// # Errors
    ///
    /// The first failing shot in the order of `shots`: its context trapped,
    /// or the resumer failed it ([`MpsResumerError`]): a noise or loss call,
    /// a gate on one qubit twice, a capacity that is no chain, or the
    /// engine's failure.
    async fn run(
        &mut self,
        machine: &QirMachine<'_>,
        shots: &[QirShotId],
    ) -> Result<Vec<Vec<OutputRecord>>, QirResumeError<Self::Error>> {
        self.run_shots(machine, shots)
    }
}

impl<E: MpsEngine> MpsResumer<E> {
    /// The run behind [`QirResumer::run`], which never waits: the groups
    /// of shots run one at a time, from a stack (see the module
    /// documentation).
    fn run_shots(
        &mut self,
        machine: &QirMachine<'_>,
        shots: &[QirShotId],
    ) -> Result<Vec<Vec<OutputRecord>>, QirResumeError<MpsResumerError<E::Error>>> {
        self.last_run = MpsRunReport::new(self.max_bond_dimension.get());
        let Some(&first_shot) = shots.first() else {
            return Ok(Vec::new());
        };
        let program = machine.program();
        let qubits = program.qubits();
        let capacity =
            if qubits == 0 {
                None
            } else {
                let sites = usize::try_from(qubits).expect("a u32 fits a usize");
                let capacity = MpsShape::qubit_chain(sites, self.max_bond_dimension.get())
                    .map_err(|source| QirResumeError::Resumer {
                        shot: first_shot,
                        error: MpsResumerError::Capacity {
                            qubits,
                            max_bond_dimension: self.max_bond_dimension.get(),
                            source,
                        },
                    })?;
                Some(capacity)
            };
        let mut run = Run {
            engine: &self.engine,
            order: QubitOrder::for_program(program),
            capacity,
            fusion: self.fusion,
            report: &mut self.last_run,
        };
        let mut pending = vec![Group {
            shots: shots
                .iter()
                .enumerate()
                .map(|(place, &id)| Shot {
                    place,
                    stream: shot_stream(self.seed, id),
                })
                .collect(),
            context: machine.start_shot(),
            state: None,
            outcomes: Vec::new(),
            steps: Rc::from([]),
            next: 0,
            grown: false,
        }];
        let mut records: Vec<Option<Vec<OutputRecord>>> = vec![None; shots.len()];
        // The first failing shot's place, and its error.
        let mut failure: Option<(usize, QirResumeError<MpsResumerError<E::Error>>)> = None;
        while let Some(mut group) = pending.pop() {
            // A group's shots are in the order of the run's: a split keeps it.
            if failure
                .as_ref()
                .is_some_and(|(place, _)| *place < group.shots[0].place)
            {
                continue;
            }
            match run.run_group(machine, &mut group, &mut pending) {
                Ok(ended) => {
                    for shot in &group.shots {
                        records[shot.place] = Some(ended.clone());
                    }
                }
                Err(failed) => {
                    // The group's shots now, which failed: a split may have
                    // moved its first shots to a part that went on apart.
                    let first = group.shots[0].place;
                    let shot = shots[first];
                    let error = match failed {
                        GroupFailure::Trapped(trap) => QirResumeError::Trapped { shot, trap },
                        GroupFailure::Resumer(error) => QirResumeError::Resumer { shot, error },
                    };
                    if failure.as_ref().is_none_or(|(place, _)| first < *place) {
                        failure = Some((first, error));
                    }
                }
            }
        }
        match failure {
            Some((_, error)) => Err(error),
            None => Ok(records
                .into_iter()
                .map(|records| records.expect("every shot ended or the run failed"))
                .collect()),
        }
    }
}

/// The stream shot `shot` draws from under the run seed `seed`: the one the
/// executor protocol (planned) will seed for that shot of the participant
/// at place 0, which this method holds in a chain of a driver and itself,
/// the chain's only participant that draws. So such a chain, its executor
/// drawing from that stream as this method does, will reproduce the
/// records a resumer gives. The protocol's seed is a hash of the run's
/// seed, the place and the shot, so shots with close ids, or close seeds,
/// draw unrelated values: with `mix(x)` the first value of
/// `SplitMix64::new(x)`, the participant's seed is
/// `mix(seed ^ mix(place + 1))`, and the shot's `mix(participant ^ shot)`.
fn shot_stream(seed: u64, shot: QirShotId) -> SplitMix64 {
    fn mix(x: u64) -> u64 {
        SplitMix64::new(x).next_u64()
    }
    let participant = mix(seed ^ mix(DRAWING_PLACE + 1));
    SplitMix64::new(mix(participant ^ u64::from(shot.0)))
}

/// The place of this method among the participants that draw in a chain
/// of the executor protocol (planned) with a driver that does not draw:
/// the first, 0. The protocol counts places over the participants that
/// draw only, so that inserting an observer (a debugger) moves no draw.
const DRAWING_PLACE: u64 = 0;

/// A shot of a group: its place among the run's shots and its stream.
struct Shot {
    place: usize,
    stream: SplitMix64,
}

/// Shots with one history: their context, their state (`None` until the
/// first step needs it), the outcomes of the batch they are in, and its
/// steps, from `next` on still to run; `grown` when gates were applied
/// since its largest bond was read.
struct Group<S> {
    shots: Vec<Shot>,
    context: QirShotContext,
    state: Option<S>,
    outcomes: Vec<QirOutcome>,
    steps: Rc<[Step]>,
    next: usize,
    grown: bool,
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

/// What every group of a run shares.
struct Run<'r, E: MpsEngine> {
    engine: &'r E,
    order: QubitOrder,
    /// The capacity of each state; `None` for a program of no qubits, which
    /// needs no state.
    capacity: Option<MpsShape>,
    fusion: GateFusion,
    report: &'r mut MpsRunReport,
}

impl<E: MpsEngine> Run<'_, E> {
    /// Runs `group` until its context ends, giving its records; the groups
    /// that split off it are pushed onto `pending`.
    fn run_group(
        &mut self,
        machine: &QirMachine<'_>,
        group: &mut Group<E::State>,
        pending: &mut Vec<Group<E::State>>,
    ) -> Result<Vec<OutputRecord>, GroupFailure<E::Error>> {
        loop {
            while let Some(step) = group.steps.get(group.next).cloned() {
                group.next += 1;
                self.run_step(&step, group, pending)?;
            }
            self.note_bond(group)?;
            match machine
                .resume(&mut group.context, &group.outcomes)
                .map_err(GroupFailure::Trapped)?
            {
                QirStop::Ended(records) => {
                    if let Some(state) = &group.state {
                        let weight = self.engine.retained_weight(state).map_err(engine_failure)?;
                        let least = &mut self.report.least_retained_weight;
                        *least = least.min(weight);
                    }
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

    fn run_step(
        &mut self,
        step: &Step,
        group: &mut Group<E::State>,
        pending: &mut Vec<Group<E::State>>,
    ) -> Result<(), GroupFailure<E::Error>> {
        match *step {
            Step::Apply(operator) => {
                let state = self.prepared(&mut group.state)?;
                group.grown = true;
                self.engine
                    .apply(state, operator.sites(), operator.matrix.row_major())
                    .map_err(engine_failure)
            }
            Step::Measure {
                site,
                result,
                reset,
            } => self.draw(group, site, reset, Some(result), pending),
            Step::Reset { site } => self.draw(group, site, true, None, pending),
        }
    }

    /// Each shot of `group` draws the value of `site`. When some drew One
    /// and others Zero, the group splits: the part with fewer shots (Zero's
    /// on a tie) goes on as `group`, and the other waits on `pending`, with
    /// a fork of the state. Each part's state keeps its value, and is
    /// returned to |0⟩ when `reset`; `result`, when given, is answered with
    /// the value.
    fn draw(
        &mut self,
        group: &mut Group<E::State>,
        site: usize,
        reset: bool,
        result: Option<u32>,
        pending: &mut Vec<Group<E::State>>,
    ) -> Result<(), GroupFailure<E::Error>> {
        self.note_bond(group)?;
        let state = self.prepared(&mut group.state)?;
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
        let ones = draws.iter().filter(|&&one| one).count();
        let one = if ones == 0 || ones == draws.len() {
            ones > 0
        } else {
            // The part with fewer shots goes on; Zero's on a tie.
            let one = 2 * ones < draws.len();
            // Forked after P(One) is read, so each copy is the state a shot
            // alone would project; and before the shots are split, so that
            // a failed fork fails every shot of the group.
            let mut fork = self.engine.fork(state).map_err(engine_failure)?;
            self.settle(&mut fork, site, !one, reset)?;
            let (mut going_on, mut waiting) = (Vec::new(), Vec::new());
            for (shot, &drew) in std::mem::take(&mut group.shots).into_iter().zip(&draws) {
                if drew == one {
                    going_on.push(shot);
                } else {
                    waiting.push(shot);
                }
            }
            group.shots = going_on;
            let mut outcomes = group.outcomes.clone();
            outcomes.extend(answer(result, !one));
            pending.push(Group {
                shots: waiting,
                context: group.context.clone(),
                state: Some(fork),
                outcomes,
                steps: Rc::clone(&group.steps),
                next: group.next,
                grown: false,
            });
            one
        };
        self.settle(state, site, one, reset)?;
        group.outcomes.extend(answer(result, one));
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
    /// were applied since it was last read: so it is read where a run of
    /// gates ends, before the draws that may shrink it.
    fn note_bond(&mut self, group: &mut Group<E::State>) -> Result<(), GroupFailure<E::Error>> {
        if let (true, Some(state)) = (group.grown, &group.state) {
            let bond = self
                .engine
                .shape(state)
                .map_err(engine_failure)?
                .largest_bond();
            self.report.largest_bond = self.report.largest_bond.max(bond);
            group.grown = false;
        }
        Ok(())
    }

    /// The group's state, prepared within the capacity if it is not yet.
    fn prepared<'s>(
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
