// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Descriptions of instruction sets: the description of the set an
//! executor accepts, and the matching of one set's instructions in another.
//!
//! An executor's instructions are a type: a Rust type
//! (`InstructionBatch<Op>`), whose set the compiler fixes, or
//! [`InstructionCall`] (`InstructionBatch<InstructionCall>`), whose set is
//! chosen at run time: read from a file (for instance, a Qodec instruction
//! set), or sent by a remote device. Either may describe the set it accepts
//! in [`Capabilities::instruction_set`](crate::Capabilities), since the type
//! fixes the shape of an instruction, not which ones the executor runs (a
//! device without `cx` still takes `InstructionBatch<Op>`). With the
//! description, a chain checks, when it is built, that each adapter sends
//! only what the downstream executor accepts ([`InstructionSet::fits`]), and,
//! when a job is given to it ([`Chain::run`](crate::Chain::run)), that the
//! executor accepts the set of the job's program. An
//! executor of `InstructionCall`s also checks each call
//! ([`InstructionSet::check_batch`]), since the compiler cannot.
//!
//! A call names its instruction by [`Opcode`], the instruction's position
//! in its set, not by mnemonic: names are resolved once, when a program or
//! an adapter is built ([`InstructionSet::opcode`]), so a call is checked
//! and dispatched by an index ([`OpcodeTable`]).
//!
//! An opcode means something only in its set: a subset, or the same
//! instructions in another order, numbers them differently. So one set's
//! instructions are matched in another once, giving each its opcode there
//! ([`InstructionSet::opcodes_in`], an [`OpcodeMap`]): an adapter resolves
//! what it sends into the downstream numbering when it is built, and a
//! chain adapts its job's numbering to its own when the job is given to it
//! ([`Chain::run`](crate::Chain::run)). Where two numberings differ, calls
//! are renumbered on their way ([`Renumbering`]).
//!
//! The description follows Qodec's: an instruction is named by its mnemonic,
//! takes block operands, then parameters, then one tag per readout, and may
//! declare what it does, its [`Action`]. A set's name is its identity: in a
//! set of the same name, an instruction is matched by mnemonic, since `x`
//! means the same at both ends. Between sets of other names, it is matched
//! by action, whatever its mnemonic (Stim's `R` is a toy's `reset`), so
//! both sets must declare it: an instruction whose action either set leaves
//! undeclared is refused, since what it does cannot be checked. Whatever
//! the names, a mnemonic that both sets declare to do different things is
//! refused, as a collision.
//!
//! - [`call`]: the calls of a set known only at run time, each naming its
//!   instruction by opcode.
//! - [`action`]: what an instruction does, by which instructions of sets
//!   of other names are matched.
//! - [`instruction`]: what an instruction type tells every boundary where
//!   its calls meet an executor: whether they name their instruction by
//!   opcode, or by their type.
//! - [`renumbered`]: how calls by opcode reach an executor that numbers
//!   their instructions otherwise.

pub(crate) mod action;
pub(crate) mod call;
pub(crate) mod instruction;
pub(crate) mod renumbered;

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Index;
use std::sync::Arc;

#[cfg(doc)]
use crate::Instruction;
use crate::{Action, CallDisplay, InstructionCall, Opcode, Operand, Rejected};

/// The kind of one operand of an instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OperandKind {
    /// The address of a block the instruction acts on: a qubit in a physical
    /// set, a block of a code in a logical one.
    Block,
    /// A real parameter, such as a rotation's angle.
    Real,
    /// An integer parameter.
    Integer,
    /// The tag of one of the instruction's readouts, the
    /// [`ShotOutcome::tag`](crate::ShotOutcome::tag) it is reported under.
    Tag,
}

/// The description of an instruction set: its name, and its instructions
/// in order, each a mnemonic, the operands it takes, and what it does when
/// the set declares it. It describes a set whatever the type of its
/// instructions: a Rust type's, or [`InstructionCall`]s'. An instruction's
/// [`Opcode`] is its position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionSet {
    name: String,
    /// The instructions, each at its opcode.
    instructions: Vec<Declaration>,
    /// Each mnemonic's opcode, to resolve names when a call is built.
    opcodes: BTreeMap<String, Opcode>,
}

/// One instruction of a set, as the set declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Declaration {
    mnemonic: String,
    kinds: Vec<OperandKind>,
    /// What it does; `None`: not declared.
    action: Option<Action>,
}

