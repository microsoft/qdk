// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A checked stream's instructions as they cross a boundary
//! ([`CheckedPieces`]): checked a piece at a time, each piece whole before
//! the receiver gets any of its instructions, so that the receiver runs
//! only instructions it accepts, as it does for a checked batch.

use std::cell::Cell;
use std::collections::VecDeque;

use super::batch_limits::{BatchFault, BatchLimits};
use crate::Instruction;

/// The instructions of a checked stream, `instructions`, as the receiver
/// pulls them: a piece of at most `most` at a time, checked against
/// `batch_limits` as a checked batch's are, but for their stated slots,
/// which a stream has none of. At the first fault, the stream ends there:
/// none of the faulty piece reaches the receiver, and the fault, with its
/// position in the stream, is left in `fault` for the boundary.
///
/// It holds one piece: the instructions checked and not yet pulled.
pub(crate) struct CheckedPieces<'a, I> {
    instructions: I,
    batch_limits: &'a BatchLimits,
    most: usize,
    /// The piece checked, as the receiver has not yet pulled it.
    piece: VecDeque<Instruction>,
    /// The instructions the receiver pulled.
    before: usize,
    /// The readout slots of the pieces checked so far.
    slots: u32,
    /// Whether the stream ended, or was refused.
    ended: bool,
    /// The stream's fault, once found.
    fault: &'a Cell<Option<StreamFault>>,
}

/// A checked stream's fault, as [`CheckedPieces`] finds it.
pub(crate) struct StreamFault {
    /// The fault, its position counted from the stream's first instruction.
    pub(crate) fault: BatchFault,
    /// Whether any of the stream reached the receiver before it.
    pub(crate) after_any: bool,
}

impl<'a, I: Iterator<Item = Instruction>> CheckedPieces<'a, I> {
    /// `instructions`, checked against `batch_limits` in pieces of at most
    /// `most`, the stream's fault left in `fault`.
    pub(crate) fn new(
        instructions: I,
        batch_limits: &'a BatchLimits,
        most: usize,
        fault: &'a Cell<Option<StreamFault>>,
    ) -> Self {
        Self {
            instructions,
            batch_limits,
            most,
            piece: VecDeque::new(),
            before: 0,
            slots: 0,
            ended: false,
            fault,
        }
    }

    /// Pulls and checks the next piece, once the one before it was pulled
    /// whole: `false` once the stream ended, or was refused.
    fn check_next_piece(&mut self) -> bool {
        self.piece
            .extend(self.instructions.by_ref().take(self.most));
        // A short piece is the last: the source is not asked again once it
        // has ended, fused or not.
        self.ended = self.piece.len() < self.most;
        if self.piece.is_empty() {
            return false;
        }
        match self
            .batch_limits
            .check_instructions(self.piece.make_contiguous(), self.slots)
        {
            Ok(slots) => {
                self.slots = slots;
                true
            }
            Err(fault) => {
                self.piece.clear();
                self.ended = true;
                self.fault.set(Some(StreamFault {
                    fault: fault.after(self.before),
                    after_any: self.before > 0,
                }));
                false
            }
        }
    }
}

impl<I: Iterator<Item = Instruction>> Iterator for CheckedPieces<'_, I> {
    type Item = Instruction;

    fn next(&mut self) -> Option<Instruction> {
        if self.piece.is_empty() && (self.ended || !self.check_next_piece()) {
            return None;
        }
        self.before += 1;
        self.piece.pop_front()
    }
}
