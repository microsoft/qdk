// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Designs: how each layer of a qodec is assembled from mix-and-match pieces.

use super::{
    layer::{Blueprint, Stack},
    plan::{LayerPlan, PlanError},
    protocol::{BoxedLayer, Never},
    runtime::QodecLayer,
};
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
    fn layer(&self, layer: &Rc<LayerPlan>) -> Result<Box<dyn LayerBlueprint>, PlanError>;
}

impl<F> Design for F
where
    F: Fn(&Rc<LayerPlan>) -> Result<Box<dyn LayerBlueprint>, PlanError>,
{
    fn layer(&self, layer: &Rc<LayerPlan>) -> Result<Box<dyn LayerBlueprint>, PlanError> {
        self(layer)
    }
}

/// The reference design: in every layer, the Pauli-frame decoder on top of the
/// spec encoder. Neither pends nor corrects, so no layer needs a controller.
///
/// ```text
///   │ Call                       │ Reply
///  ┌▼────────────────────────────┴┐
///  │ Passive<FrameDecoder>        │
///  └┬────────────────────────────▲┘
///   │ Lower<Never> = Call        │ Record
///  ┌▼────────────────────────────┴┐
///  │ SpecEncoder<Never>           │
///  └┬────────────────────────────▲┘
///   │ Call                       │ Reply
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameDecoding;

impl Design for FrameDecoding {
    fn layer(&self, layer: &Rc<LayerPlan>) -> Result<Box<dyn LayerBlueprint>, PlanError> {
        Ok(Box::new(Stack::new(
            layer.frame_decoder()?,
            layer.spec_encoder::<Never>()?,
        )))
    }
}
