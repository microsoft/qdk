// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake adapters.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::future::poll_fn;
use std::pin::pin;
use std::rc::Rc;
use std::task::Poll;

use super::{CoinGates, Log, Stream, bound, coin_gate, coin_gates, forward, mapped, zeros};
use crate::{
    Adapt, AppendOnlyReadouts, Batch, BatchReadouts, BlockType, Capabilities, Execute,
    ExecuteError, Failure, Instruction, InstructionSet, Needs, OpcodeMap, Readouts, RunRefusal,
    Seed, ShotId, ShotSet,
};

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
        coin_gates()
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
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        let map = bound(self.below.as_ref());
        let own = seed.derive("bit_flip");
        let mut sent: Vec<(ShotSet, Vec<Instruction>, bool)> = Vec::new();
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
                    batch.is_checked(),
                ));
            }
        }
        let one_each: Vec<Batch<'_>> = sent
            .iter()
            .map(|(shots, instructions, checked)| {
                Batch::new(shots, instructions).checked_if(*checked)
            })
            .collect();
        let mut answer = Readouts::new();
        below
            .execute(&one_each, seed, &mut answer.append_only())
            .await?;
        let mut rows = answer.iter();
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
    set: InstructionSet,
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

    /// A profiler accepting and sending `set`.
    pub(crate) fn on(set: InstructionSet, instructions: &Rc<Cell<usize>>, log: &Log) -> Self {
        Self {
            set,
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
        match self.busy {
            Some(why) => Err(RunRefusal::Unavailable(why.into())),
            None => Ok(()),
        }
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut AppendOnlyReadouts<'_>,
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
/// the part below accepts as `below accepts: <set>`.
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

impl Adapt for NamesBelow {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
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
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        self.log.push(format!("below: {}", below.name()));
        self.log
            .push(format!("below accepts: {}", below.accepts().name()));
        let map = bound(self.below.as_ref());
        forward(batches, seed, below, readouts, |batch| {
            mapped(map, batch.instructions)
        })
        .await
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
        coin_gates()
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
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        let map = bound(self.below.as_ref());
        let mut parts = Readouts::new();
        for batch in batches {
            let instructions = mapped(map, batch.instructions);
            parts.clear();
            for instruction in &instructions {
                let step = Batch::new(batch.shots, std::slice::from_ref(instruction))
                    .checked_if(batch.is_checked());
                below
                    .execute(&[step], seed, &mut parts.append_only())
                    .await?;
            }
            readouts.push(if parts.is_empty() {
                zeros(batch.shots.len(), 0)
            } else {
                BatchReadouts::concat_readouts(&parts)
            });
        }
        Ok(())
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
        coin_gates()
    }

    fn sends(&self) -> &InstructionSet {
        self.accepts()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    fn needs_below(&self, above: Needs) -> Needs {
        match above.blocks_of(&BlockType::qubit()) {
            0 => above,
            qubits => above.blocks(BlockType::qubit(), qubits * self.k),
        }
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
        readouts: &mut AppendOnlyReadouts<'_>,
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
/// qubits above, so it needs `qubits + 2 * carbons` qubits below. No offer,
/// a most per block type, can say what it serves; it offers each type the
/// whole pool, the most that type alone could hold. At run time it passes
/// instructions through, as a [`Spread`] of 1: the tests use it for its
/// needs and offer.
pub(crate) struct Pool(Spread);

impl Pool {
    pub(crate) fn new(log: &Log) -> Self {
        Self(Spread::new(1, log))
    }

    /// The block type it lowers onto qubits.
    pub(crate) fn carbon() -> BlockType {
        BlockType::new("carbon")
    }
}

impl Adapt for Pool {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.0.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    fn needs_below(&self, above: Needs) -> Needs {
        let qubits = above.blocks_of(&BlockType::qubit()) + 2 * above.blocks_of(&Self::carbon());
        let mut below = above;
        below.blocks.remove(&Self::carbon());
        below.blocks(BlockType::qubit(), qubits)
    }

    fn offers_above(&self, below: Capabilities) -> Capabilities {
        let mut above = below;
        let carbon = match above.max_blocks.get(&BlockType::qubit()) {
            Some(pool) => pool.map(|qubits| qubits / 2),
            None => Some(0),
        };
        above.max_blocks.insert(Self::carbon(), carbon);
        above
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        self.0.adapt(batches, seed, below, readouts).await
    }
}

/// An adapter that changes only the offer: a [`Spread`] of `k`, whose
/// offer above is `offer` applied to the spread's. Each constructor is a
/// toy: a part whose offer depends on the offer below as a real one's
/// does, or as no real part's does. At run time it passes instructions
/// through the spread.
pub(crate) struct Offering {
    spread: Spread,
    offer: Box<dyn Fn(Capabilities) -> Capabilities>,
}

impl Offering {
    fn new(k: u32, log: &Log, offer: impl Fn(Capabilities) -> Capabilities + 'static) -> Self {
        Self {
            spread: Spread::new(k, log),
            offer: Box::new(offer),
        }
    }

    /// A toy encoding whose decoder holds at most `most` logical qubits: a
    /// [`Spread`] of `k` that offers above at most `most` qubits, whatever
    /// the qubits below.
    pub(crate) fn capped(k: u32, most: u32, log: &Log) -> Self {
        Self::new(k, log, move |mut above| {
            if let Some(qubits) = above.max_blocks.get_mut(&BlockType::qubit()) {
                *qubits = Some(qubits.map_or(most, |q| q.min(most)));
            }
            above
        })
    }

    /// An adapter whose offer shrinks as the offer below grows, as no real
    /// part's does: the qubits below, up to 10, and 1 over more.
    pub(crate) fn contrary(log: &Log) -> Self {
        Self::new(1, log, |mut above| {
            if let Some(qubits) = above.max_blocks.get_mut(&BlockType::qubit()) {
                *qubits = Some(qubits.filter(|&q| q <= 10).unwrap_or(1));
            }
            above
        })
    }

    /// A toy decoder that takes a workspace from the qubits below without
    /// asking for it in its needs below, which [`Adapt::needs_below`]'s
    /// rule forbids: it offers 1 qubit above over at most 10 below, and
    /// every qubit below over more, and needs below what reaches it. So
    /// given exactly what it needs below, it is short on its own, while
    /// over more qubits it is not.
    pub(crate) fn workspace(log: &Log) -> Self {
        Self::new(1, log, |mut above| {
            if let Some(qubits) = above.max_blocks.get_mut(&BlockType::qubit()) {
                *qubits = qubits.map(|q| if q <= 10 { 1 } else { q });
            }
            above
        })
    }

    /// A toy part reserving one qubit below as an ancilla: it offers above
    /// one qubit fewer than below. Its needs below do not ask for the
    /// ancilla, since the tests that use it read only its offer.
    pub(crate) fn reserving_an_ancilla(log: &Log) -> Self {
        Self::new(1, log, |mut above| {
            if let Some(Some(qubits)) = above.max_blocks.get_mut(&BlockType::qubit()) {
                *qubits = qubits.saturating_sub(1);
            }
            above
        })
    }

    /// A toy job queue that takes at most `most` shots a run, so it lowers
    /// the offer's most shots to `most`.
    pub(crate) fn queue(most: u32, log: &Log) -> Self {
        Self::new(1, log, move |below| {
            let most = below.max_shots.map_or(most, |m| m.min(most));
            below.max_shots(most)
        })
    }

    /// A toy decoder that states its offer afresh, as
    /// `Capabilities::unlimited().max_qubits(qubits)`, whatever is offered
    /// below: seedable, and no limit on shots.
    pub(crate) fn restating(qubits: u32, log: &Log) -> Self {
        Self::new(1, log, move |_| {
            Capabilities::unlimited().max_qubits(qubits)
        })
    }

    /// The decoder [`Offering::restating`] makes, stating at most
    /// `most_shots` shots a run.
    pub(crate) fn restating_most_shots(qubits: u32, most_shots: u32, log: &Log) -> Self {
        Self::new(1, log, move |_| {
            Capabilities::unlimited()
                .max_qubits(qubits)
                .max_shots(most_shots)
        })
    }

    /// A toy link to a device whose measurements cannot be seeded: it
    /// offers what is offered below, but not seedable.
    pub(crate) fn unseedable(log: &Log) -> Self {
        Self::new(1, log, Capabilities::not_seedable)
    }

    /// A toy lowering that cannot carry readout parities: it offers none
    /// above, whatever the offer below.
    pub(crate) fn lowering_without_parity(log: &Log) -> Self {
        Self::new(1, log, Capabilities::without_readout_parity)
    }
}

impl Adapt for Offering {
    fn accepts(&self) -> &InstructionSet {
        self.spread.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.spread.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.spread.bind(below);
    }

    fn needs_below(&self, above: Needs) -> Needs {
        self.spread.needs_below(above)
    }

    fn offers_above(&self, below: Capabilities) -> Capabilities {
        (self.offer)(self.spread.offers_above(below))
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        self.spread.adapt(batches, seed, below, readouts).await
    }
}

/// An adapter that stops waiting: it polls its answer from below once,
/// drops that future unfinished, then asks below again and waits. At run
/// time it passes instructions through, as a [`Spread`] of 1.
pub(crate) struct Impatient(Spread);

impl Impatient {
    pub(crate) fn new(log: &Log) -> Self {
        Self(Spread::new(1, log))
    }
}

impl Adapt for Impatient {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.0.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        {
            let mut dropped = Readouts::new();
            let mut lent = dropped.append_only();
            let mut first = pin!(self.0.adapt(batches, seed, below, &mut lent));
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

impl Adapt for Trap {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
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
        readouts: &mut AppendOnlyReadouts<'_>,
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

/// An adapter that fails its second `adapt` ever, as a fault that does not
/// come back, and passes every other through.
pub(crate) struct FailsOnce {
    adapts: u32,
    below: Option<OpcodeMap>,
}

impl FailsOnce {
    pub(crate) fn new() -> Self {
        Self {
            adapts: 0,
            below: None,
        }
    }
}

impl Adapt for FailsOnce {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
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
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        self.adapts += 1;
        if self.adapts == 2 {
            return Err(ExecuteError::Failed {
                shot: batches[0].shots.iter().next().expect("a shot"),
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
    /// Adds an `x` on qubit 9 to each batch, beyond the needs it passes
    /// down unchanged.
    SendsBeyondTheNeeds,
    /// Sends each batch's instructions three times over.
    TriplesEachBatch,
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

impl Adapt for Misbehaving {
    fn accepts(&self) -> &InstructionSet {
        coin_gates()
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
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        match self.misbehaviour {
            Misbehaviour::PanicsWithAString => {
                panic!("the adapter broke on {} batches", batches.len())
            }
            Misbehaviour::PanicsWithANumber => std::panic::panic_any(42_u32),
            Misbehaviour::SendsBeyondTheNeeds | Misbehaviour::TriplesEachBatch => {}
        }
        let map = bound(self.below.as_ref());
        forward(batches, seed, below, readouts, |batch| {
            let once = batch.instructions;
            let mut instructions = match self.misbehaviour {
                Misbehaviour::TriplesEachBatch => [once, once, once].concat(),
                _ => once.to_vec(),
            };
            if let Misbehaviour::SendsBeyondTheNeeds = self.misbehaviour {
                instructions.push(coin_gate("x", 9));
            }
            mapped(map, &instructions)
        })
        .await
    }
}

/// An adapter that says it resolves readout parities itself, as a decoder
/// would, so that its needs below use none and it offers them above
/// whatever the part below offers, but passes every batch down as it
/// came, readout parities included: a [`Profiler`] of `set` otherwise.
pub(crate) struct ForwardsParity(Profiler);

impl ForwardsParity {
    pub(crate) fn on(set: InstructionSet, log: &Log) -> Self {
        Self(Profiler::on(set, &Rc::new(Cell::new(0)), log))
    }
}

impl Adapt for ForwardsParity {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.0.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    fn needs_below(&self, needs: Needs) -> Needs {
        Needs {
            uses_readout_parity: false,
            ..needs
        }
    }

    fn offers_above(&self, below: Capabilities) -> Capabilities {
        Capabilities {
            resolves_readout_parity: true,
            ..below
        }
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        self.0.adapt(batches, seed, below, readouts).await
    }
}

/// An adapter whose offer narrows once it is provisioned, which
/// [`Adapt::offers_above`]'s rule forbids, as a lowering that picks its
/// encoding then: at most 2 instructions a batch before, 1 after. At each
/// `adapt` it logs the most instructions the part below says it takes, and
/// passes instructions through, as a [`Spread`] of 1.
pub(crate) struct Shifting {
    provisioned: bool,
    pass_through: Spread,
    log: Log,
}

impl Shifting {
    pub(crate) fn new(log: &Log) -> Self {
        Self {
            provisioned: false,
            pass_through: Spread::new(1, log),
            log: log.clone(),
        }
    }
}

impl Adapt for Shifting {
    fn accepts(&self) -> &InstructionSet {
        self.pass_through.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.pass_through.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.pass_through.bind(below);
    }

    fn offers_above(&self, below: Capabilities) -> Capabilities {
        below.max_instructions(if self.provisioned { 1 } else { 2 })
    }

    fn provision(&mut self, _above: Needs) {
        self.provisioned = true;
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        let most = below.offers().max_instructions;
        self.log
            .push(format!("Shifting is offered {most:?} instructions a batch"));
        self.pass_through
            .adapt(batches, seed, below, readouts)
            .await
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

impl Adapt for PanicsEnding {
    fn accepts(&self) -> &InstructionSet {
        self.pass_through.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.pass_through.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.pass_through.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut AppendOnlyReadouts<'_>,
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

impl Adapt for EndsBelow {
    fn accepts(&self) -> &InstructionSet {
        self.0.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.0.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> Result<(), ExecuteError> {
        self.0.adapt(batches, seed, below, readouts).await?;
        for batch in batches {
            below.end_shots(batch.shots);
        }
        Ok(())
    }
}
