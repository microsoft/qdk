// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake adapters.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::future::poll_fn;
use std::pin::pin;
use std::rc::Rc;
use std::sync::OnceLock;
use std::task::Poll;

use super::{
    CoinGates, Log, Stream, bound, coin_capabilities, coin_gate, coin_gates, forward, mapped,
    qubits,
};
use crate::prelude::*;
use crate::{
    Batch, BatchReadouts, BlockType, Capabilities, ExecuteError, Failure, Features, Instruction,
    InstructionSet, NotReady, OpcodeMap, Readouts, Resource, Resources, Seed, ShotId, ShotSet,
};

/// A toy bit-flip noise: after each `x`, another `x` with probability
/// `probability`, drawn from its own stream per shot. Each shot goes
/// below as a batch of its own, since each may flip differently.
pub(crate) struct BitFlip {
    probability: f64,
    below: Option<OpcodeMap>,
    streams: BTreeMap<ShotId, Stream>,
    log: Log,
}

impl BitFlip {
    pub(crate) fn new(probability: f64, log: &Log) -> Self {
        Self {
            probability,
            below: None,
            streams: BTreeMap::new(),
            log: log.clone(),
        }
    }
}

impl Adapter for BitFlip {
    fn provides(&self) -> &Capabilities {
        coin_capabilities()
    }

    fn requires(&self) -> &Capabilities {
        coin_capabilities()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let map = bound(self.below.as_ref());
        let own = seed.derive("bit_flip");
        let mut sent: Vec<(ShotSet, Vec<Instruction>, u32, bool)> = Vec::new();
        for batch in batches {
            for (j, id) in batch.shots.iter().enumerate() {
                let stream = self
                    .streams
                    .entry(id)
                    .or_insert_with(|| Stream(own.shot(id)));
                let mut instructions = Vec::with_capacity(batch.instructions.len() + 1);
                for instruction in batch.instructions {
                    let flips =
                        instruction.opcode == CoinGates::X && stream.chance(self.probability);
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
                    batch.readout_slots(),
                    batch.is_checked(),
                ));
            }
        }
        let one_each: Vec<Batch<'_>> = sent
            .iter()
            .map(|(shots, instructions, slots, checked)| {
                Batch::new(shots, instructions, *slots).checked_if(*checked)
            })
            .collect();
        let mut kept = Readouts::new();
        let readouts_below = kept.readouts_for(&one_each);
        below.execute(&one_each, seed, readouts_below).await?;
        let mut ones = readouts_below.iter();
        for batch_readouts in readouts {
            for position in 0..batch_readouts.shots() {
                let one = ones.next().expect("one `BatchReadouts` below per shot");
                batch_readouts.row_mut(position).copy_from_slice(one.row(0));
                batch_readouts.set_conditions(position, one.conditions(0));
            }
        }
        Ok(())
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
    capabilities: Capabilities,
    below: Option<OpcodeMap>,
    instructions: Rc<Cell<usize>>,
    busy: Option<&'static str>,
    log: Log,
}

impl Profiler {
    /// A profiler of [`CoinGates`], counting into `instructions`.
    pub(crate) fn new(instructions: &Rc<Cell<usize>>, log: &Log) -> Self {
        Self::on(CoinGates::set(), instructions, log)
    }

    /// A profiler requiring and providing `set`, and no resource or
    /// feature, which it forwards.
    pub(crate) fn on(set: InstructionSet, instructions: &Rc<Cell<usize>>, log: &Log) -> Self {
        Self {
            capabilities: Capabilities::new(set),
            below: None,
            instructions: Rc::clone(instructions),
            busy: None,
            log: log.clone(),
        }
    }

    /// This profiler, refusing every run in `ready`, for `why`.
    pub(crate) fn busy(mut self, why: &'static str) -> Self {
        self.busy = Some(why);
        self
    }
}

impl Adapter for Profiler {
    fn provides(&self) -> &Capabilities {
        &self.capabilities
    }

