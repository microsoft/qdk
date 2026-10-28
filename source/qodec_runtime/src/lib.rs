// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! The qodec runtime: the stack of all layer runtimes plugged together.
//!
//! It executes instructions of a qodec's top ISA by lowering them one layer at
//! a time, following the qodec [lowering chain][lowering-chain], and returns
//! the top layer's results.
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
//! A qodec with `n + 1` layers has `n` layer runtimes: every layer except the
//! bottom one lowers to the layer below it, and the bottom layer's ISA is the
//! bottom backend's.
//!
//! Names follow three domains:
//! - **qodec data**: the model loaded by the [`qodec`] crate ([`qodec::Qodec`],
//!   [`qodec::Layer`], [`qodec::Gadget`], ...).
//! - **plans** (`…Plan`, in [`plan`]): shot-independent data derived once from
//!   the qodec data, such as [`QodecPlan`], [`LayerPlan`] and [`DecoderPlan`].
//! - **runtimes** (`…Runtime`): per-shot state instantiated from a plan, such
//!   as [`QodecRuntime`], [`LayerRuntime`] and [`DecoderRuntime`].
//!
//! So `Qodec → QodecPlan → QodecRuntime`: build a [`QodecPlan`] once per qodec
//! with [`QodecPlan::new`], then call [`QodecPlan::instantiate`] for every shot.
//! Layer runtimes allocate lower block ids on demand, so a program need not know
//! its size; [`QodecPlan::required_resources`] sizes the bottom backend for one
//! that does.
//!
//! Links to the qodec spec are pinned to the commit that `qodec` 0.2.1 was
//! published from (recorded in the crate's `.cargo_vcs_info.json`). Update
//! them together with the `qodec` dependency.
//!
//! [lowering-chain]: https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/model.md#the-lowering-chain

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
    BlockRef, Correction, Decoded, Decoder, DecoderPlan, DecoderRuntime, Invocation, Operation,
    Step,
};
pub use frame_decoder::{FrameDecoder, FrameDecoderPlan, FrameDecoderRuntime};
pub use layer_runtime::{LayerRuntime, LiveBlock};
pub use plan::{LayerPlan, QodecPlan, Resources};
pub use runtime::{QodecRuntime, QodecRuntimeError};
