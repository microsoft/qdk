// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part sends or accepts: an instruction set ([`BlockInstructionSet`]),
//! in Qodec's model, because it describes instruction sets in general, at
//! any level of a chain, from a simulator's gates to a code's operations
//! on its blocks. For each instruction it states, as data, the blocks it
//! takes and gives, its parameters, its readouts, and what it does, in
//! steps over Pauli operators (a Clifford, a Pauli, a rotation about a
//! Pauli, an observation); an instruction those steps cannot state is
//! declared without them. So two parts' sets can be checked against each
//! other before any shot runs, matching instructions by what they do
//! rather than by name. A part written against Qodec's sets also needs no
//! translation.
//!
//! A set declares its block types (each with the qubits one block
//! encodes) and its instructions ([`InstructionDef`]): for each, its
//! mnemonic, the blocks it takes (`in`) and gives (`out`), its classical
//! parameters, its readouts (its `observe` outcomes, then its flags), and
//! what it does ([`Meaning`]), if it says. An instruction is named by its
//! [`Opcode`], its place in the set, so that what runs compares integers:
//! a set written in code names its opcodes as constants with
//! [`instruction_set!`](crate::instruction_set), a set known only at run
//! time resolves them once with [`BlockInstructionSet::opcode`].
//!
//! A set is checked once, when it is built ([`BlockInstructionSet::new`]), which
//! refuses every fault of its declaration at once ([`DeclarationFault`]), and
//! derives what each instruction's check compares. Its submodules hold the rest
//! of the vocabulary: an instruction's declaration (`instruction_def`) and its
//! place in the set (`opcode`), the kinds of blocks (`block_type`) and an
//! instruction's operands of them (`block_operand`), its classical inputs
//! (`parameter`), Pauli operators (`pauli`), what an instruction does
//! (`meaning`), what each instruction's check compares (`layout`), the
//! instructions a batch holds and their checks (`block_instruction`), the fit of one
//! set to another (`fit`), the translation of opcodes it gives
//! (`opcode_map`), the set's capabilities, the blocks of each type a shot
//! holds and readout parities (`block_capabilities`), what the shots
//! require of a receiver, as an instruction's check reads it
//! (`block_requirement`), one readout's value or why it is lost
//! (`readout_value`), and what a part found suspect in a shot
//! (`shot_conditions`).
//!
//! The crate's `docs/instruction-sets.md` gives how a part declares its
//! sets, and when two sets fit.

mod block_capabilities;
mod block_instruction;
mod block_operand;
mod block_requirement;
mod block_type;
mod fit;
mod instruction_def;
mod layout;
mod meaning;
mod opcode;
mod opcode_map;
mod parameter;
mod pauli;
mod readout_value;
mod shot_conditions;

pub use block_capabilities::{BlockCapabilities, BlockCapabilitiesMismatch, BlockCountMismatch};
pub use block_instruction::{
    Argument, Arguments, Bit, BlockInstruction, Blocks, InstructionFault, ReadoutParity,
    SelectPattern,
};
pub use block_operand::BlockOperand;
pub use block_requirement::BlockRequirement;
pub use block_type::BlockType;
pub use fit::{BlockInstructionSetMismatch, InstructionMismatch};
pub use instruction_def::InstructionDef;
pub use meaning::{Angle, Guard, Meaning, MeaningFault, Observable};
pub use opcode::Opcode;
pub use opcode_map::OpcodeMap;
pub use parameter::{Parameter, ParameterKind};
pub use pauli::Pauli;
pub use readout_value::{ReadoutFailure, ReadoutValue};
pub use shot_conditions::ShotConditions;

use executor_protocol_core::ReadoutSlots;
use layout::Layout;
use std::collections::BTreeMap;
use std::fmt;

/// An instruction set, as Qodec declares one: its name, which says when its
/// undeclared instructions match another set's ([`name`](Self::name)), its
/// block types and its instructions, each at the place its [`Opcode`]
/// names.
///
/// Built by [`BlockInstructionSet::new`], or, for a set written in code, by
/// [`instruction_set!`](crate::instruction_set), which also names its
/// opcodes as constants.
#[derive(Clone, Debug)]
pub struct BlockInstructionSet {
    /// What it promises, in [`name`](Self::name).
    name: String,
    /// Each type and the qubits one block encodes (`blocks: {carbon: 2}`).
    block_types: Vec<(BlockType, u32)>,
    /// An opcode is an instruction's index here.
    instructions: Vec<InstructionDef>,
    /// Derived once, when the set is built: what an instruction's check
    /// compares, per opcode.
    layouts: Vec<Layout>,
    /// Each mnemonic's opcode, for resolving names.
    opcodes: BTreeMap<String, Opcode>,
}