    fn requires(&self) -> &Capabilities {
        &self.capabilities
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn ready(&mut self, _required: &Capabilities) -> Result<(), NotReady> {
        self.log.push("Profiler ready");
        match self.busy {
            Some(why) => Err(NotReady::new(why)),
            None => Ok(()),
        }
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let count: usize = batches.iter().map(|batch| batch.instructions.len()).sum();
        self.instructions.set(self.instructions.get() + count);
        let map = bound(self.below.as_ref());
        forward(batches, seed, below, readouts, |batch| {
            mapped(map, batch.instructions)
        })
        .await
    }
}

/// An adapter that passes each batch down unchanged, logging first the
/// name of the part below as `below: <name>`, as an adapter naming the
/// part below in its own errors would read it, then the name of the set
/// the part below provides as `below provides: <set>`, and the most
/// instructions a batch it provides for as `below takes <most> a batch`
/// (`None` when it states none). It states no resource, so it forwards
/// what the part below provides.
pub(crate) struct NamesBelow {
    below: Option<OpcodeMap>,
    log: Log,
}

impl NamesBelow {
    pub(crate) fn new(log: &Log) -> Self {
        Self {
            below: None,
            log: log.clone(),
        }
    }
}

impl Adapter for NamesBelow {
    fn provides(&self) -> &Capabilities {
        coin_capabilities()
    }

    fn requires(&self) -> &Capabilities {
        coin_capabilities()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.log.push(format!("below: {}", below.name()));
        self.log.push(format!(
            "below provides: {}",
            below.provides().instructions().name()
        ));
        let most = (below.provides().resources()).get(&Resource::InstructionsPerBatch);
        self.log.push(format!("below takes {most:?} a batch"));
        let map = bound(self.below.as_ref());
        forward(batches, seed, below, readouts, |batch| {
            mapped(map, batch.instructions)
        })
        .await
    }
}

/// An adapter that sends each instruction of a batch as a batch of its
/// own, one `execute` each, as a stepping debugger does, and joins their
/// readouts into those of the batch it got; so it requires batches of one
/// instruction below, and provides any count above.
#[derive(Default)]
pub(crate) struct Splitter {
    below: Option<OpcodeMap>,
}

impl Adapter for Splitter {
    fn provides(&self) -> &Capabilities {
        coin_capabilities()
    }

    fn requires(&self) -> &Capabilities {
        static ONE_EACH: OnceLock<Capabilities> = OnceLock::new();
        ONE_EACH.get_or_init(|| {
            Capabilities::new(CoinGates::set())
                .with_resources(Resources::new().with_instructions_per_batch(1))
        })
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let map = bound(self.below.as_ref());
        let mut parts: Vec<BatchReadouts> = Vec::new();
        for (batch, batch_readouts) in batches.iter().zip(readouts) {
            let instructions = mapped(map, batch.instructions);
            parts.clear();
            for (instruction, sent) in batch.instructions.iter().zip(&instructions) {
                let slots = coin_gates()
                    .readout_slots(instruction.opcode)
                    .expect("an instruction of the set it provides");
                let step = [Batch::new(batch.shots, std::slice::from_ref(sent), slots)
                    .checked_if(batch.is_checked())];
                parts.push(BatchReadouts::new(0, 0));
                let part = parts.last_mut().expect("the part just pushed");
                below
                    .execute(&step, seed, std::slice::from_mut(part))
                    .await?;
            }
            batch_readouts.concat_from(&parts);
        }
        Ok(())
    }
}

/// A toy encoding: each qubit above is `k` qubits below, of which it uses
/// the first. Made by [`new`](Self::new), it states no qubits, which it
/// forwards; [`built_for`](Self::built_for) states them.
pub(crate) struct Spread {
    k: u32,
    requires: Capabilities,
    provides: Capabilities,
    below: Option<OpcodeMap>,
    log: Log,
}

impl Spread {
    pub(crate) fn new(k: u32, log: &Log) -> Self {
        Self {
            k,
            requires: coin_capabilities().clone(),
            provides: coin_capabilities().clone(),
            below: None,
            log: log.clone(),
        }
    }

