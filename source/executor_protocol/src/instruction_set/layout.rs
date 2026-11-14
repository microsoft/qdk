// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an instruction's check compares ([`Layout`]), derived once per
//! instruction when its set is built, so that checking an instruction
//! compares integers only: its block count, each block's index against
//! the shot's blocks of its type, its arguments' kinds, its expected flags.
//!
//! Its blocks are numbered by position, as Qodec numbers `in` and `out`:
//! position `k` is the `k`-th entry of `in`, which, when `out` has a `k`-th
//! entry too, survives as it; `out`'s entries past `in`'s are new blocks.
//! So an instruction's blocks are its positions, `max(in, out)` of them.
//! A variadic operand (`[[qubit]]`) is the last position, and takes any
//! positive number of blocks.

use super::{
    BlockType, DeclarationFault, Instruction, InstructionDef, InstructionFault, ParameterKind,
    instruction::{Argument, Bit},
};

/// What an instruction's check compares, derived from its declaration.
#[derive(Clone, Debug)]
pub(crate) struct Layout {
    /// One per position, in order.
    slots: Vec<Slot>,
    /// Whether the last slot takes any positive number of blocks.
    variadic: bool,
    /// Its parameters' kinds, in order.
    parameters: Vec<ParameterKind>,
    /// Its flags, at most 64, so a flag pattern is a `u64` mask.
    flags: u32,
}

/// The block types of one position, as indices among the set's block
/// types: `pool`, the type whose blocks its index numbers (its `in` type,
/// or its `out` type for a new block), and `also`, the type it has after
/// the instruction (`pool` unless the instruction changes the block's
/// type, as Qodec's lattice-surgery merge does). Its index must be below
/// the shot's blocks of both.
#[derive(Clone, Copy, Debug)]
struct Slot {
    pool: u32,
    also: u32,
}

/// The most flags an instruction may declare: a flag pattern is a `u64`.
pub(crate) const MAX_FLAGS: usize = 64;

impl Layout {
    /// The layout of `def`, whose set declares `block_types`; or every
    /// fault of its operands and flags: a block type the set does not
    /// declare, a variadic operand out of place, more than 64 flags.
    pub(crate) fn derive(
        def: &InstructionDef,
        block_types: &[(BlockType, u32)],
    ) -> Result<Self, Vec<DeclarationFault>> {
        let mut faults = Vec::new();
        let mnemonic = || def.mnemonic.clone();
        let mut unknown: Vec<&BlockType> = Vec::new();
        let positions = def.inputs.len().max(def.outputs.len());
        let mut slots = Vec::with_capacity(positions);
        for k in 0..positions {
            let (input, output) = (def.inputs.get(k), def.outputs.get(k));
            let first = &input
                .or(output)
                .expect("a position of `in` or `out`")
                .block_type;
            let pool = index_of(block_types, first, &mut unknown);
            let also = output.map_or(pool, |output| {
                index_of(block_types, &output.block_type, &mut unknown)
            });
            slots.push(Slot { pool, also });
        }
        for block_type in unknown {
            faults.push(DeclarationFault::UnknownBlockType {
                mnemonic: mnemonic(),
                block_type: block_type.clone(),
            });
        }
        let variadic = variadic_position(def).unwrap_or_else(|()| {
            faults.push(DeclarationFault::MisplacedVariadic {
                mnemonic: mnemonic(),
            });
            false
        });
        if def.flags.len() > MAX_FLAGS {
            faults.push(DeclarationFault::TooManyFlags {
                mnemonic: mnemonic(),
                flags: def.flags.len(),
            });
        }
        if !faults.is_empty() {
            return Err(faults);
        }
        Ok(Self {
            slots,
            variadic,
            parameters: def.parameters.iter().map(|p| p.kind).collect(),
            flags: u32::try_from(def.flags.len()).expect("at most 64 flags"),
        })
    }

