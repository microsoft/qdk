// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an `execute` sends down and gets back: [`Batch`]es, each
//! instructions on a [`ShotSet`] of [`ShotId`]s, checked before they run
//! when their sender asks, and for each its [`BatchReadouts`].

mod batch_readouts;
mod shot_set;

pub use batch_readouts::{
    BatchReadouts, ReadoutFailure, ReadoutSlot, ReadoutSlots, ReadoutValue, Readouts,
    ShotConditions,
};
pub use shot_set::{ShotId, ShotSet, ShotSetFault};

use std::fmt;

use crate::InstructionSet;

/// A request: run `instructions`, of the set `S`, in order, on each of
/// `shots`, checked before it runs if its sender marked it
/// [`checked`](Self::checked).
///
/// An `execute` holds any number of batches, so that a part sends at once
/// the work of shots at different points of their programs; it answers
/// each in the caller's [`BatchReadouts`](crate::BatchReadouts) for it,
/// which the protocol shapes from the batch: a row per shot in `shots`'
/// order, of a value per readout slot of `instructions`, as many as the
/// sender states ([`readout_slots`](Self::readout_slots)).
/// How the batches of one `execute` relate, such as a shot in at most one
/// of them, is said on [`Executor::execute`](crate::Executor::execute).
///
/// [`new`](Self::new) makes a batch, unchecked; its fields are public to
/// read, its readout slots are read by
/// [`readout_slots`](Self::readout_slots), and its checks by
/// [`is_checked`](Self::is_checked).
pub struct Batch<'a, S: InstructionSet> {
    /// The shots the instructions run on, in the order of its readouts'
    /// rows.
    pub shots: &'a ShotSet,
    /// The instructions, run in order on each shot.
    pub instructions: &'a [S::Instruction],
    /// The readout slots of `instructions`, summed, as the sender states
    /// them: the readout slots of each row of its readouts.
    readout_slots: ReadoutSlots,
    /// Whether the boundary that receives the batch checks it before it
    /// runs.
    checked: bool,
}

impl<'a, S: InstructionSet> Batch<'a, S> {
    /// `instructions`, run in order on each of `shots`, whose readout
    /// slots sum to `readout_slots`, unchecked: the receiver runs them as
    /// sent, if they are no more than it takes, and the protocol shapes
    /// its readouts with `readout_slots` values per shot without reading an
    /// instruction. Mark it [`checked`](Self::checked) for the rest.
    ///
    /// The sender states the slots because it knows them where it writes
    /// the instructions, once, while each boundary below would count them
    /// again: it adds each instruction's
    /// ([`InstructionSet::readout_slots`]) as it writes it. An adapter
    /// that passes a batch's instructions on, or rewrites them one for
    /// one with the same readouts, passes its slots on as they are.
    #[must_use]
    pub const fn new(
        shots: &'a ShotSet,
        instructions: &'a [S::Instruction],
        readout_slots: ReadoutSlots,
    ) -> Self {
        Self {
            shots,
            instructions,
            readout_slots,
            checked: false,
        }
    }

    /// This batch, checked: the boundary that receives it refuses it,
    /// before anything of it runs, as an
    /// [`ExecuteRefusal`](crate::ExecuteRefusal) naming the batch, the
    /// batch's first shot and, for a fault at an instruction, its position,
    /// when it holds an instruction the receiver's set refuses
    /// ([`InstructionSet::check`]), given what arrives at the receiver,
    /// more readout slots than a `u32` counts, or readout slots other than
    /// [`readout_slots`](Self::readout_slots) states.
    ///
    /// An unchecked batch of no more instructions than the receiver takes
    /// runs as sent. A faulty one is then not reported as its sender's
    /// fault:
    /// - the receiver may panic on it, failing the run naming the receiver
    ///   ([`Failure::Panicked`](crate::Failure::Panicked));
    /// - an instruction the receiver does not accept may run as another of
    ///   its instructions, unreported;
    /// - readout slots misstated shape its readouts wrong. Too few, and the
    ///   receiver may panic writing past its rows. Too many, and the last
    ///   values of each row stay `Missing(ReadoutFailure::Other)`, at the
    ///   cost of their memory (shots × the stated slots values, allocated in
    ///   the sender's call); more than the memory holds, and the process may
    ///   end, as on an allocation that fails, rather than the run fail,
    ///   since only a checked batch's slots are compared with its
    ///   instructions before its readouts are shaped.
    ///
    /// The checks cost a pass over the batch's instructions at each boundary
    /// that receives it: little beside an executor that simulates each
    /// instruction, but more than double the time of a chain whose parts do
    /// little per instruction, such as a lowering over an executor that only
    /// counts it. An unchecked batch crosses a boundary without its
    /// instructions being read. So a chain of parts that keep the protocol's
    /// rules sends its batches unchecked. A sender may check some batches and
    /// not others: all of them while a part is developed, or those a part
    /// sends that may hold what the receiver does not accept, such as a part
    /// built from a user's description at run time.
    ///
    /// Checked or not, the chain's capabilities when it is composed, the
    /// refusals of a run ([`RunRefusal`](crate::RunRefusal)), and the
    /// batch's count of instructions against the most the receiver takes
    /// (an [`ExecuteRefusal`](crate::ExecuteRefusal)), are checked.
    #[must_use]
    pub const fn checked(self) -> Self {
        self.checked_if(true)
    }

    /// This batch, [`checked`](Self::checked) if `on`, unchecked if not:
    /// what an adapter calls on each batch it sends with the
    /// [`is_checked`](Self::is_checked) of the batch it adapts, as
    /// [`Adapter::adapt`](crate::Adapter::adapt)'s rule asks, so that the
    /// checks its sender chose reach every boundary below.
    #[must_use]
    pub const fn checked_if(self, on: bool) -> Self {
        Self {
            checked: on,
            ..self
        }
    }

    /// The readout slots of its instructions, summed, as its sender
    /// stated them in [`new`](Self::new): the readout slots of each row of
    /// its readouts.
    #[must_use]
    pub const fn readout_slots(&self) -> ReadoutSlots {
        self.readout_slots
    }

    /// Whether the boundary that receives this batch checks it before it
    /// runs, as [`checked`](Self::checked) says.
    #[must_use]
    pub const fn is_checked(&self) -> bool {
        self.checked
    }
}

#[expect(
    clippy::expl_impl_clone_on_copy,
    reason = "a derive would bound `S: Copy`"
)]
impl<S: InstructionSet> Clone for Batch<'_, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: InstructionSet> Copy for Batch<'_, S> {}

impl<S: InstructionSet> fmt::Debug for Batch<'_, S>
where
    S::Instruction: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Batch")
            .field("shots", &self.shots)
            .field("instructions", &self.instructions)
            .field("readout_slots", &self.readout_slots)
            .field("checked", &self.checked)
            .finish()
    }
}