    /// This encoding, built for `n` qubits above: it requires `k * n`
    /// qubits below, and provides `n`.
    pub(crate) fn built_for(self, n: u32) -> Self {
        let k = self.k;
        self.stating(qubits(k * n), qubits(n))
    }

    /// This encoding, requiring `requires` and providing `provides`, with
    /// no feature stated.
    fn stating(self, requires: Resources, provides: Resources) -> Self {
        Self {
            requires: Capabilities::new(CoinGates::set()).with_resources(requires),
            provides: Capabilities::new(CoinGates::set()).with_resources(provides),
            ..self
        }
    }

    /// This encoding, with `requires` and `provides` in place of its own.
    fn with_capabilities(self, requires: Capabilities, provides: Capabilities) -> Self {
        Self {
            requires,
            provides,
            ..self
        }
    }
}

impl Adapter for Spread {
    fn provides(&self) -> &Capabilities {
        &self.provides
    }

    fn requires(&self) -> &Capabilities {
        &self.requires
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn ready(&mut self, required: &Capabilities) -> Result<(), NotReady> {
        let qubits = required
            .resources()
            .blocks(&BlockType::qubit())
            .unwrap_or(0);
        self.log.push(format!("Spread ready for {qubits} qubits"));
        Ok(())
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let map = bound(self.below.as_ref());
        forward(batches, seed, below, readouts, |batch| {
            let mut instructions = mapped(map, batch.instructions);
            for instruction in &mut instructions {
                instruction.blocks[0] *= self.k;
            }
            instructions
        })
        .await
    }
}

/// A toy layout drawing two block types from one pool: each `carbon` block
/// above takes two qubits below, from the qubits that also hold the
/// qubits above. Built for `qubits` qubits and `carbons` carbon blocks
/// above, it requires `qubits + 2 * carbons` qubits below, and no carbon
/// block, and provides what it was built for. At run time it passes
/// instructions through, as a [`Spread`] of 1.
pub(crate) struct Pool(Spread);

impl Pool {
    pub(crate) fn new(qubits: u32, carbons: u32, log: &Log) -> Self {
        Self(Spread::new(1, log).stating(
            super::qubits(qubits + 2 * carbons).with_blocks(Self::carbon(), 0),
            super::qubits(qubits).with_blocks(Self::carbon(), carbons),
        ))
    }

    /// The block type it lowers onto qubits.
    pub(crate) fn carbon() -> BlockType {
        BlockType::new("carbon")
    }
}

impl Adapter for Pool {
    fn provides(&self) -> &Capabilities {
        self.0.provides()
    }

    fn requires(&self) -> &Capabilities {
        self.0.requires()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.0.adapt(batches, seed, below, readouts).await
    }
}

/// An adapter that states capabilities of its own, one toy per
/// constructor, and forwards what it does not state. At run time it
/// passes instructions through, as a [`Spread`] of 1.
pub(crate) struct Stating(Spread);

impl Stating {
    /// A toy requiring `requires` and providing `provides`, with the
    /// instructions of [`CoinGates`].
    fn new(log: &Log, requires: Capabilities, provides: Capabilities) -> Self {
        Self(Spread::new(1, log).with_capabilities(requires, provides))
    }

    /// A toy providing `resources` and `features`, requiring nothing of
    /// its own.
    fn providing(log: &Log, resources: Resources, features: Features) -> Self {
        Self::new(
            log,
            coin_capabilities().clone(),
            Capabilities::new(CoinGates::set())
                .with_resources(resources)
                .with_features(features),
        )
    }

    /// A toy part reserving one qubit below as an ancilla, built for `n`
    /// qubits above: it requires `n + 1` qubits below, and provides `n`.
    pub(crate) fn reserving_an_ancilla(n: u32, log: &Log) -> Self {
        Self(Spread::new(1, log).stating(qubits(n + 1), qubits(n)))
    }

    /// A toy job queue that takes at most `most` shots a run.
    pub(crate) fn queue(most: u32, log: &Log) -> Self {
        Self::providing(log, Resources::new().with_shots(most), Features::new())
    }

