// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FrameDecoding`]: the reference design.

use crate::{
    Design, FrameDecoderPlan, GadgetEncoderPlan, LayerBlueprint, LayerModel, Never, PlanError,
    Stack,
};
use std::sync::Arc;

/// The reference design: in every layer, the Pauli-frame decoder on top of the
/// gadget encoder. Neither pends nor corrects, so no layer needs a controller.
///
/// ```text
///   │ Call                       │ Reply
///  ┌▼────────────────────────────┴┐
///  │ Passive<FrameDecoder>        │
///  └┬────────────────────────────▲┘
///   │ Lower<Never> = Call        │ Record
///  ┌▼────────────────────────────┴┐
///  │ GadgetEncoder<Never>         │
///  └┬────────────────────────────▲┘
///   │ Call                       │ Reply
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameDecoding;

impl Design for FrameDecoding {
    fn layer(&self, layer: &Arc<LayerModel>) -> Result<Box<dyn LayerBlueprint>, PlanError> {
        Ok(Box::new(Stack::new(
            FrameDecoderPlan::plan(layer)?,
            GadgetEncoderPlan::<Never>::plan(layer)?,
        )))
    }
}