impl BlockInstructionSet {
    /// The set `name`, of `block_types` (each with the qubits one block
    /// encodes) and `instructions`, whose opcodes are their places in that
    /// order, from 0.
    ///
    /// # Errors
    ///
    /// Every fault of the declaration, in its order: a block type declared
    /// twice; then for each instruction, its mnemonic if an earlier one
    /// has it, an operand of a block type the set does not declare, a
    /// variadic operand out of place, more than 64 flags, more readout
    /// slots than a `u32` counts, a parameter or a flag named twice, and
    /// its meaning's faults ([`MeaningFault`]).
    ///
    /// Not checked: a meaning's Pauli on a qubit beyond the instruction's
    /// qubits, a block type encoding 0 qubits, and an empty set name.
    ///
    /// # Panics
    ///
    /// With 2^32 instructions or block types or more, which opcodes and
    /// block-type indices, `u32`s, cannot number.
    pub fn new(
        name: impl Into<String>,
        block_types: impl IntoIterator<Item = (BlockType, u32)>,
        instructions: impl IntoIterator<Item = InstructionDef>,
    ) -> Result<Self, Vec<DeclarationFault>> {
        let block_types: Vec<(BlockType, u32)> = block_types.into_iter().collect();
        let instructions: Vec<InstructionDef> = instructions.into_iter().collect();
        let mut faults = Vec::new();
        for (k, (block_type, _)) in block_types.iter().enumerate() {
            if block_types[..k]
                .iter()
                .any(|(earlier, _)| earlier == block_type)
            {
                faults.push(DeclarationFault::RepeatedBlockType {
                    block_type: block_type.clone(),
                });
            }
        }
        let mut opcodes = BTreeMap::new();
        let mut layouts = Vec::with_capacity(instructions.len());
        for (index, def) in instructions.iter().enumerate() {
            let opcode =
                Opcode::at(u32::try_from(index).expect("a set has fewer than 2^32 instructions"));
            if opcodes.insert(def.mnemonic.clone(), opcode).is_some() {
                faults.push(DeclarationFault::RepeatedMnemonic {
                    mnemonic: def.mnemonic.clone(),
                });
            }
            match Layout::derive(def, &block_types) {
                Ok(layout) => layouts.push(layout),
                Err(found) => faults.extend(found),
            }
            faults.extend(def.naming_faults());
            faults.extend(
                def.meaning
                    .faults(&def.parameters, def.outcomes)
                    .into_iter()
                    .map(|fault| DeclarationFault::Meaning {
                        mnemonic: def.mnemonic.clone(),
                        fault,
                    }),
            );
        }
        if !faults.is_empty() {
            return Err(faults);
        }
        Ok(Self {
            name: name.into(),
            block_types,
            instructions,
            layouts,
            opcodes,
        })
    }

    /// Its name, Qodec's `name`: a promise that every set of this name gives
    /// an instruction of one mnemonic the same meaning, declared or not. So
    /// the fit matches an instruction whose meaning neither set declares by
    /// its mnemonic, only between sets of one name; one whose meaning both
    /// declare, by its meaning, whatever the names.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// An opcode's mnemonic, for tools and messages; `None` beyond the set.
    #[must_use]
    pub fn mnemonic(&self, opcode: Opcode) -> Option<&str> {
        self.def(opcode).map(|def| def.mnemonic.as_str())
    }

    /// The opcode of `mnemonic`, for a set known only at run time (Qodec's
    /// YAML, a chain built from Python): a part resolves each name once,
    /// when it is built. A set written in code names its opcodes with
    /// [`instruction_set!`](crate::instruction_set) instead.
    #[must_use]
    pub fn opcode(&self, mnemonic: &str) -> Option<Opcode> {
        self.opcodes.get(mnemonic).copied()
    }

    /// The declaration of the instruction at `opcode`.
    pub(crate) fn def(&self, opcode: Opcode) -> Option<&InstructionDef> {
        self.instructions.get(opcode.index())
    }