    /// A declaration fake of a job splitter: it requires runs of up to
    /// `below` shots and provides runs of up to `above`, so it requires the
    /// shots again, serving them itself, but it passes each run through
    /// whole, splitting none.
    pub(crate) fn splitting_runs(below: u32, above: u32, log: &Log) -> Self {
        let shots = |most| {
            Capabilities::new(CoinGates::set()).with_resources(Resources::new().with_shots(most))
        };
        Self::new(log, shots(below), shots(above))
    }

    /// A toy sampler that says it draws its shots' readouts together with
    /// its run's seed, from counts it takes below: it requires no seeded
    /// runs below, and provides them above, with no replay.
    pub(crate) fn seeding_itself(log: &Log) -> Self {
        let stating = |features| Capabilities::new(CoinGates::set()).with_features(features);
        Self::new(
            log,
            stating(Features::new().with_seeded_runs(false)),
            stating(Features::new().with_seeded_runs(true).with_replay(false)),
        )
    }

    /// A toy replayer that runs a shot again by sending it below again,
    /// with its run's seed: it requires no replay below, and provides
    /// replay above, stating seeded runs, while it forwards whether the
    /// parts below serve seeded runs, as it does not require them.
    pub(crate) fn replaying(log: &Log) -> Self {
        let stating = |features| Capabilities::new(CoinGates::set()).with_features(features);
        Self::new(
            log,
            stating(Features::new().with_replay(false)),
            stating(Features::new().with_replay(true)),
        )
    }

    /// A toy decoder that provides `qubits` qubits, as it states them
    /// afresh; it requires no qubits of its own, so that the qubits below
    /// still bound what it provides.
    pub(crate) fn restating(qubits: u32, log: &Log) -> Self {
        Self::providing(log, super::qubits(qubits), Features::new())
    }

    /// The decoder [`Stating::restating`] makes, providing at most
    /// `most_shots` shots a run.
    pub(crate) fn restating_most_shots(qubits: u32, most_shots: u32, log: &Log) -> Self {
        Self::providing(
            log,
            super::qubits(qubits).with_shots(most_shots),
            Features::new(),
        )
    }

    /// A toy link to a device whose measurements cannot be seeded.
    pub(crate) fn without_seeded_runs(log: &Log) -> Self {
        Self::providing(
            log,
            Resources::new(),
            Features::new().with_seeded_runs(false),
        )
    }

    /// A toy lowering that cannot carry readout parities: it sends none
    /// below, and resolves none above.
    pub(crate) fn lowering_without_parity(log: &Log) -> Self {
        let no_parity = || {
            Capabilities::new(CoinGates::set())
                .with_features(Features::new().with_readout_parity(false))
        };
        Self::new(log, no_parity(), no_parity())
    }
}

impl Adapter for Stating {
    fn provides(&self) -> &Capabilities {
        self.0.provides()
    }

    fn requires(&self) -> &Capabilities {
        self.0.requires()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.0.adapt(batches, seed, below, readouts).await
    }
}

/// An adapter that stops waiting: it polls its answer from below once,
/// drops that future unfinished, then asks below again, into the same
/// readouts, and waits. At run time it passes instructions through, as a
/// [`Spread`] of 1.
pub(crate) struct Impatient(Spread);

impl Impatient {
    pub(crate) fn new(log: &Log) -> Self {
        Self(Spread::new(1, log))
    }
}

impl Adapter for Impatient {
    fn provides(&self) -> &Capabilities {
        self.0.provides()
    }

    fn requires(&self) -> &Capabilities {
        self.0.requires()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        {
            let mut first = pin!(self.0.adapt(batches, seed, below, &mut *readouts));
            poll_fn(|context| {
                let _ = first.as_mut().poll(context);
                Poll::Ready(())
            })
            .await;
        }
        self.0.adapt(batches, seed, below, readouts).await
    }
}

/// An adapter that fails a batch holding `shot`, as a program's trap, and
/// passes the others through; logging, when told to, that it ends shots.
pub(crate) struct Trap {
    shot: ShotId,
    below: Option<OpcodeMap>,
    log: Option<Log>,
}

impl Trap {
    pub(crate) fn on(shot: u32) -> Self {
        Self {
            shot: ShotId::new(shot),
            below: None,
            log: None,
        }
    }

