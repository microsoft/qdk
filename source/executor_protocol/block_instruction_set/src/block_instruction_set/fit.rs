// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Whether the instructions one part sends fit those the part below
//! accepts ([`BlockInstructionSet`]'s [`fits`](BoundaryCompatible::fits)),
//! and, if they do, the receiver's opcode for each ([`OpcodeMap`]), with
//! which the sender translates its instructions before sending them;
//! otherwise every mismatch ([`InstructionMismatch`]).
//!
//! Each sent instruction must be accepted with the same blocks (`in` and
//! `out`, their types and sizes), parameters (their kinds, in order) and
//! readouts (outcomes, then flags). It is matched by meaning where both
//! instructions declare one, so a set of another name fits when its
//! instructions do the same (Stim's `H` is a state vector's `h`); by
//! mnemonic only where neither declares one and the sets' names agree,
//! each such match listed in the [`OpcodeMap`] that the sender receives,
//! since what the instruction does is then not checked; and refused
//! otherwise. A shared set name alone proves nothing, since two sets may
//! share one: one mnemonic both sets declare to do different things is
//! refused, though another instruction does the same.

use super::{BlockInstructionSet, InstructionDef, Opcode, OpcodeMap};
use executor_protocol_core::BoundaryCompatible;
use std::fmt;

/// Every mismatch of a set's fit, in the sent set's order: why it does not
/// fit the receiver's.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockInstructionSetMismatch(Vec<InstructionMismatch>);

impl BlockInstructionSetMismatch {
    /// Each sent instruction's mismatch, in the sent set's order.
    #[must_use]
    pub fn mismatches(&self) -> &[InstructionMismatch] {
        &self.0
    }
}

/// Each mismatch, joined by `; `.
impl fmt::Display for BlockInstructionSetMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut separator = "";
        for mismatch in &self.0 {
            write!(f, "{separator}{mismatch}")?;
            separator = "; ";
        }
        Ok(())
    }
}

impl std::error::Error for BlockInstructionSetMismatch {}

impl BoundaryCompatible for BlockInstructionSet {
    type Fit = OpcodeMap;
    type Mismatch = BlockInstructionSetMismatch;

    /// Whether the instructions of `self`, the set a part sends, fit
    /// `receiver`, the set the part below accepts: the receiver's opcode
    /// for each, or every mismatch.
    ///
    /// Each is matched, with the same blocks, parameters and readouts: by
    /// meaning where it declares one, to the receiver's instruction of its
    /// mnemonic if that one does the same, else to any that does, unless
    /// that one declares it does something else, which is refused; by
    /// mnemonic where neither declares one and the sets' names agree,
    /// listed in [`OpcodeMap::by_mnemonic`] of the map the sender
    /// receives.
    ///
    /// # Errors
    ///
    /// Every mismatch, in the order of `self`'s instructions, so that whoever
    /// builds the chain fixes them at once. For each instruction not
    /// matched, the first that applies:
    /// [`Meaning`](InstructionMismatch::Meaning) when both it and the
    /// receiver's instruction of its mnemonic declare what they do, and
    /// differ; [`Missing`](InstructionMismatch::Missing) when the receiver
    /// has no instruction of its mnemonic, nor one doing the same with the
    /// same blocks, parameters and readouts; each of
    /// [`Blocks`](InstructionMismatch::Blocks),
    /// [`Parameters`](InstructionMismatch::Parameters) and
    /// [`Readouts`](InstructionMismatch::Readouts) that differ from the
    /// receiver's instruction of its mnemonic; `Meaning` when one of the
    /// two declares what it does and the other does not; else, both
    /// undeclared under sets of other names,
    /// [`OtherSet`](InstructionMismatch::OtherSet), given once, at the
    /// first such instruction, naming each.
    fn fits(
        &self,
        receiver: &BlockInstructionSet,
    ) -> Result<OpcodeMap, BlockInstructionSetMismatch> {
        let same_name = self.name == receiver.name;
        let mut opcodes = Vec::with_capacity(self.instructions.len());
        let mut by_mnemonic = Vec::new();
        let mut mismatches = Vec::new();
        // The undeclared instructions a set of another name receives, and
        // where the one mismatch naming them goes.
        let (mut other_set, mut other_set_at) = (Vec::new(), 0);
        for sent in &self.instructions {
            match self.match_in(sent, receiver) {
                Ok(Match::ByMeaning(opcode)) => opcodes.push(opcode),
                Ok(Match::ByMnemonic(opcode)) if same_name => {
                    opcodes.push(opcode);
                    by_mnemonic.push(sent.mnemonic.clone());
                }
                Ok(Match::ByMnemonic(_)) => {
                    if other_set.is_empty() {
                        other_set_at = mismatches.len();
                    }
                    other_set.push(sent.mnemonic.clone());
                }
                Err(found) => mismatches.extend(found),
            }
        }
        if !other_set.is_empty() {
            let other_set = InstructionMismatch::OtherSet {
                mnemonics: other_set,
            };
            mismatches.insert(other_set_at, other_set);
        }
        if mismatches.is_empty() {
            Ok(OpcodeMap::new(opcodes, by_mnemonic))
        } else {
            Err(BlockInstructionSetMismatch(mismatches))
        }
    }
}

