// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests' executor ([`TestDoubleExecutor`]): each shot runs on its own
//! state, made at the shot's first call from the shot's seeds by a test
//! double ([`Model`]), by the crate's per-shot rules (`ShotStates`), which
//! the executors that adapters make follow too.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ops::Shr;
use std::rc::Rc;

use crate::tests::{Count, MapExecutor};
use crate::{
    AdapterFn, Capabilities, ChainPrefix, Close, Compose, ComposedAdapters, ComposedJob, Describe,
    Executor, ExecutorReport, IncrementalReply, InstructionBatch, InstructionBatchRef, NoAdapter,
    OnePerShotAdapter, Rejected, ReplySender, ShotContext, ShotId, ShotOutcome, ShotReplies,
    ShotSet, ShotStates, Value, WithExecutor, reply_channel, wait,
};

/// How one shot of a [`TestDoubleExecutor`] runs, on its own state: it states
/// what the executor offers ([`Describe`]), makes a shot's state from the
/// shot's seeds, and gives its log probability; the state runs each batch
/// ([`Executor`]) and reports when closed ([`Close`]).
pub(in crate::tests) trait Model: Describe {
    /// The instructions its shots run.
    type Instruction;
    /// One shot's state, which runs its batches on that shot alone.
    type State: Executor<InstructionBatch<Self::Instruction>> + Close;

    /// A fresh state whose components each draw from their own seed in
    /// `ctx`: the last executor's draws from [`ShotContext::seed`]; or why
    /// it cannot be made.
    fn state(&self, ctx: &ShotContext) -> Result<Self::State, String>;

    /// The executor's check of a batch's instructions, once per batch, before
    /// any shot runs them; the default accepts any.
    ///
    /// # Errors
    ///
    /// Why the batch is refused.
    fn check(&self, batch: &[Self::Instruction]) -> Result<(), Rejected> {
        let _ = batch;
        Ok(())
    }

    /// The log probability so far of `state`'s shot: the sum of ln p over
    /// every draw made for it, by every component in it. `None`, the
    /// default, where it is not known: absent, not 0. A model whose
    /// capabilities offer `log_probability` overrides it.
    ///
    /// Read once per shot, as the executor closes.
    #[must_use]
    fn log_p(state: &Self::State) -> Option<f64> {
        let _ = state;
        None
    }
}

/// An executor whose shots each run on their own state, as its model `M`
/// says ([`Model`]): shot `j`'s state is made at the shot's first call,
/// from the shot's seeds ([`ShotContext::last_executor`]`(seed, j)`, with
/// the seed the executor was built with), and kept for its later calls;
/// nothing is shared between shots.
///
/// It is the simplest executor, and the one for programs with feedback: a
/// shot's measurements steer its own state only. Each shot draws only
/// from its own seeds, so its outcomes do not depend on which other shots
/// the executor runs, nor on how a run's shots are split over executors of
/// the same seed. It offers what `M` offers and, with `log_probability`,
/// reports each shot's [`Model::log_p`] at close. It runs batches by
/// the rules of `ShotStates`, which each shot's placed executor follows
/// too: a refusal by the first shot refuses the call; a later one fails it
/// and the executor.
pub(in crate::tests) struct TestDoubleExecutor<M: Model> {
    model: M,
    seed: u64,
    capabilities: Capabilities,
    shots: ShotStates<M::State>,
}

impl<M: Model> TestDoubleExecutor<M> {
    /// An executor whose shot `j` draws from the seeds of
    /// [`ShotContext::last_executor`]`(seed, j)`, its shots run by `model`:
    /// `seed` is the run's seed, which it passes up to the adapters composed
    /// with it, as [`ExecutorFn::seed`](crate::ExecutorFn::seed) does.
    #[must_use]
    pub(in crate::tests) fn with_model(model: M, seed: u64) -> Self {
        let mut capabilities = model.capabilities();
        capabilities.run_seed = Some(seed);
        Self {
            capabilities,
            model,
            seed,
            shots: ShotStates::default(),
        }
    }

    /// Shot `j`'s state, as its calls so far left it.
    pub(in crate::tests) fn state(&self, j: ShotId) -> Option<&M::State> {
        self.shots.get(j)
    }
}

impl<M: Model> Describe for TestDoubleExecutor<M> {
    /// The capabilities of its model.
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }
}

impl<M: Model> Executor<InstructionBatch<M::Instruction>> for TestDoubleExecutor<M> {
    type Reply = ShotReplies<<M::State as Executor<InstructionBatch<M::Instruction>>>::Reply>;