    /// The same trap, logging `"Trap end_shots"` to `log` in each
    /// `end_shots`.
    pub(crate) fn logging(self, log: &Log) -> Self {
        Self {
            log: Some(log.clone()),
            ..self
        }
    }
}

impl Adapter for Trap {
    fn provides(&self) -> &Capabilities {
        coin_capabilities()
    }

    fn requires(&self) -> &Capabilities {
        coin_capabilities()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
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
        forward(batches, seed, below, readouts, |batch| {
            mapped(map, batch.instructions)
        })
        .await
    }

    fn end_shots(&mut self, _shots: &ShotSet) {
        if let Some(log) = &self.log {
            log.push("Trap end_shots");
        }
    }
}

/// An adapter that fails its second `adapt` ever, or panics in it if made
/// `panicking`, as a fault that does not come back, and passes every other
/// through.
pub(crate) struct FailsOnce {
    adapts: u32,
    panics: bool,
    below: Option<OpcodeMap>,
}

impl FailsOnce {
    pub(crate) fn new() -> Self {
        Self {
            adapts: 0,
            panics: false,
            below: None,
        }
    }

    /// One that panics in its second `adapt` instead of failing it.
    pub(crate) fn panicking() -> Self {
        Self {
            panics: true,
            ..Self::new()
        }
    }
}

impl Adapter for FailsOnce {
    fn provides(&self) -> &Capabilities {
        coin_capabilities()
    }

    fn requires(&self) -> &Capabilities {
        coin_capabilities()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.adapts += 1;
        if self.adapts == 2 {
            assert!(!self.panics, "the adapter broke once");
            return Err(ExecuteError::Failed {
                shot: batches[0].shots.first(),
                failure: Failure::Part("the adapter failed once".into()),
            });
        }
        let map = bound(self.below.as_ref());
        forward(batches, seed, below, readouts, |batch| {
            mapped(map, batch.instructions)
        })
        .await
    }
}

/// How a [`Misbehaving`] adapter breaks the protocol's rules.
#[derive(Clone, Copy)]
pub(crate) enum Misbehaviour {
    /// Adds an `x` on qubit 99 to each batch, beyond the qubits it
    /// forwards unchanged.
    SendsBeyondItsQubits,
    /// Panics with a formatted message, a `String`.
    PanicsWithAString,
    /// Panics with a number, a payload that is not text.
    PanicsWithANumber,
}

/// An adapter of [`CoinGates`] that breaks a rule of the protocol, as
/// `Misbehaviour` says; it passes the batches through otherwise.
pub(crate) struct Misbehaving {
    misbehaviour: Misbehaviour,
    below: Option<OpcodeMap>,
}

impl Misbehaving {
    pub(crate) fn new(misbehaviour: Misbehaviour) -> Self {
        Self {
            misbehaviour,
            below: None,
        }
    }
}

impl Adapter for Misbehaving {
    fn provides(&self) -> &Capabilities {
        coin_capabilities()
    }

    fn requires(&self) -> &Capabilities {
        coin_capabilities()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        match self.misbehaviour {
            Misbehaviour::PanicsWithAString => {
                panic!("the adapter broke on {} batches", batches.len())
            }
            Misbehaviour::PanicsWithANumber => std::panic::panic_any(42_u32),
            Misbehaviour::SendsBeyondItsQubits => {}
        }
        let map = bound(self.below.as_ref());
        forward(batches, seed, below, readouts, |batch| {
            let mut instructions = batch.instructions.to_vec();
            if let Misbehaviour::SendsBeyondItsQubits = self.misbehaviour {
                instructions.push(coin_gate("x", 99));
            }
            mapped(map, &instructions)
        })
        .await
    }
}

/// An adapter that says it resolves readout parities itself, as a decoder
/// would, so that it requires none below and provides them above, but
/// passes every batch down as it came, readout parities included: a
/// [`Profiler`] of `set` otherwise.
pub(crate) struct ForwardsParity {
    profiler: Profiler,
    requires: Capabilities,
    provides: Capabilities,
}

impl ForwardsParity {
    pub(crate) fn on(set: InstructionSet, log: &Log) -> Self {
        let parity = |yes| {
            Capabilities::new(set.clone()).with_features(Features::new().with_readout_parity(yes))
        };
        Self {
            requires: parity(false),
            provides: parity(true),
            profiler: Profiler::on(set, &Rc::new(Cell::new(0)), log),
        }
    }
}

impl Adapter for ForwardsParity {
    fn provides(&self) -> &Capabilities {
        &self.provides
    }

