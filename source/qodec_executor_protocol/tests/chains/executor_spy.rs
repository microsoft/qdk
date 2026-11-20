// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ExecutorSpy`]: an executor that records each batch it receives before
//! the executor it wraps runs it, may take fewer instructions per batch
//! than that executor, may number its instructions in another order, and
//! may raise conditions on chosen shots of a batch that reads nothing.

use std::sync::Arc;

use executor_protocol::{
    Argument, Batch, BatchReadouts, Execute, ExecuteError, ExecutorSetup, Instruction,
    InstructionSet, Needs, NotReady, Offer, Seed, ShotConditions, ShotId, ShotSet,
};
use qodec_executor_protocol::QodecInstructionSet;
use qodec_runtime_recipes::Isa;

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
    /// The set it accepts in place of the executor's, if it numbers the
    /// same instructions otherwise.
    renumbered: Option<QodecInstructionSet>,
    /// The conditions it raises on a batch with no readout slots, and the
    /// shots, by id, it raises them on.
    raising: ShotConditions,
    raising_on: fn(u32) -> bool,
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
            renumbered: None,
            raising: ShotConditions::NONE,
            raising_on: |_| false,
            received: Vec::new(),
        }
    }

    /// This spy, offering at most `instructions` instructions per batch.
    pub fn taking_at_most(mut self, instructions: u32) -> Self {
        self.most_instructions = Some(instructions);
        self
    }

    /// This spy, accepting `bottom`, the wrapped executor's ISA, with its
    /// instructions numbered in reverse: a part below whose set declares
    /// the same instructions in another order, which the fit maps the
    /// sender's opcodes into.
    pub fn numbering_in_reverse(mut self, bottom: &Isa) -> Self {
        assert!(
            bottom.instructions.len() > 1,
            "precondition: reversing `{}` renumbers its instructions",
            bottom.name
        );
        let mut reversed = bottom.clone();
        reversed.instructions.reverse();
        let renumbered = QodecInstructionSet::new(&Arc::new(reversed))
            .expect("the example's bottom ISA, reversed, is a protocol set");
        self.renumbered = Some(renumbered);
        self
    }

    /// This spy, raising `conditions` on each shot of a batch with no
    /// readout slots whose id `on_shot` picks, after the executor it wraps
    /// runs the batch: a part that finds a shot's work rejected, say,
    /// though the work reads nothing.
    pub fn raising_on_batches_reading_nothing(
        mut self,
        conditions: ShotConditions,
        on_shot: fn(u32) -> bool,
    ) -> Self {
        self.raising = conditions;
        self.raising_on = on_shot;
        self
    }
}

impl<E: ExecutorSetup> ExecutorSetup for ExecutorSpy<E> {
    fn name(&self) -> &str {
        self.executor.name()
    }

    fn accepts(&self) -> &InstructionSet {
        match &self.renumbered {
            Some(renumbered) => renumbered.instruction_set(),
            None => self.executor.accepts(),
        }
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
        match &self.renumbered {
            Some(renumbered) => {
                let inner = self.executor.accepts();
                let instructions: Vec<Vec<Instruction>> = batches
                    .iter()
                    .map(|batch| {
                        batch
                            .instructions
                            .iter()
                            .map(|instruction| {
                                let mnemonic = renumbered
                                    .instruction_set()
                                    .mnemonic(instruction.opcode)
                                    .expect("the fit sends opcodes of the set the spy accepts");
                                Instruction {
                                    opcode: inner
                                        .opcode(mnemonic)
                                        .expect("the spy accepts the executor's instructions"),
                                    ..instruction.clone()
                                }
                            })
                            .collect()
                    })
                    .collect();
                let renumbered_batches: Vec<Batch<'_>> = batches
                    .iter()
                    .zip(&instructions)
                    .map(|(batch, instructions)| {
                        Batch::new(batch.shots, instructions, batch.readout_slots())
                            .checked_if(batch.is_checked())
                    })
                    .collect();
                self.executor
                    .execute(&renumbered_batches, seed, readouts)
                    .await?;
            }
            None => self.executor.execute(batches, seed, readouts).await?,
        }
        for (batch, batch_readouts) in batches
            .iter()
            .zip(readouts)
            .filter(|(_, batch_readouts)| batch_readouts.readout_slots() == 0)
        {
            for (position, shot) in batch.shots.iter().enumerate() {
                if (self.raising_on)(shot.get()) {
                    let conditions = batch_readouts.conditions(position) | self.raising;
                    batch_readouts.set_conditions(position, conditions);
                }
            }
        }
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.executor.end_shots(shots);
    }
}
