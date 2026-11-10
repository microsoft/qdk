// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Runtime`]: a qodec layer, or a chain of them, over a bottom backend, seen
//! as one backend for batches of top-ISA calls.

use super::{
    backend::Backend,
    layer::{Chain, Layer},
    protocol::{BasicReadout, Call, Fault, FaultKind, Reply},
    spec::parse_call,
};
use qodec::InstructionSet;
use std::{fmt, sync::Arc};

/// What a [`Runtime`] asks of the backend at the bottom: it answers a batch of
/// bottom-ISA calls with one result per call that has observe outcomes or
/// flags, in call order, or fails with `E`, which loses the shot. Recoverable
/// conditions go in each result's [`Status`](crate::Status) instead.
pub trait BatchBackend<E>: Backend<Vec<Call>, Result<Vec<BasicReadout>, E>> {}

impl<E, B: Backend<Vec<Call>, Result<Vec<BasicReadout>, E>> + ?Sized> BatchBackend<E> for B {}

/// A whole qodec layer: batches of ISA calls and their results on both sides.
pub trait QodecLayer:
    Layer<
        Input = Vec<Call>,
        Output = Result<Vec<Reply>, Fault>,
        LowerInput = Vec<Call>,
        LowerOutput = Result<Vec<Reply>, Fault>,
    >
{
}

impl<L> QodecLayer for L where
    L: Layer<
            Input = Vec<Call>,
            Output = Result<Vec<Reply>, Fault>,
            LowerInput = Vec<Call>,
            LowerOutput = Result<Vec<Reply>, Fault>,
        > + ?Sized
{
}

/// The layers of a qodec, top first.
pub type QodecLayers = Chain<Vec<Call>, Result<Vec<Reply>, Fault>>;

/// The runtime of one shot of a qodec: every layer chained over a bottom
/// backend that fails with `E`. See
/// [`QodecPlan::instantiate`](crate::QodecPlan::instantiate).
pub type QodecRuntime<'a, E> = Runtime<QodecLayers, Box<dyn BatchBackend<E> + 'a>, E>;

/// A qodec layer `L` over a bottom backend `B` that fails with `E`.
///
/// ```text
///   │ Vec<Call> or &[Call]   top ISA       ▲ Result<Vec<BasicReadout>, QodecRuntimeError<E>>
///  ┌▼──────────────────────────────────────┴───────────────────────────────────┐
///  │ Runtime   checks each call against the top ISA; any other error loses the │
///  │           shot                                                            │
///  │ ┌───────────────────────────────────────────────────────────────────────┐ │
///  │ │ layer: L                                                              │ │
///  │ └───────────────────────────────────────────────────────────────────────┘ │
///  └┬──────────────────────────────────────▲───────────────────────────────────┘
///   │ Vec<Call>              bottom ISA    │ Result<Vec<BasicReadout>, E>
/// ```
///
/// It answers like the [`BatchBackend`] it drives: one result per call that
/// has observe outcomes or flags, in call order.
///
/// It checks only that each call is well formed. A call on a block that is not
/// live, or of the wrong type, fails in the layers and loses the shot; stack a
/// [`TypeChecker`](crate::TypeChecker) on top to reject such a batch instead.
pub struct Runtime<L, B, E> {
    layer: L,
    bottom: Bottom<B, E>,
    top: Arc<InstructionSet>,
    poisoned: bool,
}

/// The bottom backend, seen by the layers.
struct Bottom<B, E> {
    backend: B,
    isa: Arc<InstructionSet>,
    /// The backend's error, while a [`FaultKind::Bottom`] fault carries the
    /// news up the layers.
    error: Option<E>,
}

impl<B: BatchBackend<E>, E> Backend<Vec<Call>, Result<Vec<Reply>, Fault>> for Bottom<B, E> {
    fn execute(&mut self, calls: Vec<Call>) -> Result<Vec<Reply>, Fault> {
        let shapes: Vec<(usize, usize)> = calls
            .iter()
            .filter_map(|c| shape(&self.isa, c.instruction_id))
            .collect();
        let results = match self.backend.execute(calls) {
            Ok(results) => results,
            Err(e) => {
                self.error = Some(e);
                return Err(Fault::new(FaultKind::Bottom, ""));
            }
        };
        if results.len() != shapes.len() {
            return Err(Fault::protocol(format!(
                "the backend returned {} results for {} calls that have results",
                results.len(),
                shapes.len()
            )));
        }
        if results
            .iter()
            .zip(&shapes)
            .any(|(r, &(outcomes, flags))| r.outcomes.len() != outcomes || r.flags.len() != flags)
        {
            return Err(Fault::protocol(
                "the backend returned a result of the wrong shape",
            ));
        }
        Ok(results.into_iter().map(Reply::Ready).collect())
    }
}

