// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One instruction, as a batch holds it ([`Instruction`]), how one is
//! built by name and checked against its set
//! ([`InstructionSet::instruction`]), and why one is refused
//! ([`InstructionFault`]).
//!
//! An instruction is written as in Qodec: its mnemonic, its blocks as
//! plain indices, its arguments by parameter name, and the flags it
//! expects ([`Instruction::select`]). Names are resolved, and the
//! instruction checked, when it is built, so what goes down a chain holds
//! only integers and values: its opcode, its blocks, its arguments in the
//! parameters' order. The same check runs again at each `execute` of a
//! batch its sender marked [`checked`](crate::Batch::checked), on the
//! receiver's set, since a part on its hot path builds instructions as
//! struct literals.

mod argument;
mod select_pattern;

pub use argument::{Argument, Bit, ReadoutParity};
pub use select_pattern::SelectPattern;

use super::{InstructionSet, Opcode, ParameterKind};
use smallvec::SmallVec;
use std::fmt;

/// What the shots require of a receiver, as an instruction's check reads
/// it: their blocks of each of the receiver's block types, and whether
/// they may hold readout parities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReceiverRequirement {
    /// The shot's blocks of each of the set's block types, in the set's
    /// order.
    pub(crate) blocks_of: Vec<u32>,
    /// Whether an instruction may hold a readout parity
    /// ([`Bit::Parity`](crate::Bit::Parity)).
    pub(crate) readout_parity: bool,
}

/// One instruction, as a batch holds it. Built two ways:
/// [`InstructionSet::instruction`] checks it at once, with a readable
/// fault, for writing programs; a struct literal, with opcodes from
/// [`instruction_set!`](crate::instruction_set) or
/// [`InstructionSet::opcode`], builds one on a part's hot path. Either way,
/// in a batch its sender marked [`checked`](crate::Batch::checked), the
/// protocol checks it at each `execute`, comparing integers only.
#[derive(Clone, Debug, PartialEq)]
pub struct Instruction {
    /// Its place in its set; a sender rewrites it into the receiver's with
    /// the [`OpcodeMap`](crate::OpcodeMap) of their fit.
    pub opcode: Opcode,
    /// The shot's blocks it acts on, each an index among the shot's blocks
    /// of its type: those of `in`, then `out`'s new ones.
    pub blocks: Blocks,
    /// One per parameter, in the parameters' order.
    pub arguments: Arguments,
    /// Its flags' values without noise: one pattern must match. Rare, so
    /// boxed: two words, and no allocation when absent. `Some` of no
    /// pattern is refused at an `execute` of a checked batch
    /// ([`InstructionFault::EmptySelect`]).
    pub select: Option<Box<[SelectPattern]>>,
}

/// An instruction's blocks: up to three inline, so that most instructions
/// allocate nothing.
pub type Blocks = SmallVec<[u32; 3]>;

/// An instruction's arguments: up to three inline, so that most
/// instructions allocate nothing.
pub type Arguments = SmallVec<[Argument; 3]>;

// Small vectors, because a `Vec` would allocate for every instruction
// built, and an array would cap how many blocks or arguments one takes: a
// small vector keeps its first items inline and goes to the heap only past
// them. Measured against a `Vec`, they save about 15–25 ns per instruction,
// at build and at copy. It is the usual way to keep short lists off the heap:
// `parking_lot_core` gathers the threads it wakes in a `SmallVec<[_; 8]>`,
// and the workspace already builds `smallvec` through it and `wgpu`.
//
// The instruction's size sets how fast a batch runs, as the crate's bench
// measures; smallvec's `union` feature, on in `Cargo.toml`, saves 16 bytes.
// Equal, not at most, so that a change of layout either way is deliberate:
// `Cargo.toml`'s comment states this size, the bench prints it, and a
// change is measured again.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<Instruction>() == 128);

