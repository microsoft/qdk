// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Batches handed over whole ([`WholeBatches`]): a machine's default.

use std::convert::Infallible;

use super::{QirBatchSplit, sealed::Sealed};
#[cfg(doc)]
use crate::qir::{QirMachine, QirStop};

/// Each batch is handed over whole, in one [`QirStop::Batch`]: the
/// default of [`QirMachine`]. A batch has no bound on its calls: a loop of
/// quantum calls that never reads a result adds every round to one batch.
/// A batch whose walk traps is not handed over: the trap comes at once.
/// [`QirStop::Split`] holds [`Infallible`], so a resumer's `match` has no
/// arm for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WholeBatches;

impl QirBatchSplit for WholeBatches {
    type Split<T> = Infallible;
}

impl Sealed for WholeBatches {
    #[inline]
    fn room(self, _held: usize) -> Option<usize> {
        None
    }

    #[inline]
    fn split<T>(_calls: T) -> Infallible {
        unreachable!("the walk never splits a whole batch: `room` is `None`")
    }
}
