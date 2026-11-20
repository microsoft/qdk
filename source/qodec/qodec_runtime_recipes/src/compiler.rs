// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Compiler`]: encoders without decoders, lowering a whole program ahead of
//! time.

use crate::{
    Blueprint, Call, Encoder, Fault, GadgetEncoderPlan, Layer, Lower, Lowering, Never, PlanError,
    QodecModel, Reply, from_fn,
};

/// Encoders that lower one after the other, from a qodec's top ISA to its
/// bottom ISA, with no decoder and no results.
///
/// ```text
///   top-ISA calls
///  ┌▼───────────────────┐
///  │ Encoder<Never> 0   │   records are dropped: no results ever come back
///  └┬───────────────────┘
///   ⋮
///  ┌▼───────────────────┐
///  │ Encoder<Never> n-1 │
///  └┬───────────────────┘
///   bottom-ISA calls
/// ```
///
/// Results never come back, so it lowers only programs whose gadgets don't
/// feed a readout into a later `bit` argument. Such a gadget is adaptive: it
/// needs a decoder at run time. A [`CompilerPlan`] rejects a qodec that has
/// one; encoders passed to [`Compiler::new`] instead fail when a program
/// invokes one.
pub struct Compiler {
    encoders: Vec<Box<dyn Encoder<Never>>>,
}

impl Compiler {
    /// Chains `encoders`, the first on top. They are told to expect no results
    /// (see [`Encoder::expect_no_results`]).
    #[must_use]
    pub fn new(mut encoders: Vec<Box<dyn Encoder<Never>>>) -> Self {
        for encoder in &mut encoders {
            encoder.expect_no_results();
        }
        Self { encoders }
    }

    /// A [`GadgetEncoder`](crate::GadgetEncoder) for every layer
    /// of `model`. To lower many programs, plan once with [`CompilerPlan`].
    pub fn for_model(model: &QodecModel) -> Result<Self, PlanError> {
        CompilerPlan::new(model).map(|plan| plan.instantiate())
    }

    /// Lowers `calls` of the top ISA to calls of the bottom ISA. Blocks stay
    /// live across calls, so a program can be lowered in pieces.
    pub fn lower(&mut self, calls: impl IntoIterator<Item = Call>) -> Result<Vec<Call>, Fault> {
        let mut calls: Vec<Call> = calls.into_iter().collect();
        for (k, encoder) in self.encoders.iter_mut().enumerate() {
            let mut lowered = Vec::new();
            let mut collect = from_fn(|batch: Vec<Call>| -> Result<Vec<Reply>, Fault> {
                lowered.extend(batch);
                Ok(Vec::new())
            });
            encoder
                .execute(calls.into_iter().map(Lower::Call).collect(), &mut collect)
                .map_err(|f| f.at_layer(k))?;
            calls = lowered;
        }
        Ok(calls)
    }
}

/// The blueprint of a [`Compiler`]: the encoders of every layer of a qodec,
/// planned once.
pub struct CompilerPlan {
    encoders: Vec<GadgetEncoderPlan<Never>>,
}

impl CompilerPlan {
    /// Plans a [`GadgetEncoder`](crate::GadgetEncoder) for every layer of
    /// `model`. Fails if a layer has an adaptive gadget, one that feeds a
    /// readout into a later call: it can't be lowered ahead of time.
    pub fn new(model: &QodecModel) -> Result<Self, PlanError> {
        let encoders = model
            .layers
            .iter()
            .enumerate()
            .map(|(k, layer)| {
                let fail = |e: String| PlanError(format!("layer {k}: {e}"));
                let plan = GadgetEncoderPlan::<Never>::plan(layer).map_err(|e| fail(e.0))?;
                if let Some(gadget) = plan.program().instructions.iter().find_map(|l| match l {
                    Lowering::Gadget(gadget) if gadget.is_adaptive() => Some(gadget),
                    _ => None,
                }) {
                    return Err(fail(format!(
                        "gadget `{}` feeds a readout into a later call, so it can't be lowered ahead of time",
                        gadget.name
                    )));
                }
                Ok(plan)
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { encoders })
    }
}

impl Blueprint for CompilerPlan {
    type Instance = Compiler;

    fn instantiate(&self) -> Compiler {
        Compiler::new(
            self.encoders
                .iter()
                .map(|plan| -> Box<dyn Encoder<Never>> { Box::new(plan.instantiate()) })
                .collect(),
        )
    }
}