impl InstructionSet {
    /// The instruction `mnemonic` on `blocks`, with its arguments by
    /// parameter name (`&[]` for none), checked against this set: as an
    /// `execute` checks each instruction of a checked batch, but for the
    /// shot's blocks, which are not known yet, and with its names
    /// resolved here. A slice, so that `&[]` infers.
    ///
    /// # Errors
    ///
    /// The first fault: [`UnknownMnemonic`](InstructionFault::UnknownMnemonic);
    /// an argument for a parameter the instruction lacks
    /// ([`UnknownParameter`](InstructionFault::UnknownParameter)) or given
    /// twice ([`RepeatedArgument`](InstructionFault::RepeatedArgument));
    /// then, as at an `execute`: a block count other than the
    /// instruction's, a block given twice, an argument missing or of
    /// another kind, a readout parity counting back 0.
    pub fn instruction(
        &self,
        mnemonic: &str,
        blocks: impl IntoIterator<Item = u32>,
        arguments: &[(&str, Argument)],
    ) -> Result<Instruction, InstructionFault> {
        let opcode = self
            .opcode(mnemonic)
            .ok_or_else(|| InstructionFault::UnknownMnemonic(mnemonic.to_owned()))?;
        let def = self.def(opcode).expect("an opcode of this set");
        let mut ordered: SmallVec<[Option<Argument>; 3]> =
            SmallVec::from_elem(None, def.parameters.len());
        for (name, argument) in arguments {
            let position = def
                .parameters
                .iter()
                .position(|parameter| parameter.name == *name)
                .ok_or_else(|| InstructionFault::UnknownParameter {
                    mnemonic: mnemonic.to_owned(),
                    name: (*name).to_owned(),
                })?;
            if ordered[position].replace(argument.clone()).is_some() {
                return Err(InstructionFault::RepeatedArgument {
                    mnemonic: mnemonic.to_owned(),
                    parameter: (*name).to_owned(),
                });
            }
        }
        // A missing argument ends the list, so the check names the first.
        let instruction = Instruction {
            opcode,
            blocks: blocks.into_iter().collect(),
            arguments: ordered.into_iter().map_while(|argument| argument).collect(),
            select: None,
        };
        self.check(&instruction, None)?;
        Ok(instruction)
    }

    /// Checks `instruction` against this set, and, given what its shots
    /// require, `required`, its blocks and readout parities against it:
    /// its readout slots (its outcomes, then its flags), or the first
    /// fault.
    pub(crate) fn check(
        &self,
        instruction: &Instruction,
        required: Option<&ReceiverRequirement>,
    ) -> Result<u32, InstructionFault> {
        let index = instruction.opcode.index();
        let (Some(def), Some(layout)) = (self.instructions.get(index), self.layouts.get(index))
        else {
            return Err(InstructionFault::UnknownOpcode(instruction.opcode));
        };
        layout.check(def, instruction, required)?;
        Ok(layout.readout_slots())
    }
}

/// Why an instruction is not one its set, or the executor, accepts: found
/// when it is built ([`InstructionSet::instruction`],
/// [`Instruction::select`], [`Argument::pauli`]), or at an `execute` of a
/// batch its sender marked [`checked`](crate::Batch::checked).
/// `mnemonic` is filled by whoever holds the set, so that the fault reads
/// on its own; it allocates only when there is a fault.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstructionFault {
    /// Built: no instruction of the set has this mnemonic.
    UnknownMnemonic(String),
    /// At [`Instruction::select`], or at an `execute`: an opcode beyond the
    /// set's last instruction.
    UnknownOpcode(Opcode),
    /// Both: `expected` is its count, or, with a variadic operand
    /// (`variadic`), the least.
    WrongBlockCount {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The blocks it takes, or the least, if `variadic`.
        expected: u32,
        /// The blocks it was given.
        got: u32,
        /// Whether it has a variadic operand.
        variadic: bool,
    },
    /// At an `execute`: a block at or past `required`, the shot's blocks of
    /// its type, as required across the boundary.
    BlockBeyondRequired {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The block, from 0.
        block: u32,
        /// The shot's blocks of its type, as required.
        required: u32,
    },
    /// Both: one block twice, as `cx [0, 0]`, before the instruction or
    /// after it (a block turned into another type, given the index of a
    /// block of that type that survives). Of several blocks given twice,
    /// it names the earliest repeated: the one whose second mention comes
    /// first.
    RepeatedBlock {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The block given twice: of several, the earliest repeated.
        block: u32,
    },
    /// Built: an argument named for no parameter of the instruction.
    UnknownParameter {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The name the argument was given for.
        name: String,
    },
    /// Built: two arguments for one parameter.
    RepeatedArgument {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The parameter's name.
        parameter: String,
    },
    /// Both: no argument for this parameter, the first one missing.
    MissingArgument {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The parameter's name.
        parameter: String,
    },
    /// At an `execute`: more arguments than parameters.
    ExtraArguments {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// Its parameters.
        expected: u32,
        /// The arguments given.
        got: u32,
    },
    /// Both: the argument for this parameter is not of its kind.
    WrongArgumentKind {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The parameter's name.
        parameter: String,
        /// The parameter's kind.
        expected: ParameterKind,
    },
    /// [`Instruction::select`]: a flag the instruction lacks, or, at an
    /// `execute`, a pattern over one, named by its position (`#3`).
    UnknownFlag {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The flag's name, or its position (`#3`).
        name: String,
    },
    /// [`Instruction::select`]: one flag twice in one pattern.
    RepeatedFlag {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The flag's name.
        name: String,
    },
    /// At an `execute`: a [`select`](Instruction::select) field of no
    /// patterns, `Some` of an empty list, which no shot's flags could
    /// match; the builder makes `None` of no pattern, which expects
    /// nothing.
    EmptySelect {
        /// The instruction's mnemonic.
        mnemonic: String,
    },
    /// [`Instruction::select`]: a flag's value other than 0 or 1.
    FlagValueNotABit {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The flag's name.
        name: String,
        /// The value given.
        value: u8,
    },
    /// [`Argument::pauli`]: text that is not a Pauli in Qodec's form.
    MalformedPauli(String),
    /// Both: a readout parity counting back 0, as `Argument::recent(0)`;
    /// counts back start at 1, the shot's latest readout.
    ZeroLookback {
        /// The instruction's mnemonic.
        mnemonic: String,
    },
    /// At an `execute`: a readout parity for this parameter, sent across a
    /// boundary whose required features do not state readout parities
    /// ([`Features::with_readout_parity`](crate::Features::with_readout_parity)),
    /// so that no fit checked that the receiver resolves them.
    ParityNotRequired {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The parameter's name.
        parameter: String,
    },
    /// An executor's own rule beyond the set, in its words: a batch sampler
    /// refusing a measurement before the batch's end; an argument out of
    /// its range. A rule its provided capabilities can state goes there
    /// instead.
    Unsupported {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The executor's rule, in its words.
        reason: String,
    },
}

