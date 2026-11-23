// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake executors.

use std::collections::BTreeMap;
use std::future::poll_fn;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Poll, Waker};
use std::thread;
use std::time::Duration;

use super::{Log, Stream, YieldNow, batches_of, capabilities, coin_gates, ready_for};
use crate::prelude::*;
use crate::tests::example_sets::Measuring;
use crate::tests::example_sets::{BitReadout, ShotFlags};
use crate::{
    Batch, BatchReadouts, Capabilities, ExecuteError, Features, Limited, NotReady, Resources, Seed,
    ShotId, ShotSet,
};

/// A toy executor on classical bits, as many as its instructions name:
/// `h` tosses a bit from the shot's stream, `x` flips it, and `mz` reads
/// it; any other instruction of its set does nothing, and one beyond its
/// set panics, as an executor running what it cannot decode would fail.
pub(crate) struct Bits {
    /// The set it decodes its instructions in.
    runs: Measuring,
    accepts: Measuring,
    provides: Capabilities<Measuring>,
    shots: BTreeMap<ShotId, (Vec<bool>, Stream)>,
    busy: Option<&'static str>,
    loses: bool,
    panics_ending: bool,
    /// The set `accepts` answers once ready, if it changes then.
    set_once_ready: Option<Measuring>,
    log: Log,
}

impl Bits {
    /// Bits accepting [`coin_gates`], providing batches of 64
    /// instructions, logging to `log`.
    pub(crate) fn new(log: &Log) -> Limited<Self> {
        Self::accepting(*coin_gates(), log)
    }

    /// Bits accepting `set`, which may lack some of `h`, `x` and `mz`.
    pub(crate) fn accepting(set: Measuring, log: &Log) -> Limited<Self> {
        Self {
            runs: set,
            accepts: set,
            provides: capabilities(batches_of(64), Features::unconstrained()),
            shots: BTreeMap::new(),
            busy: None,
            loses: false,
            panics_ending: false,
            set_once_ready: None,
            log: log.clone(),
        }
        .limited()
    }
}

impl Limited<Bits> {
    /// These bits, providing `resources` in place of their batches of 64.
    pub(crate) fn with_resources(mut self, resources: Resources) -> Self {
        self.part.provides = self.part.provides.map_resources(|_| resources);
        self
    }

    /// These bits, providing `features` in place of their own.
    pub(crate) fn with_features(mut self, features: Features) -> Self {
        self.part.provides = self.part.provides.map_features(|_| features);
        self
    }

    /// These bits, refusing every run in `ready`, for `why`.
    pub(crate) fn busy(mut self, why: &'static str) -> Self {
        self.part.busy = Some(why);
        self
    }

    /// These bits, accepting `set` once ready, while still running their
    /// own `h`, `x` and `mz`: a part that breaks the rule that what it
    /// accepts does not change once composed.
    pub(crate) fn accepting_once_ready(mut self, set: Measuring) -> Self {
        self.part.set_once_ready = Some(set);
        self
    }

    /// These bits, losing every value they read, and finding every shot
    /// suspect.
    pub(crate) fn losing(mut self) -> Self {
        self.part.loses = true;
        self
    }

    /// These bits, panicking in every `end_shots`, once they have ended
    /// the shots and logged it.
    pub(crate) fn panicking_ending(mut self) -> Self {
        self.part.panics_ending = true;
        self
    }
}

impl ExecutorLimits for Bits {
    fn provides(&self) -> &Capabilities<Measuring> {
        &self.provides
    }

    async fn ready_for(&mut self, above: &Capabilities<Measuring>) -> Result<(), NotReady> {
        self.log.push(ready_for("Bits", above));
        if let Some(set) = self.set_once_ready.take() {
            self.accepts = set;
        }
        match self.busy {
            Some(why) => Err(NotReady::new(why)),
            None => Ok(()),
        }
    }
}

impl Executor for Bits {
    type Accepts = Measuring;

    fn accepts(&self) -> &Measuring {
        &self.accepts
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        readouts: &mut [BatchReadouts<Measuring>],
    ) -> Result<(), ExecuteError> {
        self.log.push("Bits execute");
        for (batch, batch_readouts) in batches.iter().zip(readouts) {
            for (position, id) in batch.shots.iter().enumerate() {
                let (bits, stream) = self
                    .shots
                    .entry(id)
                    .or_insert_with(|| (Vec::new(), Stream(seed.shot(id))));
                let mut row = batch_readouts.row_mut(position).iter_mut();
                for &gate in batch.instructions {
                    let q = gate.qubit as usize;
                    if q >= bits.len() {
                        bits.resize(q + 1, false);
                    }
                    match self.runs.name(gate) {
                        Some("h") => bits[q] = stream.chance(0.5),
                        Some("x") => bits[q] = !bits[q],
                        Some("mz") => {
                            *row.next().expect("a slot per `mz`") = if self.loses {
                                BitReadout::Lost
                            } else if bits[q] {
                                BitReadout::One
                            } else {
                                BitReadout::Zero
                            };
                        }
                        Some(_) => {}
                        None => panic!("the bits run no gate at position {}", gate.at),
                    }
                }
                if self.loses {
                    batch_readouts.set_conditions(position, ShotFlags::REJECTED);
                }
            }
        }
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        for id in shots.iter() {
            self.shots.remove(&id);
        }
        self.log
            .push(format!("Bits end_shots, holding {}", self.shots.len()));
        assert!(!self.panics_ending, "end_shots broke");
    }
}

