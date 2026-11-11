// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`QodecPlan`]: a qodec's layers, each assembled by a [`Design`], from which
//! fresh layers are built per shot.

use super::design::{Design, LayerBlueprint};
use qodec_runtime_pieces::{
    Backend, Call, Fault, Isa, Layer, Link, PlanError, QodecLayer, QodecLayers, QodecModel, Reply,
};
use std::rc::Rc;

/// The plan of a whole qodec: a blueprint per layer, from which
/// [`Self::instantiate`] builds fresh layers per shot.
///
/// It only assembles layers. To run a shot, stack them on a
/// [`Runtime`](qodec_runtime_pieces::Runtime) over a bottom backend:
///
/// ```text
/// let model = qodec_runtime_reader::read(&qodec)?;
/// let plan = QodecPlan::new(&model, &design)?;
/// let mut shot = Runtime::new(plan.instantiate(), backend, plan.top(), plan.bottom());
/// ```
pub struct QodecPlan {
    blueprints: Vec<Box<dyn LayerBlueprint>>,
    top: Rc<Isa>,
    bottom: Rc<Isa>,
}

impl QodecPlan {
    /// Has `design` assemble every layer of `model`, top first.
    pub fn new(model: &QodecModel, design: &dyn Design) -> Result<Self, PlanError> {
        let blueprints = model
            .layers
            .iter()
            .enumerate()
            .map(|(k, layer)| {
                design
                    .layer(layer)
                    .map_err(|e| PlanError(format!("layer {k}: {e}")))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            blueprints,
            top: Rc::clone(&model.top),
            bottom: Rc::clone(&model.bottom),
        })
    }

    /// The top ISA, which the layers accept.
    #[must_use]
    pub fn top(&self) -> &Rc<Isa> {
        &self.top
    }

    /// The bottom ISA, which the layers emit.
    #[must_use]
    pub fn bottom(&self) -> &Rc<Isa> {
        &self.bottom
    }

    /// Fresh layers for one shot, chained top first. Each tags its faults
    /// with its index in the qodec. With no layers, the chain passes calls
    /// straight down.
    #[must_use]
    pub fn instantiate(&self) -> QodecLayers {
        let links = self
            .blueprints
            .iter()
            .enumerate()
            .map(
                |(index, blueprint)| -> Link<Vec<Call>, Result<Vec<Reply>, Fault>> {
                    Box::new(AtLayer {
                        index,
                        layer: blueprint.instantiate_layer(),
                    })
                },
            )
            .collect();
        QodecLayers::new(links)
    }
}

/// A qodec layer that tags its faults with its index in the qodec.
pub struct AtLayer<L> {
    /// The layer's index in the qodec, top first.
    pub index: usize,
    /// The layer.
    pub layer: L,
}

impl<L: QodecLayer> Layer for AtLayer<L> {
    type Input = Vec<Call>;
    type Output = Result<Vec<Reply>, Fault>;
    type LowerInput = Vec<Call>;
    type LowerOutput = Result<Vec<Reply>, Fault>;

    fn execute(
        &mut self,
        calls: Vec<Call>,
        below: &mut dyn Backend<Vec<Call>, Result<Vec<Reply>, Fault>>,
    ) -> Result<Vec<Reply>, Fault> {
        self.layer
            .execute(calls, below)
            .map_err(|fault| fault.at_layer(self.index))
    }
}
