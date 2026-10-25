// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Sessions (level L2) of the legacy simulators: per-shot sessions over
//! [`SimulatorShots`], seen by an L3 driver (S1), by an L2 driver (S2), and
//! through [`ShotStrategy`] alongside an ensemble strategy (S8), and their
//! rejections (S9).

use std::collections::BTreeMap;

use executor_protocol::{
    Batch, Capabilities, Close, Describe, Executor, Immediate, MakeShot, Need, Needs, PerShot,
    Rejected, SessionReport, ShotContext, ShotId, ShotOutcome, ShotSet, ShotStrategy, Submission,
    SubmissionRef, SubmissionWrapper, Unmet, Value, open, shot_seed, wait,
};

use super::{
    TestDoubleTerminal, loss_or_branch_program, measure_then_branch_program,
    x_reset_measure_program,
};
use crate::{
    MeasurementResult, Simulator,
    cpu_full_state_simulator::FullStateSimulator,
    execution::{
        MeasurementKind, MeasurementRequest, PreparedAdaptiveProgram, QuantumEvolutionRegion,
        RegionId, RegionInstruction, RuntimeAdapter, SimulatorReport, SimulatorShots,
        SimulatorTerminal, UnitaryOperation,
    },
    stabilizer_simulator::StabilizerSimulator,
};

fn apply(operation: UnitaryOperation) -> RegionInstruction {
    RegionInstruction::ExecuteRegion {
        region_id: RegionId::new(0),
        region: QuantumEvolutionRegion::new([operation]),
    }
}

fn measure(qubit: usize, result_id: usize) -> RegionInstruction {
    RegionInstruction::Measure(MeasurementRequest {
        kind: MeasurementKind::MeasureZ,
        qubit,
        result_id,
    })
}

/// A batch with a fair coin on each of two results.
fn coins() -> Vec<RegionInstruction> {
    vec![
        apply(UnitaryOperation::H { target: 0 }),
        measure(0, 0),
        apply(UnitaryOperation::H { target: 1 }),
        measure(1, 1),
    ]
}

/// The legacy seed of shot `j` of a session opened with `seed`.
fn legacy_seed(seed: u64, j: ShotId) -> u32 {
    let seed = shot_seed(seed, j);
    u32::try_from((seed ^ (seed >> 32)) & u64::from(u32::MAX)).expect("masked to 32 bits")
}

/// An L3 driver: one batch, its outcomes and how it ended.
fn run_batch<E: Executor<Batch<RegionInstruction>>>(
    executor: &mut E,
    batch: &[RegionInstruction],
) -> (Vec<(u32, Value)>, Result<(), String>) {
    let mut out = Vec::new();
    let end = wait(executor.call(batch).expect("accepted"), &mut out);
    (out.into_iter().map(|o| (o.tag, o.value)).collect(), end)
}

/// An L2 driver: one submission, its outcomes by shot and tag.
fn run_submission<E: Executor<Submission<RegionInstruction>>>(
    session: &mut E,
    shots: &ShotSet,
    batch: &[RegionInstruction],
) -> Result<ByShot, Rejected> {
    let mut out = Vec::new();
    let reply = session.call(SubmissionRef { shots, batch })?;
    assert_eq!(wait(reply, &mut out), Ok(()));
    Ok(out
        .into_iter()
        .map(|o| ((o.shot, o.tag), o.value))
        .collect())
}

fn programs() -> Vec<PreparedAdaptiveProgram<u64>> {
    [
        measure_then_branch_program(),
        loss_or_branch_program(false),
        x_reset_measure_program(),
    ]
    .into_iter()
    .map(|program| PreparedAdaptiveProgram::new(program).expect("test programs should prepare"))
    .collect()
}

