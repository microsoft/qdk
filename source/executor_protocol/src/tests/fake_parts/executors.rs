// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake executors.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::future::poll_fn;
use std::rc::Rc;
use std::slice;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Poll, Waker};
use std::thread;
use std::time::Duration;

use super::{CoinGates, Log, Stream, YieldNow};
use crate::{
    Batch, BatchReadouts, BlockType, Execute, ExecuteError, ExecutorSetup, Failure, FinalRow,
    Instruction, InstructionSet, InstructionStream, Needs, NotReady, Offer, Opcode, ReadoutFailure,
    ReadoutValue, Seed, ShotConditions, ShotId, ShotSet,
};

/// A toy executor on classical bits: `h` tosses a bit from the shot's
/// stream, `x` flips it, and `mz` reads it; any other instruction of its
/// set does nothing. It gives each row as soon as its shot has run
/// (`execute_each`), and runs a stream's instructions as they come, on
/// every shot, without cutting it (`execute_stream`), as a simulator does.
pub(crate) struct Bits {
    set: InstructionSet,
    opcodes: [Option<Opcode>; 3],
    offer: Offer,
    qubits: usize,
    shots: BTreeMap<ShotId, (Vec<bool>, Stream)>,
    busy: Option<&'static str>,
    /// Whether a shot loses every value it reads, and is found suspect.
    loses: fn(ShotId) -> bool,
    /// Whether each `execute` fails once it has run its shots.
    fails: bool,
    /// The set `accepts` answers once ready, if it changes then.
    set_once_ready: Option<InstructionSet>,
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
            offer: Offer::unlimited().max_qubits(64).without_readout_parity(),
            qubits: 0,
            shots: BTreeMap::new(),
            busy: None,
            loses: |_| false,
            fails: false,
            set_once_ready: None,
            log: log.clone(),
        }
    }

    /// These bits, offering `offer`.
    pub(crate) fn offering(mut self, offer: Offer) -> Self {
        self.offer = offer;
        self
    }

    /// These bits, refusing every run in `ready`, for `why`.
    pub(crate) fn busy(mut self, why: &'static str) -> Self {
        self.busy = Some(why);
        self
    }

    /// These bits, answering `set` to `accepts` once ready, while still
    /// running their own `h`, `x` and `mz`: a part that breaks the rule
    /// that its set does not change once composed.
    pub(crate) fn accepting_once_ready(mut self, set: InstructionSet) -> Self {
        self.set_once_ready = Some(set);
        self
    }

    /// These bits, losing every value they read, and finding every shot
    /// suspect.
    pub(crate) fn losing(mut self) -> Self {
        self.loses = |_| true;
        self
    }

    /// These bits, losing every value they read on the odd shots, and
    /// finding those shots suspect: the rows of a batch differ in their
    /// conditions.
    pub(crate) fn losing_odd_shots(mut self) -> Self {
        self.loses = |id| id.get() % 2 == 1;
        self
    }

    /// These bits, failing each `execute` once they have run its shots,
    /// naming its first shot, as a device whose link drops before it
    /// answers.
    pub(crate) fn failing(mut self) -> Self {
        self.fails = true;
        self
    }

    /// The answer of an `execute` on `shots` that has run.
    fn answer(&self, shots: &ShotSet) -> Result<(), ExecuteError> {
        if !self.fails {
            return Ok(());
        }
        Err(ExecuteError::Failed {
            shot: shots.iter().next().expect("a shot set is never empty"),
            failure: Failure::Part("the bits' link dropped".into()),
        })
    }
}

impl ExecutorSetup for Bits {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn offers(&self) -> Offer {
        self.offer.clone()
    }
}

impl Execute for Bits {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        let qubits = needs.blocks_of(&BlockType::qubit());
        self.log.push(format!("Bits ready for {qubits} qubits"));
        self.qubits = qubits as usize;
        if let Some(set) = self.set_once_ready.take() {
            self.set = set;
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
        self.run(batches, seed, readouts, |_, _, _| {});
        self.answer(batches[0].shots)
    }