impl<L, B, E> Runtime<L, B, E> {
    /// Runs `layer` from the `top` ISA down to `backend`, which runs the
    /// `bottom` ISA.
    pub fn new(
        layer: L,
        backend: B,
        top: &Arc<InstructionSet>,
        bottom: &Arc<InstructionSet>,
    ) -> Self {
        Self {
            layer,
            bottom: Bottom {
                backend,
                isa: Arc::clone(bottom),
                error: None,
            },
            top: Arc::clone(top),
            poisoned: false,
        }
    }

    /// The layer.
    pub fn layer(&self) -> &L {
        &self.layer
    }

    /// The layer, mutably.
    pub fn layer_mut(&mut self) -> &mut L {
        &mut self.layer
    }

    /// The backend.
    pub fn backend(&self) -> &B {
        &self.bottom.backend
    }

    /// The backend, mutably.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.bottom.backend
    }

    /// Ends the shot and returns the backend.
    pub fn into_backend(self) -> B {
        self.bottom.backend
    }

    /// Whether the shot is lost: every further batch fails.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    /// Rejects the batch, without effect, if a call is malformed.
    fn check(&self, calls: &[Call]) -> Result<(), QodecRuntimeError<E>> {
        if self.poisoned {
            return Err(QodecRuntimeError::Poisoned);
        }
        for call in calls {
            parse_call(&self.top, call.instruction_id, &call.parameters)
                .map_err(|e| QodecRuntimeError::Rejected(Fault::invalid(e).at_layer(0)))?;
        }
        Ok(())
    }
}

impl<L: QodecLayer, B: BatchBackend<E>, E> Runtime<L, B, E> {
    /// Runs checked calls. Any error loses the shot.
    fn run(&mut self, calls: Vec<Call>) -> Result<Vec<BasicReadout>, QodecRuntimeError<E>> {
        let expected = calls
            .iter()
            .filter(|c| shape(&self.top, c.instruction_id).is_some())
            .count();
        let result = self.layer.execute(calls, &mut self.bottom);
        let replies = result.map_err(|fault| {
            self.poisoned = true;
            match self.bottom.error.take() {
                Some(e) if fault.kind == FaultKind::Bottom => QodecRuntimeError::Bottom(e),
                _ => QodecRuntimeError::Fault(fault),
            }
        })?;
        if replies.len() != expected {
            self.poisoned = true;
            return Err(QodecRuntimeError::Fault(Fault::protocol(format!(
                "{} results for {expected} calls that have results",
                replies.len()
            ))));
        }
        Ok(replies.into_iter().map(Reply::into_ready).collect())
    }
}

impl<L: QodecLayer, B: BatchBackend<E>, E>
    Backend<Vec<Call>, Result<Vec<BasicReadout>, QodecRuntimeError<E>>> for Runtime<L, B, E>
{
    fn execute(&mut self, calls: Vec<Call>) -> Result<Vec<BasicReadout>, QodecRuntimeError<E>> {
        self.check(&calls)?;
        self.run(calls)
    }
}

impl<'c, L: QodecLayer, B: BatchBackend<E>, E>
    Backend<&'c [Call], Result<Vec<BasicReadout>, QodecRuntimeError<E>>> for Runtime<L, B, E>
{
    fn execute(&mut self, calls: &'c [Call]) -> Result<Vec<BasicReadout>, QodecRuntimeError<E>> {
        self.check(calls)?;
        self.run(calls.to_vec())
    }
}

/// A qodec layer that tags its faults with its index in the qodec.
pub(crate) struct AtLayer<L> {
    pub(crate) index: usize,
    pub(crate) layer: L,
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

/// The (observe count, flag count) of instruction `index` of `isa`, if it
/// returns a result.
fn shape(isa: &InstructionSet, index: usize) -> Option<(usize, usize)> {
    let instruction = isa.instructions.get(index)?;
    let shape = (instruction.observe_count(), instruction.flags.len());
    (shape.0 + shape.1 > 0).then_some(shape)
}

/// An error of a [`Runtime`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QodecRuntimeError<E> {
    /// The backend returned `Err`. The shot is lost.
    Bottom(E),
    /// A call of the batch is malformed (an unknown instruction or a bad
    /// argument) or, with a [`TypeChecker`](crate::TypeChecker) on top,
    /// ill-typed (an operand that is not a live block of the declared type).
    /// Nothing in the batch ran; the shot goes on.
    Rejected(Fault),
    /// A layer failed. The shot is lost.
    Fault(Fault),
    /// An earlier error lost the shot; start a new one.
    Poisoned,
}

impl<E: fmt::Display> fmt::Display for QodecRuntimeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QodecRuntimeError::Bottom(e) => write!(f, "bottom backend error: {e}"),
            QodecRuntimeError::Rejected(fault) | QodecRuntimeError::Fault(fault) => {
                write!(f, "{fault}")
            }
            QodecRuntimeError::Poisoned => write!(f, "the shot is lost; start a new one"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for QodecRuntimeError<E> {}
