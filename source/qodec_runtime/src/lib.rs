// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! Runs programs written for the top instruction set (ISA) of a qodec on a
//! backend for its bottom ISA.
//!
//! A qodec is a stack of ISAs: each layer's gadgets implement the layer's
//! instructions as circuits of the ISA below, for example the logical
//! operations of an error-correcting code as physical operations (see the
//! qodec [lowering chain][lowering-chain]). This crate lowers instructions one
//! layer at a time, and decodes the lower results back into each layer's
//! results on the way up.
//!
//! ```text
//!   ┌──────────────────────────────┐  Backend<Id, P, R>   (top ISA instructions)
//!   │ LayerRuntime 0               │
//!   ├──────────────────────────────┤  Backend<Id, P, R>   (layer-1 ISA instructions)
//!   │ LayerRuntime 1               │
//!   ├──────────────────────────────┤        ...
//!   │ LayerRuntime n-1             │
//!   └──────────────────────────────┘  Backend<Id, P, R>   (bottom ISA instructions)
//!      bottom backend: any implementation of Backend for the bottom ISA
//!      (a simulator, tracer, profiler, ...)
//! ```
//!
//! Every boundary speaks the same [`Backend`] interface: each
//! [`LayerRuntime`] implements [`Backend`] for the ISA above it, runs gadget
//! bodies on the [`Backend`] below it, and asks its decoder
//! ([`DecoderRuntime`]) to turn the lower results into upper results. The
//! types `Id`, `P` and `R` are the same at every boundary. A qodec with
//! `n + 1` layers has `n` layer runtimes: every layer except the bottom one
//! lowers to the layer below it, and the bottom layer's ISA is the bottom
//! backend's.
//!
//! Names follow three domains:
//! - **qodec data**: the model loaded by the [`qodec`] crate ([`qodec::Qodec`],
//!   [`qodec::Layer`], [`qodec::Gadget`], ...).
//! - **plans** (`…Plan`, in [`plan`]): shot-independent data derived once from
//!   the qodec data, such as [`QodecPlan`], [`LayerPlan`] and [`DecoderPlan`].
//! - **runtimes** (`…Runtime`): per-shot state instantiated from a plan, such
//!   as [`QodecRuntime`], [`LayerRuntime`] and [`DecoderRuntime`].
//!
//! # Usage
//!
//! 1. Build a [`QodecPlan`] once per qodec with [`QodecPlan::new`], choosing a
//!    [`DecoderPicker`] such as [`FrameDecoder`], which picks a decoder for
//!    each layer.
//! 2. For every shot, call [`QodecPlan::instantiate`] with a bottom backend.
//!    Layer runtimes allocate lower block ids on demand, so a program need not
//!    know its size; [`QodecPlan::required_resources`] sizes the bottom
//!    backend for one that does.
//! 3. Send top-ISA instructions to the [`QodecRuntime`] with
//!    [`Backend::execute_batch`]. Recoverable conditions come back in each
//!    result's [`Status`], unrecoverable errors as a [`QodecRuntimeError`].
//! 4. Drop the [`QodecRuntime`] to end the shot.
//!
//! [lowering-chain]: https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/model.md#the-lowering-chain
// Links to the qodec spec are pinned to the commit that `qodec` 0.2.1 was
// published from (recorded in the crate's `.cargo_vcs_info.json`). Update them
// together with the `qodec` dependency.

mod actions;
pub mod backend;
pub mod decoder;
pub mod frame_decoder;
pub mod layer_runtime;
pub mod parity;
pub mod pauli;
pub mod plan;
pub mod resolver;
pub mod runtime;
pub mod spec;

#[cfg(test)]
mod tests;

pub use backend::{
    Backend, BackendInstruction, BasicReadout, BatchResult, Bit, InstructionId, OwnedParam, Param,
    ParamView, PlanError, QodecError, Readout, Status,
};
pub use decoder::{
    BlockRef, Correction, Decoded, DecoderPicker, DecoderPlan, DecoderRuntime, Invocation,
    Operation, Step,
};
pub use frame_decoder::{FrameDecoder, FrameDecoderPlan, FrameDecoderRuntime};
pub use layer_runtime::{LayerRuntime, LiveBlock};
pub use plan::{LayerPlan, QodecPlan, Resources};
pub use runtime::{QodecRuntime, QodecRuntimeError};