    /// Gives each shot's row as soon as the shot has run, logging
    /// `Bits row <batch> <position>` before it gives it.
    async fn execute_each(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
        each: &mut impl FnMut(FinalRow<'_>),
    ) -> Result<(), ExecuteError> {
        self.log.push("Bits execute_each");
        let log = self.log.clone();
        self.run(batches, seed, readouts, |batch, position, readouts| {
            log.push(format!("Bits row {batch} {position}"));
            each(FinalRow::of(batch, position, readouts));
        });
        self.answer(batches[0].shots)
    }

    /// Runs each instruction on every shot as it is pulled, keeping each
    /// one's readouts, joined at the end; logs `Bits execute_stream`.
    async fn execute_stream(
        &mut self,
        stream: InstructionStream<'_, impl Iterator<Item = Instruction>>,
        seed: Seed,
        readouts: &mut BatchReadouts,
    ) -> Result<(), ExecuteError> {
        self.log.push("Bits execute_stream");
        let shots = stream.shots;
        let mut read = Vec::new();
        for instruction in stream.instructions {
            let its_slots = self.set.readout_slots(instruction.opcode).unwrap_or(0);
            let mut its = BatchReadouts::new(shots.len(), its_slots);
            let batch = Batch::new(shots, slice::from_ref(&instruction), its_slots);
            self.run(
                slice::from_ref(&batch),
                seed,
                slice::from_mut(&mut its),
                |_, _, _| {},
            );
            read.push(its);
        }
        let all_slots = read.iter().map(BatchReadouts::readout_slots).sum();
        *readouts = BatchReadouts::new(shots.len(), all_slots);
        readouts.concat_from(&read);
        self.answer(shots)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        for id in shots.iter() {
            self.shots.remove(&id);
        }
        self.log
            .push(format!("Bits end_shots, holding {}", self.shots.len()));
    }
}

impl Bits {
    /// Runs each shot of `batches`, filling its row of `readouts`, then
    /// calls `ran` with the shot's batch, its position and its batch's
    /// readouts.
    fn run(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
        mut ran: impl FnMut(usize, usize, &BatchReadouts),
    ) {
        let ([h, x, mz], loses) = (self.opcodes, self.loses);
        for (index, (batch, batch_readouts)) in batches.iter().zip(readouts).enumerate() {
            for (position, id) in batch.shots.iter().enumerate() {
                let (qubits, lost) = (self.qubits, loses(id));
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
                        *row.next().expect("a slot per `mz`") = if lost {
                            ReadoutValue::Missing(ReadoutFailure::Loss)
                        } else if bits[q] {
                            ReadoutValue::One
                        } else {
                            ReadoutValue::Zero
                        };
                    }
                }
                if lost {
                    batch_readouts.set_conditions(position, ShotConditions::REJECTED);
                }
                ran(index, position, batch_readouts);
            }
        }
    }
}

/// An executor whose answer comes on a later poll: pending once, woken at
/// once, as a device's that answers later. It forwards neither
/// `execute_each` nor `execute_stream`: through it, an executor that gives
/// its rows as they are final gives them at the end, and a stream is cut
/// into batches, by the defaults.
pub(crate) struct Later<E>(pub(crate) E);

impl<E: ExecutorSetup> ExecutorSetup for Later<E> {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn offers(&self) -> Offer {
        self.0.offers()
    }
}

impl<E: Execute> Execute for Later<E> {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        self.0.ready(needs).await
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
/// from that thread. It forwards neither `execute_each` nor
/// `execute_stream`, as [`Later`] does not.
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

impl<E: ExecutorSetup> ExecutorSetup for FromThread<E> {
    fn accepts(&self) -> &InstructionSet {
        self.inner.accepts()
    }

    fn offers(&self) -> Offer {
        self.inner.offers()
    }
}

impl<E: Execute> Execute for FromThread<E> {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        self.inner.ready(needs).await
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

/// An executor whose rows come one at a time from a worker thread, as a
/// device's or a GPU shard's do: the inner executor writes them, then the
/// worker hands each over and wakes the run, and waits until it was given
/// to `each` before it hands over the next. So each row is given on a poll
/// of its own, while the call is still pending; it logs
/// `RowsFromThread pending` on each pending poll, and
/// `RowsFromThread answers` once every row was given. A worker that stops
/// before handing over every row fails the call.
pub(crate) struct RowsFromThread<E> {
    inner: E,
    stops_after: Option<usize>,
    log: Log,
}

impl<E> RowsFromThread<E> {
    pub(crate) fn new(inner: E, log: &Log) -> Self {
        Self {
            inner,
            stops_after: None,
            log: log.clone(),
        }
    }

