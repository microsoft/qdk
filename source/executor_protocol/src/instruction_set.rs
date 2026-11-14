// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part sends or accepts: an instruction set ([`InstructionSet`]),
//! in Qodec's model, so that a part written against Qodec's sets needs no
//! translation, and two parts' sets can be checked against each other
//! before any shot runs.
//!
//! A set declares its block types (each with the qubits one block
//! encodes) and its instructions ([`InstructionDef`]): for each, its
//! mnemonic, the blocks it takes (`in`) and gives (`out`), its classical
//! parameters, its readouts (its `observe` outcomes, then its flags), and
//! what it does ([`Meaning`]), if it says. An instruction is named by its
//! [`Opcode`], its place in the set, so that what runs compares integers:
//! a set written in code names its opcodes as constants with
//! [`instruction_set!`](crate::instruction_set), a set known only at run
//! time resolves them once with [`InstructionSet::opcode`].
//!
//! A set is checked once, when it is built ([`InstructionSet::new`]),
//! which refuses every fault of its declaration at once
//! ([`DeclarationFault`]), and derives what each instruction's check
//! compares. Its submodules hold the rest of the vocabulary: the kinds of
//! blocks and an instruction's operands (`block_type`), its classical
//! inputs (`parameter`), Pauli operators (`pauli`), what an instruction
//! does (`meaning`), what each instruction's check compares (`layout`),
//! the instructions a batch holds and their checks (`instruction`), and
//! the fit of one set to another (`fit`).

mod block_type;
mod fit;
pub(crate) mod instruction;
mod layout;
mod meaning;
mod parameter;
mod pauli;

pub use block_type::{BlockOperand, BlockType};
pub use fit::{FitFaults, InstructionMismatch, OpcodeMap};
pub use instruction::{
    Argument, Arguments, Bit, Blocks, Instruction, InstructionFault, ReadoutParity, SelectPattern,
};
pub use meaning::{Angle, Guard, Meaning, MeaningFault, Observable};
pub use parameter::{Parameter, ParameterKind};
pub use pauli::Pauli;

use layout::Layout;
use std::collections::BTreeMap;
use std::fmt;

/// An instruction set, as Qodec declares one: its name, its block types
/// and its instructions, each at the place its [`Opcode`] names.
///
/// Built by [`InstructionSet::new`], or, for a set written in code, by
/// [`instruction_set!`](crate::instruction_set), which also names its
/// opcodes as constants.
#[derive(Clone, Debug)]
pub struct InstructionSet {
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

impl InstructionSet {
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
                Opcode(u32::try_from(index).expect("a set has fewer than 2^32 instructions"));
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

    /// Its name, as Qodec's `name`.
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
            .map(|index| Opcode(u32::try_from(index).expect("checked when the set is built")))
    }
}

/// An instruction's declaration in its set, as Qodec writes it.
///
/// Built field by field, or step by step from [`InstructionDef::new`]:
/// Qodec's `{mnemonic: rz, in: [qubit], out: [qubit], parameters: {theta:
/// number}}` is `InstructionDef::new("rz").acts_on([BlockType::qubit()])
/// .parameter("theta", ParameterKind::Number)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionDef {
    /// `mnemonic`: its name, unique in its set.
    pub mnemonic: String,
    /// `in`: the blocks it acts on, in order.
    pub inputs: Vec<BlockOperand>,
    /// `out`: those that survive, at the same positions, or that it makes,
    /// past `in`'s.
    pub outputs: Vec<BlockOperand>,
    /// Its classical inputs, in order.
    pub parameters: Vec<Parameter>,
    /// Its `observe` outcomes.
    pub outcomes: u32,
    /// Then its flags: its readouts are the outcomes, then the flags.
    pub flags: Vec<String>,
    /// Its Qodec `action`, in canonical form, or undeclared.
    pub meaning: Meaning,
}

