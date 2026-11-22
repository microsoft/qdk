// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The buffer a caller keeps its readouts in: [`Readouts`], one
//! [`BatchReadouts`] per batch of an `execute`.
//!
//! A part that calls `execute` passes it `readouts`, one `BatchReadouts`
//! per batch, which the protocol shapes from the batches and the executor
//! fills, as `std::io::Read::read` fills the caller's slice. So a caller
//! that sends many small `execute`s, such as a driver sending a round at a
//! time, keeps one `Readouts`, whose memory each batch's readouts reuse:
//! once it has held readouts as large, shaping them allocates nothing.

use super::BatchReadouts;
use crate::Batch;

/// The readouts of a caller's `execute`s, kept from one to the next so
/// that each reuses their memory: [`readouts_for`](Self::readouts_for)
/// gives one [`BatchReadouts`] per batch, to pass to their `execute`.
#[derive(Default)]
pub struct Readouts {
    /// One per batch: those of the last `execute` first, then any kept,
    /// with their memory, from an earlier `execute` of more batches.
    batch_readouts: Vec<BatchReadouts>,
}

impl Readouts {
    /// No readouts, and no memory yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// One `BatchReadouts` per batch of `batches`, in their order, to pass
    /// to their `execute` through a chain, whose boundary shapes them
    /// before the part below fills them: `readouts_for(batches)[i]` holds
    /// the readouts of `batches[i]`. They reuse the memory of the readouts
    /// it held before, and are not shaped here: what they hold until that
    /// `execute` is unspecified. So a caller that calls an executor's
    /// `execute` directly, outside a chain (such as the executor's own
    /// tests), passes it
    /// [`BatchReadouts::new`]`(batch.shots.len(), batch.readout_slots())`
    /// for each batch instead.
    #[inline]
    pub fn readouts_for(&mut self, batches: &[Batch<'_>]) -> &mut [BatchReadouts] {
        let count = batches.len();
        if self.batch_readouts.len() < count {
            self.batch_readouts
                .resize_with(count, || BatchReadouts::new(0, 0));
        }
        &mut self.batch_readouts[..count]
    }
}
