// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CountingExecutor`]: an executor that counts the batches it receives
//! before the executor it wraps runs them, and may take fewer instructions
//! per batch than that executor, as a device with a small instruction
//! memory does.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use executor_protocol::{
    Batch, BatchReadouts, Execute, ExecuteError, ExecutorSetup, InstructionSet, Needs, NotReady,
    Offer, Seed, ShotSet,
};

/// `executor`, counting each batch it receives.
pub struct CountingExecutor<E> {
    executor: E,
    most_instructions: Option<u32>,
    batches: Arc<AtomicUsize>,
}

impl<E> CountingExecutor<E> {
    /// `executor`, offering what it offers.
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            most_instructions: None,
            batches: Arc::default(),
        }
    }

    /// This executor, offering at most `instructions` instructions per
    /// batch.
    pub fn taking_at_most(mut self, instructions: u32) -> Self {
        self.most_instructions = Some(instructions);
        self
    }

    /// The count of batches received so far, readable while a chain holds
    /// the executor.
    pub fn batches(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.batches)
    }
}

impl<E: ExecutorSetup> ExecutorSetup for CountingExecutor<E> {
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

impl<E: Execute> Execute for CountingExecutor<E> {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        self.executor.ready(needs).await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        self.batches.fetch_add(batches.len(), Ordering::Relaxed);
        self.executor.execute(batches, seed, readouts).await
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.executor.end_shots(shots);
    }
}
