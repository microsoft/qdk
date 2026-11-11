// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Controllers: what answers a decoder that is not ready to give a result.

use super::{
    backend::Backend,
    error::PlanError,
    isa::Operand,
    layer::{Blueprint, Layer},
    layer_model::LayerModel,
    protocol::{BlockRef, Call, Fault, OwnedParam, Reply},
};
use rustc_hash::FxHashMap;
use smallvec::smallvec;
use std::rc::Rc;

/// What a decoder asks [`UntilReady`] for: one more QEC round on each of
/// these live blocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rounds {
    /// The blocks to run a round on.
    pub blocks: Vec<BlockRef>,
}

/// The plan of an [`UntilReady`] controller: which upper instruction runs a QEC
/// round on a block of each type, and how many rounds to add at most before
/// giving up.
#[derive(Clone, Debug)]
pub struct UntilReadyPlan {
    rounds: Rc<FxHashMap<String, usize>>,
    max_rounds: usize,
}

impl UntilReadyPlan {
    /// Runs `rounds[block type]`, an upper instruction id, for a round on a
    /// block of that type.
    pub fn new(rounds: impl IntoIterator<Item = (String, usize)>, max_rounds: usize) -> Self {
        Self {
            rounds: Rc::new(rounds.into_iter().collect()),
            max_rounds,
        }
    }

    /// Plans for `layer`, adding at most `max_rounds` rounds in a row. Each
    /// block type's QEC round is found by its action, which the spec makes
    /// authoritative: an instruction with a gadget that takes one block of the
    /// type and gives it back, with an empty action (the identity), no
    /// parameters and no flags. The first such instruction wins. Fails if a
    /// block type of the layer has none.
    pub fn plan(layer: &LayerModel, max_rounds: usize) -> Result<Self, PlanError> {
        let upper = &layer.upper;
        let mut rounds = Vec::new();
        for (t, name) in upper.types.names.iter().enumerate() {
            let index = upper
                .instructions
                .iter()
                .enumerate()
                .position(|(i, instruction)| {
                    matches!(
                        instruction.inputs.as_slice(),
                        [Operand { block, variadic: false }] if *block == t
                    ) && instruction.outputs == instruction.inputs
                        && instruction.action.is_empty()
                        && instruction.parameters.is_empty()
                        && instruction.flags == 0
                        && layer.gadget_of[i].is_some()
                })
                .ok_or_else(|| PlanError(format!("no instruction runs a QEC round on `{name}`")))?;
            rounds.push((name.clone(), index));
        }
        Ok(Self::new(rounds, max_rounds))
    }
}

impl Blueprint for UntilReadyPlan {
    type Instance = UntilReady;

    fn instantiate(&self) -> UntilReady {
        UntilReady { plan: self.clone() }
    }
}

/// A [`Controller`](crate::protocol::Controller) that adds QEC rounds until the
/// decoder below is ready.
///
/// ```text
///   │ Vec<Call>   upper ISA                ▲ Result<Vec<Reply>, Fault>  final, in order
///  ┌▼──────────────────────────────────────┴──────────────────────────────────┐
///  │ UntilReady                                                               │
///  │   while the answer ends in Pending(Rounds): one round on each block,     │
///  │   at most `max_rounds` times in a row                                    │
///  └┬──────────────────────────────────────▲──────────────────────────────────┘
///   │ Vec<Call>   then QEC rounds          │ Result<Vec<Reply<Rounds>>, Fault>
/// ```
///
/// The decoder decides when it is ready; this only decides how to get it more
/// information. A decoder that should give up must do so itself; past
/// `max_rounds` requests in a row, the controller fails the shot.
pub struct UntilReady {
    plan: UntilReadyPlan,
}

impl Layer for UntilReady {
    type Input = Vec<Call>;
    type Output = Result<Vec<Reply>, Fault>;
    type LowerInput = Vec<Call>;
    type LowerOutput = Result<Vec<Reply<Rounds>>, Fault>;

    fn execute(
        &mut self,
        calls: Vec<Call>,
        below: &mut dyn Backend<Vec<Call>, Result<Vec<Reply<Rounds>>, Fault>>,
    ) -> Result<Vec<Reply>, Fault> {
        let mut results = Vec::new();
        let mut batch = calls;
        let mut added = 0;
        loop {
            let mut pending = None;
            for reply in below.execute(batch)? {
                match reply {
                    Reply::Ready(readout) if pending.is_none() => {
                        results.push(Reply::Ready(readout));
                        added = 0;
                    }
                    Reply::Pending(rounds) if pending.is_none() => pending = Some(rounds),
                    _ => return Err(Fault::protocol("a reply after `Pending`")),
                }
            }
            let Some(rounds) = pending else {
                return Ok(results);
            };
            if added == self.plan.max_rounds {
                return Err(Fault::protocol(format!(
                    "the decoder is still pending after {} rounds",
                    self.plan.max_rounds
                )));
            }
            added += 1;
            batch = rounds
                .blocks
                .iter()
                .map(|block| {
                    let &instruction =
                        self.plan.rounds.get(&block.block_type).ok_or_else(|| {
                            Fault::no_implementation(format!(
                                "no QEC round for block type `{}`",
                                block.block_type
                            ))
                        })?;
                    Ok(Call {
                        instruction_id: instruction,
                        parameters: smallvec![OwnedParam::Block(block.label)],
                    })
                })
                .collect::<Result<_, Fault>>()?;
        }
    }
}
