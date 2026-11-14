// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Fake parts for the behaviour tests: a part of each role, and the parts
//! the tests compose between them, on a toy whose shot state is classical
//! bits, written in QIR's `h`, `x` and `mz`.
//!
//! - Drivers: [`CoinCopy`], which branches on a readout, and [`Program`],
//!   one batch for every shot.
//! - Executors: [`Bits`], and executors that answer later ([`Later`]),
//!   from a thread ([`FromThread`]), with the wrong shape ([`Misshapen`]),
//!   or by panicking ([`Panics`], also a driver).
//! - Adapters: [`BitFlip`], which draws; [`Profiler`], which draws nothing;
//!   [`Splitter`], which sends each instruction as a batch of its own;
//!   [`Spread`], which changes the needs and the offer; [`Trap`], which
//!   fails a shot.
//!
//! Each part writes what it does to a shared [`Log`], which the test reads
//! once the parts are composed.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::poll_fn;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;

use crate::{
    Adapt, Batch, BatchReadouts, BlockType, Capabilities, Drive, DriveError, Execute, ExecuteError,
    Failure, Instruction, InstructionSet, Needs, Opcode, OpcodeMap, ReadoutFailure, ReadoutValue,
    RunRefusal, Seed, ShotConditions, ShotId, ShotSet, instruction_set,
};

instruction_set! {
    /// QIR's `h`, `x` and `mz`: what every fake part sends and accepts,
    /// unless a test gives it another set.
    pub(crate) CoinGates { H = h(qubit), X = x(qubit), MZ = mz(qubit) -> 1 outcome }
}

/// What the fake parts did, in order, shared with the test that reads it.
#[derive(Clone, Default)]
pub(crate) struct Log(Rc<RefCell<Vec<String>>>);

impl Log {
    fn push(&self, entry: impl Into<String>) {
        self.0.borrow_mut().push(entry.into());
    }

    /// Every entry, in order.
    pub(crate) fn entries(&self) -> Vec<String> {
        self.0.borrow().clone()
    }

    /// How many entries are `entry`.
    pub(crate) fn count(&self, entry: &str) -> usize {
        self.0.borrow().iter().filter(|e| *e == entry).count()
    }
}

/// A shot's random stream: `SplitMix64`, seeded from the shot's seed.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `true` with probability `per_mille` / 1000.
    fn chance(&mut self, per_mille: u64) -> bool {
        self.next() % 1000 < per_mille
    }
}

/// `instructions`, rewritten in the receiver's opcodes.
fn mapped(map: &OpcodeMap, instructions: &[Instruction]) -> Vec<Instruction> {
    instructions
        .iter()
        .map(|instruction| {
            let mut mapped = instruction.clone();
            mapped.opcode = map.map(instruction.opcode);
            mapped
        })
        .collect()
}

/// The receiver's opcodes, given by `>>`'s `bind`.
fn bound(map: Option<&OpcodeMap>) -> &OpcodeMap {
    map.expect("`>>` binds before any `execute`")
}

/// The instruction `mnemonic` of [`CoinGates`] on `qubit`.
pub(crate) fn coin_gate(mnemonic: &str, qubit: u32) -> Instruction {
    CoinGates::set()
        .instruction(mnemonic, [qubit], &[])
        .expect("an instruction of CoinGates")
}

/// Each shot's values, in the order it read them.
pub(crate) type Values = Vec<ReadoutValue>;

// ---------------------------------------------------------------- drivers

/// A coin tossed on qubit 0, copied onto qubit 1: a toss for every shot in
/// one batch, then, by the coin, a flip and a read, or a read, as two
/// batches of one `execute`. Each shot's output is its two values.
pub(crate) struct CoinCopy {
    sequences: [Vec<Instruction>; 3],
}

impl CoinCopy {
    pub(crate) fn new() -> Self {
        Self {
            sequences: [
                vec![coin_gate("h", 0), coin_gate("mz", 0)],
                vec![coin_gate("x", 1), coin_gate("mz", 1)],
                vec![coin_gate("mz", 1)],
            ],
        }
    }
}

impl Drive for CoinCopy {
    type Output = Values;