/// S1: an L3 driver, the Runtime adapter included, sees the same over a
/// per-shot session on shot 1 as over the shot's simulator alone.
#[test]
fn s1_an_l3_driver_runs_on_a_per_shot_session() {
    fn assert_same<S: Simulator>()
    where
        S::Noise: Clone,
    {
        let maker = PerShot(SimulatorShots::<S>::new(2, 2, S::Noise::default()));
        for seed in 0..200 {
            let mut direct =
                SimulatorTerminal::<S>::new(2, 2, legacy_seed(seed, 1), S::Noise::default());
            let mut wrapped =
                SubmissionWrapper::new(open(&maker, seed, &ShotSet::one(1)).expect("opens"));
            for _ in 0..2 {
                assert_eq!(
                    run_batch(&mut wrapped, &coins()),
                    run_batch(&mut direct, &coins()),
                    "seed {seed}"
                );
            }
            assert_eq!(wrapped.close().shots, direct.close());
        }
        let programs = programs();
        for seed in 0..32 {
            let mut direct = RuntimeAdapter::new(SimulatorTerminal::<S>::new(
                2,
                2,
                legacy_seed(seed, 1),
                S::Noise::default(),
            ));
            let mut wrapped = RuntimeAdapter::new(SubmissionWrapper::new(
                open(&maker, seed, &ShotSet::one(1)).expect("opens"),
            ));
            let (mut ours, mut theirs) = (Vec::new(), Vec::new());
            let our_end = wait(wrapped.call(&programs).expect("accepted"), &mut ours);
            let their_end = wait(direct.call(&programs).expect("accepted"), &mut theirs);
            assert_eq!((ours, our_end), (theirs, their_end), "seed {seed}");
            assert_eq!(wrapped.close().inner.shots, direct.close().inner);
        }
    }
    assert_same::<FullStateSimulator>();
    assert_same::<StabilizerSimulator>();
}

/// S2: an L2 driver sees each shot of a per-shot session as that shot run
/// alone, whatever the split of J, and the coins are fair.
#[test]
fn s2_an_l2_driver_runs_each_shot_as_alone() {
    let maker = PerShot(SimulatorShots::<FullStateSimulator>::new(
        2,
        2,
        Default::default(),
    ));
    let shots = ShotSet::range(1..=300).expect("shots");
    let mut whole = open(&maker, 42, &shots).expect("opens");
    let outcomes = run_submission(&mut whole, &shots, &coins()).expect("runs");
    let mut split = BTreeMap::new();
    for part in [shots.subset(|j| j % 3 == 0), shots.subset(|j| j % 3 != 0)] {
        let part = part.expect("shots");
        let mut session = open(&maker, 42, &part).expect("opens");
        split.extend(run_submission(&mut session, &part, &coins()).expect("runs"));
    }
    assert_eq!(split, outcomes);
    for j in [1, 2, 150, 300] {
        let mut alone = SimulatorTerminal::<FullStateSimulator>::new(
            2,
            2,
            legacy_seed(42, j),
            Default::default(),
        );
        let (alone, _) = run_batch(&mut alone, &coins());
        let ours = (0..2)
            .map(|tag| (tag, outcomes[&(j, tag)]))
            .collect::<Vec<_>>();
        assert_eq!(ours, alone, "shot {j}");
    }
    for tag in 0..2 {
        let ones = shots
            .iter()
            .filter(|&&j| outcomes[&(j, tag)] == Value::One)
            .count();
        assert!(
            (110..190).contains(&ones),
            "result {tag}: P(One) = 1/2, {ones} of 300"
        );
    }
    let report = whole.close();
    assert_eq!(report.submissions, 1);
    assert_eq!(report.shots.batches, 300);
    assert_eq!(report.shots.measurements, 600);
    assert!(report.log_p.is_empty());
}

/// An ensemble terminal, as a test double: opens sessions of at most four
/// shots, held together, and runs a submission's shots in one pass. Its
/// shots are the legacy CPU simulator's, seeded as per-shot sessions seed
/// them, so the two strategies must agree.
struct TestDoubleEnsemble;

struct TestDoubleEnsembleSession {
    shots: BTreeMap<ShotId, SimulatorTerminal<FullStateSimulator>>,
    submissions: u64,
}

impl Describe for TestDoubleEnsemble {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
            .with_qubits(Some(2))
            .with_states(Some(4))
    }
}

impl ShotStrategy<RegionInstruction> for TestDoubleEnsemble {
    type Session = TestDoubleEnsembleSession;

    fn open(&self, seed: u64, shots: &ShotSet, needs: &Needs) -> Result<Self::Session, Rejected> {
        self.capabilities()
            .admit("TestDoubleEnsemble", shots, needs)?;
        Ok(TestDoubleEnsembleSession {
            shots: shots
                .iter()
                .map(|&j| {
                    let terminal =
                        SimulatorTerminal::new(2, 2, legacy_seed(seed, j), Default::default());
                    (j, terminal)
                })
                .collect(),
            submissions: 0,
        })
    }
}

