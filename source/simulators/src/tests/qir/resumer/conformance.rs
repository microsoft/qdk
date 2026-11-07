// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kit (`qir::conformance`) passes on two correct resumers over a toy
//! quantum state, one running its shots one by one and one running the
//! shots whose next batches are equal as a group, and reports the rule
//! that each defective double breaks.

use std::collections::BTreeMap;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::thread::JoinHandle;

use crate::OutputRecord;
use crate::qir::conformance::{self, Rule, SCRAMBLED};
use crate::qir::{
    QirCall, QirGate, QirMachine, QirOutcome, QirReadout, QirResumeError, QirResumer,
    QirShotContext, QirShotId, QirStop, QirTrap,
};

/// A qubit of the toy state: a basis state, or an equal superposition of
/// both, as `h` leaves a basis state.
#[derive(Clone, Copy)]
enum Qubit {
    Zero,
    One,
    Even,
}

/// A shot's toy quantum state: enough of the QIR instructions for the kit's
/// programs (`x`, `h`, `mz`, `reset`). A measurement of an `Even` qubit
/// draws its outcome from the state's seed and the draws made before it.
struct ToyState {
    qubits: Vec<Qubit>,
    seed: u32,
    draws: u32,
}

impl ToyState {
    fn new(machine: QirMachine<'_>, seed: u32) -> Self {
        Self::of(machine.program().qubits(), seed)
    }

    fn of(qubits: u32, seed: u32) -> Self {
        Self {
            qubits: vec![Qubit::Zero; qubits as usize],
            seed,
            draws: 0,
        }
    }

    /// Runs `batch`, in order (or reversed, for the double that runs it
    /// so), pushing the outcome of each measurement onto `outcomes`.
    fn run(&mut self, batch: &[QirCall], reversed: bool, outcomes: &mut Vec<QirOutcome>) {
        let calls: Box<dyn Iterator<Item = &QirCall>> = if reversed {
            Box::new(batch.iter().rev())
        } else {
            Box::new(batch.iter())
        };
        for call in calls {
            match *call {
                QirCall::Gate {
                    gate: QirGate::X,
                    qubits: (qubit, None),
                    ..
                } => {
                    let qubit = &mut self.qubits[qubit as usize];
                    *qubit = match *qubit {
                        Qubit::Zero => Qubit::One,
                        Qubit::One => Qubit::Zero,
                        Qubit::Even => Qubit::Even,
                    };
                }
                QirCall::Gate {
                    gate: QirGate::H,
                    qubits: (qubit, None),
                    ..
                } => self.qubits[qubit as usize] = Qubit::Even,
                QirCall::Measure { qubit, result } => {
                    let value = self.measure(qubit);
                    outcomes.push(QirOutcome { result, value });
                }
                QirCall::Reset { qubit } => self.qubits[qubit as usize] = Qubit::Zero,
                ref other => panic!("the toy state runs no {other}"),
            }
        }
    }

    /// The outcome of measuring `qubit`, which collapses to it.
    fn measure(&mut self, qubit: u32) -> QirReadout {
        let qubit = &mut self.qubits[qubit as usize];
        if let Qubit::Even = qubit {
            let mixed = (u64::from(self.seed) << 32 | u64::from(self.draws))
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .rotate_left(29)
                .wrapping_mul(0xBF58_476D_1CE4_E5B9);
            self.draws += 1;
            *qubit = if mixed >> 63 == 1 {
                Qubit::One
            } else {
                Qubit::Zero
            };
        }
        match qubit {
            Qubit::One => QirReadout::One,
            _ => QirReadout::Zero,
        }
    }
}

/// The id equal to `place`.
fn place_id(place: usize) -> u32 {
    u32::try_from(place).expect("fewer shots than ids")
}

/// The resumer's own failure, for the shots it was made to fail.
#[derive(Debug, PartialEq)]
struct Failed;