impl InstructionSet {
    /// The set named `name`, with no instructions. The name is the set's
    /// identity: sets of one name are matched by mnemonic
    /// ([`opcodes_in`](Self::opcodes_in)), so name a set for its source
    /// (`qep`, `c4c6`), not for its role in a chain (`device`, `sent`).
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            instructions: Vec::new(),
            opcodes: BTreeMap::new(),
        }
    }

    /// This set, with the instruction `mnemonic` taking `operands`, its
    /// action not declared: at the next opcode, or in place of the
    /// instruction of that mnemonic (its action included), at its opcode.
    ///
    /// # Panics
    ///
    /// When the set already has an instruction at opcode `u32::MAX` and
    /// `mnemonic` is new.
    #[must_use]
    pub fn with(self, mnemonic: &str, operands: impl IntoIterator<Item = OperandKind>) -> Self {
        self.insert(mnemonic, operands, None)
    }

    /// This set, with the instruction `mnemonic` taking `operands` and doing
    /// `action`, placed as [`with`](Self::with) places it.
    ///
    /// # Panics
    ///
    /// When the set already has an instruction at opcode `u32::MAX` and
    /// `mnemonic` is new.
    //
    // Known issue, open: nothing checks the blocks `action` takes and gives
    // against the `Block`s of `operands`, so a description that disagrees
    // with itself is matched as written. A loader that reads both from one
    // Qodec instruction keeps them together. A set of one name is matched
    // by mnemonic, so a set matched with itself passes whatever its actions
    // say: the check belongs here, where the instruction is declared, not
    // in matching. Pinned, between sets of other names and ignored until
    // then (it then checks the declaration instead), by
    // `an_instruction_whose_action_and_operands_disagree_on_its_blocks_is_not_matched`.
    #[must_use]
    pub fn with_action(
        self,
        mnemonic: &str,
        operands: impl IntoIterator<Item = OperandKind>,
        action: Action,
    ) -> Self {
        self.insert(mnemonic, operands, Some(action))
    }

    fn insert(
        mut self,
        mnemonic: &str,
        operands: impl IntoIterator<Item = OperandKind>,
        action: Option<Action>,
    ) -> Self {
        let instruction = Declaration {
            mnemonic: mnemonic.to_owned(),
            kinds: operands.into_iter().collect(),
            action,
        };
        if let Some(opcode) = self.opcode(mnemonic) {
            self.instructions[opcode.index()] = instruction;
        } else {
            let opcode = u32::try_from(self.instructions.len())
                .expect("a set has fewer than 2^32 instructions");
            self.opcodes
                .insert(mnemonic.to_owned(), Opcode::new(opcode));
            self.instructions.push(instruction);
        }
        self
    }

    /// Its mnemonics, in the order of their opcodes.
    pub fn mnemonics(&self) -> impl Iterator<Item = &str> {
        self.instructions
            .iter()
            .map(|instruction| instruction.mnemonic.as_str())
    }

    /// The opcode of `mnemonic`; `None` when the set has no such
    /// instruction.
    #[must_use]
    pub fn opcode(&self, mnemonic: &str) -> Option<Opcode> {
        self.opcodes.get(mnemonic).copied()
    }

    /// The mnemonic of the instruction at `opcode`; `None` when the set has
    /// no instruction there.
    #[must_use]
    pub fn mnemonic(&self, opcode: Opcode) -> Option<&str> {
        Some(&self.instructions.get(opcode.index())?.mnemonic)
    }

    /// `call`, to be written with the mnemonic of its instruction in this
    /// set, for a log or a debugger: `cx(block 0, block 1)`.
    #[must_use]
    pub fn display<'a>(&'a self, call: &'a InstructionCall) -> CallDisplay<'a> {
        CallDisplay { set: self, call }
    }

    /// The operands `mnemonic` takes; `None` when the set has no such
    /// instruction.
    #[must_use]
    pub fn operands(&self, mnemonic: &str) -> Option<&[OperandKind]> {
        Some(&self.instructions[self.opcode(mnemonic)?.index()].kinds)
    }

    /// The call of `mnemonic` on `operands`, checked: for building a
    /// program by mnemonic, once.
    ///
    /// # Errors
    ///
    /// [`InstructionMismatch::Missing`] when the set has no instruction
    /// `mnemonic`; [`InstructionMismatch::DifferentOperands`] when
    /// `operands` are not of the kinds it takes.
    pub fn call(
        &self,
        mnemonic: &str,
        operands: impl IntoIterator<Item = Operand>,
    ) -> Result<InstructionCall, InstructionMismatch> {
        let opcode = self
            .opcode(mnemonic)
            .ok_or_else(|| InstructionMismatch::Missing {
                mnemonic: mnemonic.to_owned(),
            })?;
        let call = InstructionCall::new(opcode, operands);
        self.check(&call)?;
        Ok(call)
    }

    /// Checks that `call` is an instruction of this set, with operands of
    /// the kinds it takes.
    ///
    /// # Errors
    ///
    /// [`InstructionMismatch::UnknownOpcode`] when the set has no
    /// instruction at `call`'s opcode;
    /// [`InstructionMismatch::DifferentOperands`] when `call`'s operands
    /// are not of the kinds it takes.
    pub fn check(&self, call: &InstructionCall) -> Result<(), InstructionMismatch> {
        let Some(instruction) = self.instructions.get(call.opcode.index()) else {
            return Err(InstructionMismatch::UnknownOpcode {
                opcode: call.opcode,
            });
        };
        if instruction
            .kinds
            .iter()
            .copied()
            .eq(call.operands.iter().map(Operand::kind))
        {
            return Ok(());
        }
        Err(InstructionMismatch::DifferentOperands {
            mnemonic: instruction.mnemonic.clone(),
        })
    }

    /// Checks each call of `batch`, in order, as [`check`](Self::check)
    /// does. Needed so that an executor of `InstructionCall`s refuses a
    /// batch before it runs any of it: a call the set lacks may come after
    /// calls that would already have run.
    ///
    /// # Errors
    ///
    /// [`Rejected::Instruction`] at the first call that is not an
    /// instruction of this set, with its [`InstructionMismatch`] as the
    /// reason.
    pub fn check_batch(&self, batch: &[InstructionCall]) -> Result<(), Rejected> {
        for (at, call) in batch.iter().enumerate() {
            self.check(call).map_err(|mismatch| Rejected::Instruction {
                at,
                reason: mismatch.to_string(),
            })?;
        }
        Ok(())
    }

    /// Checks that this set, the one an adapter or a caller sends, fits
    /// `accepted`, the one the downstream executor accepts: that each of its
    /// instructions is matched in `accepted`, as
    /// [`opcodes_in`](Self::opcodes_in) matches them. So any part of a set
    /// fits the set, in any order, under the set's name or, when each
    /// instruction declares what it does, under another; the empty set fits
    /// any set. Fitting goes one way: a set does not fit a part of itself.
    ///
    /// # Errors
    ///
    /// As [`opcodes_in`](Self::opcodes_in).
    pub fn fits(&self, accepted: &InstructionSet) -> Result<(), Vec<InstructionMismatch>> {
        self.opcodes_in(accepted).map(|_| ())
    }

    /// Matches each instruction of this set, the one an adapter or a caller
    /// sends, in `accepted`, the one the downstream executor accepts, and gives
    /// its opcode there: what the sender needs to send this
    /// set's calls in `accepted`'s numbering. Each is matched to an
    /// instruction of `accepted` that takes operands of the same kinds, in
    /// the same order:
    ///
    /// - by mnemonic, when `accepted` has this set's name: a set's name is
    ///   its identity, so whatever `x` means, it means the same at both
    ///   ends, and no action is needed. One set at both ends gives the
    ///   identity map, so nothing is renumbered.
    /// - by action, when it has another name: to an instruction that does
    ///   the same, under its own mnemonic if it is one, else under any
    ///   (Stim's `R` is a toy's `reset`). Both sets must declare it, since
    ///   otherwise what the instruction does cannot be checked: a mnemonic
    ///   alone is not a match.
    ///
    /// Whatever the names, a mnemonic that both sets declare to do
    /// different things is refused, though another instruction does the
    /// same: one mnemonic meaning two things at one boundary is a
    /// collision, such as two unrelated sets given one name.
    ///
    /// # Errors
    ///
    /// The [`InstructionMismatch`] of each instruction not matched, in this
    /// set's order, so that whoever builds the chain can fix them all at
    /// once: [`DifferentAction`](InstructionMismatch::DifferentAction) when
    /// both declare what the instruction of its mnemonic does, and it
    /// differs; [`Missing`](InstructionMismatch::Missing) when `accepted`
    /// has no instruction of its mnemonic, nor, under another name, one
    /// doing the same with the same operands;
    /// [`DifferentOperands`](InstructionMismatch::DifferentOperands) when
    /// the instruction of its mnemonic takes other operands;
    /// [`UndeclaredAction`](InstructionMismatch::UndeclaredAction) when
    /// `accepted`, a set of another name, has an instruction of its
    /// mnemonic taking the same operands, and either set leaves what it
    /// does undeclared.
    pub fn opcodes_in(
        &self,
        accepted: &InstructionSet,
    ) -> Result<OpcodeMap, Vec<InstructionMismatch>> {
        let same_name = self.name == accepted.name;
        let mut opcodes = Vec::with_capacity(self.instructions.len());
        let mut mismatches = Vec::new();
        for ours in &self.instructions {
            match accepted.match_of(ours, same_name) {
                Ok(opcode) => opcodes.push(opcode),
                Err(mismatch) => mismatches.push(mismatch),
            }
        }
        if mismatches.is_empty() {
            Ok(OpcodeMap {
                opcodes,
                // Known issue, not measured: a deep copy of the set on each
                // match, build time only, also where the caller holds it in
                // an `Arc` or the map is the identity and dropped. Taking
                // the caller's `Arc` would avoid it, if it shows.
                accepted: Arc::new(accepted.clone()),
            })
        } else {
            Err(mismatches)
        }
    }

    /// The opcode in this set of `ours`, an instruction of another, as
    /// [`opcodes_in`](Self::opcodes_in) matches it: by mnemonic when the
    /// other set has this one's name (`same_name`), else by action.
    fn match_of(&self, ours: &Declaration, same_name: bool) -> Result<Opcode, InstructionMismatch> {
        let mnemonic = || ours.mnemonic.clone();
        let named = self.opcode(&ours.mnemonic);
        let theirs = named.map(|opcode| &self.instructions[opcode.index()]);
        // The guard, whatever the names: one mnemonic declared to do two
        // things at one boundary is a collision, not a match to look past.
        if let Some(theirs) = theirs
            && let (Some(action), Some(their_action)) = (&ours.action, &theirs.action)
            && action != their_action
        {
            return Err(InstructionMismatch::DifferentAction {
                mnemonic: mnemonic(),
            });
        }
        if !same_name && let Some(action) = &ours.action {
            let does_the_same = |opcode: &Opcode| {
                let theirs = &self.instructions[opcode.index()];
                theirs.action.as_ref() == Some(action) && theirs.kinds == ours.kinds
            };
            let every = (0..self.instructions.len()).map(|index| {
                Opcode::new(u32::try_from(index).expect("a set has fewer than 2^32 instructions"))
            });
            if let Some(opcode) = named.into_iter().chain(every).find(does_the_same) {
                return Ok(opcode);
            }
        }
        let Some(opcode) = named else {
            return Err(InstructionMismatch::Missing {
                mnemonic: mnemonic(),
            });
        };
        let theirs = &self.instructions[opcode.index()];
        if theirs.kinds != ours.kinds {
            return Err(InstructionMismatch::DifferentOperands {
                mnemonic: mnemonic(),
            });
        }
        if same_name {
            return Ok(opcode);
        }
        // Both declare an action here only when they also take the same
        // operands and do the same, which the search above matched.
        Err(InstructionMismatch::UndeclaredAction {
            mnemonic: mnemonic(),
        })
    }

    /// The table of `handler`'s value for each instruction, by opcode: what
    /// an executor of [`InstructionCall`]s builds once, when it is made, to
    /// run each call through its opcode's entry. `handler` is given each
    /// mnemonic, in the order of their opcodes, and returns `None` for an
    /// instruction the executor cannot run.
    ///
    /// # Errors
    ///
    /// Every mnemonic for which `handler` returned `None`, in the order of
    /// their opcodes, so that whoever writes the executor sees them all at
    /// once.
    pub fn table<T>(
        &self,
        mut handler: impl FnMut(&str) -> Option<T>,
    ) -> Result<OpcodeTable<T>, Vec<String>> {
        let mut entries = Vec::with_capacity(self.instructions.len());
        let mut unhandled = Vec::new();
        for instruction in &self.instructions {
            match handler(&instruction.mnemonic) {
                Some(entry) => entries.push(entry),
                None => unhandled.push(instruction.mnemonic.clone()),
            }
        }
        if unhandled.is_empty() {
            Ok(OpcodeTable(entries))
        } else {
            Err(unhandled)
        }
    }
}

