// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake executors.

use std::collections::BTreeMap;
use std::future::poll_fn;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Poll, Waker};
use std::thread;
use std::time::Duration;

use super::{CoinGates, Log, Stream, YieldNow, qubits};
use crate::prelude::*;
use crate::{
    Batch, BatchReadouts, BlockType, Capabilities, ExecuteError, Features, InstructionSet,
    NotReady, Opcode, ReadoutFailure, ReadoutValue, Resources, Seed, ShotConditions, ShotId,
    ShotSet,
};

/// A toy executor on classical bits: `h` tosses a bit from the shot's
/// stream, `x` flips it, and `mz` reads it; any other instruction of its
/// set does nothing.
pub(crate) struct Bits {
    opcodes: [Option<Opcode>; 3],
    provides: Capabilities,
    qubits: usize,
    shots: BTreeMap<ShotId, (Vec<bool>, Stream)>,
    busy: Option<&'static str>,
    loses: bool,
    panics_ending: bool,
    /// The set `provides` answers once ready, if it changes then.
    set_once_ready: Option<InstructionSet>,
    log: Log,
}

impl Bits {
    /// Bits providing [`CoinGates`] and 64 qubits, and resolving no
    /// readout parity, logging to `log`.
    pub(crate) fn new(log: &Log) -> Self {
        Self::providing(CoinGates::set(), log)
    }

    /// Bits providing `set`, which may lack some of `h`, `x` and `mz`.
    pub(crate) fn providing(set: InstructionSet, log: &Log) -> Self {
        Self {
            opcodes: ["h", "x", "mz"].map(|mnemonic| set.opcode(mnemonic)),
            provides: Capabilities::new(set)
                .with_resources(qubits(64))
                .with_features(Features::new().with_readout_parity(false)),
            qubits: 0,
            shots: BTreeMap::new(),
            busy: None,
            loses: false,
            panics_ending: false,
            set_once_ready: None,
            log: log.clone(),
        }
    }

    /// These bits, providing `resources` in place of their 64 qubits.
    pub(crate) fn with_resources(mut self, resources: Resources) -> Self {
        self.provides = self.provides.with_resources(resources);
        self
    }

    /// These bits, providing `features` in place of their own.
    pub(crate) fn with_features(mut self, features: Features) -> Self {
        self.provides = self.provides.with_features(features);
        self
    }

    /// These bits, resolving readout parities.
    pub(crate) fn resolving_parity(self) -> Self {
        self.with_features(Features::new().with_readout_parity(true))
    }

    /// These bits, refusing every run in `ready`, for `why`.
    pub(crate) fn busy(mut self, why: &'static str) -> Self {
        self.busy = Some(why);
        self
    }

    /// These bits, providing `set` once ready, while still running their
    /// own `h`, `x` and `mz`: a part that breaks the rule that what it
    /// provides does not change once composed.
    pub(crate) fn providing_once_ready(mut self, set: InstructionSet) -> Self {
        self.set_once_ready = Some(set);
        self
    }

    /// These bits, losing every value they read, and finding every shot
    /// suspect.
    pub(crate) fn losing(mut self) -> Self {
        self.loses = true;
        self
    }

    /// These bits, panicking in every `end_shots`, once they have ended
    /// the shots and logged it.
    pub(crate) fn panicking_ending(mut self) -> Self {
        self.panics_ending = true;
        self
    }
}

impl Executor for Bits {
    fn provides(&self) -> &Capabilities {
        &self.provides
    }

    async fn ready(&mut self, required: &Capabilities) -> Result<(), NotReady> {
        let qubits = required
            .resources()
            .blocks(&BlockType::qubit())
            .unwrap_or(0);
        self.log.push(format!("Bits ready for {qubits} qubits"));
        // Each shot holds every qubit the bits provide, as a device's
        // register does, so that an encoding that does not state the
        // qubits it uses still finds them.
        let provided = self.provides.resources().blocks(&BlockType::qubit());
        self.qubits = qubits.max(provided.unwrap_or(0)) as usize;
        if let Some(set) = self.set_once_ready.take() {
            self.provides = Capabilities::new(set)
                .with_resources(self.provides.resources().clone())
                .with_features(self.provides.features().clone());
        }
        match self.busy {
            Some(why) => Err(NotReady::new(why)),
            None => Ok(()),
        }
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.log.push("Bits execute");
        let [h, x, mz] = self.opcodes;
        for (batch, batch_readouts) in batches.iter().zip(readouts) {
            for (position, id) in batch.shots.iter().enumerate() {
                let qubits = self.qubits;
                let (bits, stream) = self
                    .shots
                    .entry(id)
                    .or_insert_with(|| (vec![false; qubits], Stream(seed.shot(id))));
                let mut row = batch_readouts.row_mut(position).iter_mut();
                for instruction in batch.instructions {
                    let q = instruction.blocks[0] as usize;
                    let opcode = Some(instruction.opcode);
                    if opcode == h {
                        bits[q] = stream.chance(0.5);
                    } else if opcode == x {
                        bits[q] = !bits[q];
                    } else if opcode == mz {
                        *row.next().expect("a slot per `mz`") = if self.loses {
                            ReadoutValue::Missing(ReadoutFailure::Loss)
                        } else if bits[q] {
                            ReadoutValue::One
                        } else {
                            ReadoutValue::Zero
                        };
                    }
                }
                if self.loses {
                    batch_readouts.set_conditions(position, ShotConditions::REJECTED);
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

impl<E: Executor> Executor for Later<E> {
    fn provides(&self) -> &Capabilities {
        self.0.provides()
    }

    async fn ready(&mut self, required: &Capabilities) -> Result<(), NotReady> {
        self.0.ready(required).await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        YieldNow::default().await;
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
    pub(crate) fn new(inner: E, delay: Duration) -> Self {
        Self {
            inner,
            delay,
            gate: None,
        }
    }

    /// This executor, whose first answer waits for `gate`.
    pub(crate) fn gated(mut self, gate: mpsc::Receiver<()>) -> Self {
        self.gate = Some(gate);
        self
    }
}

/// The inner executor's result, once the worker hands it over, and the
/// waker to wake then.
struct Handover {
    result: Option<Result<(), ExecuteError>>,
    waker: Option<Waker>,
}

impl<E: Executor> Executor for FromThread<E> {
    fn provides(&self) -> &Capabilities {
        self.inner.provides()
    }

    async fn ready(&mut self, required: &Capabilities) -> Result<(), NotReady> {
        self.inner.ready(required).await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
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
