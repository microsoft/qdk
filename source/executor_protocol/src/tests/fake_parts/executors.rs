// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake executors.

use std::collections::BTreeMap;
use std::future::poll_fn;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Poll, Waker};
use std::thread;
use std::time::Duration;

use super::{CoinGates, Log, Stream, YieldNow, coin_gates, zeros};
use crate::{
    Batch, BatchReadouts, BlockType, Capabilities, Drive, DriveError, Execute, ExecuteError,
    InstructionSet, Needs, Opcode, OpcodeMap, ReadoutFailure, ReadoutValue, RunRefusal, Seed,
    ShotConditions, ShotId, ShotSet,
};

/// A toy executor on classical bits: `h` tosses a bit from the shot's
/// stream, `x` flips it, and `mz` reads it; any other instruction of its
/// set does nothing.
pub(crate) struct Bits {
    set: InstructionSet,
    opcodes: [Option<Opcode>; 3],
    offer: Capabilities,
    qubits: usize,
    shots: BTreeMap<ShotId, (Vec<bool>, Stream)>,
    busy: Option<&'static str>,
    loses: bool,
    log: Log,
}

impl Bits {
    /// Bits accepting [`CoinGates`], offering 64 qubits, logging to `log`.
    pub(crate) fn new(log: &Log) -> Self {
        Self::accepting(CoinGates::set(), log)
    }

    /// Bits accepting `set`, which may lack some of `h`, `x` and `mz`.
    pub(crate) fn accepting(set: InstructionSet, log: &Log) -> Self {
        Self {
            opcodes: ["h", "x", "mz"].map(|mnemonic| set.opcode(mnemonic)),
            set,
            offer: Capabilities::unlimited()
                .max_qubits(64)
                .without_readout_parity(),
            qubits: 0,
            shots: BTreeMap::new(),
            busy: None,
            loses: false,
            log: log.clone(),
        }
    }

    /// These bits, offering `offer`.
    pub(crate) fn offering(mut self, offer: Capabilities) -> Self {
        self.offer = offer;
        self
    }

    /// These bits, refusing every run in `ready`, for `why`.
    pub(crate) fn busy(mut self, why: &'static str) -> Self {
        self.busy = Some(why);
        self
    }

    /// These bits, losing every value they read, and finding every shot
    /// suspect.
    pub(crate) fn losing(mut self) -> Self {
        self.loses = true;
        self
    }
}

impl Execute for Bits {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn offers(&self) -> Capabilities {
        self.offer.clone()
    }

    fn provision(&mut self, needs: Needs) {
        let qubits = needs.blocks_of(&BlockType::qubit());
        self.log
            .push(format!("Bits provisioned for {qubits} qubits"));
        self.qubits = qubits as usize;
    }

    async fn ready(&mut self) -> Result<(), RunRefusal> {
        self.log.push("Bits ready");
        match self.busy {
            Some(why) => Err(RunRefusal::Unavailable(why.into())),
            None => Ok(()),
        }
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        self.log.push("Bits execute");
        let [h, x, mz] = self.opcodes;
        let mut answer = Vec::with_capacity(batches.len());
        for batch in batches {
            let reads = batch
                .instructions
                .iter()
                .filter(|instruction| Some(instruction.opcode) == mz)
                .count();
            let width = u32::try_from(reads).expect("a small batch");
            let mut rows = BatchReadouts::with_capacity(batch.shots.len(), width);
            for id in batch.shots.iter() {
                let qubits = self.qubits;
                let (bits, stream) = self
                    .shots
                    .entry(id)
                    .or_insert_with(|| (vec![false; qubits], Stream(seed.shot(id))));
                let mut row = Vec::with_capacity(reads);
                for instruction in batch.instructions {
                    let q = instruction.blocks[0] as usize;
                    let opcode = Some(instruction.opcode);
                    if opcode == h {
                        bits[q] = stream.chance(500);
                    } else if opcode == x {
                        bits[q] = !bits[q];
                    } else if opcode == mz {
                        row.push(if self.loses {
                            ReadoutValue::Missing(ReadoutFailure::Loss)
                        } else if bits[q] {
                            ReadoutValue::One
                        } else {
                            ReadoutValue::Zero
                        });
                    }
                }
                let conditions = if self.loses {
                    ShotConditions::REJECTED
                } else {
                    ShotConditions::NONE
                };
                rows.push_row(&row, conditions).expect("one value per `mz`");
            }
            answer.push(rows);
        }
        Ok(answer)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        for id in shots.iter() {
            self.shots.remove(&id);
        }
        self.log
            .push(format!("Bits end_shots, holding {}", self.shots.len()));
    }
}