/// Where one set's instructions are in another
/// ([`InstructionSet::opcodes_in`]): each instruction's opcode in the other
/// set, by its opcode in its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpcodeMap {
    /// The opcode in the other set, at each opcode of this one.
    opcodes: Vec<Opcode>,
    /// The other set, which a [`Renumbered`](renumbered::Renumbered)
    /// batch refused at a call it cannot renumber is checked against, up
    /// to that call, so that an earlier call outside is reported instead.
    pub(crate) accepted: Arc<InstructionSet>,
}

impl OpcodeMap {
    /// The opcode in the other set of the instruction at `opcode` in this
    /// one; `None` when this set has no instruction there.
    #[must_use]
    pub fn get(&self, opcode: Opcode) -> Option<Opcode> {
        self.opcodes.get(opcode.index()).copied()
    }

    /// Whether each instruction has the same opcode in both sets, so that a
    /// call needs no renumbering: true when this set's instructions are the
    /// first of the other, in the same order.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.opcodes
            .iter()
            .enumerate()
            .all(|(index, opcode)| opcode.index() == index)
    }
}

/// How calls in one numbering reach an executor that numbers the same
/// instructions otherwise: where each of the sender's instructions is in
/// the executor's set ([`Instruction::renumbering`]).
#[derive(Debug)]
pub struct Renumbering {
    // Crate-visible, not public, so that only the crate builds one: the
    // seal of `Instruction` rests on it.
    pub(crate) map: OpcodeMap,
}

