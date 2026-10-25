// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeMap;
use std::task::Waker;

use crate::{
    Batch, Capabilities, Close, Completer, Completion, ComponentId, Describe, Executor, Fold,
    Gather, Immediate, Incremental, MakeShot, Need, Needs, Open, Outcome, Rejected, ShotContext,
    ShotId, ShotOutcome, ShotSet, Status, Submission, SubmissionRef, SubmissionWrapper, Unmet,
    Value, completer, open, wait,
};

use super::Count;

/// A maker of test double shots of a one-instruction ISA: each `true` reads
/// a bit of the shot's terminal seed, so outcomes show which seed a shot
/// drew from.
#[derive(Default)]
struct TestDoubleShots {
    /// The shot whose executor refuses every call.
    refusing: Option<ShotId>,
    /// The shots whose replies wait until the test ends them.
    deferred: Vec<ShotId>,
    probabilities: bool,
}

/// One shot: its seed, its calls, and the replies it was told to defer.
struct TestDoubleShot {
    seed: u64,
    calls: u64,
    refusing: bool,
    deferred: bool,
    replies: Vec<Completer<Outcome>>,
}

impl TestDoubleShot {
    /// The value of read `i` of call `call`.
    fn bit(&self, call: u64, i: usize) -> Value {
        (self.seed >> ((call * 8 + i as u64) % 64) & 1 == 1).into()
    }
}

impl Describe for TestDoubleShots {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
            .with_qubits(Some(4))
            .with_probabilities(self.probabilities)
    }
}

impl MakeShot<bool> for TestDoubleShots {
    type Shot = TestDoubleShot;

    fn make(&self, ctx: &ShotContext) -> TestDoubleShot {
        TestDoubleShot {
            seed: ctx.rng_seed(ComponentId::TERMINAL),
            calls: 0,
            refusing: self.refusing == Some(ctx.shot()),
            deferred: self.deferred.contains(&ctx.shot()),
            replies: Vec::new(),
        }
    }

    fn log_p(shot: &TestDoubleShot) -> Option<f64> {
        #[expect(clippy::cast_precision_loss, reason = "a test's few calls")]
        Some(-(shot.calls as f64))
    }
}

impl Executor<Batch<bool>> for TestDoubleShot {
    type Response = Incremental<Outcome>;

    fn call(&mut self, batch: &[bool]) -> Result<Self::Response, Rejected> {
        if self.refusing {
            return Err(Rejected::Unavailable("busy".into()));
        }
        let (completer, reply) = completer();
        for (i, &read) in batch.iter().enumerate() {
            if read {
                completer.push(Outcome {
                    tag: u32::try_from(i).unwrap_or(u32::MAX),
                    value: self.bit(self.calls, i),
                });
            }
        }
        self.calls += 1;
        if self.deferred {
            self.replies.push(completer);
        } else {
            completer.done();
        }
        Ok(reply)
    }
}

impl Close for TestDoubleShot {
    type Report = Count;

    fn close(&mut self) -> Count {
        Count(std::mem::take(&mut self.calls))
    }
}

/// Runs `batch` on `shots` of `session` to its end.
fn run<S: Executor<Submission<bool>>>(
    session: &mut S,
    shots: &ShotSet,
    batch: &[bool],
) -> Result<(Vec<ShotOutcome>, Result<(), String>), Rejected> {
    let reply = session.call(SubmissionRef { shots, batch })?;
    let mut out = Vec::new();
    let end = wait(reply, &mut out);
    Ok((out, end))
}

fn by_shot(outcomes: &[ShotOutcome]) -> BTreeMap<ShotId, Vec<(u32, Value)>> {
    let mut shots = BTreeMap::<_, Vec<_>>::new();
    for outcome in outcomes {
        shots
            .entry(outcome.shot)
            .or_default()
            .push((outcome.tag, outcome.value));
    }
    shots
}

