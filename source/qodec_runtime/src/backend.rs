// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The backend interface, which every layer implements for the layer above and
//! consumes from the layer below, and its conventions for parameters, results
//! and errors.

use smallvec::SmallVec;
use std::{
    convert::Infallible,
    fmt,
    ops::{BitOr, BitOrAssign},
};

/// Executes ISA instructions. Every qodec layer implements it, and consumes
/// the `Backend` below it.
pub trait Backend<Id, P, R> {
    /// An unrecoverable error.
    type Error;
    /// Produces one result per instruction requiring a result, in input order.
    /// `Err` reports an unrecoverable error. Recoverable outcomes are encoded in `R`.
    fn execute_batch(
        &mut self,
        instructions: &[BackendInstruction<Id, P>],
    ) -> Result<BatchResult<R>, Self::Error>;
}

/// A borrowed backend, e.g. one that is reset and reused across shots.
impl<Id, P, R, B: Backend<Id, P, R> + ?Sized> Backend<Id, P, R> for &mut B {
    type Error = B::Error;

    fn execute_batch(
        &mut self,
        instructions: &[BackendInstruction<Id, P>],
    ) -> Result<BatchResult<R>, Self::Error> {
        (**self).execute_batch(instructions)
    }
}

impl<Id, P, R, B: Backend<Id, P, R> + ?Sized> Backend<Id, P, R> for Box<B> {
    type Error = B::Error;

    fn execute_batch(
        &mut self,
        instructions: &[BackendInstruction<Id, P>],
    ) -> Result<BatchResult<R>, Self::Error> {
        (**self).execute_batch(instructions)
    }
}

/// One instruction sent to a backend.
#[derive(Clone, Debug, PartialEq)]
pub struct BackendInstruction<Id, P> {
    /// The instruction's index in the boundary's ISA.
    pub instruction_id: Id,
    /// Block operands first, then arguments in the order the instruction
    /// declares its parameters.
    pub parameters: SmallVec<[P; 2]>,
}

/// Ordered backend results, with zero or one result stored without heap allocation.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchResult<R> {
    /// One result per instruction requiring a result, in input order.
    pub results: SmallVec<[R; 1]>,
}

/// The index of an instruction in the ISA of a boundary: its position in the
/// ISA's instruction list.
pub trait InstructionId: Copy {
    /// The id of the instruction at `index`.
    fn from_index(index: usize) -> Self;
    /// The index of this instruction.
    fn index(self) -> usize;
}

impl InstructionId for usize {
    fn from_index(index: usize) -> Self {
        index
    }
    fn index(self) -> usize {
        self
    }
}

impl InstructionId for u32 {
    fn from_index(index: usize) -> Self {
        u32::try_from(index).expect("instruction index should fit in u32")
    }
    fn index(self) -> usize {
        self as usize
    }
}

/// A backend parameter: a block operand or a classical argument.
pub trait Param: Clone {
    /// Views this parameter.
    fn view(&self) -> ParamView<'_>;
    /// Builds a parameter from a view.
    fn from_view(view: ParamView<'_>) -> Self;
}

/// A view of a parameter. One `P` type carries both block operands and
/// classical arguments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamView<'a> {
    /// A block (qubit) operand: an id at this boundary.
    Block(u64),
    /// A `bit` or `boolean` argument.
    Bool(bool),
    /// An `integer` argument. A `number` parameter also accepts any integer,
    /// and a `bit` parameter accepts 0 or 1.
    Int(i64),
    /// A `number` argument.
    Float(f64),
    /// A `string` or `pauli` argument.
    Text(&'a str),
    /// A list argument of non-negative integers.
    IntList(&'a [u64]),
    /// A list argument of strings.
    TextList(&'a [String]),
}

/// A ready-to-use owned [`Param`]. Each variant mirrors a [`ParamView`] variant.
#[derive(Clone, Debug, PartialEq)]
pub enum OwnedParam {
    /// A block operand.
    Block(u64),
    /// A `bit` or `boolean` argument.
    Bool(bool),
    /// An integer argument.
    Int(i64),
    /// A `number` argument.
    Float(f64),
    /// A `string` or `pauli` argument.
    Text(String),
    /// A list of non-negative integers.
    IntList(Vec<u64>),
    /// A list of strings.
    TextList(Vec<String>),
}

impl Param for OwnedParam {
    fn view(&self) -> ParamView<'_> {
        match self {
            OwnedParam::Block(b) => ParamView::Block(*b),
            OwnedParam::Bool(b) => ParamView::Bool(*b),
            OwnedParam::Int(i) => ParamView::Int(*i),
            OwnedParam::Float(f) => ParamView::Float(*f),
            OwnedParam::Text(s) => ParamView::Text(s),
            OwnedParam::IntList(l) => ParamView::IntList(l),
            OwnedParam::TextList(l) => ParamView::TextList(l),
        }
    }

    fn from_view(view: ParamView<'_>) -> Self {
        match view {
            ParamView::Block(b) => OwnedParam::Block(b),
            ParamView::Bool(b) => OwnedParam::Bool(b),
            ParamView::Int(i) => OwnedParam::Int(i),
            ParamView::Float(f) => OwnedParam::Float(f),
            ParamView::Text(s) => OwnedParam::Text(s.to_string()),
            ParamView::IntList(l) => OwnedParam::IntList(l.to_vec()),
            ParamView::TextList(l) => OwnedParam::TextList(l.to_vec()),
        }
    }
}

/// The aggregate result of one instruction: observe outcomes first, then
/// flags, in the order of a gadget's [`readouts`](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/gadget.md#readouts).
pub trait Readout: Sized {
    /// Builds a result.
    fn new(outcomes: &[Bit], flags: &[Bit], status: Status) -> Self;
    /// The observe outcomes, in action order.
    fn outcomes(&self) -> &[Bit];
    /// The flags, in declaration order.
    fn flags(&self) -> &[Bit];
    /// The recoverable shot-level conditions.
    fn status(&self) -> Status;
}

/// A ready-to-use [`Readout`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BasicReadout {
    /// The observe outcomes, in action order.
    pub outcomes: Vec<Bit>,
    /// The flags, in declaration order.
    pub flags: Vec<Bit>,
    /// The recoverable shot-level conditions.
    pub status: Status,
}

impl Readout for BasicReadout {
    fn new(outcomes: &[Bit], flags: &[Bit], status: Status) -> Self {
        Self {
            outcomes: outcomes.to_vec(),
            flags: flags.to_vec(),
            status,
        }
    }
    fn outcomes(&self) -> &[Bit] {
        &self.outcomes
    }
    fn flags(&self) -> &[Bit] {
        &self.flags
    }
    fn status(&self) -> Status {
        self.status
    }
}

/// `None` = unresolved (not determinable).
pub type Bit = Option<bool>;

// TODO: Explain why this is a struct instead of an enum.
/// Recoverable, shot-level conditions. Statuses merge with field-wise OR.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// A select pattern did not match.
    pub rejected: bool,
    /// Parity evidence contradicts itself.
    pub inconsistent: bool,
    /// A value the layer or its decoder needed could not be determined. For
    /// example, a `None` bit was passed to a `bit` parameter (it is replaced by
    /// 0), or a `select` hint could not be decided.
    pub unresolved: bool,
}