    /// The readout slots of the instruction at `opcode`, its outcomes then
    /// its flags; `None` beyond the set. A sender adds them up as it writes
    /// a batch, to state its slots ([`Batch::new`](executor_protocol_core::Batch::new)).
    #[inline]
    #[must_use]
    pub fn readout_slots_at(&self, opcode: Opcode) -> Option<u32> {
        self.layouts.get(opcode.index()).map(Layout::readout_slots)
    }

    /// The readout slots of `instructions`, in this set's opcodes: the sum
    /// of each one's, as a sender states them for their batch
    /// ([`Batch::new`](executor_protocol_core::Batch::new)). `None` if an opcode is beyond
    /// the set, or the sum is more than a `u32` counts.
    #[must_use]
    pub fn readout_slots_of(&self, instructions: &[BlockInstruction]) -> Option<u32> {
        instructions.iter().try_fold(0u32, |slots, instruction| {
            slots.checked_add(self.readout_slots_at(instruction.opcode)?)
        })
    }

    /// Its block types, in order.
    pub(crate) fn block_types(&self) -> impl Iterator<Item = &BlockType> {
        self.block_types.iter().map(|(block_type, _)| block_type)
    }

    /// The qubits one block of `block_type` encodes, 0 for a type the set
    /// does not declare.
    fn qubits_of(&self, block_type: &BlockType) -> u32 {
        self.block_types
            .iter()
            .find(|(known, _)| known == block_type)
            .map_or(0, |&(_, qubits)| qubits)
    }

    /// Its opcodes, in order.
    fn opcodes(&self) -> impl Iterator<Item = Opcode> + '_ {
        (0..self.instructions.len())
            .map(|index| Opcode::at(u32::try_from(index).expect("checked when the set is built")))
    }
}

/// The block set as the protocol's instruction set: an instruction's
/// readout slots and check are the set's own, and what the shots require
/// of a receiver is read from the required [`BlockCapabilities`].
impl executor_protocol_core::InstructionSet for BlockInstructionSet {
    type Instruction = BlockInstruction;
    type SetCapabilities = BlockCapabilities;
    type Requirement = BlockRequirement;
    type Fault = InstructionFault;
    type Readout = ReadoutValue;
    type ShotConditions = ShotConditions;

    const UNWRITTEN_READOUT: ReadoutValue = ReadoutValue::Missing(ReadoutFailure::Other);

    /// [`BlockCapabilities::NONE`]: no block and no readout parity needed.
    fn needs_nothing() -> BlockCapabilities {
        BlockCapabilities::NONE
    }

    /// The readout slots of the instruction at its opcode
    /// ([`readout_slots_at`](BlockInstructionSet::readout_slots_at)), and 0
    /// for an opcode beyond the set, which
    /// [`check`](executor_protocol_core::InstructionSet::check) refuses
    /// ([`InstructionFault::UnknownOpcode`]).
    #[inline]
    fn readout_slots(&self, instruction: &BlockInstruction) -> ReadoutSlots {
        self.readout_slots_at(instruction.opcode).unwrap_or(0)
    }

    /// The block counts of `required` by this set's block types, in this
    /// set's order, and whether it states readout parities `true`; what
    /// this set's [`check`](executor_protocol_core::InstructionSet::check)
    /// alone reads.
    fn requirement(&self, required: &BlockCapabilities) -> BlockRequirement {
        BlockRequirement {
            blocks_of: self
                .block_types()
                .map(|block_type| required.blocks(block_type).unwrap_or(0))
                .collect(),
            readout_parity: required.readout_parity() == Some(true),
        }
    }

    /// Checks `instruction` against this set's declaration, and its blocks
    /// and readout parities against `required`: its readout slots, or the
    /// first fault. `required` is made by this set's
    /// [`requirement`](executor_protocol_core::InstructionSet::requirement),
    /// as the core's boundaries make it: one made by another set may read
    /// another block type's count, or panic ([`BlockRequirement`]).
    #[inline]
    fn check(
        &self,
        instruction: &BlockInstruction,
        required: &BlockRequirement,
    ) -> Result<ReadoutSlots, InstructionFault> {
        self.check_against(instruction, Some(required))
    }
}