    /// Runs the instructions on each shot of the batch, in its order, making
    /// the state of each shot not called before ([`Model::state`]).
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`], [`Rejected::EarlierFailure`], the model's
    /// [`Model::check`], [`Rejected::Unavailable`] when a state cannot be made,
    /// then the rejection of the first shot called. Nothing ran in each case.
    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, M::Instruction>,
    ) -> Result<Self::Reply, Rejected> {
        let (model, seed) = (&self.model, self.seed);
        (self.shots).call(
            batch,
            |batch| model.check(batch),
            |j| {
                model
                    .state(&ShotContext::last_executor(seed, j))
                    .map_err(|reason| Rejected::Unavailable(format!("shot {j}: {reason}")))
            },
            drop,
        )
    }
}

impl<M: Model> Close for TestDoubleExecutor<M> {
    type Report = ExecutorReport<<M::State as Close>::Report>;

    /// Closes each shot's state made before a failed call, if any, and
    /// folds their reports; adds each such shot's log probability, when its
    /// model offers `log_probability` and knows the shot's. The shots' states
    /// are dropped.
    fn close(&mut self) -> Self::Report {
        let log_probability = self.capabilities.log_probability;
        (self.shots).close(
            |_| {},
            |shot| M::log_p(shot).filter(|_| log_probability),
            drop,
        )
    }
}

/// `>>` with the tests' executor on the right, as an executor's crate
/// implements it.
macro_rules! compose_by_shr {
    ($($generics:ident),* ; $left:ty) => {
        impl<$($generics,)* M: Model> Shr<TestDoubleExecutor<M>> for $left
        where
            Self: Compose<TestDoubleExecutor<M>, WithExecutor>,
        {
            type Output = <Self as Compose<TestDoubleExecutor<M>, WithExecutor>>::Output;

            fn shr(self, rhs: TestDoubleExecutor<M>) -> Self::Output {
                self.compose(rhs)
            }
        }
    };
}

compose_by_shr!(T, P, L; ComposedJob<T, P, L>);
compose_by_shr!(T, L; ChainPrefix<T, L>);
compose_by_shr!(X, Y; ComposedAdapters<X, Y>);
compose_by_shr!(I; NoAdapter<I>);
compose_by_shr!(I, O, S, N, L, C, R; AdapterFn<I, O, S, N, L, C, R>);
compose_by_shr!(I, O, S, N, L, C, R; OnePerShotAdapter<I, O, S, N, L, C, R>);
compose_by_shr!(I, F; MapExecutor<I, F>);

/// Makes test double states of a one-instruction ISA: each `true` reads
/// a bit of the last executor's seed in the shot, so outcomes show which
/// seed a shot drew from.
#[derive(Default)]
pub(in crate::tests) struct TestDoubleModel {
    /// The shot whose executor refuses every call.
    pub(in crate::tests) refusing: Option<ShotId>,
    /// The shot whose state failed in a call whose reply did not show it
    /// yet: it refuses every call for that failure (`lost`).
    pub(in crate::tests) failed_unseen: Option<ShotId>,
    /// The shot whose state cannot be made the first time it is asked
    /// for: its memory is in use.
    pub(in crate::tests) unavailable_once: Cell<Option<ShotId>>,
    /// The shot whose batches fail as they are called.
    pub(in crate::tests) failing: Option<ShotId>,
    /// The shot whose batches are cancelled as they are called, below the
    /// state (by an executor it places in turn, dropped).
    pub(in crate::tests) cancelling: Option<ShotId>,
    /// The call of the `failing` or `cancelling` shot, counted from 0, from
    /// which its batches fail or are cancelled; those before end `Done`.
    pub(in crate::tests) failing_from_call: u64,
    /// The shot whose state the model panics making: a model with a bug.
    pub(in crate::tests) panicking: Option<ShotId>,
    /// Whether the executor's check refuses a batch with a `false`.
    pub(in crate::tests) refuses_false: bool,
    /// The most shots held at once; `None`: no limit.
    pub(in crate::tests) max_shots: Option<u64>,
    /// The shots whose replies wait until the test ends them.
    pub(in crate::tests) deferred: Vec<ShotId>,
    pub(in crate::tests) log_probability: bool,
    /// Whether it offers no resumable shots: an executor that does not continue
    /// a shot in a later call. It keeps its states all the same, but a job
    /// refused for that need runs none.
    pub(in crate::tests) without_resumable_shots: bool,
    /// The states made so far, shared with the test.
    pub(in crate::tests) made: Rc<Cell<usize>>,
    /// What the shots ran, and their deferred replies, shared with the
    /// test.
    pub(in crate::tests) seen: Rc<RefCell<Seen>>,
}

/// What the states of a [`TestDoubleModel`] ran, and the replies they were
/// told to defer, which the test ends.
#[derive(Default)]
pub(in crate::tests) struct Seen {
    /// The calls each state made so far ran.
    pub(in crate::tests) calls: BTreeMap<ShotId, u64>,
    /// Each deferred shot's pending replies, in the order of its calls.
    pub(in crate::tests) replies: BTreeMap<ShotId, Vec<ReplySender<ShotOutcome>>>,
}

/// One shot's state: its seed, its calls, and whether it defers its replies.
pub(in crate::tests) struct TestDoubleState {
    shot: ShotId,
    seed: u64,
    calls: u64,
    /// What it refuses every call with, if it does.
    refuses: Option<Rejected>,
    failing: bool,
    cancelling: bool,
    /// The first of its calls that fails or is cancelled, if it does.
    failing_from_call: u64,
    deferred: bool,
    seen: Rc<RefCell<Seen>>,
}

impl TestDoubleState {
    /// The calls it ran.
    pub(in crate::tests) fn calls(&self) -> u64 {
        self.calls
    }

    /// The value of read `i` of call `call`.
    fn bit(&self, call: u64, i: usize) -> Value {
        (self.seed >> ((call * 8 + i as u64) % 64) & 1 == 1).into()
    }
}

impl Describe for TestDoubleModel {
    fn capabilities(&self) -> Capabilities {
        let mut offered = Capabilities::default()
            .with_qubits(4)
            .with_log_probability(self.log_probability)
            .with_resumable_shots(!self.without_resumable_shots);
        offered.max_shots = self.max_shots;
        offered
    }
}

impl Model for TestDoubleModel {
    type Instruction = bool;
    type State = TestDoubleState;

    fn check(&self, batch: &[bool]) -> Result<(), Rejected> {
        match batch.iter().position(|&read| !read) {
            Some(at) if self.refuses_false => Err(Rejected::Instruction {
                at,
                reason: "not a read".into(),
            }),
            _ => Ok(()),
        }
    }

    fn state(&self, ctx: &ShotContext) -> Result<TestDoubleState, String> {
        let j = ctx.shot();
        assert!(self.panicking != Some(j), "the model has a bug");
        if self.unavailable_once.get() == Some(j) {
            self.unavailable_once.set(None);
            return Err("memory in use".into());
        }
        self.made.set(self.made.get() + 1);
        self.seen.borrow_mut().calls.insert(j, 0);
        Ok(TestDoubleState {
            shot: j,
            seed: ctx.seed(),
            calls: 0,
            refuses: if self.refusing == Some(j) {
                Some(Rejected::Unavailable("busy".into()))
            } else if self.failed_unseen == Some(j) {
                Some(Rejected::EarlierFailure("lost".into()))
            } else {
                None
            },
            failing: self.failing == Some(j),
            cancelling: self.cancelling == Some(j),
            failing_from_call: self.failing_from_call,
            deferred: self.deferred.contains(&j),
            seen: Rc::clone(&self.seen),
        })
    }

    fn log_p(state: &TestDoubleState) -> Option<f64> {
        #[expect(clippy::cast_precision_loss, reason = "a test's few calls")]
        Some(-(state.calls as f64))
    }
}

impl Executor<InstructionBatch<bool>> for TestDoubleState {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        batch.shots.check_only(self.shot)?;
        if let Some(refusal) = &self.refuses {
            return Err(refusal.clone());
        }
        let (sender, reply) = reply_channel();
        for (i, &read) in batch.instructions.iter().enumerate() {
            if read {
                sender.push(ShotOutcome::new(
                    self.shot,
                    u32::try_from(i).unwrap_or(u32::MAX),
                    self.bit(self.calls, i),
                ));
            }
        }
        let ends_badly = self.calls >= self.failing_from_call;
        self.calls += 1;
        let mut seen = self.seen.borrow_mut();
        seen.calls.insert(self.shot, self.calls);
        if self.failing && ends_badly {
            sender.fail("lost".into());
        } else if self.cancelling && ends_badly {
            sender.cancel();
        } else if self.deferred {
            seen.replies.entry(self.shot).or_default().push(sender);
        } else {
            sender.done();
        }
        Ok(reply)
    }
}

impl Close for TestDoubleState {
    type Report = Count;

    fn close(&mut self) -> Count {
        Count(std::mem::take(&mut self.calls))
    }
}

/// Runs `batch` on `shots` of `executor` to its end.
pub(in crate::tests) fn run<I, E: Executor<InstructionBatch<I>>>(
    executor: &mut E,
    shots: &ShotSet,
    batch: &[I],
) -> Result<(Vec<ShotOutcome>, Result<(), String>), Rejected> {
    let reply = executor.call(InstructionBatchRef {
        shots,
        instructions: batch,
    })?;
    let mut out = Vec::new();
    let end = wait(reply, &mut out);
    Ok((out, end))
}

/// Runs `batch` on `shots` of `executor`, a step that sets a test's scene:
/// the executor accepts it and it ends well, as `why` says.
pub(in crate::tests) fn step<I, E: Executor<InstructionBatch<I>>>(
    executor: &mut E,
    shots: &ShotSet,
    batch: &[I],
    why: &str,
) {
    match run(executor, shots, batch) {
        Ok((_, Ok(()))) => {}
        other => panic!("{why}, but: {other:?}"),
    }
}
