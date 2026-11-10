// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! Mix-and-match pieces that implement the interfaces of
//! [`qodec_runtime_interface`], and the plans and helpers they share.
//!
//! The interface crate shapes the system: [`Backend`]s and [`Layer`]s that
//! stack, the [`protocol`] between layers, and the [`Encoder`], [`Decoder`]
//! and [`Controller`] roles. This crate re-exports it, so one dependency is
//! enough to build a runtime.
//!
//! # The library
//!
//! | Piece | Role | What it does |
//! |---|---|---|
//! | [`SpecEncoder<C>`] | `Encoder<C>` | Runs the layer's gadgets, as the spec describes |
//! | [`Passive<D>`] | `Decoder<Never, Never>` | Turns any [`Decode`] into a decoder |
//! | [`FrameDecoder`] | [`Decode`] | Tracks Pauli frames through checks, readouts and frames |
//! | [`UntilReady`] | `Controller<Rounds>` | Adds QEC rounds until the decoder is ready |
//! | [`Runtime`] | [`Backend`] | A qodec layer, or a chain of them, over a [`BatchBackend`] |
//! | [`TypeChecker<E>`] | [`Layer`] | Rejects ill-typed batches before they reach a runtime |
//! | [`Compiler`] | — | Chains encoders without decoders, ahead of time |
//! | [`FrameDecoding`] | [`Design`] | The reference assembly of every layer |
//!
//! Each piece has a [`Blueprint`] (its `…Plan`): shot-independent data derived
//! once, from which a fresh piece is built per shot. Pieces plan from a
//! [`LayerPlan`]: one layer's ISAs, codes and gadgets, checked and indexed.
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
pub mod checker;
pub mod compiler;
pub mod controller;
pub mod decoder;
pub mod design;
pub mod encoder;
pub mod frame_decoder;
pub mod parity;
pub mod plan;
pub mod resolver;
pub mod runtime;
pub mod spec;

#[cfg(test)]
mod tests;

pub use qodec_runtime_interface::*;

pub use checker::TypeChecker;
pub use compiler::Compiler;
pub use controller::{Rounds, UntilReady, UntilReadyPlan};
pub use decoder::{Decode, Passive};
pub use design::{Design, FrameDecoding, LayerBlueprint};
pub use encoder::{LiveBlock, PlannedFix, SpecEncoder, SpecEncoderPlan};
pub use frame_decoder::{FrameDecoder, FrameDecoderPlan};
pub use plan::{LayerPlan, PlanError, QodecPlan, Resources};
pub use runtime::{
    BatchBackend, QodecLayer, QodecLayers, QodecRuntime, QodecRuntimeError, Runtime,
};