    /// This executor, whose worker stops after handing over `rows` rows.
    pub(crate) fn stopping_after(mut self, rows: usize) -> Self {
        self.stops_after = Some(rows);
        self
    }
}

/// The rows a worker handed over, by batch and position, until they are
/// given; whether it stopped; and the waker to wake when it hands one
/// over or stops.
#[derive(Default)]
struct RowQueue {
    rows: Vec<(usize, usize)>,
    stopped: bool,
    waker: Option<Waker>,
}

/// The worker's end of a [`RowQueue`]: dropped, as the worker stops, it
/// marks the queue stopped and wakes the run, so that the call answers
/// rather than waits forever.
struct RowSender(Arc<Mutex<RowQueue>>);

impl RowSender {
    fn send(&self, row: (usize, usize)) {
        let mut queue = self.0.lock().expect("no panic while held");
        queue.rows.push(row);
        if let Some(waker) = queue.waker.take() {
            waker.wake();
        }
    }
}

impl Drop for RowSender {
    fn drop(&mut self) {
        if let Ok(mut queue) = self.0.lock() {
            queue.stopped = true;
            if let Some(waker) = queue.waker.take() {
                waker.wake();
            }
        }
    }
}

impl<E: ExecutorSetup> ExecutorSetup for RowsFromThread<E> {
    fn accepts(&self) -> &InstructionSet {
        self.inner.accepts()
    }

    fn offers(&self) -> Offer {
        self.inner.offers()
    }
}

impl<E: Execute> Execute for RowsFromThread<E> {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        self.inner.ready(needs).await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.execute_each(batches, seed, readouts, &mut |_| {})
            .await
    }

    async fn execute_each(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
        each: &mut impl FnMut(FinalRow<'_>),
    ) -> Result<(), ExecuteError> {
        self.inner.execute(batches, seed, readouts).await?;
        let rows: Vec<(usize, usize)> = readouts
            .iter()
            .enumerate()
            .flat_map(|(batch, rows)| (0..rows.shots()).map(move |position| (batch, position)))
            .collect();
        let every_row = rows.len();
        let handed_over = self.stops_after.unwrap_or(every_row);
        let queue = Arc::new(Mutex::new(RowQueue::default()));
        let (acknowledge, acknowledged) = mpsc::channel::<()>();
        let worker = {
            let sender = RowSender(Arc::clone(&queue));
            thread::spawn(move || {
                for row in rows.into_iter().take(handed_over) {
                    sender.send(row);
                    if acknowledged.recv().is_err() {
                        return;
                    }
                }
            })
        };
        let mut given = 0;
        poll_fn(|context| {
            let (rows, stopped) = {
                let mut queue = queue.lock().expect("no panic while held");
                queue.waker = Some(context.waker().clone());
                (std::mem::take(&mut queue.rows), queue.stopped)
            };
            for (batch, position) in rows {
                each(FinalRow::of(batch, position, &readouts[batch]));
                given += 1;
                // A worker that stopped no longer listens.
                let _ = acknowledge.send(());
            }
            if stopped {
                Poll::Ready(())
            } else {
                self.log.push("RowsFromThread pending");
                Poll::Pending
            }
        })
        .await;
        worker.join().expect("the worker ends");
        if given < every_row {
            return Err(ExecuteError::Failed {
                shot: batches[0].shots.iter().next().expect("a batch has shots"),
                failure: Failure::Part("the worker stopped before giving every row".into()),
            });
        }
        self.log.push("RowsFromThread answers");
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.inner.end_shots(shots);
    }
}

/// An executor that logs, at each `execute`, how many instructions its
/// batch holds and how many its stream's sender had produced by then
/// (`execute of <n>, <p> produced`), as the counter `produced` says: what
/// shows how a default `execute_stream` cuts a stream, and that it pulls
/// no instruction beyond the piece it runs. It forwards no
/// `execute_stream`, so the default cuts each stream sent to it.
pub(crate) struct SeesPieces<E> {
    inner: E,
    produced: Rc<Cell<usize>>,
    log: Log,
}

impl<E> SeesPieces<E> {
    pub(crate) fn new(inner: E, produced: &Rc<Cell<usize>>, log: &Log) -> Self {
        Self {
            inner,
            produced: Rc::clone(produced),
            log: log.clone(),
        }
    }
}

impl<E: ExecutorSetup> ExecutorSetup for SeesPieces<E> {
    fn accepts(&self) -> &InstructionSet {
        self.inner.accepts()
    }

    fn offers(&self) -> Offer {
        self.inner.offers()
    }
}

impl<E: Execute> Execute for SeesPieces<E> {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        self.inner.ready(needs).await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let instructions: usize = batches.iter().map(|batch| batch.instructions.len()).sum();
        self.log.push(format!(
            "execute of {instructions}, {} produced",
            self.produced.get()
        ));
        self.inner.execute(batches, seed, readouts).await
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.inner.end_shots(shots);
    }
}