impl Status {
    /// Only `rejected`.
    pub const REJECTED: Status = Status {
        rejected: true,
        inconsistent: false,
        unresolved: false,
    };
    /// Only `inconsistent`.
    pub const INCONSISTENT: Status = Status {
        rejected: false,
        inconsistent: true,
        unresolved: false,
    };
    /// Only `unresolved`.
    pub const UNRESOLVED: Status = Status {
        rejected: false,
        inconsistent: false,
        unresolved: true,
    };
}

impl BitOrAssign for Status {
    fn bitor_assign(&mut self, rhs: Status) {
        *self = *self | rhs;
    }
}

impl BitOr for Status {
    type Output = Status;
    fn bitor(self, rhs: Status) -> Status {
        Status {
            rejected: self.rejected || rhs.rejected,
            inconsistent: self.inconsistent || rhs.inconsistent,
            unresolved: self.unresolved || rhs.unresolved,
        }
    }
}

/// Unrecoverable errors. Recoverable conditions are reported in [`Status`]
/// instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QodecError<E> {
    /// The backend below returned `Err`.
    Lower(E),
    /// The backend below broke the result contract: wrong number of `R`s, or an
    /// `R` of the wrong shape. Also used when a decoder breaks its output contract.
    Protocol,
    /// Unknown Id, bad operands or arguments, block not live.
    InvalidInstruction,
    /// An instruction has no gadget and no other handling.
    NoImplementation,
    /// A previous `Err` left the layer inconsistent.
    Poisoned,
}

impl<E> QodecError<E> {
    /// Splits a `Lower` error from the errors raised by this layer itself.
    pub fn split_lower(self) -> Result<QodecError<Infallible>, E> {
        match self {
            QodecError::Lower(e) => Err(e),
            QodecError::Protocol => Ok(QodecError::Protocol),
            QodecError::InvalidInstruction => Ok(QodecError::InvalidInstruction),
            QodecError::NoImplementation => Ok(QodecError::NoImplementation),
            QodecError::Poisoned => Ok(QodecError::Poisoned),
        }
    }
}

impl<E: fmt::Display> fmt::Display for QodecError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QodecError::Lower(e) => write!(f, "lower backend error: {e}"),
            QodecError::Protocol => write!(f, "result contract violated"),
            QodecError::InvalidInstruction => write!(f, "invalid instruction"),
            QodecError::NoImplementation => write!(f, "instruction has no implementation"),
            QodecError::Poisoned => write!(f, "layer is poisoned; start a new shot"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for QodecError<E> {}

/// A configuration error: a manifest, plan, decoder or resource requirement is
/// not supported or not well formed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanError(pub String);

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PlanError {}

impl From<String> for PlanError {
    fn from(message: String) -> Self {
        PlanError(message)
    }
}

impl From<&str> for PlanError {
    fn from(message: &str) -> Self {
        PlanError(message.to_string())
    }
}
