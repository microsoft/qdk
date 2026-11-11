// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! Assembles qodec runtimes from mix-and-match pieces: a [`Design`] chooses
//! the pieces of each layer of a [`QodecModel`], and a [`QodecPlan`] holds
//! their blueprints and builds fresh layers per shot.
//!
//! It knows no particular piece: ready-made designs, such as the reference
//! one, are recipes, which live elsewhere (`qodec_runtime_recipes`). It
//! re-exports the pieces crate, and through it the model and interface
//! crates.
//!
//! # Usage
//!
//! 1. Read a qodec into a [`QodecModel`] (`qodec_runtime_reader::read`).
//! 2. Build a [`QodecPlan`] once with [`QodecPlan::new`] and a [`Design`].
//! 3. For every shot, stack [`QodecPlan::instantiate`] on a [`Runtime`] over
//!    a bottom backend, and send it batches of top-ISA calls with
//!    [`Backend::execute`]. Recoverable conditions come back in each result's
//!    [`Status`], unrecoverable errors as a [`QodecRuntimeError`].
//! 4. Drop the runtime to end the shot.

pub mod design;
pub mod plan;

pub use qodec_runtime_pieces::*;

pub use design::{Design, LayerBlueprint};
pub use plan::{AtLayer, QodecPlan};
