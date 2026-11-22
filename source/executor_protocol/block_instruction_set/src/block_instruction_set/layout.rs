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

use std::collections::BTreeSet;

use super::{
    Argument, Bit, BlockInstruction, BlockRequirement, BlockType, DeclarationFault, InstructionDef,
    InstructionFault, ParameterKind,
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
    /// Its readout slots, its outcomes and then its flags.
    readout_slots: u32,
}

/// The block types of one position, as indices among the set's block
/// types: `input`, its type before the instruction (`None` for a new
/// block), and `output`, its type after (`None` for a block the
/// instruction consumes). They differ when the instruction changes the
/// block's type, as Qodec's lattice-surgery merge does, and the block
/// keeps its index: so its index must be below the shot's blocks of each.
#[derive(Clone, Copy, Debug)]
struct Slot {
    input: Option<u32>,
    output: Option<u32>,
}

impl Slot {
    /// Its types, before then after, each at most once.
    fn types(self) -> impl Iterator<Item = u32> {
        let after = self.output.filter(|&output| Some(output) != self.input);
        self.input.into_iter().chain(after)
    }

    /// Whether the blocks of this position and `other`'s are one block
    /// when they have the same index: one before the instruction (both of
    /// one `in` type) or one after it (both of one `out` type).
    fn shares_a_block_with(self, other: Self) -> bool {
        (self.input.is_some() && self.input == other.input)
            || (self.output.is_some() && self.output == other.output)
    }
}

/// The most flags an instruction may declare: a flag pattern is a `u64`.
const MAX_FLAGS: usize = 64;

impl Layout {
    /// How many readout slots each of its instructions reads: its outcomes,
    /// then its flags.
    pub(crate) const fn readout_slots(&self) -> u32 {
        self.readout_slots
    }

    /// The layout of `def`, whose set declares `block_types`; or every
    /// fault of its operands and flags: a block type the set does not
    /// declare, a variadic operand out of place, more than 64 flags, or
    /// more readout slots than a `u32` counts.
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
            let [input, output] = [&def.inputs, &def.outputs].map(|operands| {
                let operand = operands.get(k)?;
                Some(index_of(block_types, &operand.block_type, &mut unknown))
            });
            slots.push(Slot { input, output });
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
        let flags = u32::try_from(def.flags.len())
            .ok()
            .filter(|&flags| flags as usize <= MAX_FLAGS);
        if flags.is_none() {
            faults.push(DeclarationFault::TooManyFlags {
                mnemonic: mnemonic(),
                flags: def.flags.len(),
            });
        }
        let readout_slots = u32::try_from(def.flags.len())
            .ok()
            .and_then(|flags| def.outcomes.checked_add(flags));
        if readout_slots.is_none() {
            faults.push(DeclarationFault::TooManyReadoutSlots {
                mnemonic: mnemonic(),
                outcomes: def.outcomes,
                flags: def.flags.len(),
            });
        }
        let (Some(flags), Some(readout_slots)) = (flags, readout_slots) else {
            return Err(faults);
        };
        if !faults.is_empty() {
            return Err(faults);
        }
        Ok(Self {
            slots,
            variadic,
            parameters: def.parameters.iter().map(|p| p.kind).collect(),
            flags,
            readout_slots,
        })
    }

    /// Checks `instruction`, of `def`, against this layout, and, given what
    /// its shots require, `required`, each block's index against their
    /// blocks of its type and each readout parity against whether they
    /// use any; the
    /// first fault: a block count other than the layout's (or, for a
    /// variadic operand, below its least), a block beyond the shot's, a
    /// block given twice, an argument missing, extra or of another kind, a
    /// readout parity counting back 0 or not required, a select of
    /// no patterns, or a flag pattern over flags it lacks.
    pub(crate) fn check(
        &self,
        def: &InstructionDef,
        instruction: &BlockInstruction,
        required: Option<&BlockRequirement>,
    ) -> Result<(), InstructionFault> {
        let mnemonic = || def.mnemonic.clone();
        let blocks = &instruction.blocks;
        let least = self.slots.len();
        if blocks.len() != least && !(self.variadic && blocks.len() > least) {
            return Err(InstructionFault::WrongBlockCount {
                mnemonic: mnemonic(),
                expected: count(least),
                got: count(blocks.len()),
                variadic: self.variadic,
            });
        }
        if let Some(BlockRequirement { blocks_of, .. }) = required {
            for (position, &block) in blocks.iter().enumerate() {
                for block_type in self.slot(position).types() {
                    let required = blocks_of[block_type as usize];
                    if block >= required {
                        return Err(InstructionFault::BlockBeyondRequired {
                            mnemonic: mnemonic(),
                            block,
                            required,
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
            if let Argument::Bit(Bit::Parity(parity)) = argument {
                if parity.back().contains(&0) {
                    return Err(InstructionFault::ZeroLookback {
                        mnemonic: mnemonic(),
                    });
                }
                if required.is_some_and(|required| !required.readout_parity) {
                    return Err(InstructionFault::ParityNotRequired {
                        mnemonic: mnemonic(),
                        parameter: def.parameters[position].name.clone(),
                    });
                }
            }
        }
        if let Some(patterns) = &instruction.select {
            if patterns.is_empty() {
                return Err(InstructionFault::EmptySelect {
                    mnemonic: mnemonic(),
                });
            }
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

    /// A block given twice, if any: two positions of one index that are one
    /// block before the instruction or after it
    /// ([`Slot::shares_a_block_with`]). Of several, the one at the earliest
    /// position that repeats a block before it.
    fn repeated_block(&self, blocks: &[u32]) -> Option<u32> {
        // Pairwise for the few blocks most instructions take; sorted for a
        // variadic operand's many.
        const PAIRWISE: usize = 16;
        if blocks.len() <= PAIRWISE {
            for (j, &block) in blocks.iter().enumerate() {
                let slot = self.slot(j);
                if blocks[..j]
                    .iter()
                    .enumerate()
                    .any(|(i, &earlier)| earlier == block && self.slot(i).shares_a_block_with(slot))
                {
                    return Some(block);
                }
            }
            return None;
        }
        // Each block as it is before the instruction (`false`) and after it
        // (`true`), with its type there: one block twice is a pair.
        let mut keyed: Vec<(bool, u32, u32)> = Vec::with_capacity(2 * blocks.len());
        for (position, &block) in blocks.iter().enumerate() {
            let slot = self.slot(position);
            keyed.extend(slot.input.map(|input| (false, input, block)));
            keyed.extend(slot.output.map(|output| (true, output, block)));
        }
        keyed.sort_unstable();
        if keyed.windows(2).all(|pair| pair[0] != pair[1]) {
            return None;
        }
        // A fault: name the block the pairwise scan names, the earliest
        // position's repeat, not the smallest key.
        let mut seen = BTreeSet::new();
        blocks.iter().enumerate().find_map(|(position, &block)| {
            let slot = self.slot(position);
            let before = slot.input.map(|input| (false, input, block));
            let after = slot.output.map(|output| (true, output, block));
            let repeats = [before, after]
                .into_iter()
                .flatten()
                .any(|key| !seen.insert(key));
            repeats.then_some(block)
        })
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
    // So a variadic entry is the instruction's last position: were its
    // side the shorter, the other side would have there a fixed entry or a
    // variadic one not last, both refused above.
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
