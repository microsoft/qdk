// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A readout error written by hand: an adapter that draws random numbers,
//! from seeds of its own, which `compose` gives it when it builds it, and
//! flips each readout of the executor below with chance ½.

use std::collections::BTreeMap;

use executor_protocol::{
    Adapter, AdapterBoundary, AdapterSeeds, Apply, Close, ComponentId, Executor, ImmediateReply,
    InstructionBatch, InstructionBatchRef, Rejected, ShotId, ShotOutcome, Value, shot_seed, wait,
};

use crate::toy::Op;

/// The id the readout error draws with: its seeds are its own, apart from
/// the simulator's.
pub const READOUT: ComponentId = ComponentId::named("readout");

/// Whether the readout error flips the `k`-th readout of a shot in which it
/// draws from `seed`: its `k`-th draw, [`shot_seed`]`(seed, k)`, is odd.
pub fn flips(seed: u64, k: u32) -> bool {
    shot_seed(seed, k) & 1 == 1
}

/// The readout error, as an adapter: it states the id it draws with, and
/// keeps the seeds `compose` gives it for that id.
#[derive(Default)]
pub struct ReadoutError {
    seeds: Option<AdapterSeeds>,
}

impl Adapter for ReadoutError {
    type Accepts = Op;
    type Sends = Op;

    fn draws(&self) -> Option<ComponentId> {
        Some(READOUT)
    }

    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        Self {
            seeds: boundary.drawing_seeds(),
        }
    }
}

impl<E: Executor<InstructionBatch<Op>> + Close> Apply<E> for ReadoutError {
    type Output = Flips<E>;

    fn apply(self, downstream: E) -> Flips<E> {
        Flips {
            seeds: self
                .seeds
                .expect("applied once composed, and a chain giving it no seeds is refused"),
            drawn: BTreeMap::new(),
            inner: downstream,
        }
    }
}

/// The executor the readout error makes of the executor below: its
/// outcomes, each readout flipped where the shot's next draw says so.
pub struct Flips<E> {
    seeds: AdapterSeeds,
    /// How many readouts of each shot it has drawn for.
    drawn: BTreeMap<ShotId, u32>,
    inner: E,
}

impl<E: Executor<InstructionBatch<Op>>> Executor<InstructionBatch<Op>> for Flips<E> {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, Op>) -> Result<Self::Reply, Rejected> {
        let mut outcomes = Vec::new();
        if let Err(reason) = wait(self.inner.call(batch)?, &mut outcomes) {
            return Ok(ImmediateReply::failed(outcomes, reason));
        }
        for outcome in &mut outcomes {
            let k = self.drawn.entry(outcome.shot).or_insert(0);
            if flips(self.seeds.shot(outcome.shot).seed(), *k) {
                outcome.value = match outcome.value {
                    Value::Zero => Value::One,
                    Value::One => Value::Zero,
                    error @ Value::Error(_) => error,
                };
            }
            *k += 1;
        }
        Ok(ImmediateReply::done(outcomes))
    }
}

impl<E: Close> Close for Flips<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.inner.close()
    }
}
