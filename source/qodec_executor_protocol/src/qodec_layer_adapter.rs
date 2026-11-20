// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`QodecLayerAdapter`]: a run of consecutive layers of a qodec as an
//! adapter, accepting the ISA above its first layer and sending the ISA
//! below its last.
//!
//! A user brackets a qodec's layers as their chain needs: all of them in
//! one adapter, or one adapter per layer, so that a part (a debugger, a
//! noise model, a counter) sits between two layers. Each bracketing gives
//! the same readouts, since each layer sees the same calls.
//!
//! Each shot has its own layers, built from the qodec's plan at the shot's
//! first batch and dropped when the shot ends, as Qodec's runtime does for
//! a shot it runs directly. Each batch runs through them shot by shot: the
//! layers call the part below as often as they need, with one-shot
//! batches, each split to the most instructions the part below takes, and
//! wait for each answer (`below_bridge.rs` says why they wait by
//! blocking).

mod below_bridge;
mod shot_layers;

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;

use executor_protocol::{
    Adapt, AdapterSetup, Batch, BatchReadouts, BlockType, DeclarationFault, Execute, ExecuteError,
    ExecuteRefusal, Failure, InstructionSet, Needs, NotReady, Offer, OpcodeMap, Readouts, Seed,
    ShotConditions, ShotId, ShotSet,
};
use qodec_runtime_recipes::{
    Backend as _, Call, Fault, FrameDecoding, PlanError, QodecLayers, QodecModel, QodecPlan,
    QodecRuntimeError, Resources, Runtime,
};

use crate::QodecInstructionSet;
use crate::qodec_instruction_set::{LiveBlocks, write_results};
use below_bridge::BelowBridge;
use shot_layers::ShotLayers;

/// Layers `layers` of a qodec, top first, run per shot as Qodec's runtime
/// runs them, each layer assembled by the reference design
/// ([`FrameDecoding`]).
pub struct QodecLayerAdapter {
    name: String,
    /// The layers' indices in the whole qodec.
    layers: Range<usize>,
    /// The layers alone, as a qodec of their own.
    model: QodecModel,
    plan: QodecPlan,
    accepts: QodecInstructionSet,
    sends: QodecInstructionSet,
    /// The set below's opcodes, once `>>` binds them.
    below: Option<OpcodeMap>,
    shots: BTreeMap<ShotId, ShotState>,
    readouts_below: Readouts,
}

/// A shot's layers, its live blocks above them, and whether an error lost
/// the shot.
struct ShotState {
    layers: QodecLayers,
    live: LiveBlocks,
    lost: bool,
}

impl QodecLayerAdapter {
    /// Layers `layers` of `qodec`, top first: `0..1` is its top layer.
    ///
    /// # Errors
    ///
    /// When `layers` is empty or reaches past the qodec's layers, when the
    /// reference design cannot assemble one of them, or when the ISA above
    /// or below them is not a protocol instruction set.
    pub fn new(qodec: &QodecModel, layers: Range<usize>) -> Result<Self, QodecLayerAdapterError> {
        if layers.is_empty() || layers.end > qodec.layers.len() {
            return Err(QodecLayerAdapterError::NoSuchLayers {
                layers,
                qodec_layers: qodec.layers.len(),
            });
        }
        let models = qodec.layers[layers.clone()].to_vec();
        let model = QodecModel {
            top: models[0].upper.clone(),
            bottom: models[models.len() - 1].lower.clone(),
            layers: models,
        };
        let plan = QodecPlan::new(&model, &FrameDecoding).map_err(QodecLayerAdapterError::Plan)?;
        let set = |isa| {
            QodecInstructionSet::new(isa).map_err(|faults| QodecLayerAdapterError::InstructionSet {
                isa: isa.name.clone(),
                faults,
            })
        };
        let accepts = set(&model.top)?;
        let sends = set(&model.bottom)?;
        Ok(Self {
            name: format!(
                "Qodec layers {}..{} ({} to {})",
                layers.start, layers.end, model.top.name, model.bottom.name
            ),
            layers,
            model,
            plan,
            accepts,
            sends,
            below: None,
            shots: BTreeMap::new(),
            readouts_below: Readouts::new(),
        })
    }

