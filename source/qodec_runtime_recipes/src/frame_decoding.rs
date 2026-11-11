// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FrameDecoding`]: the reference design.

use qodec_runtime_builder::{
    Design, FrameDecoderPlan, LayerBlueprint, LayerModel, Never, PlanError, SpecEncoderPlan, Stack,
};
use std::rc::Rc;

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
    fn layer(&self, layer: &Rc<LayerModel>) -> Result<Box<dyn LayerBlueprint>, PlanError> {
        Ok(Box::new(Stack::new(
            FrameDecoderPlan::plan(layer)?,
            SpecEncoderPlan::<Never>::plan(layer)?,
        )))
    }
}