impl Executor<Submission<RegionInstruction>> for TestDoubleEnsembleSession {
    type Response = Immediate<ShotOutcome>;

    fn call(
        &mut self,
        submission: SubmissionRef<'_, RegionInstruction>,
    ) -> Result<Self::Response, Rejected> {
        if let Some(&j) = submission
            .shots
            .iter()
            .find(|j| !self.shots.contains_key(j))
        {
            return Err(Rejected::ShotNotInSession(j));
        }
        let mut out = Vec::new();
        for &j in submission.shots {
            let Some(terminal) = self.shots.get_mut(&j) else {
                continue;
            };
            let (outcomes, _) = run_batch(terminal, submission.batch);
            out.extend(outcomes.into_iter().map(|(tag, value)| ShotOutcome {
                shot: j,
                tag,
                value,
            }));
        }
        self.submissions += 1;
        Ok(Immediate::done(out))
    }
}

impl Describe for TestDoubleEnsembleSession {
    fn capabilities(&self) -> Capabilities {
        TestDoubleEnsemble.capabilities()
    }
}

impl Close for TestDoubleEnsembleSession {
    type Report = SessionReport<SimulatorReport>;

    fn close(&mut self) -> Self::Report {
        let mut report = SessionReport::<SimulatorReport> {
            submissions: std::mem::take(&mut self.submissions),
            ..SessionReport::default()
        };
        for terminal in self.shots.values_mut() {
            executor_protocol::Fold::fold(&mut report.shots, terminal.close());
        }
        report
    }
}

/// Outcomes by shot and tag.
type ByShot = BTreeMap<(ShotId, u32), Value>;

/// One driver over [`ShotStrategy`]: opens a session with its needs, runs
/// three submissions, and closes it. Returns each submission's outcomes and
/// the submissions the session reports.
fn drive<S: ShotStrategy<RegionInstruction>>(
    strategy: &S,
    shots: &ShotSet,
    needs: &Needs,
) -> Result<(Vec<ByShot>, u64), Rejected>
where
    S::Session: Close<Report = SessionReport<SimulatorReport>>,
{
    let mut session = strategy.open(5, shots, needs)?;
    let outcomes = (0..3)
        .map(|_| run_submission(&mut session, shots, &coins()))
        .collect::<Result<Vec<_>, _>>()?;
    let report = session.close();
    assert_eq!(report.shots.batches, 3 * shots.len() as u64);
    Ok((outcomes, report.submissions))
}

/// S8: one driver over [`ShotStrategy`] runs on the per-shot strategy and on
/// an ensemble one, which agree; a need a strategy cannot meet is refused
/// when the session opens, naming the component.
#[test]
fn s8_one_driver_opens_per_shot_and_ensemble_sessions() {
    let per_shot = PerShot(SimulatorShots::<FullStateSimulator>::new(
        2,
        2,
        Default::default(),
    ));
    let shots = ShotSet::range(7..=10).expect("shots");
    let replayable = Needs::default().with_replayable(true);
    let ours = drive(&per_shot, &shots, &replayable).expect("runs");
    let theirs = drive(&TestDoubleEnsemble, &shots, &replayable).expect("runs");
    assert_eq!(ours, theirs);
    assert_eq!(ours.1, 3);
    let unmet = |opens: &dyn Fn(&Needs) -> Result<(), Rejected>, needs: &Needs| match opens(needs) {
        Err(Rejected::Unmet(Unmet {
            component,
            need,
            needed,
            offered,
        })) => Some((component, need, needed, offered)),
        _ => None,
    };
    let per_shot_opens = |needs: &Needs| drive(&per_shot, &shots, needs).map(|_| ());
    let ensemble_opens = |needs: &Needs| drive(&TestDoubleEnsemble, &shots, needs).map(|_| ());
    let maker = std::any::type_name::<SimulatorShots<FullStateSimulator>>().to_owned();
    assert_eq!(
        unmet(&per_shot_opens, &Needs::default().with_qubits(Some(3))),
        Some((maker.clone(), Need::Qubits, 3, 2))
    );
    assert_eq!(
        unmet(&per_shot_opens, &Needs::default().with_probabilities(true)),
        Some((maker, Need::Probabilities, 1, 0))
    );
    let ensemble = "TestDoubleEnsemble".to_owned();
    assert_eq!(
        unmet(&ensemble_opens, &Needs::default().with_qubits(Some(3))),
        Some((ensemble.clone(), Need::Qubits, 3, 2))
    );
    let five = ShotSet::range(1..=5).expect("shots");
    assert_eq!(
        unmet(
            &|needs: &Needs| drive(&TestDoubleEnsemble, &five, needs).map(|_| ()),
            &Needs::default()
        ),
        Some((ensemble, Need::States, 5, 4))
    );
    assert!(
        drive(&per_shot, &five, &Needs::default()).is_ok(),
        "a state per shot"
    );
}

