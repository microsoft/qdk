// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! The interfaces of the qodec runtime: what a component is, how components
//! compose, what flows between them, and the roles they play. Implementations
//! live elsewhere: `qodec_runtime_model` is the model of a qodec that pieces
//! plan from, `qodec_runtime_reader` reads a qodec into it,
//! `qodec_runtime_pieces` is a library of pieces, `qodec_runtime_builder`
//! assembles them, and `qodec_runtime_recipes` has ready-made designs. This
//! crate only shapes the system.
//!
//! A qodec is a stack of instruction sets (ISAs): each layer's gadgets
//! implement the layer's instructions as circuits of the ISA below (see the
//! qodec [lowering chain][lowering-chain]). The runtime runs programs for the
//! top ISA on a backend for the bottom ISA.
//!
//! # Two interfaces
//!
//! A [`Backend`] answers each input it is given. A [`Layer`] answers each
//! input by calling the backend below it, as often as it needs: data flows
//! down as arguments and up as return values.
//!
//! ```text
//!  ┌─────────────────┐                  ┌─────────────────┐
//!  │ Input           │                  │ Output          │
//!  └─┬───────────────┘                  └─▲───────────────┘
//!    │ execute(input, below)              │ return
//!  ┌─▼────────────────────────────────────┴───────────────┐
//!  │                         Layer                        │
//!  └─┬────────────────────────────────────▲───────────────┘
//!    │ below.execute(..), 0..n times      │ return
//!  ┌─▼───────────────┐                  ┌─┴───────────────┐
//!  │ LowerInput      │                  │ LowerOutput     │
//!  └─────────────────┘                  └─────────────────┘
//! ```
//!
//! [`Stack`] puts a layer on a backend, which makes a backend, or on another
//! layer, which makes a layer. [`Chain`] stacks any number of layers. A
//! [`Blueprint`] is the shot-independent plan of a component, from which a
//! fresh one is built per shot; blueprints stack like their instances.
//!
//! # Three roles
//!
//! The [`protocol`] fixes what each layer takes and answers:
//!
//! ```text
//!   │ Vec<Call>                          ▲ Result<Vec<Reply>, Fault>      (upper ISA)
//!  ┌▼────────────────────────────────────┴┐
//!  │ Controller<N>  (if a decoder pends)  │
//!  └┬────────────────────────────────────▲┘
//!   │ Vec<Call>                          │ Result<Vec<Reply<N>>, Fault>
//!  ┌▼────────────────────────────────────┴┐
//!  │ Decoder<N, C>                        │
//!  └┬────────────────────────────────────▲┘
//!   │ Vec<Lower<C>>                      │ Result<Vec<Record>, Fault>     (gadget level)
//!  ┌▼────────────────────────────────────┴┐
//!  │ Encoder<C>                           │
//!  └┬────────────────────────────────────▲┘
//!   │ Vec<Call>                          │ Result<Vec<Reply>, Fault>      (lower ISA)
//! ```
//!
//! - An [`Encoder`] lowers calls with the layer's gadgets, and answers with
//!   each gadget's readout record.
//! - A [`Decoder`] turns records into results. It may ask for more
//!   information (`Reply::Pending(N)`) or correct live blocks (`Lower::Fix(C)`).
//! - A [`Controller`] answers `Pending(N)`, for example by adding QEC rounds.
//!
//! `N` and `C` are agreed by the neighbors that exchange them. [`Never`] for
//! either means the message is never sent, and the type checker proves it: a
//! decoder that never pends needs no controller, because its `Reply<Never>`
//! is a plain result.
//!
//! [lowering-chain]: https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/model.md#the-lowering-chain
// Links to the qodec spec are pinned to the commit that `qodec` 0.2.1 was
// published from (recorded in the crate's `.cargo_vcs_info.json`). Update them
// together with the `qodec` dependency.

pub mod backend;
pub mod layer;
pub mod pauli;
pub mod protocol;

pub use backend::{Backend, FromFn, from_fn};
pub use layer::{Blueprint, Chain, Layer, Link, Stack};
pub use protocol::{
    BasicReadout, Bit, BlockRef, BoxedLayer, Call, Controller, Correction, Decoder, Encoder, Fault,
    FaultKind, Fix, GadgetId, Invocation, Lower, Never, OwnedParam, Record, Reply, Status, Step,
};