/// What a defective double does wrong; a correct resumer has none.
#[derive(Clone, Copy, PartialEq)]
enum Defect {
    /// Rule 1: one context for every shot, never restarted.
    SharesAContext,
    /// Rule 2: runs each batch's calls in reverse.
    RunsBatchesReversed,
    /// Rule 3: gives a batch's outcomes to its results in the order of
    /// their ids, not each to its call's result.
    PairsOutcomesWithResultsInOrder,
    /// Rule 3: gives the outcomes of each shot's first batch to the next
    /// shot of its group (grouping only).
    GivesFirstOutcomesToTheNextShot,
    /// Rule 4: gives the records in the order of the shots' ids.
    GivesRecordsInIdOrder,
    /// Rule 5: names a trapping shot by its place in `shots`.
    NamesATrapByPlace,
    /// Rule 5: reports every trap as a division by zero at instruction 0,
    /// not as the trap met.
    ReplacesTheTrap,
    /// Rule 6: names a trapping shot by the first shot of `shots`, as a
    /// grouping resumer might name a trap by its group's first shot.
    NamesATrapByTheFirstShotListed,
    /// Rule 6: names its own failure by the lowest id among the shots it
    /// fails, as a resumer of a device that lists failures by shot might.
    NamesItsFailureByTheLowestFailingId,
    /// Rule 6: runs the shots last to first, reporting the first failure
    /// it meets.
    ReportsTheFirstFailureMetLastToFirst,
    /// Rule 6: stops at the first failure it meets, round by round
    /// (grouping only).
    StopsAtTheFirstFailureMet,
    /// Rule 7: seeds each shot's state by its place in `shots`.
    SeedsByPlace,
    /// Rule 7: seeds each shot's state by its id and the runs before, as a
    /// resumer drawing seeds from a generator it never resets would.
    SeedsByIdAndRunCount,
    /// Rule 7: starts each shot's qubits where the last run of its id left
    /// them, its seed restarted, as a resumer pooling states by shot id
    /// that reseeds them without resetting their qubits would.
    KeepsAShotsQubitsByItsId,
    /// Rule 7: sizes every state by the qubits of its first run's program,
    /// as a resumer building its simulator once, for its first program,
    /// would.
    SizesStatesByTheFirstProgram,
    /// Rule 8: runs shot 0 when given no shots.
    RunsAShotWhenGivenNone,
    /// Rule 8: stops once any shot ends (grouping only).
    StopsOnceAShotEnds,
    /// Panics at a shot that ends with no batch.
    PanicsAtAShotWithNoBatch,
}

/// A resumer of the tests over the toy state, failing its own work for
/// the shots `failing` at their first batch, with the defects `defects`.
#[derive(Default)]
struct Toy {
    failing: Vec<QirShotId>,
    defects: Vec<Defect>,
    /// The context of every shot, for [`Defect::SharesAContext`].
    shared: Option<QirShotContext>,
    /// The runs started, the running one included, for
    /// [`Defect::SeedsByIdAndRunCount`].
    runs: u32,
    /// The qubits the last run of each shot id left, for
    /// [`Defect::KeepsAShotsQubitsByItsId`].
    kept_qubits: BTreeMap<QirShotId, Vec<Qubit>>,
    /// The qubits of the first run's program, for
    /// [`Defect::SizesStatesByTheFirstProgram`].
    first_qubits: Option<u32>,
    /// Added to each shot's id to seed its state, so that a test may pick
    /// the outcomes the shots draw.
    salt: u32,
}

impl Toy {
    fn with(defects: &[Defect]) -> Self {
        Self {
            defects: defects.to_vec(),
            ..Self::default()
        }
    }

    fn failing(failing: &[QirShotId], defects: &[Defect]) -> Self {
        Self {
            failing: failing.to_vec(),
            ..Self::with(defects)
        }
    }

    fn has(&self, defect: Defect) -> bool {
        self.defects.contains(&defect)
    }

    /// The seed of the state of `shot`, at `place` in `shots`: its id (or
    /// its place, for the double that seeds so).
    fn seed(&self, place: usize, shot: QirShotId) -> u32 {
        if self.has(Defect::SeedsByPlace) {
            place_id(place)
        } else if self.has(Defect::SeedsByIdAndRunCount) {
            shot.0 + 1000 * (self.runs - 1)
        } else {
            shot.0.wrapping_add(self.salt)
        }
    }
}

