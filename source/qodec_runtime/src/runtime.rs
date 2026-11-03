// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The qodec runtime.
//!
//! A [`QodecRuntime`] is the runtime of one shot: a fresh [`LayerRuntime`] per
//! layer on top of a bottom backend supplied by the caller, which is any
//! [`Backend`] for the bottom ISA, such as a simulator, tracer, or profiler. It
//! is instantiated from a [`QodecPlan`](super::plan::QodecPlan). Dropping it
//! ends the shot.

use super::{
    backend::{
        Backend, BackendInstruction, BatchResult, InstructionId, Param, QodecError, Readout,
    },
    decoder::DecoderPlan,
    layer_runtime::LayerRuntime,
    plan::LayerPlan,
};
use std::{convert::Infallible, fmt, rc::Rc};

/// A member of the runtime with a flattened error. The number of layers is
/// only known at run time, so the runtime is type-erased.
type DynBackend<'a, Id, P, R, E> = Box<dyn Backend<Id, P, R, Error = QodecRuntimeError<E>> + 'a>;

/// The runtime of one shot. It accepts top-ISA instructions. Dropping it ends
/// the shot.
pub struct QodecRuntime<'a, Id, P, R, E> {
    top: DynBackend<'a, Id, P, R, E>,
}

impl<'a, Id, P, R, E> QodecRuntime<'a, Id, P, R, E>
where
    Id: InstructionId + 'a,
    P: Param + 'a,
    R: Readout + 'static,
    E: 'a,
{
    /// Instantiates fresh layer and decoder runtimes over `backend`, bottom up.
    pub(crate) fn new<B>(
        layer_plans: &[Rc<LayerPlan>],
        decoder_plans: &[Box<dyn DecoderPlan<R>>],
        backend: B,
    ) -> Self
    where
        B: Backend<Id, P, R, Error = E> + 'a,
    {
        let mut top: DynBackend<'a, Id, P, R, E> = Box::new(BottomNode(backend));
        for depth in (0..layer_plans.len()).rev() {
            let layer = LayerRuntime::new(
                Rc::clone(&layer_plans[depth]),
                decoder_plans[depth].as_ref(),
                top,
            );
            top = Box::new(LayerNode { layer, depth });
        }
        Self { top }
    }
}

impl<Id, P, R, E> Backend<Id, P, R> for QodecRuntime<'_, Id, P, R, E> {
    type Error = QodecRuntimeError<E>;

    fn execute_batch(
        &mut self,
        instructions: &[BackendInstruction<Id, P>],
    ) -> Result<BatchResult<R>, Self::Error> {
        self.top.execute_batch(instructions)
    }
}

/// The bottom backend, with its error wrapped in [`QodecRuntimeError::Backend`].
struct BottomNode<B>(B);

impl<Id, P, R, B: Backend<Id, P, R>> Backend<Id, P, R> for BottomNode<B> {
    type Error = QodecRuntimeError<B::Error>;

    fn execute_batch(
        &mut self,
        instructions: &[BackendInstruction<Id, P>],
    ) -> Result<BatchResult<R>, Self::Error> {
        self.0
            .execute_batch(instructions)
            .map_err(QodecRuntimeError::Backend)
    }
}

/// A layer runtime, with its errors flattened.
struct LayerNode<'a, Id, P, R, E> {
    layer: LayerRuntime<Id, P, R, DynBackend<'a, Id, P, R, E>>,
    depth: usize,
}

impl<Id, P, R, E> Backend<Id, P, R> for LayerNode<'_, Id, P, R, E>
where
    Id: InstructionId,
    P: Param,
    R: Readout,
{
    type Error = QodecRuntimeError<E>;

    fn execute_batch(
        &mut self,
        instructions: &[BackendInstruction<Id, P>],
    ) -> Result<BatchResult<R>, Self::Error> {
        self.layer
            .execute_batch(instructions)
            .map_err(|e| match e.split_lower() {
                Ok(error) => QodecRuntimeError::Layer {
                    layer: self.depth,
                    error,
                },
                Err(lower) => lower,
            })
    }
}

/// A [`QodecRuntime`] error, flattened across layers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QodecRuntimeError<E> {
    /// The bottom backend returned `Err`.
    Backend(E),
    /// Layer `layer` (0 = top) raised `error`.
    Layer {
        /// The layer's index, 0 being the top.
        layer: usize,
        /// The error, raised by the layer itself.
        error: QodecError<Infallible>,
    },
}

impl<E: fmt::Display> fmt::Display for QodecRuntimeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QodecRuntimeError::Backend(e) => write!(f, "bottom backend error: {e}"),
            QodecRuntimeError::Layer { layer, error } => write!(f, "layer {layer}: {error}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for QodecRuntimeError<E> {}