/// Shot `j` draws from `ShotContext::new(seed, j)` whatever session holds
/// it: the outcomes of a split of J are those of one session over J.
#[test]
fn a_shots_outcomes_do_not_depend_on_how_the_shots_are_split() {
    let maker = TestDoubleShots::default();
    let batch = [true, false, true, true];
    let all = ShotSet::range(1..=6).expect("shots");
    let mut whole = open(&maker, 9, &all).expect("opens");
    let first = run(&mut whole, &all, &batch).expect("runs");
    let second = run(&mut whole, &all, &batch).expect("runs");
    assert_eq!(first.1, Ok(()));
    assert_eq!(
        first.0.iter().map(|o| o.shot).collect::<Vec<_>>(),
        [1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 5, 6, 6, 6],
        "shot by shot, in the submission's order"
    );
    let mut split = BTreeMap::new();
    for part in [vec![4, 1], vec![6], vec![2, 5, 3]] {
        let part = ShotSet::new(part).expect("shots");
        let mut session = open(&maker, 9, &part).expect("opens");
        let (outcomes, _) = run(&mut session, &part, &batch).expect("runs");
        split.extend(by_shot(&outcomes));
        let (later, _) = run(&mut session, &part, &batch).expect("runs");
        assert_eq!(
            by_shot(&later),
            by_shot(&second.0)
                .into_iter()
                .filter(|(j, _)| part.contains(j))
                .collect()
        );
    }
    assert_eq!(split, by_shot(&first.0));
    assert_ne!(first.0, second.0, "a shot's state carries over");
}

/// A submission may name a subset of the session's shots, in any order; a
/// shot the session does not hold refuses it, with nothing run.
#[test]
fn a_submission_runs_on_the_shots_it_names() {
    let mut session = open(
        &TestDoubleShots::default(),
        1,
        &ShotSet::range(1..=3).expect("shots"),
    )
    .expect("opens");
    let (outcomes, end) = run(
        &mut session,
        &ShotSet::new(vec![3, 1]).expect("shots"),
        &[true],
    )
    .expect("runs");
    assert_eq!(end, Ok(()));
    assert_eq!(outcomes.iter().map(|o| o.shot).collect::<Vec<_>>(), [3, 1]);
    assert_eq!(
        run(
            &mut session,
            &ShotSet::new(vec![2, 4]).expect("shots"),
            &[true]
        )
        .err(),
        Some(Rejected::ShotNotInSession(4))
    );
    assert_eq!(session.shot(2).map(|shot| shot.calls), Some(0));
    assert_eq!(session.shot(4).map(|shot| shot.calls), None);
    let report = session.close();
    assert_eq!((report.submissions, report.shots), (1, Count(2)));
    assert!(report.log_p.is_empty(), "the maker does not offer it");
    assert_eq!(session.shot(1).map(|shot| shot.calls), None, "closed");
    assert_eq!(
        run(&mut session, &ShotSet::one(1), &[true]).err(),
        Some(Rejected::Closed)
    );
}

/// A rejection by the first shot called refuses the submission; one by a
/// later shot is too late to refuse, so it fails the submission and the
/// session, and a closed session says so first.
#[test]
fn a_shots_rejection_refuses_or_fails_the_submission() {
    let maker = TestDoubleShots {
        refusing: Some(2),
        ..TestDoubleShots::default()
    };
    let all = ShotSet::range(1..=3).expect("shots");
    let mut session = open(&maker, 1, &all).expect("opens");
    assert_eq!(
        run(
            &mut session,
            &ShotSet::new(vec![2, 1]).expect("shots"),
            &[true]
        )
        .err(),
        Some(Rejected::Unavailable("busy".into()))
    );
    assert_eq!(
        session.shot(1).map(|shot| shot.calls),
        Some(0),
        "nothing ran"
    );
    let (outcomes, end) = run(&mut session, &all, &[true]).expect("starts");
    assert_eq!(end, Err("shot 2: unavailable: busy".into()));
    assert_eq!(by_shot(&outcomes).keys().copied().collect::<Vec<_>>(), [1]);
    assert_eq!(session.shot(3).map(|shot| shot.calls), Some(0), "not run");
    assert_eq!(
        run(&mut session, &ShotSet::one(1), &[true]).err(),
        Some(Rejected::SessionFailed("shot 2: unavailable: busy".into()))
    );
    let report = session.close();
    assert_eq!((report.submissions, report.shots), (1, Count(1)));
    assert_eq!(
        run(&mut session, &ShotSet::one(1), &[true]).err(),
        Some(Rejected::Closed)
    );
}