/// An executor whose answer comes on a later poll: pending once, woken at
/// once, as a device's that answers later.
pub(crate) struct Later<E>(pub(crate) E);

impl<E: Execute> Execute for Later<E> {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn offers(&self) -> Capabilities {
        self.0.offers()
    }

    fn provision(&mut self, needs: Needs) {
        self.0.provision(needs);
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        YieldNow::default().await;
        self.0.execute(batches, seed).await
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

/// An answer, once the worker hands it over, and the waker to wake then.
struct Handover {
    answer: Option<Result<Vec<BatchReadouts>, ExecuteError>>,
    waker: Option<Waker>,
}

impl<E: Execute> Execute for FromThread<E> {
    fn accepts(&self) -> &InstructionSet {
        self.inner.accepts()
    }

    fn offers(&self) -> Capabilities {
        self.inner.offers()
    }

    fn provision(&mut self, needs: Needs) {
        self.inner.provision(needs);
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let answer = self.inner.execute(batches, seed).await;
        let handover = Arc::new(Mutex::new(Handover {
            answer: None,
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
                handover.answer = Some(answer);
                if let Some(waker) = handover.waker.take() {
                    waker.wake();
                }
            })
        };
        let answer = poll_fn(|context| {
            let mut handover = handover.lock().expect("no panic while held");
            if let Some(answer) = handover.answer.take() {
                Poll::Ready(answer)
            } else {
                handover.waker = Some(context.waker().clone());
                Poll::Pending
            }
        })
        .await;
        worker.join().expect("the worker ends");
        answer
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.inner.end_shots(shots);
    }
}

/// How a [`Misshapen`] executor's answer differs from the batches'.
#[derive(Clone, Copy)]
pub(crate) enum Misshape {
    /// A readouts more than the batches.
    ExtraReadouts,
    /// A row fewer than the last batch's shots.
    MissingRow,
    /// A value more in each row than the last batch's readout slots.
    ExtraValue,
}

/// An executor of [`CoinGates`] whose answer has the wrong shape, in its
/// last batch's readouts: every value `Zero`.
pub(crate) struct Misshapen(pub(crate) Misshape);

impl Execute for Misshapen {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
    }

    fn offers(&self) -> Capabilities {
        Capabilities::unlimited()
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        _seed: Seed,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let last = batches.len() - 1;
        let mut answer: Vec<BatchReadouts> = batches
            .iter()
            .enumerate()
            .map(|(index, batch)| {
                let reads = batch
                    .instructions
                    .iter()
                    .filter(|instruction| instruction.opcode == CoinGates::MZ)
                    .count();
                let (rows, width) = match self.0 {
                    Misshape::MissingRow if index == last => (batch.shots.len() - 1, reads),
                    Misshape::ExtraValue if index == last => (batch.shots.len(), reads + 1),
                    _ => (batch.shots.len(), reads),
                };
                zeros(rows, width)
            })
            .collect();
        if let Misshape::ExtraReadouts = self.0 {
            answer.push(zeros(1, 0));
        }
        Ok(answer)
    }
}

/// When a [`Panics`] part panics.
#[derive(Clone, Copy)]
pub(crate) enum PanicsWhen {
    /// While its method makes the future it returns.
    MakingItsFuture,
    /// While its future is polled.
    Polled,
}

/// A part that panics, as a driver in `drive` and as an executor in
/// `execute`.
pub(crate) struct Panics(pub(crate) PanicsWhen);

/// A part's panic while its future is polled.
fn broken<T>() -> T {
    panic!("the part broke while it was polled")
}

impl Drive for Panics {
    type Output = ();

    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    fn needs(&self) -> Needs {
        Needs::qubits(1)
    }

    fn drive(
        &mut self,
        _shots: &ShotSet,
        _seed: Seed,
        _below: &mut impl Execute,
    ) -> impl Future<Output = Result<Vec<()>, DriveError>> {
        assert!(
            matches!(self.0, PanicsWhen::Polled),
            "the part broke while making its future"
        );
        async { broken() }
    }
}

impl Execute for Panics {
    fn accepts(&self) -> &InstructionSet {
        Drive::sends(self)
    }

    fn offers(&self) -> Capabilities {
        Capabilities::unlimited()
    }

    fn execute(
        &mut self,
        _batches: &[Batch<'_>],
        _seed: Seed,
    ) -> impl Future<Output = Result<Vec<BatchReadouts>, ExecuteError>> {
        assert!(
            matches!(self.0, PanicsWhen::Polled),
            "the part broke while making its future"
        );
        async { broken() }
    }
}
