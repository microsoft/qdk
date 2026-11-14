// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The first contract between two parts: whether the instructions one
//! sends fit those the other accepts ([`InstructionSet::fits`]), and, if
//! they do, the receiver's opcode for each ([`OpcodeMap`]), with which the
//! sender rewrites its instructions once; otherwise every mismatch
//! ([`InstructionMismatch`], gathered per boundary in [`FitFaults`]).
//!
//! Each sent instruction must be accepted with the same blocks (`in` and
//! `out`, their types and sizes), parameters (their kinds, in order) and
//! readouts (outcomes, then flags). It is matched by meaning where both
//! sets declare one, so a set of another name fits when its instructions do
//! the same (Stim's `H` is a state vector's `h`); by mnemonic only where
//! neither declares one and the sets' names agree, each such match listed
//! in the [`OpcodeMap`] that the sender's `bind` receives, since what the
//! instruction does is then not checked; and refused otherwise. A shared
//! set name alone proves nothing, since two sets may share one: one
//! mnemonic both sets declare to do different things is refused, though
//! another instruction does the same.

use super::{InstructionDef, InstructionSet, Opcode};
use std::fmt;

impl InstructionSet {
    /// Whether the instructions of `self`, the set a part sends, fit
    /// `receiver`, the set the part below accepts: the receiver's opcode
    /// for each, or every mismatch.
    ///
    /// Each is matched, with the same blocks, parameters and readouts: by
    /// meaning where it declares one, to the receiver's instruction of its
    /// mnemonic if that one does the same, else to any that does; by
    /// mnemonic where neither declares one and the sets' names agree,
    /// listed in [`OpcodeMap::by_mnemonic`] of the map the sender's `bind`
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
    /// [`OtherSet`](InstructionMismatch::OtherSet), given once.
    pub fn fits(&self, receiver: &InstructionSet) -> Result<OpcodeMap, Vec<InstructionMismatch>> {
        let same_name = self.name == receiver.name;
        let mut opcodes = Vec::with_capacity(self.instructions.len());
        let mut by_mnemonic = Vec::new();
        let mut mismatches = Vec::new();
        for sent in &self.instructions {
            match self.match_in(sent, receiver, same_name) {
                Ok(Match::ByMeaning(opcode)) => opcodes.push(opcode),
                Ok(Match::ByMnemonic(opcode)) => {
                    opcodes.push(opcode);
                    by_mnemonic.push(sent.mnemonic.clone());
                }
                Err(found) => {
                    for mismatch in found {
                        if mismatch != InstructionMismatch::OtherSet
                            || !mismatches.contains(&InstructionMismatch::OtherSet)
                        {
                            mismatches.push(mismatch);
                        }
                    }
                }
            }
        }
        if mismatches.is_empty() {
            Ok(OpcodeMap {
                opcodes,
                by_mnemonic,
            })
        } else {
            Err(mismatches)
        }
    }

    /// The receiver's opcode for `sent`, an instruction of `self`, as
    /// [`fits`](Self::fits) matches it, or its mismatches.
    fn match_in(
        &self,
        sent: &InstructionDef,
        receiver: &InstructionSet,
        same_name: bool,
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
        if same_name {
            Ok(Match::ByMnemonic(opcode))
        } else {
            Err(vec![InstructionMismatch::OtherSet])
        }
    }

    /// How `theirs`, of `receiver`, differs from `sent`, of `self`, in
    /// blocks, parameters and readouts: each that differs.
    fn shape_mismatches(
        &self,
        sent: &InstructionDef,
        receiver: &InstructionSet,
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

/// How a sent instruction was matched.
enum Match {
    ByMeaning(Opcode),
    ByMnemonic(Opcode),
}

fn flag_count(def: &InstructionDef) -> u32 {
    u32::try_from(def.flags.len()).expect("at most 64 flags, checked when the set is built")
}

/// The receiver's opcode for each opcode of the sending set: what a
/// sender rewrites its instructions with, once, when it is bound below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpcodeMap {
    /// Indexed by the sent opcode.
    opcodes: Vec<Opcode>,
    by_mnemonic: Vec<String>,
}

impl OpcodeMap {
    /// The receiver's opcode for `sent`, an opcode of the sending set: an
    /// index.
    ///
    /// # Panics
    ///
    /// If `sent` is not of the sending set.
    #[must_use]
    pub fn map(&self, sent: Opcode) -> Opcode {
        self.opcodes[sent.index()]
    }

    /// The mnemonics of the sent instructions matched by mnemonic alone,
    /// in the sending set's order: neither set declares what they do, so
    /// that they do the same rests on the sets' shared name. Only the
    /// sender, whose `bind` receives this map, sees the list: the protocol
    /// reports it nowhere else.
    #[must_use]
    pub fn by_mnemonic(&self) -> &[String] {
        &self.by_mnemonic
    }
}

/// Why one sent instruction does not fit the receiver: what
/// [`InstructionSet::fits`] finds.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstructionMismatch {
    /// Neither set declares what its instructions do, and their names
    /// differ, so a mnemonic proves nothing.
    OtherSet,
    /// The receiver has no instruction of this mnemonic, nor one doing the
    /// same with the same blocks, parameters and readouts.
    Missing { mnemonic: String },
    /// Its `in` or `out` differ: their block types, their sizes, or which
    /// are variadic.
    Blocks { mnemonic: String },
    /// Its parameters' kinds, in order, differ.
    Parameters { mnemonic: String },
    /// Its readouts differ: `(outcomes, flags)` of each side.
    Readouts {
        mnemonic: String,
        sent: (u32, u32),
        accepted: (u32, u32),
    },
    /// The receiver's instruction of this mnemonic does something else, or
    /// one side declares what it does and the other does not.
    Meaning { mnemonic: String },
}

impl fmt::Display for InstructionMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstructionMismatch::OtherSet => {
                f.write_str("another set, and no instruction declares what it does")
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
                write!(f, "`{mnemonic}` is accepted as doing something else")
            }
        }
    }
}

/// The mismatches of one boundary of a chain: the set sent, the set
/// accepted, by their names, and every mismatch between them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FitFaults {
    pub sent: String,
    pub accepted: String,
    pub mismatches: Vec<InstructionMismatch>,
}

impl fmt::Display for FitFaults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` does not fit `{}`:", self.sent, self.accepted)?;
        for mismatch in &self.mismatches {
            write!(f, " {mismatch};")?;
        }
        Ok(())
    }
}

impl std::error::Error for FitFaults {}
