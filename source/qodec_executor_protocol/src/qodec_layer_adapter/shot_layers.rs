// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ShotLayers`]: a shot's layers, borrowed, as one [`Layer`].
//!
//! Qodec's [`Runtime`](qodec_runtime_recipes::Runtime) owns the layer it
//! runs, and Qodec implements [`Layer`] for a boxed layer but not for a
//! borrowed one. The adapter keeps each shot's layers from batch to batch,
//! and builds a runtime over them for each batch, since the runtime also
//! owns the part below, which the adapter borrows only for one `adapt`.

use qodec_runtime_recipes::{Backend, Call, Fault, Layer, QodecLayers, Reply};

/// A shot's layers, borrowed for one batch.
pub(super) struct ShotLayers<'a>(pub(super) &'a mut QodecLayers);

impl Layer for ShotLayers<'_> {
    type Input = Vec<Call>;
    type Output = Result<Vec<Reply>, Fault>;
    type LowerInput = Vec<Call>;
    type LowerOutput = Result<Vec<Reply>, Fault>;

    fn execute(
        &mut self,
        calls: Vec<Call>,
        below: &mut dyn Backend<Vec<Call>, Result<Vec<Reply>, Fault>>,
    ) -> Result<Vec<Reply>, Fault> {
        self.0.execute(calls, below)
    }
}
