// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Designs: how each layer of a qodec is assembled from mix-and-match pieces.

use qodec_runtime_model::{Blueprint, BoxedLayer, LayerModel, PlanError, QodecLayer};
use std::sync::Arc;

/// The blueprint of a whole qodec layer, with its instance boxed. Every
/// [`Blueprint`] of a [`QodecLayer`] is one.
///
/// It is `Send + Sync`, so a [`QodecPlan`](crate::QodecPlan) can be shared
/// by shots on many threads; each shot's layers are its own.
pub trait LayerBlueprint: Send + Sync {
    /// A fresh layer for one shot.
    fn instantiate_layer(&self) -> BoxedLayer;
}

impl<B> LayerBlueprint for B
where
    B: Blueprint + Send + Sync,
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
    fn layer(&self, layer: &Arc<LayerModel>) -> Result<Box<dyn LayerBlueprint>, PlanError>;
}

/// Automatically implement [`Design`] for functions.
impl<F> Design for F
where
    F: Fn(&Arc<LayerModel>) -> Result<Box<dyn LayerBlueprint>, PlanError>,
{
    fn layer(&self, layer: &Arc<LayerModel>) -> Result<Box<dyn LayerBlueprint>, PlanError> {
        self(layer)
    }
}
