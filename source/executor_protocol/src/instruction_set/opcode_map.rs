// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The translation of a sender's opcodes into its receiver's
//! ([`OpcodeMap`]), which the fit of two sets gives.

use super::Opcode;

/// The receiver's opcode for each opcode of the sending set: an opcode
/// is an instruction's place in its set, and the part below may number
/// the same instructions differently. `>>` makes it once, at the fit, and
/// hands it to the sender's `bind`; the sender translates every
/// instruction it sends with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpcodeMap {
    /// Indexed by the sent opcode.
    opcodes: Vec<Opcode>,
    by_mnemonic: Vec<String>,
}

impl OpcodeMap {
    /// The map of `opcodes`, indexed by the sent opcode, with the
    /// mnemonics of those `by_mnemonic`, as the fit finds them.
    pub(super) fn new(opcodes: Vec<Opcode>, by_mnemonic: Vec<String>) -> Self {
        Self {
            opcodes,
            by_mnemonic,
        }
    }

    /// The receiver's opcode for `sent`, an opcode of the sending set. The
    /// map holds one opcode per instruction of that set, by position, and
    /// does not know the set itself: an opcode of another set is not
    /// detected when its position is within the sending set, and gives the
    /// receiver's opcode for the sending set's instruction there.
    ///
    /// # Panics
    ///
    /// If `sent` lies beyond the sending set's last instruction.
    #[must_use]
    pub fn map(&self, sent: Opcode) -> Opcode {
        self.opcodes[sent.index()]
    }

    /// The mnemonics of the sent instructions matched by mnemonic alone,
    /// in the sending set's order: neither set declares what they do, so
    /// that they do the same rests on the sets' shared name. Only the
    /// sender, which receives this map, sees the list: the protocol
    /// reports it nowhere else.
    #[must_use]
    pub fn by_mnemonic(&self) -> &[String] {
        &self.by_mnemonic
    }
}