    fn sends(&self) -> &InstructionSet {
        static SET: std::sync::OnceLock<InstructionSet> = std::sync::OnceLock::new();
        SET.get_or_init(CoinGates::set)
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in self.sequences.iter_mut().flatten() {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        Needs::qubits(2)
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<Values>, DriveError> {
        let tossed = below
            .execute(
                &[Batch {
                    shots,
                    instructions: &self.sequences[0],
                }],
                seed,
            )
            .await?;
        let coin = &tossed[0];
        let mut outputs: Vec<Values> = (0..shots.len()).map(|i| coin.row(i).to_vec()).collect();
        let at = |value: ReadoutValue| -> Vec<usize> {
            (0..shots.len())
                .filter(|&i| coin.row(i) == [value])
                .collect()
        };
        let (heads, tails) = (at(ReadoutValue::One), at(ReadoutValue::Zero));
        let sets: Vec<(&[usize], ShotSet, &[Instruction])> =
            [(&heads, &self.sequences[1]), (&tails, &self.sequences[2])]
                .into_iter()
                .filter_map(|(positions, sequence)| {
                    Some((
                        positions.as_slice(),
                        shots.select(positions)?,
                        sequence.as_slice(),
                    ))
                })
                .collect();
        let batches: Vec<Batch<'_>> = sets
            .iter()
            .map(|(_, shots, instructions)| Batch {
                shots,
                instructions,
            })
            .collect();
        let read = below.execute(&batches, seed).await?;
        for ((positions, _, _), rows) in sets.iter().zip(&read) {
            for (j, &i) in positions.iter().enumerate() {
                outputs[i].extend_from_slice(rows.row(j));
            }
        }
        below.end_shots(shots);
        Ok(outputs)
    }
}

/// A driver that sends its instructions to every shot, as one batch of one
/// `execute`; each shot's output is its row and its conditions.
pub(crate) struct Program {
    set: InstructionSet,
    instructions: Vec<Instruction>,
    needs: Needs,
}

impl Program {
    /// The instructions `text`, each a mnemonic of [`CoinGates`] and its
    /// qubit, needing the qubits they name.
    pub(crate) fn new(text: &[(&str, u32)]) -> Self {
        let qubits = text.iter().map(|&(_, qubit)| qubit + 1).max().unwrap_or(0);
        Self {
            set: CoinGates::set(),
            instructions: text
                .iter()
                .map(|&(mnemonic, qubit)| coin_gate(mnemonic, qubit))
                .collect(),
            needs: Needs::qubits(qubits),
        }
    }

    /// This program, stating `needs` instead.
    pub(crate) fn needing(mut self, needs: Needs) -> Self {
        self.needs = needs;
        self
    }
}

impl Drive for Program {
    type Output = (Values, ShotConditions);

    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in &mut self.instructions {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        self.needs.clone()
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<Self::Output>, DriveError> {
        let batch = Batch {
            shots,
            instructions: &self.instructions,
        };
        let readouts = below.execute(&[batch], seed).await?;
        below.end_shots(shots);
        Ok((0..shots.len())
            .map(|i| (readouts[0].row(i).to_vec(), readouts[0].conditions(i)))
            .collect())
    }
}

// ---------------------------------------------------------------- executors

/// A toy executor on classical bits: `h` tosses a bit from the shot's
/// stream, `x` flips it, and `mz` reads it.
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
                    } else if self.loses {
                        row.push(ReadoutValue::Missing(ReadoutFailure::Loss));
                    } else {
                        row.push(if bits[q] {
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
    /// A row fewer than the batch's shots.
    MissingRow,
    /// A value more in each row than the batch's readout slots.
    ExtraValue,
}

/// An executor of [`CoinGates`] whose answer has the wrong shape: every value
/// `Zero`.
pub(crate) struct Misshapen(pub(crate) Misshape);

impl Execute for Misshapen {
    fn accepts(&self) -> &InstructionSet {
        static SET: std::sync::OnceLock<InstructionSet> = std::sync::OnceLock::new();
        SET.get_or_init(CoinGates::set)
    }

    fn offers(&self) -> Capabilities {
        Capabilities::unlimited()
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        _seed: Seed,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let mut answer: Vec<BatchReadouts> = batches
            .iter()
            .map(|batch| {
                let reads = batch
                    .instructions
                    .iter()
                    .filter(|instruction| instruction.opcode == CoinGates::MZ)
                    .count();
                let (rows, width) = match self.0 {
                    Misshape::ExtraReadouts => (batch.shots.len(), reads),
                    Misshape::MissingRow => (batch.shots.len() - 1, reads),
                    Misshape::ExtraValue => (batch.shots.len(), reads + 1),
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

/// `rows` rows of `width` values `Zero`.
fn zeros(rows: usize, width: usize) -> BatchReadouts {
    let mut readouts =
        BatchReadouts::with_capacity(rows, u32::try_from(width).expect("a small width"));
    for _ in 0..rows {
        readouts
            .push_row(&vec![ReadoutValue::Zero; width], ShotConditions::NONE)
            .expect("`width` values");
    }
    readouts
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
        static SET: std::sync::OnceLock<InstructionSet> = std::sync::OnceLock::new();
        SET.get_or_init(CoinGates::set)
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

// ---------------------------------------------------------------- adapters

/// A toy bit-flip noise: after each `x`, another `x` with probability
/// `per_mille` / 1000, drawn from its own stream per shot. Each shot goes
/// below as a batch of its own, since each may flip differently.
pub(crate) struct BitFlip {
    per_mille: u64,
    below: Option<OpcodeMap>,
    streams: BTreeMap<ShotId, Stream>,
    log: Log,
}

impl BitFlip {
    pub(crate) fn new(per_mille: u64, log: &Log) -> Self {
        Self {
            per_mille,
            below: None,
            streams: BTreeMap::new(),
            log: log.clone(),
        }
    }
}

impl Adapt for BitFlip {
    fn accepts(&self) -> &InstructionSet {
        static SET: std::sync::OnceLock<InstructionSet> = std::sync::OnceLock::new();
        SET.get_or_init(CoinGates::set)
    }

    fn sends(&self) -> &InstructionSet {
        self.accepts()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let map = bound(self.below.as_ref());
        let own = seed.derive("bit_flip");
        let mut sent: Vec<(ShotSet, Vec<Instruction>)> = Vec::new();
        for batch in batches {
            for (j, id) in batch.shots.iter().enumerate() {
                let stream = self
                    .streams
                    .entry(id)
                    .or_insert_with(|| Stream(own.shot(id)));
                let mut instructions = Vec::with_capacity(batch.instructions.len() + 1);
                for instruction in batch.instructions {
                    let flips = instruction.opcode == CoinGates::X && stream.chance(self.per_mille);
                    let mut mapped = instruction.clone();
                    mapped.opcode = map.map(instruction.opcode);
                    instructions.push(mapped.clone());
                    if flips {
                        instructions.push(mapped);
                    }
                }
                sent.push((
                    batch.shots.select(&[j]).expect("a position of the batch"),
                    instructions,
                ));
            }
        }
        let one_each: Vec<Batch<'_>> = sent
            .iter()
            .map(|(shots, instructions)| Batch {
                shots,
                instructions,
            })
            .collect();
        let answer = below.execute(&one_each, seed).await?;
        let mut rows = answer.iter();
        let mut readouts = Vec::with_capacity(batches.len());
        for batch in batches {
            let mine: Vec<&BatchReadouts> = rows.by_ref().take(batch.shots.len()).collect();
            let mut joined = BatchReadouts::with_capacity(mine.len(), mine[0].readouts());
            for one in mine {
                joined
                    .push_row(one.row(0), one.conditions(0))
                    .expect("one shot's readouts");
            }
            readouts.push(joined);
        }
        Ok(readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        for id in shots.iter() {
            self.streams.remove(&id);
        }
        self.log
            .push(format!("BitFlip end_shots, holding {}", self.streams.len()));
    }
}

/// An adapter that counts the instructions passing through it and changes
/// nothing: it draws nothing, and sends each batch as it came.
pub(crate) struct Profiler {
    set: InstructionSet,
    below: Option<OpcodeMap>,
    instructions: Rc<Cell<usize>>,
    log: Log,
}

impl Profiler {
    /// A profiler of [`CoinGates`], counting into `instructions`.
    pub(crate) fn new(instructions: &Rc<Cell<usize>>, log: &Log) -> Self {
        Self::on(CoinGates::set(), instructions, log)
    }

    /// A profiler accepting and sending `set`.
    pub(crate) fn on(set: InstructionSet, instructions: &Rc<Cell<usize>>, log: &Log) -> Self {
        Self {
            set,
            below: None,
            instructions: Rc::clone(instructions),
            log: log.clone(),
        }
    }
}

impl Adapt for Profiler {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn ready(&mut self) -> Result<(), RunRefusal> {
        self.log.push("Profiler ready");
        Ok(())
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let map = bound(self.below.as_ref());
        let sent: Vec<Vec<Instruction>> = batches
            .iter()
            .map(|batch| mapped(map, batch.instructions))
            .collect();
        let count: usize = sent.iter().map(Vec::len).sum();
        self.instructions.set(self.instructions.get() + count);
        let below_batches: Vec<Batch<'_>> = batches
            .iter()
            .zip(&sent)
            .map(|(batch, instructions)| Batch {
                shots: batch.shots,
                instructions,
            })
            .collect();
        below.execute(&below_batches, seed).await
    }
}

/// An adapter that sends each instruction of a batch as a batch of its
/// own, one `execute` each, as a stepping debugger does, and joins their
/// readouts.
#[derive(Default)]
pub(crate) struct Splitter {
    below: Option<OpcodeMap>,
}

impl Adapt for Splitter {
    fn accepts(&self) -> &InstructionSet {
        static SET: std::sync::OnceLock<InstructionSet> = std::sync::OnceLock::new();
        SET.get_or_init(CoinGates::set)
    }

    fn sends(&self) -> &InstructionSet {
        self.accepts()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let map = bound(self.below.as_ref());
        let mut answer = Vec::with_capacity(batches.len());
        for batch in batches {
            let instructions = mapped(map, batch.instructions);
            let mut parts = Vec::with_capacity(instructions.len());
            for instruction in &instructions {
                let step = Batch {
                    shots: batch.shots,
                    instructions: std::slice::from_ref(instruction),
                };
                parts.extend(below.execute(&[step], seed).await?);
            }
            answer.push(if parts.is_empty() {
                zeros(batch.shots.len(), 0)
            } else {
                BatchReadouts::concat_readouts(&parts)
            });
        }
        Ok(answer)
    }
}

/// A toy encoding: each qubit above is `k` qubits below, of which it uses
/// the first. It needs `k` times the qubits below, and offers a `k`-th of
/// them above.
pub(crate) struct Spread {
    k: u32,
    below: Option<OpcodeMap>,
    log: Log,
}

impl Spread {
    pub(crate) fn new(k: u32, log: &Log) -> Self {
        Self {
            k,
            below: None,
            log: log.clone(),
        }
    }
}

impl Adapt for Spread {
    fn accepts(&self) -> &InstructionSet {
        static SET: std::sync::OnceLock<InstructionSet> = std::sync::OnceLock::new();
        SET.get_or_init(CoinGates::set)
    }

    fn sends(&self) -> &InstructionSet {
        self.accepts()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    fn needs_below(&self, above: Needs) -> Needs {
        let qubits = above.blocks_of(&BlockType::qubit()) * self.k;
        above.blocks(BlockType::qubit(), qubits)
    }

    fn offers_above(&self, below: Capabilities) -> Capabilities {
        let mut above = below;
        if let Some(Some(qubits)) = above.max_blocks.get_mut(&BlockType::qubit()) {
            *qubits /= self.k;
        }
        above
    }

    fn provision(&mut self, above: Needs) {
        let qubits = above.blocks_of(&BlockType::qubit());
        self.log
            .push(format!("Spread provisioned for {qubits} qubits"));
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let map = bound(self.below.as_ref());
        let sent: Vec<Vec<Instruction>> = batches
            .iter()
            .map(|batch| {
                let mut instructions = mapped(map, batch.instructions);
                for instruction in &mut instructions {
                    instruction.blocks[0] *= self.k;
                }
                instructions
            })
            .collect();
        let below_batches: Vec<Batch<'_>> = batches
            .iter()
            .zip(&sent)
            .map(|(batch, instructions)| Batch {
                shots: batch.shots,
                instructions,
            })
            .collect();
        below.execute(&below_batches, seed).await
    }
}

/// An adapter that fails a batch holding `shot`, as a program's trap, and
/// passes the others through.
pub(crate) struct Trap {
    shot: ShotId,
    below: Option<OpcodeMap>,
}

impl Trap {
    pub(crate) fn on(shot: u32) -> Self {
        Self {
            shot: ShotId::new(shot),
            below: None,
        }
    }
}

impl Adapt for Trap {
    fn accepts(&self) -> &InstructionSet {
        static SET: std::sync::OnceLock<InstructionSet> = std::sync::OnceLock::new();
        SET.get_or_init(CoinGates::set)
    }

    fn sends(&self) -> &InstructionSet {
        self.accepts()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        if batches
            .iter()
            .any(|batch| batch.shots.iter().any(|id| id == self.shot))
        {
            return Err(ExecuteError::Failed {
                shot: self.shot,
                failure: Failure::Part("the program trapped".into()),
            });
        }
        let map = bound(self.below.as_ref());
        let sent: Vec<Vec<Instruction>> = batches
            .iter()
            .map(|batch| mapped(map, batch.instructions))
            .collect();
        let below_batches: Vec<Batch<'_>> = batches
            .iter()
            .zip(&sent)
            .map(|(batch, instructions)| Batch {
                shots: batch.shots,
                instructions,
            })
            .collect();
        below.execute(&below_batches, seed).await
    }
}

// ---------------------------------------------------------------- futures

/// Gives the other tasks a turn: pending once, and woken at once.
#[derive(Default)]
pub(crate) struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        context.waker().wake_by_ref();
        Poll::Pending
    }
}