/// Why a set's declaration is refused, when it is built.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeclarationFault {
    /// The set declares a block type twice.
    RepeatedBlockType {
        /// The block type declared twice.
        block_type: BlockType,
    },
    /// The set declares two instructions of one mnemonic.
    RepeatedMnemonic {
        /// The mnemonic declared twice.
        mnemonic: String,
    },
    /// An operand of a block type the set does not declare.
    UnknownBlockType {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The operand's block type.
        block_type: BlockType,
    },
    /// A variadic operand that is not the last of its side, or a position
    /// both sides declare that only one makes variadic. A variadic operand
    /// last on the longer side, at a position the other side does not
    /// declare, is no fault.
    MisplacedVariadic {
        /// The instruction's mnemonic.
        mnemonic: String,
    },
    /// Two parameters of one name.
    RepeatedParameter {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The parameter's name.
        parameter: String,
    },
    /// Two flags of one name.
    RepeatedFlag {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The flag's name.
        flag: String,
    },
    /// More than 64 flags: a flag pattern is a `u64`.
    TooManyFlags {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// Its flags.
        flags: usize,
    },
    /// More readout slots, outcomes and flags together, than a `u32`
    /// counts: no batch's readouts could hold them.
    TooManyReadoutSlots {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// Its outcomes.
        outcomes: u32,
        /// Its flags.
        flags: usize,
    },
    /// A fault of its meaning ([`MeaningFault`]).
    Meaning {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The fault of its meaning.
        fault: MeaningFault,
    },
}

impl fmt::Display for DeclarationFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeclarationFault::RepeatedBlockType { block_type } => {
                write!(f, "block type `{block_type}` is declared twice")
            }
            DeclarationFault::RepeatedMnemonic { mnemonic } => {
                write!(f, "`{mnemonic}` is declared twice")
            }
            DeclarationFault::UnknownBlockType {
                mnemonic,
                block_type,
            } => write!(
                f,
                "`{mnemonic}` acts on undeclared block type `{block_type}`"
            ),
            DeclarationFault::MisplacedVariadic { mnemonic } => {
                write!(f, "`{mnemonic}` has a variadic operand out of place")
            }
            DeclarationFault::RepeatedParameter {
                mnemonic,
                parameter,
            } => write!(f, "`{mnemonic}` declares parameter `{parameter}` twice"),
            DeclarationFault::RepeatedFlag { mnemonic, flag } => {
                write!(f, "`{mnemonic}` declares flag `{flag}` twice")
            }
            DeclarationFault::TooManyFlags { mnemonic, flags } => {
                write!(f, "`{mnemonic}` declares {flags} flags, at most 64")
            }
            DeclarationFault::TooManyReadoutSlots {
                mnemonic,
                outcomes,
                flags,
            } => write!(
                f,
                "`{mnemonic}` declares {outcomes} outcomes and {flags} flags, more readout \
                 slots than a `u32` counts"
            ),
            DeclarationFault::Meaning { mnemonic, fault } => {
                write!(f, "`{mnemonic}`'s action: {fault}")
            }
        }
    }
}

impl std::error::Error for DeclarationFault {}

