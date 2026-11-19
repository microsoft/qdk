// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot's row, final before its `execute` answers: [`FinalRow`], as
//! [`Execute::execute_each`](crate::Execute::execute_each) gives it.

use crate::{Batch, BatchReadouts, ReadoutValue, ShotConditions};

/// A shot's row, final before its `execute` answers, as
/// [`Execute::execute_each`](crate::Execute::execute_each) gives it: where
/// it is in the call (its batch and the shot's position there), and what it
/// holds. It carries the row itself, not its batch's readouts, since the
/// call holds `readouts` until it answers.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct FinalRow<'a> {
    /// The batch's index in the `execute`.
    pub batch: usize,
    /// The shot's position in the batch's set.
    pub position: usize,
    /// The row's values, as `readouts[batch].row(position)` holds them
    /// when the `execute` answers `Ok`.
    pub values: &'a [ReadoutValue],
    /// The row's conditions, as `readouts[batch].conditions(position)`
    /// holds them when the `execute` answers `Ok`.
    pub conditions: ShotConditions,
}

impl<'a> FinalRow<'a> {
    /// The row at `position` of `readouts`, the readouts of the `batch`th
    /// batch of its `execute`.
    ///
    /// # Panics
    ///
    /// If `position` is not a row of `readouts`.
    #[must_use]
    pub fn of(batch: usize, position: usize, readouts: &'a BatchReadouts) -> Self {
        Self {
            batch,
            position,
            values: readouts.row(position),
            conditions: readouts.conditions(position),
        }
    }
}

/// Gives `each` every row of `readouts`, one per batch of `batches`, batch
/// by batch: what the default `_each` forms give once `execute` or `adapt`
/// answers `Ok`.
pub(crate) fn give_every_row(
    batches: &[Batch<'_>],
    readouts: &[BatchReadouts],
    each: &mut impl FnMut(FinalRow<'_>),
) {
    for (batch, (sent, readouts)) in batches.iter().zip(readouts).enumerate() {
        for position in 0..sent.shots.len() {
            each(FinalRow::of(batch, position, readouts));
        }
    }
}