type Run = Result<Vec<Vec<OutputRecord>>, QirResumeError<Failed>>;

/// The correct resumer that runs its shots one by one, in the order of
/// `shots`, each from a context of its own to its end; a double, with
/// defects.
struct Sequential(Toy);

impl QirResumer for Sequential {
    type Error = Failed;

    fn run(&mut self, machine: &QirMachine<'_>, shots: &[QirShotId]) -> impl Future<Output = Run> {
        std::future::ready(self.run_now(*machine, shots))
    }
}

impl Sequential {
    fn run_now(&mut self, machine: QirMachine<'_>, shots: &[QirShotId]) -> Run {
        let toy = &mut self.0;
        if shots.is_empty() && toy.has(Defect::RunsAShotWhenGivenNone) {
            return self.run_now(machine, &[QirShotId(0)]);
        }
        let mut order: Vec<usize> = (0..shots.len()).collect();
        if toy.has(Defect::GivesRecordsInIdOrder) {
            order.sort_by_key(|&place| shots[place]);
        }
        if toy.has(Defect::ReportsTheFirstFailureMetLastToFirst) {
            order.reverse();
        }
        let mut records = vec![Vec::new(); shots.len()];
        toy.runs += 1;
        for &place in &order {
            let shot = shots[place];
            let seed = toy.seed(place, shot);
            let name = if toy.has(Defect::NamesATrapByPlace) {
                QirShotId(place_id(place))
            } else {
                shot
            };
            records[place] = run_shot(toy, machine, shot, seed).map_err(|error| match error {
                ShotError::Trapped(_) if toy.has(Defect::ReplacesTheTrap) => {
                    ShotError::Trapped(QirTrap::DivisionByZero { instruction: 0 }).renamed(name)
                }
                ShotError::Trapped(_) if toy.has(Defect::NamesATrapByTheFirstShotListed) => {
                    error.renamed(shots[0])
                }
                ShotError::Failed if toy.has(Defect::NamesItsFailureByTheLowestFailingId) => {
                    let lowest = shots
                        .iter()
                        .copied()
                        .filter(|shot| toy.failing.contains(shot))
                        .min()
                        .expect("the failing shot is among the shots");
                    error.renamed(lowest)
                }
                _ => error.renamed(name),
            })?;
        }
        if toy.has(Defect::GivesRecordsInIdOrder) {
            records = order.iter().map(|&place| records[place].clone()).collect();
        }
        Ok(records)
    }
}

/// How a shot of [`run_shot`] failed, before the run names it.
enum ShotError {
    Trapped(QirTrap),
    Failed,
}

impl ShotError {
    fn renamed(self, shot: QirShotId) -> QirResumeError<Failed> {
        match self {
            Self::Trapped(trap) => QirResumeError::Trapped { shot, trap },
            Self::Failed => QirResumeError::Resumer {
                shot,
                error: Failed,
            },
        }
    }
}