/// An executor whose answer comes on a later poll: pending once, woken at
/// once, as a device's that answers later.
pub(crate) struct Later<E>(pub(crate) E);

impl<E: Executor> ExecutorLimits for Later<E> {
    fn provides(&self) -> &Capabilities<E::Accepts> {
        self.0.provided()
    }

    async fn ready_for(&mut self, above: &Capabilities<E::Accepts>) -> Result<(), NotReady> {
        self.0.ready_given(above).await
    }
}

impl<E: Executor> Executor for Later<E> {
    type Accepts = E::Accepts;

    fn accepts(&self) -> &E::Accepts {
        self.0.accepts()
    }

    async fn ready(&mut self) -> Result<(), NotReady> {
        self.0.ready().await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_, E::Accepts>],
        seed: Seed,
        readouts: &mut [BatchReadouts<E::Accepts>],
    ) -> Result<(), ExecuteError> {
        YieldNow::default().await;
        self.0.execute(batches, seed, readouts).await
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.0.end_shots(shots);
    }
}

/// An executor running as the one it holds, but keeping the default
/// `provides`: it states nothing.
pub(crate) struct StatesNoLimits<E>(pub(crate) E);

impl<E: Executor> ExecutorLimits for StatesNoLimits<E> {
    async fn ready_for(&mut self, above: &Capabilities<E::Accepts>) -> Result<(), NotReady> {
        self.0.ready_given(above).await
    }
}

impl<E: Executor> Executor for StatesNoLimits<E> {
    type Accepts = E::Accepts;

    fn accepts(&self) -> &E::Accepts {
        self.0.accepts()
    }

    async fn ready(&mut self) -> Result<(), NotReady> {
        self.0.ready().await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_, E::Accepts>],
        seed: Seed,
        readouts: &mut [BatchReadouts<E::Accepts>],
    ) -> Result<(), ExecuteError> {
        self.0.execute(batches, seed, readouts).await
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.0.end_shots(shots);
    }
}

/// An executor whose answer a worker thread hands over, after `delay`, and
/// at the first `execute` only once `gate` opens: the run's waker is woken
/// from that thread.
pub(crate) struct FromThread<E> {
    inner: E,
    delay: Duration,
    gate: Option<mpsc::Receiver<()>>,
}

impl<E> FromThread<E> {
    /// `inner`, answering from a thread after `delay`, stating the limits
    /// `inner` states.
    pub(crate) fn new(inner: E, delay: Duration) -> Limited<Self> {
        Limited {
            part: Self {
                inner,
                delay,
                gate: None,
            },
        }
    }
}

impl<E> Limited<FromThread<E>> {
    /// This executor, whose first answer waits for `gate`.
    pub(crate) fn gated(mut self, gate: mpsc::Receiver<()>) -> Self {
        self.part.gate = Some(gate);
        self
    }
}

/// The inner executor's result, once the worker hands it over, and the
/// waker to wake then.
struct Handover {
    result: Option<Result<(), ExecuteError>>,
    waker: Option<Waker>,
}

impl<E: Executor> ExecutorLimits for FromThread<E> {
    fn provides(&self) -> &Capabilities<E::Accepts> {
        self.inner.provided()
    }

    async fn ready_for(&mut self, above: &Capabilities<E::Accepts>) -> Result<(), NotReady> {
        self.inner.ready_given(above).await
    }
}

impl<E: Executor> Executor for FromThread<E> {
    type Accepts = E::Accepts;

    fn accepts(&self) -> &E::Accepts {
        self.inner.accepts()
    }

    async fn ready(&mut self) -> Result<(), NotReady> {
        self.inner.ready().await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_, E::Accepts>],
        seed: Seed,
        readouts: &mut [BatchReadouts<E::Accepts>],
    ) -> Result<(), ExecuteError> {
        let result = self.inner.execute(batches, seed, readouts).await;
        let handover = Arc::new(Mutex::new(Handover {
            result: None,
            waker: None,
        }));
        let worker = {
            let handover = Arc::clone(&handover);
            let gate = self.gate.take();
            let delay = self.delay;
            thread::spawn(move || {
                if let Some(gate) = gate {
                    gate.recv().expect("the test opens the gate");
                }
                thread::sleep(delay);
                let mut handover = handover.lock().expect("no panic while held");
                handover.result = Some(result);
                if let Some(waker) = handover.waker.take() {
                    waker.wake();
                }
            })
        };
        let result = poll_fn(|context| {
            let mut handover = handover.lock().expect("no panic while held");
            if let Some(result) = handover.result.take() {
                Poll::Ready(result)
            } else {
                handover.waker = Some(context.waker().clone());
                Poll::Pending
            }
        })
        .await;
        worker.join().expect("the worker ends");
        result
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.inner.end_shots(shots);
    }
}
