// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The call of an instruction of a set known only at run time
//! ([`InstructionCall`]): the instruction, by its [`Opcode`] in the set, and
//! its [`Operands`]. It is the instruction type of an executor whose set is
//! chosen at run time (`InstructionBatch<InstructionCall>`); the set's
//! description checks a call ([`InstructionSet::check`]) and writes it with
//! its mnemonic ([`InstructionSet::display`], a [`CallDisplay`]).
//!
//! A call's operands are held in the call itself, up to
//! [`Operands::INLINE`] of them, so building a call of that many allocates
//! no memory.

use std::fmt;

use crate::{InstructionSet, OperandKind};

/// One operand of an [`InstructionCall`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operand {
    /// The address of a block.
    Block(u32),
    /// A real parameter.
    Real(f64),
    /// An integer parameter.
    Integer(i64),
    /// The tag of a readout.
    Tag(u32),
}

impl Operand {
    /// Its kind.
    #[must_use]
    pub fn kind(&self) -> OperandKind {
        match self {
            Operand::Block(_) => OperandKind::Block,
            Operand::Real(_) => OperandKind::Real,
            Operand::Integer(_) => OperandKind::Integer,
            Operand::Tag(_) => OperandKind::Tag,
        }
    }
}

impl fmt::Display for Operand {
    /// Its kind and value: `block 0`, `real 0.5`, `integer 3`, `tag 5`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Block(b) => write!(f, "block {b}"),
            Operand::Real(r) => write!(f, "real {r}"),
            Operand::Integer(i) => write!(f, "integer {i}"),
            Operand::Tag(t) => write!(f, "tag {t}"),
        }
    }
}

/// An instruction of a described set, by its position in the set: the
/// order the set declares it in, from 0. It means something only in that
/// set. A program or an adapter gets one from the set, by mnemonic
/// ([`InstructionSet::opcode`]); a decoder that reads calls as bytes, or a
/// table that fixes a device's numbering, builds one from its index
/// ([`Opcode::new`]), and the set refuses a call at an opcode it lacks
/// ([`InstructionSet::check`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Opcode(u32);

impl Opcode {
    /// The opcode of the instruction at position `index` of its set.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Its position in its set, to index a table of the set's instructions.
    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A call of an instruction of a described set: the instruction at
/// `opcode`, applied to `operands`. It is the instruction type of an
/// executor that accepts a set known only at run time
/// (`InstructionBatch<InstructionCall>`).
#[derive(Clone, Debug, PartialEq)]
pub struct InstructionCall {
    /// The instruction applied, by its opcode in the set.
    pub opcode: Opcode,
    /// Its operands, in the order of the instruction's [`OperandKind`]s.
    pub operands: Operands,
}

impl InstructionCall {
    /// The call of the instruction at `opcode` on `operands`, unchecked:
    /// for an adapter that resolved `opcode` when it was built. A program
    /// built by mnemonic uses [`InstructionSet::call`].
    #[must_use]
    pub fn new(opcode: Opcode, operands: impl IntoIterator<Item = Operand>) -> Self {
        Self {
            opcode,
            operands: operands.into_iter().collect(),
        }
    }
}

/// The operands of an [`InstructionCall`], read as a slice
/// (`call.operands[0]`, `call.operands.iter()`). Up to [`Operands::INLINE`]
/// are held in the call itself, so building, copying or renumbering a call
/// allocates no memory; more are held on the heap.
pub struct Operands(Holder);

/// Where [`Operands`] are held: in the call while they fit, and on the heap
/// only beyond, so that two equal lists are held the same way.
#[derive(Clone)]
enum Holder {
    /// The first `len` of `slots`; the others are [`UNUSED`].
    Inline {
        len: u8,
        slots: [Operand; Operands::INLINE],
    },
    /// More than [`Operands::INLINE`] operands.
    Heap(Vec<Operand>),
}

/// The value of an inline slot past the operands, never read.
const UNUSED: Operand = Operand::Integer(0);

impl Operands {
    /// How many operands are held in the call: the most that any of QIR's
    /// instructions takes (`ccx`'s three qubits, or `rzz`'s angle and two
    /// qubits).
    pub const INLINE: usize = 3;
}

impl Clone for Operands {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }

    /// Reuses `self`'s heap memory when both are on the heap: for the
    /// renumbering `compose` inserts, which copies calls into a buffer it
    /// keeps across batches.
    #[inline]
    fn clone_from(&mut self, source: &Self) {
        match (&mut self.0, &source.0) {
            (Holder::Inline { len, slots }, Holder::Inline { len: l, slots: s }) => {
                *len = *l;
                *slots = *s;
            }
            (Holder::Heap(kept), Holder::Heap(operands)) => kept.clone_from(operands),
            (kept, operands) => *kept = operands.clone(),
        }
    }
}

impl FromIterator<Operand> for Operands {
    fn from_iter<I: IntoIterator<Item = Operand>>(operands: I) -> Self {
        let mut operands = operands.into_iter().fuse();
        let mut slots = [UNUSED; Operands::INLINE];
        let mut len = 0;
        for slot in &mut slots {
            let Some(operand) = operands.next() else {
                break;
            };
            *slot = operand;
            len += 1;
        }
        match operands.next() {
            None => Self(Holder::Inline { len, slots }),
            Some(next) => {
                let mut heap = slots.to_vec();
                heap.push(next);
                heap.extend(operands);
                Self(Holder::Heap(heap))
            }
        }
    }
}

impl std::ops::Deref for Operands {
    type Target = [Operand];

    #[inline]
    fn deref(&self) -> &[Operand] {
        match &self.0 {
            Holder::Inline { len, slots } => &slots[..usize::from(*len)],
            Holder::Heap(operands) => operands,
        }
    }
}

impl<'a> IntoIterator for &'a Operands {
    type Item = &'a Operand;
    type IntoIter = std::slice::Iter<'a, Operand>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl PartialEq for Operands {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl fmt::Debug for Operands {
    /// As a slice: `[Block(0), Tag(1)]`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

/// A call written with the mnemonic its set gives its opcode
/// ([`InstructionSet::display`]): `cx(block 0, block 1)`; at an opcode the
/// set lacks, the opcode instead: `opcode 7(block 0)`.
pub struct CallDisplay<'a> {
    pub(super) set: &'a InstructionSet,
    pub(super) call: &'a InstructionCall,
}

impl fmt::Display for CallDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.set.mnemonic(self.call.opcode) {
            Some(mnemonic) => f.write_str(mnemonic)?,
            None => write!(f, "opcode {}", self.call.opcode.index())?,
        }
        f.write_str("(")?;
        for (i, operand) in self.call.operands.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{operand}")?;
        }
        f.write_str(")")
    }
}