    /// Every layer of `qodec`.
    ///
    /// # Errors
    ///
    /// As [`new`](Self::new).
    pub fn whole(qodec: &QodecModel) -> Result<Self, QodecLayerAdapterError> {
        Self::new(qodec, 0..qodec.layers.len())
    }

    /// The needs below the layers for a program that needs `above`: the
    /// blocks Qodec's model says the layers lower them to.
    fn needs_for(&self, above: &Needs) -> Result<Needs, PlanError> {
        let top = Resources::blocks(
            above
                .blocks
                .iter()
                .map(|(block_type, &n)| (block_type.name().to_owned(), n as usize)),
        );
        let resources = self.model.required_resources(top)?;
        let lower = resources.last().expect("the top's resources, at least");
        let types = &self.model.bottom.types;
        let mut needs = Needs::default();
        let count = |n: usize| u32::try_from(n).map_err(|_| PlanError::from("over 2^32 blocks"));
        if lower.qubits > 0 {
            let only = types.only().ok_or_else(|| {
                PlanError::from(format!(
                    "the layers lower to {} qubits, and the ISA below has {} block types, \
                     so which type holds them is not known",
                    lower.qubits,
                    types.names.len()
                ))
            })?;
            needs = needs.blocks(
                BlockType::new(types.names[only].clone()),
                count(lower.qubits)?,
            );
        }
        for (name, &n) in &lower.blocks {
            if n > 0 {
                needs = needs.blocks(BlockType::new(name.clone()), count(n)?);
            }
        }
        if above.replayable {
            needs = needs.with_replay();
        }
        Ok(needs)
    }
}

impl AdapterSetup for QodecLayerAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn accepts(&self) -> &InstructionSet {
        self.accepts.instruction_set()
    }

    fn sends(&self) -> &InstructionSet {
        self.sends.instruction_set()
    }

    /// The blocks the layers lower `above`'s to. When Qodec's model cannot
    /// say, none: [`ready`](Adapt::ready) then answers why.
    fn needs_below(&self, above: Needs) -> Needs {
        self.needs_for(&above).unwrap_or_else(|_| {
            let needs = Needs::default();
            if above.replayable {
                needs.with_replay()
            } else {
                needs
            }
        })
    }

    /// Any number of blocks of each type of the ISA the layers accept: how
    /// many the part below holds is checked at its own boundary, against
    /// what the layers lower them to. Any number of instructions per batch:
    /// the layers lower them to batches of their own, which the adapter
    /// splits to the most the part below takes. No readout parity: Qodec's
    /// calls take values. The shots, seeding and replay of the part below.
    fn offers_above(&self, below: Offer) -> Offer {
        let mut above = below;
        above.max_blocks = self
            .model
            .top
            .types
            .names
            .iter()
            .map(|name| (BlockType::new(name.clone()), None))
            .collect();
        above.max_instructions = None;
        above.resolves_readout_parity = false;
        above
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }
}

