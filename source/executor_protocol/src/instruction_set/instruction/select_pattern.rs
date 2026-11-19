// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The flags an instruction expects without noise ([`SelectPattern`]), set
//! by name with [`Instruction::select`], as Qodec's per-call `select`:
//! `select: [reject: 0, leak: 0]` is two patterns, of which one must
//! match. A sampler or a decoder may use them to keep or drop a shot.

use super::super::InstructionSet;
use super::{Instruction, InstructionFault};

/// One pattern of an instruction's flags, over its flags by position: the
/// flags it names (`mask`) and their values (`bits`). A shot's flags
/// match it when they agree on every flag it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectPattern {
    mask: u64,
    /// Within `mask`.
    bits: u64,
}

impl SelectPattern {
    /// Whether `flags`, the instruction's flags of one shot (flag `k` in
    /// bit `k`), agree with this pattern on every flag it names.
    #[must_use]
    pub fn matches(self, flags: u64) -> bool {
        flags & self.mask == self.bits
    }

    /// The flags it names, flag `k` in bit `k`.
    pub(crate) fn mask(self) -> u64 {
        self.mask
    }
}

impl Instruction {
    /// This instruction, expecting its flags without noise to match one of
    /// `patterns`, each a list of flags by name with their values, 0 or 1:
    /// `[[("reject", 0)], [("leak", 0)]]` is Qodec's `select: [reject: 0,
    /// leak: 0]`, either flag 0; `[[("reject", 0), ("leak", 1)]]` its
    /// `select: [{reject: 0, leak: 1}]`, both. No pattern (`[]`) expects
    /// nothing, as in Qodec. An empty pattern (`[[]]`) names no flag, so
    /// every shot's flags match it.
    ///
    /// # Errors
    ///
    /// The first fault: an opcode beyond `set`'s last instruction
    /// ([`UnknownOpcode`](InstructionFault::UnknownOpcode)), a flag it
    /// lacks ([`UnknownFlag`](InstructionFault::UnknownFlag)), one flag
    /// twice in a pattern
    /// ([`RepeatedFlag`](InstructionFault::RepeatedFlag)), a value other
    /// than 0 or 1
    /// ([`FlagValueNotABit`](InstructionFault::FlagValueNotABit)).
    pub fn select<'n>(
        mut self,
        set: &InstructionSet,
        patterns: impl IntoIterator<Item = impl IntoIterator<Item = (&'n str, u8)>>,
    ) -> Result<Self, InstructionFault> {
        let def = set
            .def(self.opcode)
            .ok_or(InstructionFault::UnknownOpcode(self.opcode))?;
        let mnemonic = || def.mnemonic.clone();
        let mut built: Vec<SelectPattern> = Vec::new();
        for pattern in patterns {
            let mut select = SelectPattern { mask: 0, bits: 0 };
            for (name, value) in pattern {
                let flag = def
                    .flags
                    .iter()
                    .position(|flag| flag == name)
                    .ok_or_else(|| InstructionFault::UnknownFlag {
                        mnemonic: mnemonic(),
                        name: name.to_owned(),
                    })?;
                let bit = 1u64 << flag;
                if select.mask & bit != 0 {
                    return Err(InstructionFault::RepeatedFlag {
                        mnemonic: mnemonic(),
                        name: name.to_owned(),
                    });
                }
                let set_bit = match value {
                    0 => 0,
                    1 => bit,
                    _ => {
                        return Err(InstructionFault::FlagValueNotABit {
                            mnemonic: mnemonic(),
                            name: name.to_owned(),
                            value,
                        });
                    }
                };
                select.mask |= bit;
                select.bits |= set_bit;
            }
            built.push(select);
        }
        self.select = (!built.is_empty()).then(|| built.into_boxed_slice());
        Ok(self)
    }
}