/// A value `T` for each instruction of a set, at its opcode
/// ([`InstructionSet::table`]): an executor's handlers, which it indexes by
/// a call's opcode.
#[derive(Clone, Debug)]
pub struct OpcodeTable<T>(Vec<T>);

impl<T> Index<Opcode> for OpcodeTable<T> {
    type Output = T;

    /// The entry of the instruction at `opcode`.
    ///
    /// # Panics
    ///
    /// When `opcode` is not of the set the table was built from: an
    /// executor checks its batch first ([`InstructionSet::check_batch`]).
    fn index(&self, opcode: Opcode) -> &T {
        &self.0[opcode.index()]
    }
}

/// Why an instruction is not one of a set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstructionMismatch {
    /// The set has no instruction of this mnemonic.
    Missing {
        /// The mnemonic.
        mnemonic: String,
    },
    /// The set has no instruction at this opcode: the call was built for
    /// another set.
    UnknownOpcode {
        /// The opcode.
        opcode: Opcode,
    },
    /// The set has the instruction, taking other operands.
    DifferentOperands {
        /// The mnemonic.
        mnemonic: String,
    },
    /// The set has an instruction of this mnemonic that does something
    /// else: both sets declare what it does, and it differs.
    DifferentAction {
        /// The mnemonic.
        mnemonic: String,
    },
    /// The set, of another name, has an instruction of this mnemonic,
    /// taking the same operands, but either set (or both) leaves what it
    /// does undeclared, so whether it does the same cannot be checked.
    UndeclaredAction {
        /// The mnemonic.
        mnemonic: String,
    },
}

/// Writes `mismatches` separated by commas, for the messages of the
/// errors that carry all of them.
pub(crate) fn write_mismatches(
    f: &mut fmt::Formatter<'_>,
    mismatches: &[InstructionMismatch],
) -> fmt::Result {
    for (i, mismatch) in mismatches.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{mismatch}")?;
    }
    Ok(())
}

impl fmt::Display for InstructionMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstructionMismatch::Missing { mnemonic } => write!(f, "no instruction `{mnemonic}`"),
            InstructionMismatch::UnknownOpcode { opcode } => {
                write!(f, "no instruction at opcode {}", opcode.index())
            }
            InstructionMismatch::DifferentOperands { mnemonic } => {
                write!(f, "`{mnemonic}` takes other operands")
            }
            InstructionMismatch::DifferentAction { mnemonic } => {
                write!(f, "`{mnemonic}` does something else")
            }
            InstructionMismatch::UndeclaredAction { mnemonic } => {
                write!(f, "`{mnemonic}` has no declared action to check")
            }
        }
    }
}

impl std::error::Error for InstructionMismatch {}