/// A maker of test double shots, of which shot `refusing` refuses every
/// batch.
struct TestDoubleShots {
    refusing: ShotId,
}

impl Describe for TestDoubleShots {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl MakeShot<RegionInstruction> for TestDoubleShots {
    type Shot = TestDoubleTerminal;

    fn make(&self, ctx: &ShotContext) -> TestDoubleTerminal {
        let terminal = TestDoubleTerminal::new(None);
        if ctx.shot() == self.refusing {
            terminal.rejecting_from(0, Rejected::Unavailable("busy".into()))
        } else {
            terminal
        }
    }
}

/// S9: a submission refused before it runs leaves every shot as it was; a
/// shot that refuses after others ran fails the submission and the
/// session; a shot's simulator is reachable between calls.
#[test]
fn s9_a_session_refuses_or_fails_and_its_shots_stay_reachable() {
    let maker = PerShot(SimulatorShots::<FullStateSimulator>::new(
        2,
        2,
        Default::default(),
    ));
    let all = ShotSet::range(1..=3).expect("shots");
    let mut session = open(&maker, 1, &all).expect("opens");
    let flip = [apply(UnitaryOperation::X { target: 0 }), measure(0, 0)];
    assert_eq!(
        run_submission(&mut session, &all, &[flip[0].clone(), measure(2, 0)]).err(),
        Some(Rejected::Instruction {
            at: 1,
            reason: "qubit 2 is beyond the simulator's 2 qubits".into()
        })
    );
    assert_eq!(
        run_submission(
            &mut session,
            &ShotSet::new(vec![2, 4]).expect("shots"),
            &flip
        )
        .err(),
        Some(Rejected::ShotNotInSession(4))
    );
    let result = |session: &executor_protocol::SubmissionUnwrapper<_>, j| {
        session
            .shot(j)
            .map(|shot: &SimulatorTerminal<FullStateSimulator>| shot.simulator().measurements()[0])
    };
    assert_eq!(
        result(&session, 2),
        Some(MeasurementResult::Zero),
        "nothing ran"
    );
    run_submission(&mut session, &ShotSet::one(2), &flip).expect("runs");
    assert_eq!(result(&session, 2), Some(MeasurementResult::One));
    assert_eq!(result(&session, 1), Some(MeasurementResult::Zero));
    let report = session.close();
    assert_eq!((report.submissions, report.shots.batches), (1, 1));
    assert_eq!(result(&session, 2), None, "closed");
    assert_eq!(
        run_submission(&mut session, &all, &flip).err(),
        Some(Rejected::Closed)
    );

    let mut session = open(&PerShot(TestDoubleShots { refusing: 2 }), 1, &all).expect("opens");
    assert_eq!(
        run_submission(
            &mut session,
            &ShotSet::new(vec![2, 1]).expect("shots"),
            &flip
        )
        .err(),
        Some(Rejected::Unavailable("busy".into()))
    );
    let reply = session
        .call(SubmissionRef {
            shots: &all,
            batch: &flip,
        })
        .expect("shot 1 ran");
    let mut out = Vec::new();
    assert_eq!(
        wait(reply, &mut out),
        Err("shot 2: unavailable: busy".into())
    );
    assert_eq!(out.iter().map(|o| o.shot).collect::<Vec<_>>(), [1]);
    assert_eq!(
        run_submission(&mut session, &ShotSet::one(3), &flip).err(),
        Some(Rejected::SessionFailed("shot 2: unavailable: busy".into()))
    );
}