    fn requires(&self) -> &Capabilities {
        &self.requires
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.profiler.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.profiler.adapt(batches, seed, below, readouts).await
    }
}

/// An adapter that says it replays its shots itself, as one that keeps each
/// shot's readouts would: it runs no shot again below, but replays with
/// its run's seed, so it requires seeded runs and no replay below, and
/// provides replay above: a [`Profiler`] otherwise.
pub(crate) struct ReplaysItself {
    profiler: Profiler,
    requires: Capabilities,
    provides: Capabilities,
}

impl ReplaysItself {
    pub(crate) fn new(log: &Log) -> Self {
        let stating = |features| Capabilities::new(CoinGates::set()).with_features(features);
        Self {
            requires: stating(Features::new().with_replay(false).with_seeded_runs(true)),
            provides: stating(Features::new().with_replay(true)),
            profiler: Profiler::new(&Rc::new(Cell::new(0)), log),
        }
    }
}

impl Adapter for ReplaysItself {
    fn provides(&self) -> &Capabilities {
        &self.provides
    }

    fn requires(&self) -> &Capabilities {
        &self.requires
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.profiler.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.profiler.adapt(batches, seed, below, readouts).await
    }
}

/// An adapter that panics in each `end_shots`, or in its first only, with
/// its message, a `&str`. At run time it passes instructions through, as a
/// [`Spread`] of 1.
pub(crate) struct PanicsEnding {
    message: &'static str,
    /// Whether it panics in its first `end_shots` only.
    once: bool,
    panicked: bool,
    pass_through: Spread,
}

impl PanicsEnding {
    /// One that panics with `"end_shots broke"`.
    pub(crate) fn new(log: &Log) -> Self {
        Self::saying("end_shots broke", log)
    }

    /// One that panics with `message`.
    pub(crate) fn saying(message: &'static str, log: &Log) -> Self {
        Self {
            message,
            once: false,
            panicked: false,
            pass_through: Spread::new(1, log),
        }
    }

    /// One that panics with `"end_shots broke"` in its first `end_shots`
    /// only, as a fault that does not come back.
    pub(crate) fn once(log: &Log) -> Self {
        Self {
            once: true,
            ..Self::new(log)
        }
    }
}

impl Adapter for PanicsEnding {
    fn provides(&self) -> &Capabilities {
        self.pass_through.provides()
    }

    fn requires(&self) -> &Capabilities {
        self.pass_through.requires()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.pass_through.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.pass_through
            .adapt(batches, seed, below, readouts)
            .await
    }

    fn end_shots(&mut self, _shots: &ShotSet) {
        if !(self.once && self.panicked) {
            self.panicked = true;
            std::panic::panic_any(self.message);
        }
    }
}

/// An adapter that, inside `adapt`, ends below the shots of each batch
/// once their answer is in, as an adapter freeing state below early
/// would. At run time it passes instructions through, as a [`Spread`] of
/// 1.
pub(crate) struct EndsBelow(Spread);

impl EndsBelow {
    pub(crate) fn new(log: &Log) -> Self {
        Self(Spread::new(1, log))
    }
}

impl Adapter for EndsBelow {
    fn provides(&self) -> &Capabilities {
        self.0.provides()
    }

    fn requires(&self) -> &Capabilities {
        self.0.requires()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.0.adapt(batches, seed, below, readouts).await?;
        for batch in batches {
            below.end_shots(batch.shots);
        }
        Ok(())
    }
}