/// Runs `shot` to its end, on a toy state seeded with `seed`.
fn run_shot(
    toy: &mut Toy,
    machine: QirMachine<'_>,
    shot: QirShotId,
    seed: u32,
) -> Result<Vec<OutputRecord>, ShotError> {
    let fails = toy.failing.contains(&shot);
    let reversed = toy.has(Defect::RunsBatchesReversed);
    let panics = toy.has(Defect::PanicsAtAShotWithNoBatch);
    let mispairs = toy.has(Defect::PairsOutcomesWithResultsInOrder);
    let keeps_qubits = toy.has(Defect::KeepsAShotsQubitsByItsId);
    let sizes_by_the_first = toy.has(Defect::SizesStatesByTheFirstProgram);
    let mut own = None;
    let context = if toy.has(Defect::SharesAContext) {
        toy.shared.get_or_insert_with(|| machine.start_shot())
    } else {
        own.insert(machine.start_shot())
    };
    let mut state = if sizes_by_the_first {
        let qubits = *toy
            .first_qubits
            .get_or_insert_with(|| machine.program().qubits());
        ToyState::of(qubits, seed)
    } else {
        ToyState::new(machine, seed)
    };
    for (qubit, kept) in state
        .qubits
        .iter_mut()
        .zip(toy.kept_qubits.remove(&shot).unwrap_or_default())
    {
        *qubit = kept;
    }
    let mut outcomes = Vec::new();
    let mut batches = 0;
    loop {
        match machine
            .resume(context, &outcomes)
            .map_err(ShotError::Trapped)?
        {
            QirStop::Batch(batch) => {
                if fails {
                    return Err(ShotError::Failed);
                }
                batches += 1;
                outcomes.clear();
                state.run(batch, reversed, &mut outcomes);
                if mispairs {
                    let mut results: Vec<u32> = outcomes.iter().map(|o| o.result).collect();
                    results.sort_unstable();
                    for (outcome, result) in outcomes.iter_mut().zip(results) {
                        outcome.result = result;
                    }
                }
            }
            QirStop::Ended(records) => {
                assert!(
                    batches > 0 || !panics,
                    "the double panics at a shot with no batch"
                );
                if keeps_qubits {
                    toy.kept_qubits.insert(shot, state.qubits);
                }
                return Ok(records.to_vec());
            }
        }
    }
}

/// A shot of a [`Grouping`] run.
struct GroupedShot {
    id: QirShotId,
    context: QirShotContext,
    state: ToyState,
    outcomes: Vec<QirOutcome>,
    records: Option<Vec<OutputRecord>>,
    /// The index of its batch among the round's distinct batches; `None`
    /// once it ended or failed.
    group: Option<usize>,
}

/// The correct resumer that runs its shots together, in rounds, as the
/// grouping resumer of the integration tests does: each round resumes every
/// running shot, in the order of `shots`, and the shots that stop at equal
/// batches form a group, which runs its batch on each of its shots' states
/// in turn. After a failure, it drops the shots after the failing one and
/// runs those before it on, until they end or fail, then reports the
/// first failing shot in the order of `shots`; a double, with defects.
struct Grouping(Toy);

impl QirResumer for Grouping {
    type Error = Failed;

    async fn run(&mut self, machine: &QirMachine<'_>, shots: &[QirShotId]) -> Run {
        let toy = &self.0;
        let machine = *machine;
        let mut run = GroupedRun {
            shots: shots
                .iter()
                .enumerate()
                .map(|(place, &id)| GroupedShot {
                    id,
                    context: machine.start_shot(),
                    state: ToyState::new(machine, toy.seed(place, id)),
                    outcomes: Vec::new(),
                    records: None,
                    group: None,
                })
                .collect(),
            failure: None,
        };
        let mut first_round = true;
        loop {
            let (batches, ended) = run.next_round(machine, toy);
            if batches.is_empty()
                || (run.failure.is_some() && toy.has(Defect::StopsAtTheFirstFailureMet))
                || (ended && toy.has(Defect::StopsOnceAShotEnds))
            {
                break;
            }
            run.run_groups(
                &batches,
                first_round && toy.has(Defect::GivesFirstOutcomesToTheNextShot),
            );
            first_round = false;
        }
        match run.failure {
            Some((_, error)) => Err(error),
            None => Ok(run
                .shots
                .into_iter()
                .map(|shot| shot.records.unwrap_or_default())
                .collect()),
        }
    }
}

/// The state of a [`Grouping`] run: a [`GroupedShot`] per shot, in the
/// order of `shots`, and the first failure in that order, with its place.
struct GroupedRun {
    shots: Vec<GroupedShot>,
    failure: Option<(usize, QirResumeError<Failed>)>,
}

impl GroupedRun {
    /// The shots still running: those before the first failure.
    fn running(&self) -> usize {
        self.failure
            .as_ref()
            .map_or(self.shots.len(), |(place, _)| *place)
    }

