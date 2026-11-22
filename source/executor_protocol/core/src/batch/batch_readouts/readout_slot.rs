// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A readout's slot in a batch's rows ([`ReadoutSlot`]): its position,
//! the same on every shot.

/// A readout slot, by its position in a batch's rows, k: slot k is the same
/// readout on every shot of the batch. It must be below the rows'
/// [`readout_slots`](crate::BatchReadouts::readout_slots), which
/// [`BatchReadouts::value`](crate::BatchReadouts::value) checks: those the
/// batch's sender states, since the protocol shapes the rows from the
/// batch.
///
/// A `u32`, at no cost over one, so that a slot is not mistaken for a
/// shot's position or a count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ReadoutSlot(u32);

impl ReadoutSlot {
    /// Slot `k`.
    #[must_use]
    pub const fn new(k: u32) -> Self {
        Self(k)
    }

    /// The slot's position, k.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}