impl BlockInstructionSet {
    /// The receiver's opcode for `sent`, an instruction of `self`, as
    /// [`fits`](BoundaryCompatible::fits) matches it, or its mismatches; a
    /// match by mnemonic, whatever the sets' names, which `fits` refuses
    /// between sets of other names.
    fn match_in(
        &self,
        sent: &InstructionDef,
        receiver: &BlockInstructionSet,
    ) -> Result<Match, Vec<InstructionMismatch>> {
        let mnemonic = || sent.mnemonic.clone();
        let named = receiver.opcode(&sent.mnemonic);
        let theirs = named.map(|opcode| receiver.def(opcode).expect("an opcode of the receiver"));
        // Whatever the names, one mnemonic declared to do two things at one
        // boundary is a collision, not a match to look past.
        if let Some(theirs) = theirs
            && sent.meaning.is_declared()
            && theirs.meaning.is_declared()
            && sent.meaning != theirs.meaning
        {
            return Err(vec![InstructionMismatch::Meaning {
                mnemonic: mnemonic(),
            }]);
        }
        if sent.meaning.is_declared() {
            let does_the_same = |opcode: &Opcode| {
                let candidate = receiver.def(*opcode).expect("an opcode of the receiver");
                candidate.meaning == sent.meaning
                    && self.shape_mismatches(sent, receiver, candidate).is_empty()
            };
            if let Some(opcode) = named
                .into_iter()
                .chain(receiver.opcodes())
                .find(does_the_same)
            {
                return Ok(Match::ByMeaning(opcode));
            }
        }
        let (Some(opcode), Some(theirs)) = (named, theirs) else {
            return Err(vec![InstructionMismatch::Missing {
                mnemonic: mnemonic(),
            }]);
        };
        let shape = self.shape_mismatches(sent, receiver, theirs);
        if !shape.is_empty() {
            return Err(shape);
        }
        // Here both declaring would have matched by meaning above.
        if sent.meaning.is_declared() || theirs.meaning.is_declared() {
            return Err(vec![InstructionMismatch::Meaning {
                mnemonic: mnemonic(),
            }]);
        }
        Ok(Match::ByMnemonic(opcode))
    }