    /// Resumes every running shot that has not ended, in order, grouping
    /// them by their batches, until one fails: gives the round's distinct
    /// batches, in the order of each one's first shot, and whether a shot
    /// ended.
    fn next_round(&mut self, machine: QirMachine<'_>, toy: &Toy) -> (Vec<Vec<QirCall>>, bool) {
        let mut batches: Vec<Vec<QirCall>> = Vec::new();
        let mut ended = false;
        let running = self.running();
        for (place, shot) in self.shots[..running].iter_mut().enumerate() {
            if shot.records.is_some() {
                continue;
            }
            shot.group = None;
            let error = match machine.resume(&mut shot.context, &shot.outcomes) {
                Ok(QirStop::Ended(records)) => {
                    shot.records = Some(records.to_vec());
                    ended = true;
                    continue;
                }
                Ok(QirStop::Batch(_)) if toy.failing.contains(&shot.id) => {
                    QirResumeError::Resumer {
                        shot: shot.id,
                        error: Failed,
                    }
                }
                Ok(QirStop::Batch(batch)) => {
                    let group = batches
                        .iter()
                        .position(|equal| equal[..] == *batch)
                        .unwrap_or_else(|| {
                            batches.push(batch.to_vec());
                            batches.len() - 1
                        });
                    shot.group = Some(group);
                    continue;
                }
                Err(trap) => QirResumeError::Trapped {
                    shot: shot.id,
                    trap,
                },
            };
            self.failure = Some((place, error));
            break;
        }
        (batches, ended)
    }

    /// Runs each group's batch on its running shots' states, giving each
    /// shot its outcomes; or, when `misdeliver`, each shot's outcomes to
    /// the next shot of its group.
    fn run_groups(&mut self, batches: &[Vec<QirCall>], misdeliver: bool) {
        let running = self.running();
        for (group, batch) in batches.iter().enumerate() {
            let members: Vec<usize> = (0..running)
                .filter(|&place| self.shots[place].group == Some(group))
                .collect();
            for &place in &members {
                let shot = &mut self.shots[place];
                shot.outcomes.clear();
                shot.state.run(batch, false, &mut shot.outcomes);
            }
            if misdeliver {
                let mut given: Vec<_> = members
                    .iter()
                    .map(|&place| self.shots[place].outcomes.clone())
                    .collect();
                given.rotate_right(1);
                for (&place, outcomes) in members.iter().zip(given) {
                    self.shots[place].outcomes = outcomes;
                }
            }
        }
    }
}

/// The correct resumer whose run waits, as one waiting on a device does:
/// its future is pending until another thread wakes it, then runs the
/// shots as [`Sequential`] does.
struct Waiting(Sequential);

impl QirResumer for Waiting {
    type Error = Failed;

    async fn run(&mut self, machine: &QirMachine<'_>, shots: &[QirShotId]) -> Run {
        WokenByAnotherThread::default().await;
        self.0.run_now(*machine, shots)
    }
}

/// The correct resumer that awaits before running each batch, as one
/// sending each batch to a device would: its run is pending once for each
/// batch of each shot, then runs the batch on a toy state seeded by the
/// shot's id, failing at the first batch of the shots `failing`.
struct AwaitsEachBatch {
    failing: Vec<QirShotId>,
}

impl QirResumer for AwaitsEachBatch {
    type Error = Failed;

    async fn run(&mut self, machine: &QirMachine<'_>, shots: &[QirShotId]) -> Run {
        let mut records = Vec::new();
        for &shot in shots {
            let mut context = machine.start_shot();
            let mut state = ToyState::new(*machine, shot.0);
            let mut outcomes = Vec::new();
            loop {
                match machine
                    .resume(&mut context, &outcomes)
                    .map_err(|trap| QirResumeError::Trapped { shot, trap })?
                {
                    QirStop::Batch(batch) => {
                        WokenByAnotherThread::default().await;
                        if self.failing.contains(&shot) {
                            return Err(QirResumeError::Resumer {
                                shot,
                                error: Failed,
                            });
                        }
                        outcomes.clear();
                        state.run(batch, false, &mut outcomes);
                    }
                    QirStop::Ended(ended) => {
                        records.push(ended.to_vec());
                        break;
                    }
                }
            }
        }
        Ok(records)
    }
}

