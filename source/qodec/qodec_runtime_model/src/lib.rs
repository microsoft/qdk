// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! The model of the qodec runtime: the vocabulary that pieces, the code that
//! reads a qodec, and the code that assembles runtimes share.
//!
//! - [`QodecModel`] and [`LayerModel`]: a qodec resolved into plain data,
//!   shaped like the qodec spec. Pieces plan from it.
//! - [`Isa`]: block types and instruction signatures with their actions, and
//!   the spec rules that say what a call means.
//! - [`select`]: whether flags satisfy a `select` hint, and default policies
//!   for what to make of it.
//! - [`parity`] and [`action`]: parity expressions, and instruction actions.
//!
//! It holds no behavior that a piece could choose differently. It re-exports
//! the interface crate.
// Links to the qodec spec are pinned to the commit that `qodec` 0.2.1 was
// published from (recorded in the crate's `.cargo_vcs_info.json`). Update them
// together with the qodec version this model follows.

pub mod action;
pub mod error;
pub mod isa;
pub mod layer_model;
pub mod parity;
pub mod select;

pub use qodec_runtime_interface::*;

pub use action::{ActionStep, Angle, CliffordMap, ParsedPauli, PauliExpr, StepAction};
pub use error::PlanError;
pub use isa::{BoundOperand, Isa, Operand, ParamKind, Parameter, ParsedCall, Signature, TypeTable};
pub use layer_model::{
    Arg, CallModel, CodeModel, EncodingModel, FrameKey, GadgetModel, LayerModel, QodecModel,
    Resources,
};
pub use parity::{Basis, Parity, Side, Var};
pub use select::{CompiledSelect, SelectPolicy, Selection};
