// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! A library of mix-and-match pieces for the qodec runtime. Each module is
//! one piece: a [`Layer`] (or a [`Backend`]), what it runs on, and how it
//! plans that from a [`LayerModel`]. Adding a piece is adding a module; no
//! other code needs to know about it.
//!
//! | Piece | Role | Plans from a layer |
//! |---|---|---|
//! | [`SpecEncoder<C>`] | `Encoder<C>` | [`SpecEncoderPlan::plan`]: each upper instruction's gadget, as slots and call templates |
//! | [`Passive<D>`] | `Decoder<Never, Never>` | [`RecordLayout::plan`]; turns any [`Decode`] into a decoder |
//! | [`FrameDecoder`] | [`Decode`] | [`FrameDecoderPlan::plan`]: each gadget's checks, readouts, frames and sign transport |
//! | [`UntilReady`] | `Controller<Rounds>` | [`UntilReadyPlan::plan`]: each block type's QEC round |
//! | [`TypeChecker<E>`] | [`Layer`] | an [`Isa`]: the top ISA's signatures |
//! | [`Resolver`] | [`Fixes`] | an [`Isa`]: applies corrections with the lower ISA's instructions |
//! | [`Runtime`] | [`Backend`] | any qodec layer over a [`BatchBackend`], checked against the top ISA |
//!
//! A piece's `…Plan` is its [`Blueprint`]: shot-independent data from which a
//! fresh piece is built per shot. This crate re-exports the model crate, and
//! through it the interface crate.

pub mod encoder;
pub mod frame_decoder;
pub mod passive;
pub mod resolver;
pub mod runtime;
pub mod type_checker;
pub mod until_ready;

pub use qodec_runtime_model::*;

pub use encoder::{
    EncoderProgram, Fixes, GadgetCall, GadgetProgram, LiveBlock, LogicalOps, Lowering,
    OutputEncoding, PlannedFix, SpecEncoder, SpecEncoderPlan, check_pauli_corrections,
};
pub use frame_decoder::{CodeShape, FrameDecoder, FrameDecoderPlan, FrameGadget, Transport};
pub use passive::{BodySelect, Decode, Passive, RecordLayout};
pub use resolver::{Resolver, ResolverFixes};
pub use runtime::{
    BatchBackend, QodecLayer, QodecLayers, QodecRuntime, QodecRuntimeError, Runtime,
};
pub use type_checker::TypeChecker;
pub use until_ready::{Rounds, UntilReady, UntilReadyPlan};

// Plans and the model they come from are shared by shots on many threads.
const _: () = {
    const fn shareable<T: Send + Sync>() {}
    shareable::<QodecModel>();
    shareable::<SpecEncoderPlan<Never>>();
    shareable::<SpecEncoderPlan<Correction>>();
    shareable::<FrameDecoderPlan>();
    shareable::<UntilReadyPlan>();
};