/// A future pending until a thread it starts at its first poll wakes it,
/// with no delay; it joins the thread once woken.
#[derive(Default)]
struct WokenByAnotherThread {
    waking: Option<(Arc<AtomicBool>, JoinHandle<()>)>,
}

impl Future for WokenByAnotherThread {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        match self.waking.take() {
            None => {
                let woken = Arc::new(AtomicBool::new(false));
                let waker = context.waker().clone();
                let flag = woken.clone();
                let thread = std::thread::spawn(move || {
                    flag.store(true, Ordering::Release);
                    waker.wake();
                });
                self.waking = Some((woken, thread));
                Poll::Pending
            }
            Some((woken, thread)) if woken.load(Ordering::Acquire) => {
                thread.join().expect("the waking thread ends");
                Poll::Ready(())
            }
            waking => {
                self.waking = waking;
                Poll::Pending
            }
        }
    }
}

/// The rules the kit reports broken by resumers made by `make`, each once,
/// with the runs that fail on purpose.
fn broken<R: QirResumer>(mut make: impl FnMut(&[QirShotId]) -> R) -> Vec<Rule>
where
    R::Error: std::fmt::Debug,
{
    let mut rules: Vec<Rule> = conformance::faults(&mut make, true)
        .into_iter()
        .map(|fault| fault.rule)
        .collect();
    rules.dedup();
    rules
}

#[test]
fn a_resumer_running_its_shots_one_by_one_keeps_every_rule() {
    conformance::check_with_failing_shots(|failing| Sequential(Toy::failing(failing, &[])));
}

#[test]
fn a_resumer_running_equal_batches_as_a_group_keeps_every_rule() {
    conformance::check_with_failing_shots(|failing| Grouping(Toy::failing(failing, &[])));
}

/// The kit waits on a resumer whose run is pending until it is woken.
#[test]
fn a_resumer_whose_run_waits_keeps_every_rule() {
    conformance::check_with_failing_shots(|failing| {
        Waiting(Sequential(Toy::failing(failing, &[])))
    });
}

/// The kit waits on a resumer whose run is pending before each batch, many
/// times in one run.
#[test]
fn a_resumer_awaiting_before_each_batch_keeps_every_rule() {
    conformance::check_with_failing_shots(|failing| AwaitsEachBatch {
        failing: failing.to_vec(),
    });
}

/// A resumer that cannot fail on purpose is checked on every rule but the
/// one on its own failures.
#[test]
fn check_runs_without_failing_shots() {
    conformance::check(|| Sequential(Toy::default()));
}

#[test]
fn sharing_a_context_between_shots_breaks_rule_1() {
    let rules = broken(|failing| Sequential(Toy::failing(failing, &[Defect::SharesAContext])));
    assert!(rules.contains(&Rule::ContextPerShot), "{rules:?}");
}

#[test]
fn running_a_batch_out_of_order_breaks_rule_2() {
    let rules = broken(|failing| Sequential(Toy::failing(failing, &[Defect::RunsBatchesReversed])));
    assert!(rules.contains(&Rule::BatchInOrder), "{rules:?}");
}

#[test]
fn giving_a_shot_another_shots_outcomes_breaks_rule_3() {
    let rules = broken(|failing| {
        Grouping(Toy::failing(
            failing,
            &[Defect::GivesFirstOutcomesToTheNextShot],
        ))
    });
    assert!(rules.contains(&Rule::OutcomesToTheirShot), "{rules:?}");
}

#[test]
fn giving_outcomes_to_results_in_the_order_of_their_ids_breaks_rule_3() {
    let rules = broken(|failing| {
        Sequential(Toy::failing(
            failing,
            &[Defect::PairsOutcomesWithResultsInOrder],
        ))
    });
    assert!(rules.contains(&Rule::OutcomesToTheirShot), "{rules:?}");
}

