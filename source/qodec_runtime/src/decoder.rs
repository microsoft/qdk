// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Runtime types shared by a layer runtime and its decoder, and the
//! decoder interface: a [`Decoder`] (the chosen algorithm) is planned once per
//! layer into a [`DecoderPlan`], which instantiates a [`DecoderRuntime`] per
//! shot.

use super::{
    backend::{Bit, PlanError, Status},
    plan::{GadgetPlan, LayerPlan},
};
use qodec::Argument;
use std::{collections::BTreeMap, rc::Rc};

/// A live upper block. Every output gets a new generation, so state keyed by
/// an older reference never reaches it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockRef {
    /// The upper block id chosen by the caller.
    pub label: u64,
    /// Incremented every time the label is (re)bound to an output.
    pub generation: u64,
    /// The upper block type.
    pub block_type: String,
}

/// One gadget invocation, as seen by the decoder.
#[derive(Clone, Debug)]
pub struct Invocation {
    /// Sequence number within the shot.
    pub id: u64,
    /// The gadget being invoked.
    pub gadget: Rc<GadgetPlan>,
    /// The upper block operands, in order.
    pub operands: Vec<u64>,
    /// The instruction's arguments, by parameter name.
    pub args: BTreeMap<String, Argument>,
    /// The input blocks, one per `in` operand.
    pub input_refs: Vec<BlockRef>,
    /// The (fresh) output blocks, one per `out` operand.
    pub output_refs: Vec<BlockRef>,
}

/// A decoded upper result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decoded {
    /// One per observe outcome of the implemented instruction.
    pub outcomes: Vec<Bit>,
    /// One per flag of the implemented instruction.
    pub flags: Vec<Bit>,
    /// Recoverable conditions raised while decoding.
    pub status: Status,
}

/// A generic operation. `targets` index into the concatenated code qubits of
/// the correction's blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct Operation {
    /// A generic name such as `x`, `cx` or `rz`, matched against lower actions.
    pub name: String,
    /// Code-qubit indices.
    pub targets: Vec<usize>,
    /// The rotation angle, for rotations.
    pub angle: Option<f64>,
}

/// An operation a decoder applies to live blocks. The layer runtime lowers it
/// to instructions of its lower ISA.
#[derive(Clone, Debug, PartialEq)]
pub struct Correction {
    /// The targeted blocks. They must be live, with the same generation.
    pub blocks: Vec<BlockRef>,
    /// The operation, on the concatenated code qubits of `blocks`.
    pub op: Operation,
}

/// One step of a decoder coroutine: yield a correction, or finish.
#[derive(Clone, Debug, PartialEq)]
pub enum Step<T> {
    /// Apply this correction, then resume the coroutine.
    Correct(Correction),
    /// The coroutine finished with this value.
    Done(T),
}

/// The per-shot state of a decoder. It lives for one shot and is dropped with
/// its layer runtime.
///
/// `before` and `decode` are coroutines. Each yields corrections through
/// [`Step::Correct`] and is then resumed with the lower result of that
/// correction, or with `None` if it produced no result. At most one coroutine
/// runs at a time.
pub trait DecoderRuntime<R> {
    /// May target `inv.input_refs` and other live blocks. Never called for a
    /// deferrable decoder.
    fn before(&mut self, _inv: &Invocation) -> Step<()> {
        Step::Done(())
    }

    /// Resumes `before` with the result of its last correction.
    fn resume_before(&mut self, _last: Option<&R>) -> Step<()> {
        Step::Done(())
    }

    /// May target `inv.output_refs` and other live blocks, never
    /// `inv.input_refs`, which are dead by then.
    fn decode(&mut self, inv: &Invocation, raw: &[Bit]) -> Step<Decoded>;

    /// Called only after `decode` or `resume_decode` yielded a correction.
    fn resume_decode(&mut self, last: Option<&R>) -> Step<Decoded>;

    /// The blocks will never be used again.
    fn discarded(&mut self, _refs: &[BlockRef]) {}
}

/// A decoder planned for one layer. It instantiates a [`DecoderRuntime`] per
/// shot.
pub trait DecoderPlan<R> {
    /// True iff `before` is absent and `decode` never yields corrections. Such
    /// decodes are queued and run at flush time.
    fn deferrable(&self) -> bool;
    /// Instantiates a fresh decoder runtime for one shot.
    fn instantiate(&self) -> Box<dyn DecoderRuntime<R>>;
}

/// A decoding algorithm, chosen for a whole qodec. It is planned once per
/// layer into a [`DecoderPlan`].
pub trait Decoder<R> {
    /// Whether this algorithm is deferrable (see [`DecoderPlan::deferrable`]).
    /// Known before any plan exists.
    fn deferrable(&self) -> bool;
    /// Plans this decoder for one layer.
    fn plan(&self, layer: &Rc<LayerPlan>) -> Result<Box<dyn DecoderPlan<R>>, PlanError>;
}
