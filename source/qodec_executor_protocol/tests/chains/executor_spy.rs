// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ExecutorSpy`]: an executor that records each batch it receives before
//! the executor it wraps runs it, and may take fewer instructions per batch
//! than that executor.

use executor_protocol::{
    Batch, BatchReadouts, Execute, ExecuteError, ExecutorSetup, InstructionSet, Needs, NotReady,
    Offer, Seed, ShotId, ShotSet,
};

/// What the spy saw of a batch it received.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivedBatch {
    /// Its shots' ids, in its set's order.
    pub shots: Vec<u32>,
    /// How many instructions it holds.
    pub instructions: usize,
    /// Whether its sender marked it checked.
    pub checked: bool,
}

/// `executor`, recording each batch it receives.
pub struct ExecutorSpy<E> {
    executor: E,
    most_instructions: Option<u32>,
    /// Each batch received, in order: only those the protocol's boundary
    /// let through.
    pub received: Vec<ReceivedBatch>,
}

impl<E> ExecutorSpy<E> {
    /// A spy on `executor`, offering what it offers.
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            most_instructions: None,
            received: Vec::new(),
        }
    }

    /// This spy, offering at most `instructions` instructions per batch.
    pub fn taking_at_most(mut self, instructions: u32) -> Self {
        self.most_instructions = Some(instructions);
        self
    }
}

impl<E: ExecutorSetup> ExecutorSetup for ExecutorSpy<E> {
    fn name(&self) -> &str {
        self.executor.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.executor.accepts()
    }

    fn offers(&self) -> Offer {
        let offer = self.executor.offers();
        match self.most_instructions {
            Some(most) => offer.max_instructions(most),
            None => offer,
        }
    }
}

impl<E: Execute> Execute for ExecutorSpy<E> {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        self.executor.ready(needs).await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.received
            .extend(batches.iter().map(|batch| ReceivedBatch {
                shots: batch.shots.iter().map(ShotId::get).collect(),
                instructions: batch.instructions.len(),
                checked: batch.is_checked(),
            }));
        self.executor.execute(batches, seed, readouts).await
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.executor.end_shots(shots);
    }
}