#[test]
fn giving_records_in_the_order_of_ids_breaks_rule_4() {
    let rules =
        broken(|failing| Sequential(Toy::failing(failing, &[Defect::GivesRecordsInIdOrder])));
    assert!(rules.contains(&Rule::RecordsAtTheirPlace), "{rules:?}");
}

#[test]
fn naming_a_trapping_shot_by_its_place_breaks_rule_5() {
    let rules = broken(|failing| Sequential(Toy::failing(failing, &[Defect::NamesATrapByPlace])));
    assert!(rules.contains(&Rule::TrapNamesItsShot), "{rules:?}");
}

#[test]
fn replacing_the_trap_met_breaks_rule_5() {
    let rules = broken(|failing| Sequential(Toy::failing(failing, &[Defect::ReplacesTheTrap])));
    assert!(rules.contains(&Rule::TrapNamesItsShot), "{rules:?}");
}

#[test]
fn reporting_the_first_failure_met_last_to_first_breaks_rule_6() {
    let rules = broken(|failing| {
        Sequential(Toy::failing(
            failing,
            &[Defect::ReportsTheFirstFailureMetLastToFirst],
        ))
    });
    assert!(rules.contains(&Rule::FirstFailureInOrder), "{rules:?}");
}

/// A grouping resumer that stops at the first failure it meets, round by
/// round.
#[test]
fn a_grouping_resumer_stopping_at_the_first_failure_met_breaks_rule_6() {
    let rules =
        broken(|failing| Grouping(Toy::failing(failing, &[Defect::StopsAtTheFirstFailureMet])));
    assert!(rules.contains(&Rule::FirstFailureInOrder), "{rules:?}");
}

/// Only some of the shots trap, so a trap named by the first shot listed
/// names a shot that does not trap.
#[test]
fn naming_a_trap_by_the_first_shot_listed_breaks_rule_6() {
    let rules = broken(|failing| {
        Sequential(Toy::failing(
            failing,
            &[Defect::NamesATrapByTheFirstShotListed],
        ))
    });
    assert!(rules.contains(&Rule::FirstFailureInOrder), "{rules:?}");
}

/// The resumer fails for two shots, the one listed first not the one with
/// the lower id, so a failure named by the lowest id names the wrong one.
#[test]
fn naming_its_own_failure_by_the_lowest_failing_id_breaks_rule_6() {
    let rules = broken(|failing| {
        Sequential(Toy::failing(
            failing,
            &[Defect::NamesItsFailureByTheLowestFailingId],
        ))
    });
    assert!(rules.contains(&Rule::FirstFailureInOrder), "{rules:?}");
}

/// `check`, whose resumers cannot fail on purpose, still checks rule 6 on
/// shots that all trap.
#[test]
fn check_finds_the_first_failure_reported_out_of_order() {
    let mut make =
        |_: &[QirShotId]| Sequential(Toy::with(&[Defect::ReportsTheFirstFailureMetLastToFirst]));
    let rules: Vec<Rule> = conformance::faults(&mut make, false)
        .into_iter()
        .map(|fault| fault.rule)
        .collect();
    assert!(rules.contains(&Rule::FirstFailureInOrder), "{rules:?}");
}

/// `check`, whose resumers cannot fail on purpose, still finds a grouping
/// resumer that stops at the first trap it meets, round by round, even
/// when its seeds make the shots listed first trap in earlier rounds than
/// those after them.
#[test]
fn check_finds_a_grouping_resumer_stopping_at_the_first_trap_met() {
    // The first outcome of a shot seeded `seed`: `One` traps in the first
    // round, `Zero` in the second.
    let traps_early = |seed: u32| {
        let mut state = ToyState {
            qubits: vec![Qubit::Even],
            seed,
            draws: 0,
        };
        state.measure(0) == QirReadout::One
    };
    let salt = (0..10_000)
        .find(|&salt| {
            let early: Vec<bool> = SCRAMBLED
                .iter()
                .map(|&id| traps_early(id.wrapping_add(salt)))
                .collect();
            let firsts = early.iter().take_while(|&&early| early).count();
            firsts > 0 && firsts < early.len() && early[firsts..].iter().all(|&early| !early)
        })
        .expect("some salt makes the shots listed first trap early, the others late");
    let mut make = |_: &[QirShotId]| {
        Grouping(Toy {
            salt,
            ..Toy::with(&[Defect::StopsAtTheFirstFailureMet])
        })
    };
    let rules: Vec<Rule> = conformance::faults(&mut make, false)
        .into_iter()
        .map(|fault| fault.rule)
        .collect();
    assert!(rules.contains(&Rule::FirstFailureInOrder), "{rules:?}");
}

