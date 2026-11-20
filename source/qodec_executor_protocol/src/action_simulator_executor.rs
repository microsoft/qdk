// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ActionSimulatorExecutor`]: a qodec's bottom ISA, run on
//! `qdk_simulators`' action simulators, as an executor.
//!
//! Each shot has its own simulator under Qodec's
//! [`ActionTranslator`], which turns each bottom-ISA call into the steps of
//! its Qodec action, as Qodec's runtime runs a shot directly. The shot's
//! simulator draws from the shot's stream of the run's seed
//! ([`Seed::shot`]), so a shot gives the same readouts alone, with others,
//! and driven directly with a simulator seeded with that stream.

use std::collections::BTreeMap;
use std::sync::Arc;

use executor_protocol::{
    Batch, BatchReadouts, BlockType, DeclarationFault, Execute, ExecuteError, ExecuteRefusal,
    ExecutorSetup, Failure, InstructionSet, Offer, Seed, ShotId, ShotSet,
};
use qdk_simulators::action_simulators::{FullStateActionSimulator, StabilizerActionSimulator};
use qodec_runtime_recipes::{ActionOp, ActionTranslator, Backend, Call, Isa, Stack};

use crate::QodecInstructionSet;
use crate::qodec_instruction_set::{LiveBlocks, write_results};

/// The engine under the translator: an action simulator.
type ActionEngine = Box<dyn Backend<Vec<ActionOp>, Result<Vec<bool>, String>>>;

/// A qodec's bottom ISA, run per shot on an action simulator.
pub struct ActionSimulatorExecutor {
    name: &'static str,
    accepts: QodecInstructionSet,
    offer: Offer,
    /// The simulator of a shot, from the shot's stream.
    engine_for_shot: fn(u64, usize) -> ActionEngine,
    /// The qubits a full-state simulator holds.
    qubits: usize,
    shots: BTreeMap<ShotId, ShotSimulator>,
}

/// A shot's translator and simulator, its live blocks, and whether an
/// error lost the shot.
struct ShotSimulator {
    stack: Stack<ActionTranslator, ActionEngine>,
    live: LiveBlocks,
    lost: bool,
}

impl ActionSimulatorExecutor {
    /// `bottom` on a stabilizer simulator: any number of qubits, and
    /// rotations by multiples of π/2 only.
    ///
    /// # Errors
    ///
    /// The faults the protocol finds in `bottom`'s declaration.
    pub fn stabilizer(bottom: &Arc<Isa>) -> Result<Self, Vec<DeclarationFault>> {
        let accepts = QodecInstructionSet::new(bottom)?;
        let offer = offer(bottom, |_| None);
        Ok(Self {
            name: "Qodec stabilizer action simulator",
            accepts,
            offer,
            engine_for_shot: |stream, _| Box::new(StabilizerActionSimulator::new(stream)),
            qubits: 0,
            shots: BTreeMap::new(),
        })
    }

    /// `bottom` on a state-vector simulator of `qubits` qubits: any
    /// rotation. A block holds as many of them as its type's capacity.
    ///
    /// Each block type is offered `qubits` over its capacity on its own, so
    /// for a `bottom` of several types, the blocks a shot holds together may
    /// need more qubits than the simulator has, which fails the shot when it
    /// takes them; Qodec's bottom ISAs have one type. Each shot holds its own
    /// simulator, of `2^qubits` amplitudes of 16 bytes, from its first batch
    /// until its end, and the offer bounds no shots: a run of 2000 shots of
    /// 20 qubits that ends its shots only at the run's end holds 32 GiB.
    ///
    /// # Errors
    ///
    /// The faults the protocol finds in `bottom`'s declaration.
    pub fn full_state(bottom: &Arc<Isa>, qubits: u32) -> Result<Self, Vec<DeclarationFault>> {
        let accepts = QodecInstructionSet::new(bottom)?;
        let offer = offer(bottom, |cap| {
            Some(qubits / u32::try_from(cap).expect("a block holds fewer than 2^32 qubits"))
        });
        Ok(Self {
            name: "Qodec full-state action simulator",
            accepts,
            offer,
            engine_for_shot: full_state_engine,
            qubits: usize::try_from(qubits).expect("a `u32` fits a `usize`"),
            shots: BTreeMap::new(),
        })
    }
}