/// A set written in code, and a `const` [`Opcode`] for each of its
/// instructions, from one list, so the two cannot disagree. For each
/// instruction: its constant, its mnemonic, the blocks it acts on in
/// place (its `in` and its `out`), its parameters after a `;`, and the
/// outcomes it gives after `->` (`1 outcome` or `2 outcomes`; either word
/// for any count), as in the crate's tests' `RotationGates`, copied here:
///
/// ```ignore
/// instruction_set! {
///     /// QIR's gates, as a state-vector simulator accepts them.
///     RotationGates {
///         H = h(qubit),
///         RZ = rz(qubit; theta: Number),
///         CX = cx(qubit, qubit),
///         MZ = mz(qubit) -> 1 outcome,
///     }
/// }
/// ```
///
/// It gives `RotationGates::set()`, the set, built once and cloned, named
/// `RotationGates`, and `RotationGates::H`, …, its opcodes. Its block types
/// are `qubit`, one qubit each, unless listed after the name, as Qodec's
/// `blocks`: `pub CodeGates [carbon: 2] { … }`. Its instructions declare no
/// meaning and no flags: a set that needs them is built with
/// [`BlockInstructionSet::new`]. Attributes before the name go on its
/// type, and a `#[cfg(…)]` also on its constants and `set()`, so a set
/// compiled out leaves nothing behind.
///
/// # Panics
///
/// `set()` panics, on every call, if the declaration has a fault
/// ([`DeclarationFault`]), such as a block type not listed.
#[macro_export]
macro_rules! instruction_set {
    // Each constant is its place in `$all`, the list of every constant:
    // an enum's discriminant, so no recursion limits the set's size.
    (@opcodes $all:tt $($constant:ident $mnemonic:ident)*) => {
        $($crate::instruction_set!(@opcode $all $constant $mnemonic);)*
    };
    (@opcode [$($all:ident)*] $constant:ident $mnemonic:ident) => {
        #[doc = concat!("The opcode of `", stringify!($mnemonic), "`.")]
        pub const $constant: $crate::Opcode = {
            #[allow(dead_code, non_camel_case_types, clippy::upper_case_acronyms)]
            enum Places {
                $($all),*
            }
            $crate::Opcode::__from_macro(Places::$constant as u32)
        };
    };
    (@blocks) => {
        ::std::vec![($crate::BlockType::qubit(), 1)]
    };
    (@blocks [$($block:ident : $qubits:literal),+]) => {
        ::std::vec![$(($crate::BlockType::new(stringify!($block)), $qubits)),+]
    };
    (@outcomes $outcomes:literal outcome) => {
        $outcomes
    };
    (@outcomes $outcomes:literal outcomes) => {
        $outcomes
    };
    // Splits the declaration's attributes, one at a time: its `cfg`s go on
    // the set's `impl` too, so a declaration compiled out leaves no `impl`
    // of a missing type; the others (docs, derives, `cfg_attr`s, which may
    // hold derives) on its type alone.
    (@split [$($cfg:tt)*] [$($other:tt)*] #[cfg($($condition:tt)*)] $($rest:tt)*) => {
        $crate::instruction_set!(
            @split [$($cfg)* #[cfg($($condition)*)]] [$($other)*] $($rest)*
        );
    };
    (@split [$($cfg:tt)*] [$($other:tt)*] #[$($attribute:tt)*] $($rest:tt)*) => {
        $crate::instruction_set!(
            @split [$($cfg)*] [$($other)* #[$($attribute)*]] $($rest)*
        );
    };
    (
        @split [$($cfg:tt)*] [$($other:tt)*]
        $visibility:vis $name:ident $([$($block:ident : $qubits:literal),+ $(,)?])? {
            $($constant:ident = $mnemonic:ident (
                $($operand:ident),* $(; $($parameter:ident : $kind:ident),* $(,)?)?
            ) $(-> $outcomes:literal $unit:ident)?),* $(,)?
        }
    ) => {
        $($cfg)*
        $($other)*
        $visibility struct $name;

        $($cfg)*
        impl $name {
            $crate::instruction_set!(@opcodes [$($constant)*] $($constant $mnemonic)*);

            /// The set, built on the first call, then cloned.
            #[must_use]
            pub fn set() -> $crate::BlockInstructionSet {
                static SET: ::std::sync::OnceLock<$crate::BlockInstructionSet> =
                    ::std::sync::OnceLock::new();
                SET.get_or_init(|| {
                    let blocks: ::std::vec::Vec<($crate::BlockType, u32)> =
                        $crate::instruction_set!(@blocks $([$($block : $qubits),+])?);
                    $crate::BlockInstructionSet::new(
                        stringify!($name),
                        blocks,
                        ::std::vec![$(
                            $crate::InstructionDef::new(stringify!($mnemonic))
                                .acts_on(::std::vec::Vec::<$crate::BlockType>::from([
                                    $($crate::BlockType::new(stringify!($operand))),*
                                ]))
                                $($(.parameter(stringify!($parameter), $crate::ParameterKind::$kind))*)?
                                $(.outcomes($crate::instruction_set!(@outcomes $outcomes $unit)))?
                        ),*],
                    )
                    .unwrap_or_else(|faults| {
                        let faults: ::std::vec::Vec<::std::string::String> =
                            faults.iter().map(::std::string::ToString::to_string).collect();
                        panic!(
                            "`{}` declares a faulty set: {}",
                            stringify!($name),
                            faults.join("; ")
                        )
                    })
                })
                .clone()
            }
        }
    };
    ($($declaration:tt)*) => {
        $crate::instruction_set!(@split [] [] $($declaration)*);
    };
}
