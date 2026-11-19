// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A checked stream's instructions as they cross a boundary
//! ([`CheckedPieces`]): checked a piece at a time, in the pieces the
//! receiver's default `execute_stream` cuts, each piece whole before the
//! receiver gets any of its instructions, the first before the receiver
//! is called, so that the receiver runs only instructions it accepts, as
//! it does for a checked batch, a refused stream never reaches it, and the
//! boundary holds one piece however much the receiver takes at once.

use std::cell::Cell;

use super::batch_limits::{BatchFault, BatchLimits};
use crate::Instruction;

/// The instructions of a checked stream, `instructions`, as the receiver
/// pulls them: a piece of at most `most` at a time, checked against
/// `batch_limits` as a checked batch's are, but for their stated slots,
/// which a stream has none of. The first piece is checked when it is made,
/// so that a fault there refuses the stream before the receiver is called.
/// At a later piece's fault, the stream ends there: none of the faulty
/// piece reaches the receiver, and the fault, with the instructions before
/// its piece, is left in `later_fault` for the boundary.
///
/// It holds one piece: the instructions checked and not yet pulled, at
/// most `most`, whatever the receiver pulls at once.
pub(crate) struct CheckedPieces<'a, I> {
    instructions: I,
    batch_limits: &'a BatchLimits,
    most: usize,
    /// The piece checked, as the receiver has not yet pulled it, last
    /// first, so that each pull pops it.
    piece: Vec<Instruction>,
    /// The instructions the receiver pulled.
    before: usize,
    /// The readout slots of the pieces checked so far.
    slots: u32,
    /// Whether a piece was refused, which ends the stream.
    refused: bool,
    /// The fault of a piece after the first, once found, its position
    /// counted in its piece, and the instructions before that piece.
    later_fault: &'a Cell<Option<(BatchFault, usize)>>,
}

impl<'a, I: Iterator<Item = Instruction>> CheckedPieces<'a, I> {
    /// `instructions`, checked against `batch_limits` in pieces of at most
    /// `most`, its first piece pulled and checked now; or that piece's
    /// fault. A later piece's fault is left in `later_fault`.
    pub(crate) fn first_checked(
        instructions: I,
        batch_limits: &'a BatchLimits,
        most: usize,
        later_fault: &'a Cell<Option<(BatchFault, usize)>>,
    ) -> Result<Self, BatchFault> {
        let mut pieces = Self {
            instructions,
            batch_limits,
            most,
            piece: Vec::new(),
            before: 0,
            slots: 0,
            refused: false,
            later_fault,
        };
        pieces.check_next_piece()?;
        Ok(pieces)
    }

    /// Whether the stream ends where the receiver stopped pulling: no
    /// checked instruction is left for it, and its sender, asked once more
    /// unless the stream already ended, gives none. So that the boundary
    /// can tell a receiver that took the stream whole without pulling its
    /// final `None` from one that answered early, pulling the sender at
    /// most once, and checking nothing more.
    pub(crate) fn ends_here(&mut self) -> bool {
        self.piece.is_empty() && (self.refused || self.instructions.next().is_none())
    }

    /// Pulls and checks the next piece, once the one before it was pulled
    /// whole; on a fault, the stream ends, and the fault is returned, its
    /// position counted in the piece.
    fn check_next_piece(&mut self) -> Result<(), BatchFault> {
        // The boundary fuses `instructions`, so a pull after the stream's
        // end makes an empty piece.
        self.piece
            .extend(self.instructions.by_ref().take(self.most));
        if self.piece.is_empty() {
            return Ok(());
        }
        match self
            .batch_limits
            .check_instructions(&self.piece, self.slots)
        {
            Ok(slots) => {
                self.slots = slots;
                self.piece.reverse();
                Ok(())
            }
            Err(fault) => {
                self.piece.clear();
                self.refused = true;
                Err(fault)
            }
        }
    }
}

impl<I: Iterator<Item = Instruction>> Iterator for CheckedPieces<'_, I> {
    type Item = Instruction;

    fn next(&mut self) -> Option<Instruction> {
        if self.piece.is_empty() {
            if self.refused {
                return None;
            }
            if let Err(fault) = self.check_next_piece() {
                self.later_fault.set(Some((fault, self.before)));
            }
        }
        let instruction = self.piece.pop()?;
        self.before += 1;
        Some(instruction)
    }
}