/// A state-vector simulator of `qubits` qubits, seeded with the low half of
/// `stream`, since it takes a 32-bit seed.
fn full_state_engine(stream: u64, qubits: usize) -> ActionEngine {
    #[expect(clippy::cast_possible_truncation, reason = "the stream's low half")]
    let seed = stream as u32;
    Box::new(FullStateActionSimulator::new(qubits, seed))
}

/// The offer of an executor of `bottom`: every block type of it, as many as
/// `max` says for its capacity; any number of shots and instructions,
/// seeded, replayable; no readout parity, since Qodec's calls take values.
fn offer(bottom: &Isa, max: impl Fn(usize) -> Option<u32>) -> Offer {
    let mut offer = Offer::unlimited().without_readout_parity();
    offer.max_blocks = bottom
        .types
        .names
        .iter()
        .zip(&bottom.types.caps)
        .map(|(name, &cap)| (BlockType::new(name.clone()), max(cap)))
        .collect();
    offer
}

impl ExecutorSetup for ActionSimulatorExecutor {
    fn name(&self) -> &str {
        self.name
    }

    fn accepts(&self) -> &InstructionSet {
        self.accepts.instruction_set()
    }

    fn offers(&self) -> Offer {
        self.offer.clone()
    }
}

impl Execute for ActionSimulatorExecutor {
    /// Runs each batch on each of its shots' simulators, in order. A shot
    /// an earlier error lost, in any batch, is refused before anything
    /// runs. A step the simulator cannot run fails the batch at that shot,
    /// and loses it. An instruction with an argument Qodec's calls have no
    /// form for, such as a readout parity, fails its batch before any of
    /// the batch's shots runs, and loses none; only an unchecked batch can
    /// carry one. An instruction that would prepare a block at the index of
    /// a live block of another type, which Qodec's one label per block
    /// cannot tell apart ([`TranslationFault`](crate::TranslationFault)),
    /// checked or not, fails the batch at that shot, before any of the
    /// batch runs on it, without losing it.
    #[expect(
        clippy::unused_async_trait_impl,
        reason = "a part that never waits keeps the trait's `async fn` form"
    )]
    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        for batch in batches {
            if let Some(shot) = batch
                .shots
                .iter()
                .find(|shot| self.shots.get(shot).is_some_and(|state| state.lost))
            {
                return Err(ExecuteError::Refused(ExecuteRefusal::EarlierFailure(shot)));
            }
        }
        for (batch, batch_readouts) in batches.iter().zip(readouts) {
            let calls = batch
                .instructions
                .iter()
                .map(|instruction| self.accepts.call(instruction))
                .collect::<Result<Vec<Call>, _>>();
            for (position, shot) in batch.shots.iter().enumerate() {
                let calls = match &calls {
                    Ok(calls) => calls.clone(),
                    Err(fault) => {
                        return Err(ExecuteError::Failed {
                            shot,
                            failure: Failure::Part(Box::new(fault.clone())),
                        });
                    }
                };
                let state = self.shots.entry(shot).or_insert_with(|| ShotSimulator {
                    stack: Stack::new(
                        ActionTranslator::new(self.accepts.isa()),
                        (self.engine_for_shot)(seed.shot(shot), self.qubits),
                    ),
                    live: LiveBlocks::default(),
                    lost: false,
                });
                let live = state
                    .live
                    .after(self.accepts.isa(), &calls)
                    .map_err(|fault| ExecuteError::Failed {
                        shot,
                        failure: Failure::Part(Box::new(fault)),
                    })?;
                match state.stack.execute(calls) {
                    Ok(results) => {
                        let conditions = write_results(&results, batch_readouts.row_mut(position));
                        batch_readouts.set_conditions(position, conditions);
                        state.live = live;
                    }
                    Err(error) => {
                        state.lost = true;
                        return Err(ExecuteError::Failed {
                            shot,
                            failure: Failure::Part(error.into()),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        for shot in shots.iter() {
            self.shots.remove(&shot);
        }
    }
}