/// The session's reply is pending while a shot's is, with the events so
/// far; the shot's executor is reachable between calls; a failed reply
/// fails the session when polled.
#[test]
fn a_submission_ends_when_its_last_shot_does() {
    let maker = TestDoubleShots {
        deferred: vec![2],
        ..TestDoubleShots::default()
    };
    let all = ShotSet::range(1..=2).expect("shots");
    let mut session = open(&maker, 1, &all).expect("opens");
    let mut reply = session
        .call(SubmissionRef {
            shots: &all,
            batch: &[true],
        })
        .expect("starts");
    let mut out = Vec::new();
    assert_eq!(reply.poll_events(&mut out, Waker::noop()), Status::Pending);
    assert_eq!(
        out.iter().map(|o| o.shot).collect::<Vec<_>>(),
        [1, 2],
        "shot 2's read was pushed before its end"
    );
    let shot_2 = session.shot_mut(2).expect("held");
    let pending = shot_2.replies.pop().expect("deferred");
    pending.fail("lost".into());
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        Status::Failed("lost".into())
    );
    assert_eq!(
        run(&mut session, &all, &[true]).err(),
        Some(Rejected::SessionFailed("lost".into()))
    );
}

#[test]
fn gather_reports_a_partial_start_after_its_parts_end() {
    let done = |values: Vec<u32>| {
        Immediate::done(
            values
                .into_iter()
                .map(|tag| Outcome {
                    tag,
                    value: Value::One,
                })
                .collect(),
        )
    };
    let mut gather = Gather::new(
        vec![(5, Some(done(vec![0, 1]))), (2, Some(done(vec![3])))],
        Some("shot 7: refused".into()),
    );
    let mut out = Vec::new();
    assert_eq!(
        gather.poll_events(&mut out, Waker::noop()),
        Status::Failed("shot 7: refused".into())
    );
    assert_eq!(
        out.iter().map(|o| (o.shot, o.tag)).collect::<Vec<_>>(),
        [(5, 0), (5, 1), (2, 3)]
    );
}

/// The session's needs are checked when it opens, naming the maker; it
/// reports each shot's probability when the maker offers them.
#[test]
fn needs_are_checked_at_open_and_probabilities_reported_at_close() {
    let shots = ShotSet::range(1..=2).expect("shots");
    let maker = TestDoubleShots::default();
    let unmet = |needs: &Needs| match maker.open(1, &shots, needs) {
        Err(Rejected::Unmet(Unmet {
            component, need, ..
        })) => Some((component, need)),
        _ => None,
    };
    let name = std::any::type_name::<TestDoubleShots>().to_owned();
    assert_eq!(
        unmet(&Needs::default().with_qubits(Some(5))),
        Some((name.clone(), Need::Qubits))
    );
    assert_eq!(
        unmet(&Needs::default().with_probabilities(true)),
        Some((name, Need::Probabilities))
    );
    let weighed = TestDoubleShots {
        probabilities: true,
        ..TestDoubleShots::default()
    };
    let mut session = weighed
        .open(
            1,
            &shots,
            &Needs::default()
                .with_probabilities(true)
                .with_replayable(true),
        )
        .expect("opens");
    assert!(session.capabilities().probabilities);
    assert_eq!(run(&mut session, &shots, &[true]).expect("runs").1, Ok(()));
    assert_eq!(
        run(&mut session, &ShotSet::one(2), &[true])
            .expect("runs")
            .1,
        Ok(())
    );
    let mut report = session.close();
    assert_eq!(
        (report.log_p.get(1), report.log_p.get(2)),
        (Some(-1.0), Some(-2.0))
    );
    report.fold(report.clone());
    assert_eq!(report.submissions, 4);
    assert_eq!(report.log_p.get(2), Some(-4.0));
}

/// The L3 wrapper over a per-shot session on shot 1 is the shot's executor
/// alone.
#[test]
fn a_wrapped_session_is_its_shots_executor() {
    let maker = TestDoubleShots::default();
    let mut wrapped = SubmissionWrapper::new(open(&maker, 3, &ShotSet::one(1)).expect("opens"));
    let mut alone = maker.make(&ShotContext::new(3, 1));
    for batch in [&[true, true][..], &[false, true, true]] {
        let (mut ours, mut theirs) = (Vec::new(), Vec::new());
        assert_eq!(wait(wrapped.call(batch).expect("runs"), &mut ours), Ok(()));
        assert_eq!(wait(alone.call(batch).expect("runs"), &mut theirs), Ok(()));
        assert_eq!(ours, theirs);
    }
    assert_eq!(wrapped.capabilities(), maker.capabilities());
    assert_eq!(wrapped.inner().shot(1).map(|shot| shot.calls), Some(2));
    assert_eq!(wrapped.close().shots, alone.close());
    let mut elsewhere = SubmissionWrapper::new(open(&maker, 3, &ShotSet::one(2)).expect("opens"));
    assert_eq!(
        elsewhere.call(&[true]).err(),
        Some(Rejected::ShotNotInSession(1))
    );
}
