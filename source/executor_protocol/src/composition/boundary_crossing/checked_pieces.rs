// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A checked stream's instructions as they cross a boundary
//! ([`CheckedPieces`]): checked a piece at a time, each piece whole before
//! the receiver gets any of its instructions, the first before the
//! receiver is called, so that the receiver runs only instructions it
//! accepts, as it does for a checked batch, and a refused stream never
//! reaches it.

use std::cell::Cell;
use std::collections::VecDeque;

use super::batch_limits::{BatchFault, BatchLimits};
use crate::Instruction;

/// The instructions of a checked stream, `instructions`, as the receiver
/// pulls them: a piece of at most `most` at a time, checked against
/// `batch_limits` as a checked batch's are, but for their stated slots,
/// which a stream has none of. The first piece is checked when it is made,
/// so that a fault there refuses the stream before the receiver is called.
/// At a later piece's fault, the stream ends there: none of the faulty
/// piece reaches the receiver, and the fault, with its position in the
/// stream, is left in `later_fault` for the boundary.
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
    /// The fault of a piece after the first, once found, its position
    /// counted from the stream's first instruction.
    later_fault: &'a Cell<Option<BatchFault>>,
}

impl<'a, I: Iterator<Item = Instruction>> CheckedPieces<'a, I> {
    /// `instructions`, checked against `batch_limits` in pieces of at most
    /// `most`, its first piece pulled and checked now; or that piece's
    /// fault. A later piece's fault is left in `later_fault`.
    pub(crate) fn first_checked(
        instructions: I,
        batch_limits: &'a BatchLimits,
        most: usize,
        later_fault: &'a Cell<Option<BatchFault>>,
    ) -> Result<Self, BatchFault> {
        let mut pieces = Self {
            instructions,
            batch_limits,
            most,
            piece: VecDeque::new(),
            before: 0,
            slots: 0,
            ended: false,
            later_fault,
        };
        pieces.check_next_piece()?;
        Ok(pieces)
    }

    /// Pulls and checks the next piece, once the one before it was pulled
    /// whole; on a fault, the stream ends, and the fault is returned, its
    /// position counted from the stream's first instruction.
    fn check_next_piece(&mut self) -> Result<(), BatchFault> {
        self.piece
            .extend(self.instructions.by_ref().take(self.most));
        // A short piece is the last: the source is not asked again once it
        // has ended, fused or not.
        self.ended = self.piece.len() < self.most;
        if self.piece.is_empty() {
            return Ok(());
        }
        match self
            .batch_limits
            .check_instructions(self.piece.make_contiguous(), self.slots)
        {
            Ok(slots) => {
                self.slots = slots;
                Ok(())
            }
            Err(fault) => {
                self.piece.clear();
                self.ended = true;
                Err(fault.after(self.before))
            }
        }
    }
}

impl<I: Iterator<Item = Instruction>> Iterator for CheckedPieces<'_, I> {
    type Item = Instruction;

    fn next(&mut self) -> Option<Instruction> {
        if self.piece.is_empty() {
            if self.ended {
                return None;
            }
            if let Err(fault) = self.check_next_piece() {
                self.later_fault.set(Some(fault));
            }
        }
        let instruction = self.piece.pop_front()?;
        self.before += 1;
        Some(instruction)
    }
}