impl InstructionDef {
    /// The instruction `mnemonic`, on no block, with no parameter, readout
    /// or meaning yet.
    #[must_use]
    pub fn new(mnemonic: impl Into<String>) -> Self {
        Self {
            mnemonic: mnemonic.into(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            parameters: Vec::new(),
            outcomes: 0,
            flags: Vec::new(),
            meaning: Meaning::undeclared(),
        }
    }

    /// Takes `operands` (`in`), consuming them.
    #[must_use]
    pub fn takes(mut self, operands: impl IntoIterator<Item = impl Into<BlockOperand>>) -> Self {
        self.inputs = operands.into_iter().map(Into::into).collect();
        self
    }

    /// Gives `operands` (`out`): those at `in`'s positions survive, the
    /// others are made.
    #[must_use]
    pub fn gives(mut self, operands: impl IntoIterator<Item = impl Into<BlockOperand>>) -> Self {
        self.outputs = operands.into_iter().map(Into::into).collect();
        self
    }

    /// Acts on `operands` in place: they are both its `in` and its `out`,
    /// as a gate's qubits.
    #[must_use]
    pub fn acts_on(self, operands: impl IntoIterator<Item = impl Into<BlockOperand>>) -> Self {
        let operands: Vec<BlockOperand> = operands.into_iter().map(Into::into).collect();
        self.takes(operands.clone()).gives(operands)
    }

    /// Then the parameter `name` of kind `kind`.
    #[must_use]
    pub fn parameter(mut self, name: impl Into<String>, kind: ParameterKind) -> Self {
        self.parameters.push(Parameter::new(name, kind));
        self
    }

    /// Gives `outcomes` outcomes.
    #[must_use]
    pub fn outcomes(mut self, outcomes: u32) -> Self {
        self.outcomes = outcomes;
        self
    }

    /// Reports `flags`, after its outcomes.
    #[must_use]
    pub fn flags(mut self, flags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.flags = flags.into_iter().map(Into::into).collect();
        self
    }

    /// Does what `meaning` says.
    #[must_use]
    pub fn meaning(mut self, meaning: Meaning) -> Self {
        self.meaning = meaning;
        self
    }

    /// A parameter or a flag it names twice, each repeat.
    fn naming_faults(&self) -> Vec<DeclarationFault> {
        let mut faults = Vec::new();
        for (k, parameter) in self.parameters.iter().enumerate() {
            if self.parameters[..k]
                .iter()
                .any(|earlier| earlier.name == parameter.name)
            {
                faults.push(DeclarationFault::RepeatedParameter {
                    mnemonic: self.mnemonic.clone(),
                    parameter: parameter.name.clone(),
                });
            }
        }
        for (k, flag) in self.flags.iter().enumerate() {
            if self.flags[..k].contains(flag) {
                faults.push(DeclarationFault::RepeatedFlag {
                    mnemonic: self.mnemonic.clone(),
                    flag: flag.clone(),
                });
            }
        }
        faults
    }
}

/// An instruction's place in its set. Its index is private: a part gets
/// its opcodes from [`instruction_set!`](crate::instruction_set)'s
/// constants or from [`InstructionSet::opcode`], and `match`es on them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Opcode(u32);

impl Opcode {
    /// The opcode at `index`, for the constants of
    /// [`instruction_set!`](crate::instruction_set) only: elsewhere it would
    /// name an instruction no set has resolved.
    #[doc(hidden)]
    #[must_use]
    pub const fn __from_macro(index: u32) -> Self {
        Opcode(index)
    }

    /// Its index in its set.
    pub(crate) fn index(self) -> usize {
        self.0 as usize
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
    /// variadic on one side and not on the other.
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
    /// counts: no answer's width could hold them.
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
/// [`InstructionSet::new`].
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
    (
        $(#[$attribute:meta])*
        $visibility:vis $name:ident $([$($block:ident : $qubits:literal),+ $(,)?])? {
            $($constant:ident = $mnemonic:ident (
                $($operand:ident),* $(; $($parameter:ident : $kind:ident),* $(,)?)?
            ) $(-> $outcomes:literal $unit:ident)?),* $(,)?
        }
    ) => {
        $(#[$attribute])*
        $visibility struct $name;

        impl $name {
            $crate::instruction_set!(@opcodes [$($constant)*] $($constant $mnemonic)*);

            /// The set, built on the first call, then cloned.
            #[must_use]
            pub fn set() -> $crate::InstructionSet {
                static SET: ::std::sync::OnceLock<$crate::InstructionSet> =
                    ::std::sync::OnceLock::new();
                SET.get_or_init(|| {
                    let blocks: ::std::vec::Vec<($crate::BlockType, u32)> =
                        $crate::instruction_set!(@blocks $([$($block : $qubits),+])?);
                    $crate::InstructionSet::new(
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
                        panic!("`{}` declares a faulty set: {faults:?}", stringify!($name))
                    })
                })
                .clone()
            }
        }
    };
}
