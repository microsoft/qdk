// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an `execute` asks to run: [`Batch`]es, each instructions on shots.

use crate::{Instruction, ShotSet};

/// A request: run `instructions`, in order, on each of `shots`.
///
/// An `execute` holds any number of batches, so that a part sends at once
/// the work of shots at different points of their programs; it appends one
/// [`BatchReadouts`](crate::BatchReadouts) per batch to the caller's
/// [`Readouts`](crate::Readouts), in the batches' order, with a row per shot in `shots`' order and a value per readout
/// slot of `instructions`. How the batches of one `execute` relate, such
/// as a shot in at most one of them, is said on
/// [`Execute::execute`](crate::Execute::execute).
#[derive(Clone, Copy, Debug)]
pub struct Batch<'a> {
    /// The shots the instructions run on, in the order of the answer's
    /// rows.
    pub shots: &'a ShotSet,
    /// The instructions, run in order on each shot.
    pub instructions: &'a [Instruction],
}
