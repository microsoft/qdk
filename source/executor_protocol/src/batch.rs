// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an `execute` sends down and gets back: [`Batch`]es, each
//! instructions on a [`ShotSet`] of [`ShotId`]s, checked before they run
//! when their sender asks, and for each its [`BatchReadouts`]; and the
//! batch whose instructions come as they are produced, an
//! [`InstructionStream`].

mod batch_readouts;
mod instruction_stream;
mod shot_set;

pub use batch_readouts::{
    BatchReadouts, ReadoutFailure, ReadoutSlot, ReadoutValue, Readouts, ShotConditions,
};
pub use instruction_stream::InstructionStream;
pub use shot_set::{ShotId, ShotSet, ShotSetFault};

use crate::Instruction;

/// A request: run `instructions`, in order, on each of `shots`, checked
/// before it runs if its sender marked it [`checked`](Self::checked).
///
/// An `execute` holds any number of batches, so that a part sends at once
/// the work of shots at different points of their programs; it answers
/// each in the caller's [`BatchReadouts`](crate::BatchReadouts) for it,
/// which the protocol shapes from the batch: a row per shot in `shots`'
/// order, of a value per readout slot of `instructions`, as many as the
/// sender states ([`readout_slots`](Self::readout_slots)).
/// How the batches of one `execute` relate, such as a shot in at most one
/// of them, is said on [`Execute::execute`](crate::Execute::execute).
///
/// [`new`](Self::new) makes a batch, unchecked; its fields are public to
/// read, its readout slots are read by
/// [`readout_slots`](Self::readout_slots), and its checks by
/// [`is_checked`](Self::is_checked).
#[derive(Clone, Copy, Debug)]
pub struct Batch<'a> {
    /// The shots the instructions run on, in the order of its readouts'
    /// rows.
    pub shots: &'a ShotSet,
    /// The instructions, run in order on each shot.
    pub instructions: &'a [Instruction],
    /// The readout slots of `instructions`, summed, as the sender states
    /// them: the readout slots of each row of its readouts.
    readout_slots: u32,
    /// Whether the boundary that receives the batch checks it before it
    /// runs.
    checked: bool,
}

impl<'a> Batch<'a> {
    /// `instructions`, run in order on each of `shots`, whose readout
    /// slots sum to `readout_slots`, unchecked: the receiver runs them as
    /// sent, if they are no more than it takes, and the protocol shapes
    /// its readouts with `readout_slots` values per shot without reading an
    /// instruction. Mark it [`checked`](Self::checked) for the rest.
    ///
    /// The sender states the slots because it knows them where it writes
    /// the instructions, once, while each boundary below would count them
    /// again: [`InstructionSet::readout_slots_of`] sums them, or the sender
    /// adds each instruction's ([`InstructionSet::readout_slots`]) as it
    /// writes it. An adapter that passes a batch's instructions on, or
    /// rewrites only their opcodes into the receiver's set
    /// ([`OpcodeMap`](crate::OpcodeMap)), passes its slots on as they are.
    ///
    /// [`InstructionSet::readout_slots_of`]: crate::InstructionSet::readout_slots_of
    /// [`InstructionSet::readout_slots`]: crate::InstructionSet::readout_slots
    #[must_use]
    pub const fn new(
        shots: &'a ShotSet,
        instructions: &'a [Instruction],
        readout_slots: u32,
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
    /// when it holds an instruction the receiver does not accept (an opcode
    /// beyond its set, or blocks, arguments or a flag `select` that do not
    /// match its instruction), a block beyond the shots' blocks of its type,
    /// or given twice by one instruction, a readout parity the shots' needs
    /// do not use, more readout slots than a `u32` counts, or readout slots
    /// other than [`readout_slots`](Self::readout_slots) states.
    ///
    /// An unchecked batch of no more instructions than the receiver takes
    /// runs as sent. A faulty one is then not reported as its sender's
    /// fault:
    /// - the receiver may panic on it, failing the run naming the receiver
    ///   ([`Failure::Panicked`](crate::Failure::Panicked));
    /// - an opcode of another set may run as another of the receiver's
    ///   instructions, unreported;
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
    /// Checked or not, the chain's fit and admission by `>>`, the refusals
    /// of a run ([`RunRefusal`](crate::RunRefusal)), and the batch's count
    /// of instructions against the most the receiver takes (an
    /// [`ExecuteRefusal`](crate::ExecuteRefusal)), are checked.
    #[must_use]
    pub const fn checked(self) -> Self {
        self.checked_if(true)
    }

    /// This batch, [`checked`](Self::checked) if `on`, unchecked if not:
    /// what an adapter calls on each batch it sends with the
    /// [`is_checked`](Self::is_checked) of the batch it adapts, as
    /// [`Adapt::adapt`](crate::Adapt::adapt)'s rule asks, so that the checks
    /// its sender chose reach every boundary below.
    #[must_use]
    pub const fn checked_if(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }

    /// The readout slots of its instructions, summed, as its sender
    /// stated them in [`new`](Self::new): the readout slots of each row of
    /// its readouts.
    #[must_use]
    pub const fn readout_slots(&self) -> u32 {
        self.readout_slots
    }

    /// Whether the boundary that receives this batch checks it before it
    /// runs, as [`checked`](Self::checked) says.
    #[must_use]
    pub const fn is_checked(&self) -> bool {
        self.checked
    }
}
