// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ExecutorSpy`]: an executor that records each batch it receives before
//! the executor it wraps runs it, may take fewer instructions per batch
//! than that executor, and may raise conditions on the shots of a batch
//! that reads nothing.

use executor_protocol::{
    Argument, Batch, BatchReadouts, Execute, ExecuteError, ExecutorSetup, InstructionSet, Needs,
    NotReady, Offer, Seed, ShotConditions, ShotId, ShotSet,
};

/// What the spy saw of a batch it received.
#[derive(Clone, Debug, PartialEq)]
pub struct ReceivedBatch {
    /// Its shots' ids, in its set's order.
    pub shots: Vec<u32>,
    /// How many instructions it holds.
    pub instructions: usize,
    /// Whether its sender marked it checked.
    pub checked: bool,
    /// Its readout slots per shot.
    pub readout_slots: u32,
    /// Its instructions' `number` arguments, in order.
    pub numbers: Vec<f64>,
}

/// `executor`, recording each batch it receives.
pub struct ExecutorSpy<E> {
    executor: E,
    most_instructions: Option<u32>,
    /// The conditions it raises on each shot of a batch with no readout
    /// slots.
    raising: ShotConditions,
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
            raising: ShotConditions::NONE,
            received: Vec::new(),
        }
    }

    /// This spy, offering at most `instructions` instructions per batch.
    pub fn taking_at_most(mut self, instructions: u32) -> Self {
        self.most_instructions = Some(instructions);
        self
    }

    /// This spy, raising `conditions` on each shot of a batch with no
    /// readout slots, after the executor it wraps runs the batch: a part
    /// that finds a shot's work rejected, say, though the work reads
    /// nothing.
    pub fn raising_on_batches_reading_nothing(mut self, conditions: ShotConditions) -> Self {
        self.raising = conditions;
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
        self.received.extend(batches.iter().map(|batch| {
            ReceivedBatch {
                shots: batch.shots.iter().map(ShotId::get).collect(),
                instructions: batch.instructions.len(),
                checked: batch.is_checked(),
                readout_slots: batch.readout_slots(),
                numbers: batch
                    .instructions
                    .iter()
                    .flat_map(|instruction| &instruction.arguments)
                    .filter_map(|argument| match argument {
                        Argument::Number(number) => Some(*number),
                        _ => None,
                    })
                    .collect(),
            }
        }));
        self.executor.execute(batches, seed, readouts).await?;
        for batch_readouts in readouts
            .iter_mut()
            .filter(|batch_readouts| batch_readouts.readout_slots() == 0)
        {
            for position in 0..batch_readouts.shots() {
                let conditions = batch_readouts.conditions(position) | self.raising;
                batch_readouts.set_conditions(position, conditions);
            }
        }
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.executor.end_shots(shots);
    }
}