impl Adapt for QodecLayerAdapter {
    #[expect(
        clippy::unused_async_trait_impl,
        reason = "a part that never waits keeps the trait's `async fn` form"
    )]
    async fn ready(&mut self, above: &Needs) -> Result<(), NotReady> {
        self.needs_for(above).map(|_| ()).map_err(NotReady::new)
    }

    /// Runs each batch through each of its shots' layers, in order. A shot
    /// an earlier error lost, in any batch, is refused before anything
    /// runs. An error below, or a layer's fault, fails the batch at that
    /// shot and loses it; a refusal below too, since the layers may have
    /// run part of the shot's batch before it. A call with the wrong
    /// arguments for its instruction, which Qodec's runtime rejects before
    /// running it, or with an argument Qodec's calls have no form for,
    /// such as a readout parity, which the layers cannot translate, fails
    /// the batch without losing any of its shots; only an unchecked batch
    /// can carry one. A call that would prepare a block at the index of a
    /// live block of another type, which Qodec's one label per block cannot
    /// tell apart ([`TranslationFault`](crate::TranslationFault)), checked
    /// or not, fails the batch at that shot, before any of the batch runs on
    /// it, without losing it. A call on a block that is not live is a fault
    /// of the layers, which loses the shot, as in Qodec's runtime; Qodec's
    /// drivers reject such a batch first, with their `TypeChecker`, which
    /// is not ported. An unchecked batch's instruction of another set, or
    /// readout slots fewer than its instructions read, panics, as the core
    /// lets a receiver of an unchecked batch do.
    ///
    /// Each shot's conditions are those its layers' results raise, and
    /// those the part below raises on any of the shot's batches below,
    /// whether or not that batch reads a value.
    #[expect(
        clippy::unused_async_trait_impl,
        reason = "the layers wait for the part below by blocking, so `adapt` never awaits"
    )]
    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
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
        let map = self
            .below
            .as_ref()
            .expect("`>>` binds an adapter before it runs");
        let most_instructions = below.offers().max_instructions;
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
                let state = self.shots.entry(shot).or_insert_with(|| ShotState {
                    layers: self.plan.instantiate(),
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
                let one_shot = ShotSet::ids([shot.get()]);
                let mut raised = ShotConditions::NONE;
                let outcome = {
                    let bridge = BelowBridge {
                        below: &mut *below,
                        shot: &one_shot,
                        seed,
                        checked: batch.is_checked(),
                        sends: &self.sends,
                        map,
                        readouts: &mut self.readouts_below,
                        raised: &mut raised,
                        most_instructions,
                    };
                    Runtime::new(
                        ShotLayers(&mut state.layers),
                        bridge,
                        self.plan.top(),
                        self.plan.bottom(),
                    )
                    .execute(calls)
                };
                match outcome {
                    Ok(results) => {
                        let conditions = write_results(&results, batch_readouts.row_mut(position));
                        batch_readouts.set_conditions(position, conditions | raised);
                        state.live = live;
                    }
                    Err(error) => {
                        state.lost = !matches!(error, QodecRuntimeError::Rejected(_));
                        return Err(self.error_of(shot, error));
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

impl QodecLayerAdapter {
    /// The protocol's error for `error`, the layers' on `shot`.
    fn error_of(&self, shot: ShotId, error: QodecRuntimeError<ExecuteError>) -> ExecuteError {
        let at_layer = |fault: Fault| Fault {
            layer: fault.layer.map(|k| self.layers.start + k),
            ..fault
        };
        match error {
            // The layers ran part of the batch before the part below
            // refused theirs, so the batch failed rather than was refused.
            QodecRuntimeError::Bottom(ExecuteError::Refused(refusal)) => ExecuteError::Failed {
                shot,
                failure: Failure::Refused(refusal),
            },
            QodecRuntimeError::Bottom(error) => error,
            QodecRuntimeError::Rejected(fault) | QodecRuntimeError::Fault(fault) => {
                ExecuteError::Failed {
                    shot,
                    failure: Failure::Part(Box::new(at_layer(fault))),
                }
            }
            QodecRuntimeError::Poisoned => unreachable!(
                "a runtime is made for each shot of each `adapt`, and runs once, after the \
                 shots an earlier error lost are refused, so it is never poisoned"
            ),
        }
    }
}

/// Why a run of a qodec's layers cannot be an adapter.
#[derive(Debug)]
#[non_exhaustive]
pub enum QodecLayerAdapterError {
    /// The layers are none, or reach past the qodec's.
    NoSuchLayers {
        /// The layers asked for.
        layers: Range<usize>,
        /// How many layers the qodec has.
        qodec_layers: usize,
    },
    /// The reference design cannot assemble a layer.
    Plan(PlanError),
    /// The ISA above or below the layers is not a protocol instruction set.
    InstructionSet {
        /// The ISA's name.
        isa: String,
        /// What the protocol finds in it.
        faults: Vec<DeclarationFault>,
    },
}

impl fmt::Display for QodecLayerAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchLayers {
                layers,
                qodec_layers,
            } => write!(
                f,
                "layers {}..{} are not a run of the qodec's {qodec_layers} layers",
                layers.start, layers.end
            ),
            Self::Plan(error) => write!(f, "{error}"),
            Self::InstructionSet { isa, faults } => {
                write!(f, "ISA `{isa}` is not a protocol instruction set:")?;
                for fault in faults {
                    write!(f, " {fault};")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for QodecLayerAdapterError {}
