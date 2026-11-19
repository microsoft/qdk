// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A stream of instructions, the streamed counterpart of a batch:
//! [`InstructionStream`], what
//! [`Execute::execute_stream`](crate::Execute::execute_stream) runs.

use crate::{Instruction, ShotSet};

/// A request, as a [`Batch`](crate::Batch) is, whose instructions come one
/// by one from an iterator rather than as a slice: run `instructions`, in
/// order, on each of `shots`, checked before they run if its sender marked
/// it [`checked`](Self::checked).
///
/// For a sender whose program is too long to hold, such as a time
/// evolution of millions of instructions per shot, or a program
/// interpreted as it runs: it hands the iterator to
/// [`Execute::execute_stream`](crate::Execute::execute_stream), and the
/// receiver pulls instructions as it runs them, by default in pieces of
/// the most it takes, so neither side holds the whole program. Its
/// instructions are in the receiver's opcodes, as a batch's are, and it
/// states no readout slots: the sender could not count them without
/// holding the program, so the receiver counts them as it runs.
///
/// [`new`](Self::new) makes a stream, unchecked; its `shots` and
/// `instructions` are public, and its checks are read by
/// [`is_checked`](Self::is_checked).
#[derive(Clone, Debug)]
pub struct InstructionStream<'a, I> {
    /// The shots the instructions run on, in the order of its readouts'
    /// rows.
    pub shots: &'a ShotSet,
    /// The instructions, run in order on each shot, pulled as they run.
    /// The stream ends at the iterator's first `None`, which need not be
    /// fused: each boundary the stream crosses gives its receiver nothing
    /// after it, and the default cuts never pull past it.
    pub instructions: I,
    /// Whether each boundary that receives it checks each piece before it
    /// runs.
    checked: bool,
}

impl<'a, I: Iterator<Item = Instruction>> InstructionStream<'a, I> {
    /// `instructions`, run in order on each of `shots`, unchecked: the
    /// receiver runs them as sent, as it runs an unchecked
    /// [`Batch`](crate::Batch). Mark it [`checked`](Self::checked) for the
    /// checks.
    #[must_use]
    pub const fn new(shots: &'a ShotSet, instructions: I) -> Self {
        Self {
            shots,
            instructions,
            checked: false,
        }
    }

    /// This stream, checked: each boundary that receives it checks each
    /// piece's instructions as it checks a
    /// [`checked`](crate::Batch::checked) batch's, each piece before any of
    /// its instructions reaches the
    /// receiver, and the first before the receiver is called, so a refused
    /// piece runs nothing; the pieces before it may have run. A refusal
    /// names the stream as batch 0, and an instruction by its position in
    /// the stream.
    #[must_use]
    pub fn checked(self) -> Self {
        self.checked_if(true)
    }

    /// This stream, [`checked`](Self::checked) if `on`, unchecked if not:
    /// what an adapter calls on each stream it sends with the
    /// [`is_checked`](Self::is_checked) of the stream it adapts, so that
    /// the checks its sender chose reach every boundary below.
    #[must_use]
    pub fn checked_if(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }

    /// Whether each boundary that receives this stream checks it, as
    /// [`checked`](Self::checked) says.
    #[must_use]
    pub const fn is_checked(&self) -> bool {
        self.checked
    }
}
