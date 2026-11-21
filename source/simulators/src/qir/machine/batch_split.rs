// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Whether a [`QirMachine`] hands a batch over whole or split
//! ([`QirBatchSplit`]): a choice its resumer makes once, when it takes the
//! machine ([`QirMachine::with_split`]).
//!
//! A batch ends only where a shot needs one of its outcomes, or writes
//! again a result the batch writes, so a program that reads no result until
//! its end, and writes each result once, is one batch, which grows with the
//! program: an unrolled Trotter evolution is one long line of gates. A
//! resumer that streams the calls to its target (one that runs a batch's
//! first calls while the machine walks the rest, or whose queue is bounded)
//! takes such a batch in pieces of at most `n` calls ([`SplitBatches`]),
//! and the context then holds no more than `n` calls at once. Every other
//! resumer takes whole batches ([`WholeBatches`], the default), and its
//! `match` on the stops has no arm for a split, which never comes.
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
    /// a batch is split; a type with no value where none is, so that
    /// a `match` on the stops needs no arm for it.
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
