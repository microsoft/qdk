// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The run a chain gives with its job ([`Run`]): given, it runs nothing;
//! waited for, or awaited, its driver drives its program, and it closes the
//! executor. The run owns the chain: its driver only calls the executor.

use std::future::{Future, IntoFuture};
use std::pin::Pin;

use crate::reply::parker::block_on;
use crate::shot_set::ShotIndex;
use crate::{
    Close, Driver, Executor, Instruction, InstructionBatch, InstructionBatchRef, Program, Rejected,
    ShotSet,
};

/// The executor a driver of a program `P` calls once its job is given to a
/// chain of `E`: `E`, as the program's calls reach it
/// ([`Instruction::Downstream`]).
type Called<P, E> = <<P as Program>::Instruction as Instruction>::Downstream<E>;

/// A chain's job ([`Chain::run`](crate::Chain::run)), its program with the
/// chain's driver, which drives it when the run is waited for
/// ([`Run::wait`]) or awaited. Until then, nothing runs.
///
/// Known limit, an open question: a run whose future is dropped before its
/// driver ends (a timeout, a `select!`), or whose driver, or an
/// executor's `call`, panics, does not close its executor; an executor
/// holding a device then releases it only in its own `Drop`, if it has one.
/// Whether a run closes its executor then (on drop, which would run on
/// unwinding too) or is stopped through the run is not decided yet; it goes
/// with the planned cancellation.
///
/// # Type parameters
///
/// - `T`: the chain's driver ([`Driver`]), which drives the program.
/// - `P`: the job's program ([`Program`]), whose output the run gives.
/// - `E`: the chain's executor, adapters included, which the run closes
///   once the driver's future has ended.
#[must_use = "a run runs nothing until it is waited for or awaited"]
pub struct Run<T, P: Program, E: Executor<InstructionBatch<P::Instruction>> + Close> {
    driver: T,
    program: P,
    shots: ShotSet,
    /// The executor, as the driver's calls reach it, decided when the job
    /// was given to the chain ([`Chain::run`](crate::Chain::run)).
    executor: Called<P, E>,
}

impl<T, P: Program, E: Executor<InstructionBatch<P::Instruction>> + Close> Run<T, P, E> {
    /// `program`, for `driver` to drive on `shots` and `executor`, which
    /// meets its needs.
    pub(crate) fn new(driver: T, program: P, shots: ShotSet, executor: Called<P, E>) -> Self {
        Self {
            driver,
            program,
            shots,
            executor,
        }
    }
}

impl<T, P, E> Run<T, P, E>
where
    T: Driver<P>,
    P: Program,
    E: Executor<InstructionBatch<P::Instruction>> + Close,
{
    /// Runs on this thread: the driver drives the program, then the run
    /// closes the executor, once, when the driver's future has ended. Gives
    /// the program's output and the executor's report. The thread parks
    /// while the driver waits for a reply.
    ///
    /// Do not call it from inside a driver's `drive`: the nested run parks
    /// this thread on the parker this run's wakes go to, so it can take a
    /// wake meant for this run, which then parks forever. Await the nested
    /// run instead.
    pub fn wait(self) -> (P::Output, E::Report) {
        block_on(self.run())
    }

    /// The run, as a future: the same steps as [`wait`](Run::wait).
    async fn run(self) -> (P::Output, E::Report) {
        let Self {
            driver,
            program,
            shots,
            mut executor,
        } = self;
        let output = driver
            .drive(program, &mut HeldShots::new(&mut executor, &shots), &shots)
            .await;
        (output, executor.close())
    }
}

/// The executor restricted to the shots of the job: a call that names
/// another shot is refused ([`Rejected::ShotNotHeld`]) before the executor
/// sees it.
struct HeldShots<'a, E> {
    executor: &'a mut E,
    shots: ShotIndex,
}

impl<'a, E> HeldShots<'a, E> {
    fn new(executor: &'a mut E, shots: &ShotSet) -> Self {
        let mut index = ShotIndex::default();
        for &j in shots {
            index.push(j);
        }
        Self {
            executor,
            shots: index,
        }
    }
}

impl<I, E: Executor<InstructionBatch<I>>> Executor<InstructionBatch<I>> for HeldShots<'_, E> {
    type Reply = E::Reply;

    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<E::Reply, Rejected> {
        if let Some(&j) = batch.shots.iter().find(|&&j| self.shots.get(j).is_none()) {
            return Err(Rejected::ShotNotHeld(j));
        }
        self.executor.call(batch)
    }
}

/// A run awaited: the same steps as [`Run::wait`], on the caller's
/// executor of futures.
///
/// Its future is boxed, since the driver's future has no name to give it
/// otherwise, and it is not `Send`, even when the run is, since a
/// driver's future need not be; it is awaited on the task that holds it,
/// and a caller that needs a `Send` future waits for the run instead
/// ([`Run::wait`]), on any thread a `Send` run moves to. The driver, the
/// program and the executor are `'static` (this impl's bounds, checked when
/// compiled), since the boxed future names no lifetime: a run whose driver,
/// program or executor borrows is waited for instead.
//
// Accepted limits: the two above, blocking neither this commit nor a later
// one; the first is pinned by `an_awaited_runs_future_is_not_send`, in
// `src/tests/compose/run.rs`, the second by the bounds.
//
// Design question, open (blocks neither this commit nor a later one): a
// run whose future is dropped before the driver's ends, or that a panic
// unwinds, never closes its executor (`Run`'s doc): `close` runs only
// after the driver's future. One answer for both cases, to settle with
// the planned cancellation (a caller stops a run through the run: a handle
// on `Run`, or a close on drop, which would run on unwinding too).
impl<T, P, E> IntoFuture for Run<T, P, E>
where
    T: Driver<P> + 'static,
    P: Program + 'static,
    E: Executor<InstructionBatch<P::Instruction>> + Close + 'static,
{
    type Output = (P::Output, E::Report);
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.run())
    }
}
