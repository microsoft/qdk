// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! Builds qodec runtimes: plans a [`Qodec`](qodec::Qodec) once, then
//! assembles a runtime per shot from the mix-and-match pieces of
//! [`qodec_runtime_pieces`].
//!
//! The pieces run on plain data and know nothing of the `qodec` model. This
//! crate is where the model is read: it checks and indexes each layer once
//! into a [`LayerPlan`], and compiles from it what each piece runs on, such as
//! a [`SpecEncoder`]'s [`EncoderProgram`] or a [`FrameDecoder`]'s
//! [`FrameGadget`]s. It re-exports the pieces crate, and through it the
//! interface crate, so one dependency is enough to build a runtime.
//!
//! | Item | What it does |
//! |---|---|
//! | [`QodecPlan`] | Plans a whole qodec, and instantiates a [`QodecRuntime`] per shot |
//! | [`LayerPlan`] | Plans one layer, and gives the blueprints of its pieces: [`LayerPlan::spec_encoder`], [`LayerPlan::frame_decoder`], [`LayerPlan::until_ready`] |
//! | [`Design`] | Chooses how to assemble each layer; [`FrameDecoding`] is the reference one |
//! | [`Compiler`] | Chains encoders without decoders, ahead of time |
//!
//! # Usage
//!
//! 1. Build a [`QodecPlan`] once per qodec with [`QodecPlan::new`], choosing a
//!    [`Design`] such as [`FrameDecoding`], which assembles each layer.
//! 2. For every shot, call [`QodecPlan::instantiate`] with a bottom backend.
//!    Encoders allocate lower block ids on demand, so a program need not know
//!    its size; [`QodecPlan::required_resources`] sizes the bottom backend for
//!    one that does.
//! 3. Send batches of top-ISA calls to the [`QodecRuntime`] with
//!    [`Backend::execute`]. Recoverable conditions come back in each result's
//!    [`Status`], unrecoverable errors as a [`QodecRuntimeError`].
//! 4. Drop the [`QodecRuntime`] to end the shot.
// Links to the qodec spec are pinned to the commit that `qodec` 0.2.1 was
// published from (recorded in the crate's `.cargo_vcs_info.json`). Update them
// together with the `qodec` dependency.

mod actions;
pub mod compiler;
pub mod design;
mod frame;
pub mod plan;
pub mod resolver;
pub mod spec;

#[cfg(test)]
mod tests;

pub use qodec_runtime_pieces::*;

pub use compiler::Compiler;
pub use design::{Design, FrameDecoding, LayerBlueprint};
pub use plan::{LayerPlan, PlanError, PlannedFix, QodecPlan, Resources};
