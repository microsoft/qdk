// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an `execute` asks to run: [`Batch`]es, each instructions on shots,
//! checked before they run when their sender asks.

use crate::{Instruction, ShotSet};

/// A request: run `instructions`, in order, on each of `shots`, checked
/// before it runs if its sender marked it [`checked`](Self::checked).
///
/// An `execute` holds any number of batches, so that a part sends at once
/// the work of shots at different points of their programs; it appends one
/// [`BatchReadouts`](crate::BatchReadouts) per batch to the caller's
/// [`Readouts`](crate::Readouts), in the batches' order, with a row per
/// shot in `shots`' order and a value per readout slot of `instructions`.
/// How the batches of one `execute` relate, such as a shot in at most one
/// of them, is said on [`Execute::execute`](crate::Execute::execute).
///
/// [`new`](Self::new) makes a batch, unchecked; its fields are public to
/// read, and its checks are read by [`is_checked`](Self::is_checked).
#[derive(Clone, Copy, Debug)]
pub struct Batch<'a> {
    /// The shots the instructions run on, in the order of the answer's
    /// rows.
    pub shots: &'a ShotSet,
    /// The instructions, run in order on each shot.
    pub instructions: &'a [Instruction],
    /// Whether the boundary that receives the batch checks it before it
    /// runs.
    checked: bool,
}

impl<'a> Batch<'a> {
    /// `instructions`, run in order on each of `shots`, unchecked: the
    /// receiver runs them as sent, and its answer is checked for what costs
    /// nothing per instruction, a readouts for each batch and a row for
    /// each shot. Mark it [`checked`](Self::checked) for the rest.
    #[must_use]
    pub const fn new(shots: &'a ShotSet, instructions: &'a [Instruction]) -> Self {
        Self {
            shots,
            instructions,
            checked: false,
        }
    }

    /// This batch, checked: the boundary that receives it refuses it,
    /// before anything of it runs, as an
    /// [`ExecuteRefusal`](crate::ExecuteRefusal) naming the batch, the
    /// instruction and the batch's first shot, when it holds an instruction
    /// the receiver does not accept (an opcode beyond its set, or blocks,
    /// arguments or a flag `select` that do not match its instruction), a
    /// block beyond the shots' blocks of its type, or given twice by one
    /// instruction, a readout parity the shots' needs do not use, more
    /// instructions than the receiver takes, or more readout slots than a
    /// `u32` counts; and it checks that each row of the answer holds a
    /// value per readout slot.
    ///
    /// An unchecked batch runs as sent. A faulty one is then not reported
    /// as its sender's fault: the receiver may panic on it, failing the
    /// run naming the receiver
    /// ([`Failure::Panicked`](crate::Failure::Panicked)), and an opcode of
    /// another set may run as another of the receiver's instructions,
    /// unreported. The checks cost a pass over the batch's instructions,
    /// which about doubles the protocol's own time per instruction, so a
    /// chain of parts that keep the protocol's rules sends its batches
    /// unchecked. A sender may check some batches and not others: all of
    /// them while a part is developed, or those a part sends that may hold
    /// what the receiver does not accept, such as a part built from a
    /// user's description at run time.
    ///
    /// Checked or not, the chain's fit and admission by `>>`, the refusals
    /// of a run ([`RunRefusal`](crate::RunRefusal)), and the answer's count
    /// of readouts and rows are checked.
    #[must_use]
    pub const fn checked(self) -> Self {
        self.checked_if(true)
    }

    /// This batch, [`checked`](Self::checked) if `on`, unchecked if not:
    /// what an adapter calls on each batch it sends with the
    /// [`is_checked`](Self::is_checked) of the batch it adapts, so that the
    /// checks its sender chose reach every boundary below.
    #[must_use]
    pub const fn checked_if(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }

    /// Whether the boundary that receives this batch checks it before it
    /// runs, as [`checked`](Self::checked) says.
    #[must_use]
    pub const fn is_checked(&self) -> bool {
        self.checked
    }
}