impl fmt::Display for InstructionFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstructionFault::UnknownMnemonic(mnemonic) => {
                write!(f, "no instruction `{mnemonic}` in the set")
            }
            InstructionFault::UnknownOpcode(opcode) => {
                write!(f, "no instruction of opcode {} in the set", opcode.index())
            }
            InstructionFault::WrongBlockCount {
                mnemonic,
                expected,
                got,
                variadic,
            } => {
                let least = if *variadic { "at least " } else { "" };
                let blocks = if *expected == 1 { "block" } else { "blocks" };
                write!(
                    f,
                    "`{mnemonic}` takes {least}{expected} {blocks}, given {got}"
                )
            }
            InstructionFault::BlockBeyondRequired {
                mnemonic,
                block,
                required,
            } => write!(
                f,
                "`{mnemonic}` acts on block {block}, beyond the {required} of its type required"
            ),
            InstructionFault::RepeatedBlock { mnemonic, block } => {
                write!(f, "`{mnemonic}` is given block {block} twice")
            }
            InstructionFault::UnknownParameter { mnemonic, name } => {
                write!(f, "`{mnemonic}` has no parameter `{name}`")
            }
            InstructionFault::RepeatedArgument {
                mnemonic,
                parameter,
            } => write!(f, "`{mnemonic}` is given `{parameter}` twice"),
            InstructionFault::MissingArgument {
                mnemonic,
                parameter,
            } => write!(f, "`{mnemonic}` is missing its argument `{parameter}`"),
            InstructionFault::ExtraArguments {
                mnemonic,
                expected,
                got,
            } => {
                let arguments = if *expected == 1 {
                    "argument"
                } else {
                    "arguments"
                };
                write!(f, "`{mnemonic}` takes {expected} {arguments}, given {got}")
            }
            InstructionFault::WrongArgumentKind {
                mnemonic,
                parameter,
                expected,
            } => write!(f, "`{mnemonic}` needs a `{expected}` for `{parameter}`"),
            InstructionFault::UnknownFlag { mnemonic, name } => {
                write!(f, "`{mnemonic}` has no flag `{name}`")
            }
            InstructionFault::RepeatedFlag { mnemonic, name } => {
                write!(
                    f,
                    "`{mnemonic}`'s flag `{name}` is selected twice in one pattern"
                )
            }
            InstructionFault::EmptySelect { mnemonic } => {
                write!(
                    f,
                    "`{mnemonic}` selects on no pattern, which no flags match"
                )
            }
            InstructionFault::FlagValueNotABit {
                mnemonic,
                name,
                value,
            } => write!(
                f,
                "`{mnemonic}`'s flag `{name}` is selected as {value}, not 0 or 1"
            ),
            InstructionFault::MalformedPauli(text) => {
                write!(f, "`{text}` is not a Pauli operator")
            }
            InstructionFault::ZeroLookback { mnemonic } => {
                write!(f, "`{mnemonic}` counts back 0 readouts; the latest is 1")
            }
            InstructionFault::ParityNotRequired {
                mnemonic,
                parameter,
            } => write!(
                f,
                "`{mnemonic}` is given a readout parity for `{parameter}`, \
                 and readout parities are not required"
            ),
            InstructionFault::Unsupported { mnemonic, reason } => {
                write!(f, "`{mnemonic}` is not supported: {reason}")
            }
        }
    }
}

impl std::error::Error for InstructionFault {}
