// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! A library of mix-and-match pieces that implement the interfaces of
//! [`qodec_runtime_interface`]. Each piece is a [`Layer`] (or a [`Backend`])
//! that runs on its own plain data: no qodec model, no parsing of a qodec.
//! Building that data from a qodec layer is the job of another crate
//! (`qodec_runtime_builder`), so a piece can be reused, tested or replaced
//! without it.
//!
//! This crate re-exports the interface crate, so one dependency is enough to
//! build a runtime from pieces.
//!
//! # The library
//!
//! | Piece | Role | Runs on |
//! |---|---|---|
//! | [`SpecEncoder<C>`] | `Encoder<C>` | [`EncoderProgram`]: each upper instruction's gadget, as slots and call templates |
//! | [`Passive<D>`] | `Decoder<Never, Never>` | [`RecordLayout`]: the upper ISA, and each gadget's `select` hints; turns any [`Decode`] into a decoder |
//! | [`FrameDecoder`] | [`Decode`] | [`FrameGadget`]s: each gadget's checks, readouts, frames and sign transport |
//! | [`UntilReady`] | `Controller<Rounds>` | [`UntilReadyPlan`]: each block type's QEC round |
//! | [`TypeChecker<E>`] | [`Layer`] | [`Isa`]: the top ISA's signatures |
//! | [`Runtime`] | [`Backend`] | [`Isa`]s of the top and bottom; any qodec layer over a [`BatchBackend`] |
//!
//! A piece's `…Plan` is its [`Blueprint`]: shot-independent data from which a
//! fresh piece is built per shot.

pub mod action;
pub mod checker;
pub mod controller;
pub mod decoder;
pub mod encoder;
pub mod frame_decoder;
pub mod isa;
pub mod parity;
pub mod runtime;

pub use qodec_runtime_interface::*;

pub use action::{ActionStep, CliffordMap, ParsedPauli, PauliExpr, StepAction};
pub use checker::TypeChecker;
pub use controller::{Rounds, UntilReady, UntilReadyPlan};
pub use decoder::{BodySelect, Decode, Passive, RecordLayout};
pub use encoder::{
    CompiledArg, EncoderProgram, Fixes, GadgetCall, GadgetProgram, LiveBlock, LogicalOps, Lowering,
    OutputEncoding, SpecEncoder, SpecEncoderPlan,
};
pub use frame_decoder::{
    CodeShape, FrameDecoder, FrameDecoderPlan, FrameGadget, FrameKey, Transport,
};
pub use isa::{Isa, Operand, ParamKind, Parameter, Signature, TypeTable};
pub use runtime::{
    AtLayer, BatchBackend, QodecLayer, QodecLayers, QodecRuntime, QodecRuntimeError, Runtime,
};
