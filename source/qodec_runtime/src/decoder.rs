// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The decoder interface, and the runtime types shared by a layer runtime and
//! its decoder. A [`DecoderPicker`], chosen for a whole qodec, plans a decoder
//! for each layer as a [`DecoderPlan`], which instantiates a [`DecoderRuntime`]
//! per shot.

use super::{
    backend::{Bit, PlanError, Status},
    plan::{GadgetPlan, LayerPlan},
};
use qodec::Argument;
use std::{collections::BTreeMap, rc::Rc};

/// Chooses and plans the decoder of each layer of a qodec.
///
/// [`QodecPlan::new`](crate::QodecPlan::new) calls [`Self::plan`] once per
/// layer, top first. Different layers may get different decoders, so whether a
/// decoder is deferrable is a property of each [`DecoderPlan`], not of the
/// picker.
pub trait DecoderPicker<R> {
    /// Plans the decoder of one layer. It can choose based on anything in
    /// `layer`, such as the names of its instruction sets (`layer.isa.name`,
    /// `layer.lower_isa.name`), its codes, or its gadgets.
    fn plan(&self, layer: &Rc<LayerPlan>) -> Result<Box<dyn DecoderPlan<R>>, PlanError>;
}

/// A decoder planned for one layer. It instantiates a [`DecoderRuntime`] per
/// shot.
pub trait DecoderPlan<R> {
    /// Whether this decoder only keeps track of what it learns, and never
    /// changes the quantum state below.
    ///
    /// A decoder can act on what it learns in two ways. It can record it, for
    /// example "this block's logical X sign is flipped", and use it when it
    /// interprets later results. Or it can issue corrections: lower
    /// instructions, such as an `x` on a lower block, that the layer must send
    /// below right away (see [`Step::Correct`]). A decoder is deferrable iff it
    /// never issues corrections: it has no `before`, and its `decode` never
    /// yields [`Step::Correct`]. [`FrameDecoder`](crate::FrameDecoder) is
    /// deferrable.
    ///
    /// A deferrable decoder lets the layer send a whole batch's lower
    /// instructions below in one lower batch: the layer queues each gadget's
    /// decode and runs them when it flushes. Otherwise, the layer decodes each
    /// gadget right after its body, flushing first if the body's results are
    /// still pending, and sends any corrections before it runs the next gadget.
    ///
    /// A decoder that isn't deferrable may correct with `x` and `z`, so
    /// [`QodecPlan::new`](crate::QodecPlan::new) then requires the lower ISA to
    /// have those instructions (see [`LayerPlan::check_pauli_corrections`]).
    fn deferrable(&self) -> bool;
    /// Instantiates a fresh decoder runtime for one shot.
    fn instantiate(&self) -> Box<dyn DecoderRuntime<R>>;
}

/// The per-shot state of a decoder. It lives for one shot and is dropped with
/// its layer runtime. [`BlockRef`] generations start over in every shot, so it
/// must not share block state with other shots.
///
/// `before` and `decode` are coroutines. Each yields corrections through
/// [`Step::Correct`] and is then resumed with the lower result of that
/// correction, or with `None` if it produced no result. At most one coroutine
/// runs at a time.
pub trait DecoderRuntime<R> {
    /// Runs before the gadget body of `inv`, while its inputs are still live.
    /// Its corrections may target `inv.input_refs` and other live blocks.
    /// Never called for a deferrable decoder.
    fn before(&mut self, _inv: &Invocation) -> Step<()> {
        Step::Done(())
    }

    /// Resumes `before` with the result of its last correction.
    fn resume_before(&mut self, _last: Option<&R>) -> Step<()> {
        Step::Done(())
    }

    /// Decodes `inv` from `raw`, the circuit readout record of its gadget body.
    /// Its corrections may target `inv.output_refs` and other live blocks,
    /// never `inv.input_refs`, which are dead by then.
    fn decode(&mut self, inv: &Invocation, raw: &[Bit]) -> Step<Decoded>;

    /// Resumes `decode` with the result of its last correction. Called only
    /// after `decode` or `resume_decode` yielded a correction.
    fn resume_decode(&mut self, last: Option<&R>) -> Step<Decoded>;

    /// The blocks will never be used again, so state kept for them can be
    /// dropped.
    fn discarded(&mut self, _refs: &[BlockRef]) {}
}

/// One gadget invocation, as seen by the decoder.
#[derive(Clone, Debug)]
pub struct Invocation {
    /// Sequence number within the shot, from 0.
    pub id: u64,
    /// The gadget being invoked.
    pub gadget: Rc<GadgetPlan>,
    /// The upper block operands, in order.
    pub operands: Vec<u64>,
    /// The instruction's arguments, by parameter name.
    pub args: BTreeMap<String, Argument>,
    /// The input blocks, one per `in` operand.
    pub input_refs: Vec<BlockRef>,
    /// The output blocks, one per `out` operand, each with a new generation.
    pub output_refs: Vec<BlockRef>,
}

/// A reference to one binding of an upper block label. Every gadget output
/// binds its label with a new generation, so decoder state keyed by an older
/// reference never applies to the new block.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockRef {
    /// The upper block id chosen by the caller.
    pub label: u64,
    /// Incremented every time the label is (re)bound to an output.
    pub generation: u64,
    /// The upper block type.
    pub block_type: String,
}

/// One step of a decoder coroutine: yield a correction, or finish.
#[derive(Clone, Debug, PartialEq)]
pub enum Step<T> {
    /// Apply this correction, then resume the coroutine.
    Correct(Correction),
    /// The coroutine finished with this value.
    Done(T),
}

/// An operation a decoder applies to live blocks. The layer runtime lowers it
/// to instructions of its lower ISA, of which at most one may have observe
/// outcomes or flags; the coroutine is resumed with that instruction's result.
/// A correction that would need more than one fails with `NoImplementation`.
#[derive(Clone, Debug, PartialEq)]
pub struct Correction {
    /// The targeted blocks. They must be distinct and live, with their current
    /// generation.
    pub blocks: Vec<BlockRef>,
    /// The operation.
    pub op: Operation,
}

/// A generic operation, which the layer resolves against the actions of its
/// lower ISA with [`Resolver`](crate::resolver::Resolver).
#[derive(Clone, Debug, PartialEq)]
pub struct Operation {
    /// A generic name such as `x`, `cx` or `rz`.
    pub name: String,
    /// Indices into the concatenated code qubits of the correction's blocks.
    pub targets: Vec<usize>,
    /// The rotation angle, for rotations.
    pub angle: Option<f64>,
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
