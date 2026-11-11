// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Designs: how each layer of a qodec is assembled from mix-and-match pieces.

use qodec_runtime_pieces::{Blueprint, BoxedLayer, LayerModel, PlanError, QodecLayer};
use std::rc::Rc;

/// The blueprint of a whole qodec layer, with its instance boxed. Every
/// [`Blueprint`] of a [`QodecLayer`] is one.
pub trait LayerBlueprint {
    /// A fresh layer for one shot.
    fn instantiate_layer(&self) -> BoxedLayer;
}

impl<B> LayerBlueprint for B
where
    B: Blueprint,
    B::Instance: QodecLayer + 'static,
{
    fn instantiate_layer(&self) -> BoxedLayer {
        Box::new(self.instantiate())
    }
}

/// Chooses how to assemble each layer of a qodec.
///
/// [`QodecPlan::new`](crate::QodecPlan::new) calls [`Self::layer`] once per
/// layer, top first, so different layers may be assembled differently. A
/// closure with the same signature is a design.
pub trait Design {
    /// The blueprint of `layer`.
    fn layer(&self, layer: &Rc<LayerModel>) -> Result<Box<dyn LayerBlueprint>, PlanError>;
}

/// Automatically implement [`Design`] for functions.
impl<F> Design for F
where
    F: Fn(&Rc<LayerModel>) -> Result<Box<dyn LayerBlueprint>, PlanError>,
{
    fn layer(&self, layer: &Rc<LayerModel>) -> Result<Box<dyn LayerBlueprint>, PlanError> {
        self(layer)
    }
}
