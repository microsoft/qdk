// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! Reads a [`Qodec`](qodec::Qodec) into the runtime's model: [`read`] checks
//! every layer once against the qodec spec and resolves it into a
//! [`QodecModel`](qodec_runtime_model::QodecModel) of plain data on interned
//! ids. This is the only crate of the runtime that knows the `qodec` crate:
//! pieces plan from the model, and runtimes are assembled from it.
// Links to the qodec spec are pinned to the commit that `qodec` 0.2.1 was
// published from (recorded in the crate's `.cargo_vcs_info.json`). Update them
// together with the `qodec` dependency.

mod actions;
mod read;
mod spec;

#[cfg(test)]
mod tests;

pub use read::read;
