// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! Assembles qodec runtimes from mix-and-match pieces: a [`Design`] chooses
//! the pieces of each layer of a [`QodecModel`], and a [`QodecPlan`] holds
//! their blueprints and builds fresh layers per shot.
//!
//! It knows no particular piece, and does not depend on the library of pieces
//! (`qodec_runtime_pieces`): a piece only needs to be a [`Blueprint`] of a
//! [`QodecLayer`]. Ready-made designs, such as the reference one, are recipes,
//! which live elsewhere (`qodec_runtime_recipes`). It re-exports the model
//! crate, and through it the interface crate.
//!
//! # Usage
//!
//! 1. Read a qodec into a [`QodecModel`] (`qodec_runtime_reader::read`).
//! 2. Build a [`QodecPlan`] once with [`QodecPlan::new`] and a [`Design`].
//! 3. For every shot, stack [`QodecPlan::instantiate`] on a runtime, such as
//!    `qodec_runtime_pieces::Runtime`, over a bottom backend, and send it
//!    batches of top-ISA calls with [`Backend::execute`]. Recoverable
//!    conditions come back in each result's [`Status`], unrecoverable errors
//!    as the runtime's error (`qodec_runtime_pieces::QodecRuntimeError`).
//! 4. Drop the runtime to end the shot.

pub mod design;
pub mod plan;

pub use qodec_runtime_model::*;

pub use design::{Design, LayerBlueprint};
pub use plan::{AtLayer, QodecPlan};

// A plan is shared by shots on many threads.
const _: () = {
    const fn shareable<T: Send + Sync>() {}
    shareable::<QodecPlan>();
};
