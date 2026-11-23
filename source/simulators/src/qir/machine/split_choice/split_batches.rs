// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Batches handed over in pieces of at most `n` calls ([`SplitBatches`]).

use std::num::NonZeroUsize;

use super::{QirSplitChoice, sealed::Sealed};
#[cfg(doc)]
use crate::qir::{QirCall, QirMachine, QirStop};

/// Each batch is handed over in pieces of at most `n` quantum calls
/// ([`QirCall`]): a [`QirStop::Split`] for each piece but the last, which
/// is the batch's [`QirStop::Batch`]. For a resumer that streams the calls
/// to its target, so that the context never holds more than `n` calls.
///
/// The batch itself does not change: the pieces of a batch that ends, in
/// order, are the calls of the batch a machine of
/// [`WholeBatches`](super::WholeBatches) gives, and
/// its outcomes are asked once, at its end, by the resume after its
/// [`QirStop::Batch`]. The resume after a [`QirStop::Split`] takes no
/// outcomes. A batch of at most `n` calls comes whole, as a
/// [`QirStop::Batch`].
///
/// A batch whose walk traps has no end. Handed over whole, none of its
/// calls is: the trap comes at once. Split, every call walked before the
/// trap is handed over, those since the last stop as a last split
/// ([`QirStop::Split`]), and the next resume gives the trap; so a resumer
/// that streams has sent every call the shot made before it trapped.
///
/// Each stop, a split or the batch's end, holds at most `n` calls: the
/// batch is cut where an (`n` + 1)-th call would join the piece, wherever
/// that call comes from, including inside a run of calls the program
/// prepared together, and before a trap. A piece holds at least one call.
///
/// For example, a resumer that streams each batch to its target drives a
/// shot's context as below, where `send` sends calls to the target and
/// `receive` gives the batch's outcomes once its last calls are sent:
///
/// ```
/// # use std::num::NonZeroUsize;
/// # use qdk_simulators::OutputRecord;
/// # use qdk_simulators::qir::{
/// #     QirCall, QirMachine, QirOutcome, QirProgram, QirShotContext, QirStop,
/// #     QirTrap, SplitBatches,
/// # };
/// # fn shot(
/// #     program: &QirProgram,
/// #     context: &mut QirShotContext,
/// #     mut send: impl FnMut(&[QirCall]),
/// #     mut receive: impl FnMut(&mut Vec<QirOutcome>),
/// # ) -> Result<Vec<OutputRecord>, QirTrap> {
/// let machine = QirMachine::new(program).with_split(SplitBatches::at_most(
///     NonZeroUsize::new(1024).expect("1024 is not zero"),
/// ));
/// machine.restart_shot(context);
/// let mut outcomes = Vec::new();
/// loop {
///     match machine.resume(context, &outcomes)? {
///         QirStop::Split(calls) => {
///             outcomes.clear();
///             send(calls);
///         }
///         QirStop::Batch(calls) => {
///             outcomes.clear();
///             send(calls);
///             receive(&mut outcomes);
///         }
///         QirStop::Ended(records) => return Ok(records.to_vec()),
///     }
/// }
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitBatches {
    max_calls: NonZeroUsize,
}

impl SplitBatches {
    /// Pieces of at most `calls` quantum calls each. A count of zero is
    /// no piece size, so the type refuses it.
    #[must_use]
    pub fn at_most(calls: NonZeroUsize) -> Self {
        Self { max_calls: calls }
    }
}

impl QirSplitChoice for SplitBatches {
    type Split<T> = T;
}

impl Sealed for SplitBatches {
    #[inline]
    fn room(self, held: usize) -> Option<usize> {
        Some(self.max_calls.get().saturating_sub(held))
    }

    #[inline]
    fn piece<T>(calls: T) -> T {
        calls
    }
}
