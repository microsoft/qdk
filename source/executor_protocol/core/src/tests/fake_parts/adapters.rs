// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake adapters.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::future::poll_fn;
use std::pin::pin;
use std::rc::Rc;
use std::task::Poll;

use super::{
    BEYOND_THE_SET, Log, Stream, batches_of, bound, capabilities, coin_gates, forward, mapped,
    ready_for,
};
use crate::prelude::*;
use crate::tests::example_sets::{Gate, Measuring};
use crate::{
    Batch, BatchReadouts, Capabilities, ExecuteError, Failure, Features, NotReady, Readouts,
    Resources, Seed, ShotId, ShotSet, StatesLimits,
};

/// A toy bit-flip noise: after each `x`, another `x` with probability
/// `probability`, drawn from its own stream per shot. Each shot goes
/// below as a batch of its own, since each may flip differently.
pub(crate) struct BitFlip {
    probability: f64,
    below: Option<Vec<u32>>,
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
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
    ) -> Result<(), ExecuteError> {
        let map = bound(self.below.as_ref());
        let own = seed.derive("bit_flip");
        let mut sent: Vec<(ShotSet, Vec<Gate>, u32, bool)> = Vec::new();
        for batch in batches {
            for (j, id) in batch.shots.iter().enumerate() {
                let stream = self
                    .streams
                    .entry(id)
                    .or_insert_with(|| Stream(own.shot(id)));
                let mut instructions = Vec::with_capacity(batch.instructions.len() + 1);
                for &gate in batch.instructions {
                    let flips =
                        coin_gates().name(gate) == Some("x") && stream.chance(self.probability);
                    let mapped = Measuring::bound(map, gate);
                    instructions.push(mapped);
                    if flips {
                        instructions.push(mapped);
                    }
                }
                sent.push((
                    batch.shots.subset(&[j]).expect("a position of the batch"),
                    instructions,
                    batch.readout_slots(),
                    batch.is_checked(),
                ));
            }
        }
        let one_each: Vec<Batch<'_, Measuring>> = sent
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
    set: Measuring,
    below: Option<Vec<u32>>,
    instructions: Rc<Cell<usize>>,
    busy: Option<&'static str>,
    log: Log,
}

impl Profiler {
    /// A profiler of [`coin_gates`], counting into `instructions`.
    pub(crate) fn new(instructions: &Rc<Cell<usize>>, log: &Log) -> Self {
        Self::on(*coin_gates(), instructions, log)
    }