    /// Checks `instruction`, of `def`, against this layout, and, given
    /// `blocks_of` (the shot's blocks of each of the set's block types, in
    /// the set's order), each block's index against them; the first fault:
    /// a block count other than the layout's (or, for a variadic operand,
    /// below its least), a block beyond the shot's, a block given twice,
    /// an argument missing, extra or of another kind, a readout parity
    /// counting back 0, a flag pattern over flags it lacks.
    pub(crate) fn check(
        &self,
        def: &InstructionDef,
        instruction: &Instruction,
        blocks_of: Option<&[u32]>,
    ) -> Result<(), InstructionFault> {
        let mnemonic = || def.mnemonic.clone();
        let blocks = &instruction.blocks;
        let least = self.slots.len();
        if blocks.len() != least && !(self.variadic && blocks.len() > least) {
            return Err(InstructionFault::WrongBlockCount {
                mnemonic: mnemonic(),
                expected: count(least),
                got: count(blocks.len()),
            });
        }
        if let Some(blocks_of) = blocks_of {
            for (position, &block) in blocks.iter().enumerate() {
                let slot = self.slot(position);
                for needs in [blocks_of[slot.pool as usize], blocks_of[slot.also as usize]] {
                    if block >= needs {
                        return Err(InstructionFault::BlockBeyondNeeds {
                            mnemonic: mnemonic(),
                            block,
                            needs,
                        });
                    }
                }
            }
        }
        if let Some(block) = self.repeated_block(blocks) {
            return Err(InstructionFault::RepeatedBlock {
                mnemonic: mnemonic(),
                block,
            });
        }
        let arguments = &instruction.arguments;
        if let Some(parameter) = def.parameters.get(arguments.len()) {
            return Err(InstructionFault::MissingArgument {
                mnemonic: mnemonic(),
                parameter: parameter.name.clone(),
            });
        }
        if arguments.len() > self.parameters.len() {
            return Err(InstructionFault::ExtraArguments {
                mnemonic: mnemonic(),
                expected: count(self.parameters.len()),
                got: count(arguments.len()),
            });
        }
        for (position, (argument, &expected)) in arguments.iter().zip(&self.parameters).enumerate()
        {
            if argument.kind() != expected {
                return Err(InstructionFault::WrongArgumentKind {
                    mnemonic: mnemonic(),
                    parameter: def.parameters[position].name.clone(),
                    expected,
                });
            }
            if let Argument::Bit(Bit::Parity(parity)) = argument
                && parity.back().contains(&0)
            {
                return Err(InstructionFault::ZeroLookback {
                    mnemonic: mnemonic(),
                });
            }
        }
        if let Some(patterns) = &instruction.select {
            let undeclared = u64::MAX.checked_shl(self.flags).unwrap_or(0);
            if let Some(pattern) = patterns.iter().find(|p| p.mask() & undeclared != 0) {
                let flag = (pattern.mask() & undeclared).trailing_zeros();
                return Err(InstructionFault::UnknownFlag {
                    mnemonic: mnemonic(),
                    name: format!("#{flag}"),
                });
            }
        }
        Ok(())
    }

    /// The slot of the block at `position`: those past the last slot are
    /// the variadic operand's.
    fn slot(&self, position: usize) -> Slot {
        self.slots[position.min(self.slots.len() - 1)]
    }

    /// A block given twice in one pool, if any: the first found.
    fn repeated_block(&self, blocks: &[u32]) -> Option<u32> {
        // Pairwise for the few blocks most instructions take; sorted for a
        // variadic operand's many.
        const PAIRWISE: usize = 16;
        if blocks.len() <= PAIRWISE {
            for (j, &block) in blocks.iter().enumerate() {
                let pool = self.slot(j).pool;
                if blocks[..j]
                    .iter()
                    .enumerate()
                    .any(|(i, &earlier)| earlier == block && self.slot(i).pool == pool)
                {
                    return Some(block);
                }
            }
            return None;
        }
        let mut keyed: Vec<(u32, u32)> = blocks
            .iter()
            .enumerate()
            .map(|(position, &block)| (self.slot(position).pool, block))
            .collect();
        keyed.sort_unstable();
        keyed
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0].1)
    }
}

/// Whether `def` has a variadic operand, which must be the last position:
/// on each side, only the last entry may be variadic, and a position both
/// sides declare is variadic on both or on neither. `Err` otherwise.
fn variadic_position(def: &InstructionDef) -> Result<bool, ()> {
    for side in [&def.inputs, &def.outputs] {
        if side.iter().rev().skip(1).any(|operand| operand.variadic) {
            return Err(());
        }
    }
    if def
        .inputs
        .iter()
        .zip(&def.outputs)
        .any(|(input, output)| input.variadic != output.variadic)
    {
        return Err(());
    }
    let variadic = def
        .inputs
        .iter()
        .chain(&def.outputs)
        .any(|operand| operand.variadic);
    // So a variadic entry is last on its side, and, its side being the
    // shorter, the other's entry at its position would be fixed: it is the
    // last position.
    Ok(variadic)
}

/// The index of `block_type` among `block_types`; 0 for a type they lack,
/// which joins `unknown`, once.
fn index_of<'a>(
    block_types: &[(BlockType, u32)],
    block_type: &'a BlockType,
    unknown: &mut Vec<&'a BlockType>,
) -> u32 {
    if let Some(index) = block_types
        .iter()
        .position(|(known, _)| known == block_type)
    {
        return u32::try_from(index).expect("a set has fewer than 2^32 block types");
    }
    if !unknown.contains(&block_type) {
        unknown.push(block_type);
    }
    0
}

/// A count of blocks or arguments, as a fault reports it.
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