#[test]
fn seeding_a_shot_by_its_place_breaks_rule_7() {
    let rules = broken(|failing| Sequential(Toy::failing(failing, &[Defect::SeedsByPlace])));
    assert!(rules.contains(&Rule::IndependentOfOtherShots), "{rules:?}");
}

/// The kit runs shots alone one after the other on one resumer, so a
/// resumer whose seeds depend on its earlier runs gives other records.
#[test]
fn seeding_a_shot_by_the_runs_before_breaks_rule_7() {
    let rules =
        broken(|failing| Sequential(Toy::failing(failing, &[Defect::SeedsByIdAndRunCount])));
    assert!(rules.contains(&Rule::IndependentOfOtherShots), "{rules:?}");
}

/// The kit runs `x` and `mz` twice on the same shot ids of one resumer,
/// so a resumer that keeps each id's qubits across runs records the qubit
/// flipped back.
#[test]
fn keeping_a_shot_s_qubits_for_its_id_across_runs_breaks_rule_7() {
    let rules =
        broken(|failing| Sequential(Toy::failing(failing, &[Defect::KeepsAShotsQubitsByItsId])));
    assert!(rules.contains(&Rule::IndependentOfOtherShots), "{rules:?}");
}

/// The kit runs a program of one qubit, then one of four, on one resumer,
/// so a resumer sizing its states by its first program has too few qubits.
#[test]
fn sizing_states_by_the_first_program_breaks_rule_7() {
    let rules = broken(|failing| {
        Sequential(Toy::failing(
            failing,
            &[Defect::SizesStatesByTheFirstProgram],
        ))
    });
    assert!(rules.contains(&Rule::IndependentOfOtherShots), "{rules:?}");
}

#[test]
fn running_a_shot_when_given_none_breaks_rule_8() {
    let rules =
        broken(|failing| Sequential(Toy::failing(failing, &[Defect::RunsAShotWhenGivenNone])));
    assert!(rules.contains(&Rule::Edges), "{rules:?}");
}

#[test]
fn stopping_once_a_shot_ends_breaks_rule_8() {
    let rules = broken(|failing| Grouping(Toy::failing(failing, &[Defect::StopsOnceAShotEnds])));
    assert!(rules.contains(&Rule::Edges), "{rules:?}");
}

/// A resumer that panics at a program with no quantum call breaks only the
/// rule whose check runs that program: the kit catches the panic and checks
/// the other rules.
#[test]
fn a_panic_is_a_fault_of_the_rule_whose_check_met_it() {
    let rules =
        broken(|failing| Sequential(Toy::failing(failing, &[Defect::PanicsAtAShotWithNoBatch])));
    assert_eq!(rules, [Rule::Edges]);
}

/// One check reports every rule a resumer breaks, not only the first.
#[test]
fn check_names_every_rule_broken() {
    let panic = catch_unwind(AssertUnwindSafe(|| {
        conformance::check(|| {
            Sequential(Toy::with(&[
                Defect::RunsBatchesReversed,
                Defect::NamesATrapByPlace,
            ]))
        });
    }))
    .expect_err("the resumer breaks two rules");
    let message = panic
        .downcast_ref::<String>()
        .expect("the check panics with a formatted message");
    for rule in [Rule::BatchInOrder, Rule::TrapNamesItsShot] {
        assert!(message.contains(&rule.to_string()), "{message}");
    }
}