    /// A profiler accepting and sending `set`, not stating any resource
    /// or feature: it passes what arrives below.
    pub(crate) fn on(set: Measuring, instructions: &Rc<Cell<usize>>, log: &Log) -> Self {
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

impl Adapter for Profiler {
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        &self.set
    }

    fn sends(&self) -> &Measuring {
        &self.set
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.below = Some(below);
    }

    async fn ready(&mut self) -> Result<(), NotReady> {
        self.log.push("Profiler ready");
        match self.busy {
            Some(why) => Err(NotReady::new(why)),
            None => Ok(()),
        }
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
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
/// part below in its own errors would read it, then the set the part
/// below accepts as `below accepts: <set>`, and the most instructions a
/// batch it provides for as `below takes <most> a batch` (`None` when it
/// does not state one). It does not state any resource or feature.
pub(crate) struct NamesBelow {
    below: Option<Vec<u32>>,
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
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
    ) -> Result<(), ExecuteError> {
        self.log.push(format!("below: {}", below.name()));
        self.log
            .push(format!("below accepts: {:?}", below.accepts()));
        let most = (below.provided()).and_then(|limits| limits.resources().instructions_per_batch);
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
/// instruction below, whatever count arrives.
pub(crate) struct Splitter {
    below: Option<Vec<u32>>,
}

impl Splitter {
    pub(crate) fn new() -> StatesLimits<Self> {
        Self { below: None }.stating_limits()
    }
}

impl AdapterLimits for Splitter {
    /// What arrives, in batches of one instruction.
    fn requires(&self, above: &Capabilities<Measuring>) -> Capabilities<Measuring> {
        (above.clone()).map_resources(|resources| resources.with_instructions_per_batch(1))
    }
}

impl Adapter for Splitter {
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
    ) -> Result<(), ExecuteError> {
        let map = bound(self.below.as_ref());
        let mut parts: Vec<BatchReadouts<Measuring>> = Vec::new();
        for (batch, batch_readouts) in batches.iter().zip(readouts) {
            let instructions = mapped(map, batch.instructions);
            parts.clear();
            for (gate, sent) in batch.instructions.iter().zip(&instructions) {
                let slots = coin_gates().readout_slots(gate);
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

/// The instructions a batch `capabilities` state may hold, or 0.
fn instructions_of(capabilities: &Capabilities<Measuring>) -> u32 {
    capabilities.resources().instructions_per_batch.unwrap_or(0)
}

/// What arrives, in batches of `n` instructions.
fn in_batches_of(above: &Capabilities<Measuring>, n: u32) -> Capabilities<Measuring> {
    (above.clone()).map_resources(|resources| resources.with_instructions_per_batch(n))
}

/// A toy encoding: each qubit above is `k` qubits below, of which it uses
/// the first. Made by [`new`](Self::new), it states nothing;
/// [`built_for`](Self::built_for) states the instructions a batch it takes
/// may hold, and that it requires `k` instructions below for each that
/// arrives: an upper bound, as an encoding writing each gate in up to `k`
/// would state, though this toy writes each in one.
pub(crate) struct Spread {
    k: u32,
    provides: Option<Capabilities<Measuring>>,
    /// Whether it states its batches, once built.
    built: bool,
    below: Option<Vec<u32>>,
    log: Log,
}

impl Spread {
    pub(crate) fn new(k: u32, log: &Log) -> StatesLimits<Self> {
        Self {
            k,
            provides: None,
            built: false,
            below: None,
            log: log.clone(),
        }
        .stating_limits()
    }
}

impl StatesLimits<Spread> {
    /// This encoding, built for batches of `n` instructions above: it
    /// takes at most `n`, and requires `k` instructions below for each
    /// that arrives.
    pub(crate) fn built_for(mut self, n: u32) -> Self {
        self.part.provides = Some(capabilities(batches_of(n), Features::unconstrained()));
        self.part.built = true;
        self
    }
}

impl AdapterLimits for Spread {
    fn provides(&self) -> Option<&Capabilities<Measuring>> {
        self.provides.as_ref()
    }

    /// What arrives, with `k` instructions a batch for each that arrives,
    /// once built.
    fn requires(&self, above: &Capabilities<Measuring>) -> Capabilities<Measuring> {
        if self.built {
            in_batches_of(above, self.k * instructions_of(above))
        } else {
            above.clone()
        }
    }

    async fn ready_for(&mut self, above: &Capabilities<Measuring>) -> Result<(), NotReady> {
        self.log.push(ready_for("Spread", above));
        Ok(())
    }
}

impl Adapter for Spread {
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
    ) -> Result<(), ExecuteError> {
        let map = bound(self.below.as_ref());
        forward(batches, seed, below, readouts, |batch| {
            let mut instructions = mapped(map, batch.instructions);
            for gate in &mut instructions {
                gate.qubit *= self.k;
            }
            instructions
        })
        .await
    }
}

/// What a [`Stating`] toy requires below, given what arrives.
type Requiring = Box<dyn Fn(&Capabilities<Measuring>) -> Capabilities<Measuring>>;

/// An adapter that states capabilities of its own, one toy per
/// constructor: the limits it takes ([`AdapterLimits::provides`]), and
/// what it requires below given what arrives
/// ([`AdapterLimits::requires`]). At run time it passes instructions
/// through, as a [`Spread`] of 1.
pub(crate) struct Stating {
    spread: StatesLimits<Spread>,
    provides: Option<Capabilities<Measuring>>,
    requires: Requiring,
}

impl Stating {
    /// A toy taking `provides` (`None`: no limit), and requiring below
    /// what `requires` makes of what arrives.
    fn new(
        log: &Log,
        provides: Option<Capabilities<Measuring>>,
        requires: impl Fn(&Capabilities<Measuring>) -> Capabilities<Measuring> + 'static,
    ) -> StatesLimits<Self> {
        Self {
            spread: Spread::new(1, log),
            provides,
            requires: Box::new(requires),
        }
        .stating_limits()
    }

    /// A toy taking `resources` and `features`, and passing what arrives
    /// below.
    fn providing(log: &Log, resources: Resources, features: Features) -> StatesLimits<Self> {
        Self::new(log, Some(capabilities(resources, features)), Clone::clone)
    }

    /// A toy interleaver, sending batches of `least` instructions at
    /// least: it requires the instructions a batch that arrive, or
    /// `least` if fewer arrive.
    pub(crate) fn interleaving(least: u32, log: &Log) -> StatesLimits<Self> {
        Self::new(log, None, move |above| {
            in_batches_of(above, instructions_of(above).max(least))
        })
    }

    /// A toy part adding `k` ancilla resets to each batch: it requires `k`
    /// instructions a batch more than arrive.
    pub(crate) fn reserving_ancillas(k: u32, log: &Log) -> StatesLimits<Self> {
        Self::new(log, None, move |above| {
            in_batches_of(above, instructions_of(above) + k)
        })
    }

    /// A toy job splitter, which splits each run into runs of `shots`: it
    /// requires runs of `shots` below, whatever arrives. A declaration
    /// fake: it passes each run below whole, splitting none.
    pub(crate) fn splitting_runs(shots: u32, log: &Log) -> StatesLimits<Self> {
        Self::new(log, None, move |above| {
            (above.clone()).map_resources(|resources| resources.with_shots(shots))
        })
    }

    /// A toy repetition, running each shot `times` times below: it
    /// requires `times` as many shots below as arrive, and does not state
    /// shots if none arrive. A declaration fake: it passes each run below
    /// as it is.
    pub(crate) fn repeating_shots(times: u32, log: &Log) -> StatesLimits<Self> {
        Self::new(log, None, move |above| match above.resources().shots {
            Some(shots) => (above.clone())
                .map_resources(|resources| resources.with_shots(shots.saturating_mul(times))),
            None => above.clone(),
        })
    }

    /// A toy job queue that takes at most `most` shots a run.
    pub(crate) fn queue(most: u32, log: &Log) -> StatesLimits<Self> {
        Self::providing(
            log,
            Resources::unconstrained().with_shots(most),
            Features::unconstrained(),
        )
    }

    /// A toy cap that takes batches of at most `n` instructions, as a
    /// decoder built for that many does.
    pub(crate) fn capping(n: u32, log: &Log) -> StatesLimits<Self> {
        Self::providing(log, batches_of(n), Features::unconstrained())
    }

    /// A toy replayer that keeps each shot's readouts and gives them back
    /// on replay: it requires no replay below, and passes seeded runs.
    pub(crate) fn replaying(log: &Log) -> StatesLimits<Self> {
        Self::new(log, None, |above| {
            (above.clone()).map_features(|features| features.with_replay(false))
        })
    }

    /// A toy sampler that draws its shots' readouts together, from counts
    /// it takes below with its run's seed: it takes no replay, and
    /// requires no seeded runs below.
    pub(crate) fn seeding_itself(log: &Log) -> StatesLimits<Self> {
        Self::new(
            log,
            Some(capabilities(
                Resources::unconstrained(),
                Features::unconstrained().with_replay(false),
            )),
            |above| (above.clone()).map_features(|features| features.with_seeded_runs(false)),
        )
    }
}

impl AdapterLimits for Stating {
    fn provides(&self) -> Option<&Capabilities<Measuring>> {
        self.provides.as_ref()
    }

    fn requires(&self, above: &Capabilities<Measuring>) -> Capabilities<Measuring> {
        (self.requires)(above)
    }

    async fn ready_for(&mut self, above: &Capabilities<Measuring>) -> Result<(), NotReady> {
        self.spread.part.ready_for(above).await
    }
}

impl Adapter for Stating {
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.spread.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
    ) -> Result<(), ExecuteError> {
        self.spread.adapt(batches, seed, below, readouts).await
    }
}

/// An adapter that stops waiting: it polls its answer from below once,
/// drops that future unfinished, then asks below again, into the same
/// readouts, and waits. At run time it passes instructions through, as a
/// [`Spread`] of 1.
pub(crate) struct Impatient(StatesLimits<Spread>);

impl Impatient {
    pub(crate) fn new(log: &Log) -> Self {
        Self(Spread::new(1, log))
    }
}

impl Adapter for Impatient {
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
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
    below: Option<Vec<u32>>,
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
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
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
    below: Option<Vec<u32>>,
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
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
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
    /// Adds [`BEYOND_THE_SET`] to each batch.
    SendsBeyondItsSet,
    /// Panics with a formatted message, a `String`.
    PanicsWithAString,
    /// Panics with a number, a payload that is not text.
    PanicsWithANumber,
}

/// An adapter of [`coin_gates`] that breaks a rule of the protocol, as
/// `Misbehaviour` says; it passes the batches through otherwise.
pub(crate) struct Misbehaving {
    misbehaviour: Misbehaviour,
    below: Option<Vec<u32>>,
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
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
    ) -> Result<(), ExecuteError> {
        match self.misbehaviour {
            Misbehaviour::PanicsWithAString => {
                panic!("the adapter broke on {} batches", batches.len())
            }
            Misbehaviour::PanicsWithANumber => std::panic::panic_any(42_u32),
            Misbehaviour::SendsBeyondItsSet => {}
        }
        let map = bound(self.below.as_ref());
        forward(batches, seed, below, readouts, |batch| {
            let mut instructions = mapped(map, batch.instructions);
            if let Misbehaviour::SendsBeyondItsSet = self.misbehaviour {
                instructions.push(BEYOND_THE_SET);
            }
            instructions
        })
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
    pass_through: StatesLimits<Spread>,
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
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.pass_through.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
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
pub(crate) struct EndsBelow(StatesLimits<Spread>);

impl EndsBelow {
    pub(crate) fn new(log: &Log) -> Self {
        Self(Spread::new(1, log))
    }
}

impl Adapter for EndsBelow {
    type Set = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.0.bind(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
    ) -> Result<(), ExecuteError> {
        self.0.adapt(batches, seed, below, readouts).await?;
        for batch in batches {
            below.end_shots(batch.shots);
        }
        Ok(())
    }
}

/// A translating adapter of [`coin_gates`] to the same gates, each sent
/// as itself, encoded as its binding to the receiver's set says, with its
/// readouts and its shots' conditions as they came: a translating adapter
/// that is no [`Adapter`], whose binding is not `()`.
#[derive(Default)]
pub(crate) struct GateForGate {
    below: Option<Vec<u32>>,
}

impl crate::TranslatingAdapter for GateForGate {
    type Accepts = Measuring;
    type Sends = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    fn requires(&self, above: &Capabilities<Measuring>) -> Capabilities<Measuring> {
        above.clone()
    }

    fn bind(&mut self, below: Vec<u32>) {
        self.below = Some(below);
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Measuring>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
        readouts: &mut [BatchReadouts<Measuring>],
    ) -> Result<(), ExecuteError> {
        let map = bound(self.below.as_ref());
        forward(batches, seed, below, readouts, |batch| {
            mapped(map, batch.instructions)
        })
        .await
    }
}