    /// How `theirs`, of `receiver`, differs from `sent`, of `self`, in
    /// blocks, parameters and readouts: each that differs.
    fn shape_mismatches(
        &self,
        sent: &InstructionDef,
        receiver: &BlockInstructionSet,
        theirs: &InstructionDef,
    ) -> Vec<InstructionMismatch> {
        let mnemonic = || sent.mnemonic.clone();
        let mut mismatches = Vec::new();
        let same_operands = |ours: &[super::BlockOperand], other: &[super::BlockOperand]| {
            ours.len() == other.len()
                && ours.iter().zip(other).all(|(a, b)| {
                    a == b && self.qubits_of(&a.block_type) == receiver.qubits_of(&b.block_type)
                })
        };
        if !same_operands(&sent.inputs, &theirs.inputs)
            || !same_operands(&sent.outputs, &theirs.outputs)
        {
            mismatches.push(InstructionMismatch::Blocks {
                mnemonic: mnemonic(),
            });
        }
        if !sent
            .parameters
            .iter()
            .map(|p| p.kind)
            .eq(theirs.parameters.iter().map(|p| p.kind))
        {
            mismatches.push(InstructionMismatch::Parameters {
                mnemonic: mnemonic(),
            });
        }
        let readouts = |def: &InstructionDef| (def.outcomes, flag_count(def));
        if readouts(sent) != readouts(theirs) {
            mismatches.push(InstructionMismatch::Readouts {
                mnemonic: mnemonic(),
                sent: readouts(sent),
                accepted: readouts(theirs),
            });
        }
        mismatches
    }
}

/// Why one sent instruction does not fit the receiver: what
/// [`BlockInstructionSet`]'s [`fits`](BoundaryCompatible::fits) finds.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstructionMismatch {
    /// These instructions declare no meaning, nor do the receiver's of
    /// their mnemonics, and the sets' names differ, so their mnemonics
    /// prove nothing.
    OtherSet {
        /// The instructions' mnemonics, in the sent set's order.
        mnemonics: Vec<String>,
    },
    /// The receiver has no instruction of this mnemonic, nor one doing the
    /// same with the same blocks, parameters and readouts.
    Missing {
        /// The instruction's mnemonic.
        mnemonic: String,
    },
    /// Its `in` or `out` differ: their block types, their sizes, or which
    /// are variadic.
    Blocks {
        /// The instruction's mnemonic.
        mnemonic: String,
    },
    /// Its parameters' kinds, in order, differ.
    Parameters {
        /// The instruction's mnemonic.
        mnemonic: String,
    },
    /// Its readouts differ: `(outcomes, flags)` of each side.
    Readouts {
        /// The instruction's mnemonic.
        mnemonic: String,
        /// The sent instruction's `(outcomes, flags)`.
        sent: (u32, u32),
        /// The accepted instruction's `(outcomes, flags)`.
        accepted: (u32, u32),
    },
    /// The receiver's instruction of this mnemonic does something else, or
    /// one side declares what it does and the other does not.
    Meaning {
        /// The instruction's mnemonic.
        mnemonic: String,
    },
}

impl fmt::Display for InstructionMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstructionMismatch::OtherSet { mnemonics } => {
                f.write_str("no meaning is declared for ")?;
                let mut separator = "";
                for mnemonic in mnemonics {
                    write!(f, "{separator}`{mnemonic}`")?;
                    separator = ", ";
                }
                f.write_str(", and the sets' names differ, so a mnemonic proves nothing")
            }
            InstructionMismatch::Missing { mnemonic } => {
                write!(f, "`{mnemonic}` is not accepted")
            }
            InstructionMismatch::Blocks { mnemonic } => {
                write!(f, "`{mnemonic}` is accepted on other blocks")
            }
            InstructionMismatch::Parameters { mnemonic } => {
                write!(f, "`{mnemonic}` is accepted with other parameters")
            }
            InstructionMismatch::Readouts {
                mnemonic,
                sent,
                accepted,
            } => write!(
                f,
                "`{mnemonic}` gives {} outcomes and {} flags, accepted with {} and {}",
                sent.0, sent.1, accepted.0, accepted.1
            ),
            InstructionMismatch::Meaning { mnemonic } => {
                write!(
                    f,
                    "`{mnemonic}` is accepted as doing something else, or only one side declares what it does"
                )
            }
        }
    }
}

/// How a sent instruction was matched.
enum Match {
    ByMeaning(Opcode),
    ByMnemonic(Opcode),
}

fn flag_count(def: &InstructionDef) -> u32 {
    u32::try_from(def.flags.len()).expect("at most 64 flags, checked when the set is built")
}
