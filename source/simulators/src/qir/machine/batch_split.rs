// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Whether a [`QirMachine`] hands a batch over whole or in pieces
//! ([`QirBatchSplit`]), chosen once by its resumer
//! ([`QirMachine::with_split`]). Why a batch may need splitting:
//! `design.md`, "A batch whole or in pieces".
//!
//! The choice is a type parameter, not a field read as the walk runs, so
//! that the walk is compiled once per choice: under [`WholeBatches`] it
//! counts nothing, and runs as fast as if no batch could be split.

mod split_batches;
mod whole_batches;

pub use split_batches::SplitBatches;
pub use whole_batches::WholeBatches;

#[cfg(doc)]
use super::{QirMachine, QirStop};
use std::fmt::Debug;

/// Whether a machine hands a batch over whole ([`WholeBatches`]) or in
/// pieces ([`SplitBatches`]). Sealed: the machine's walk implements each
/// choice.
pub trait QirBatchSplit: Copy + Debug + sealed::Sealed {
    /// What a [`QirStop::Split`] holds: `T`, the calls of a piece, where
    /// a batch is split; a type with no value where none is.
    type Split<T>;
}

pub(super) mod sealed {
    use super::QirBatchSplit;

    /// What the walk asks of a split choice: hidden from resumers, which
    /// only choose one.
    pub trait Sealed {
        /// How many more calls the batch's piece may take, holding `held`
        /// since its last stop: `None` where a batch is never split.
        fn room(self, held: usize) -> Option<usize>;

        /// The [`QirBatchSplit::Split`] of `calls`, the calls of a piece.
        fn split<T>(calls: T) -> <Self as QirBatchSplit>::Split<T>
        where
            Self: QirBatchSplit;
    }
}
