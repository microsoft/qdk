// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The backend interface, which every layer implements for the layer above and
//! drives on the layer below, and its conventions for instruction ids,
//! parameters, results and errors.

use smallvec::SmallVec;
use std::{
    convert::Infallible,
    fmt,
    ops::{BitOr, BitOrAssign},
};

/// Executes batches of instructions of one ISA.
///
/// `Id` identifies an instruction (see [`InstructionId`]), `P` is a block
/// operand or argument (see [`Param`]), and `R` is the result of one
/// instruction (see [`Readout`]).
pub trait Backend<Id, P, R> {
    /// An unrecoverable error.
    type Error;
    /// Produces one result per instruction that has observe outcomes or flags,
    /// in input order. `Err` reports an unrecoverable error. Recoverable
    /// conditions are reported in `R` instead (see [`Status`]).
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
    /// The instruction's position in this boundary's ISA.
    pub instruction_id: Id,
    /// Block operands first, then arguments in the order the instruction
    /// declares its parameters.
    pub parameters: SmallVec<[P; 2]>,
}

/// The results of one [`Backend::execute_batch`] call. Zero or one result is
/// stored without heap allocation.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchResult<R> {
    /// One result per instruction that has observe outcomes or flags, in input
    /// order.
    pub results: SmallVec<[R; 1]>,
}

/// Identifies an instruction by its position in one boundary's ISA instruction
/// list.
///
/// The same `Id` type is used at every boundary, but its values are per
/// boundary: id 3 above a layer and id 3 below it name unrelated instructions
/// of two different ISAs.
///
/// Layers index their plans with `usize` and convert to and from the caller's
/// `Id` type, which may be more compact, such as `u32`. This is a trait rather
/// than `From`/`Into` because std has no lossless conversions between `usize`
/// and `u32`, so each implementation picks its own overflow policy.
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

/// `from_index` panics if the index does not fit in a `u32`.
impl InstructionId for u32 {
    fn from_index(index: usize) -> Self {
        u32::try_from(index).expect("instruction index should fit in u32")
    }
    fn index(self) -> usize {
        self as usize
    }
}

/// A backend parameter: a block operand or a classical argument. Layers read
/// parameters through `view` and build the ones they send below with
/// `from_view`, so `P` can be any type that converts to and from
/// [`ParamView`], such as [`OwnedParam`].
pub trait Param: Clone {
    /// Views this parameter.
    fn view(&self) -> ParamView<'_>;
    /// Builds a parameter from a view.
    fn from_view(view: ParamView<'_>) -> Self;
}

/// A borrowed view of a [`Param`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamView<'a> {
    /// A block operand: a block id at this boundary.
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
    /// The spec's "list of non-negative integers" [argument](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/representations/yaml.md#inline-yaml-calls).
    /// The spec has no list parameter kind; [`spec::matches`](crate::spec::matches)
    /// decides which kinds accept a list.
    IntList(&'a [u64]),
    /// The spec's "list of strings" [argument](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/representations/yaml.md#inline-yaml-calls).
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

/// The result of one instruction: its observe outcomes, then its flags, in the
/// order of a gadget's [`readouts`](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/gadget.md#readouts),
/// and a [`Status`].
pub trait Readout: Sized {
    /// Builds a result.
    fn new(outcomes: &[Bit], flags: &[Bit], status: Status) -> Self;
    /// The observe outcomes, in action order.
    fn outcomes(&self) -> &[Bit];
    /// The flags, in declaration order.
    fn flags(&self) -> &[Bit];
    /// The recoverable conditions reported with this result.
    fn status(&self) -> Status;
}

/// A ready-to-use [`Readout`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BasicReadout {
    /// The observe outcomes, in action order.
    pub outcomes: Vec<Bit>,
    /// The flags, in declaration order.
    pub flags: Vec<Bit>,
    /// The recoverable conditions reported with this result.
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

/// A result bit, or `None` if it could not be determined.
pub type Bit = Option<bool>;

/// Recoverable conditions. Several can hold at once, so this is a set of flags
/// rather than an enum, and statuses merge with field-wise OR.
///
/// A layer runtime ORs every condition it raises, or receives in a result from
/// below, into a shot status, and ORs that into every result it returns for
/// the rest of the shot. Results are built at the end of a batch, so a result
/// may carry a condition first raised by a later instruction of the same
/// batch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// No pattern of a call's `select` matched its flags.
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

/// Unrecoverable errors of a layer whose lower backend fails with `E`.
/// Recoverable conditions are reported in [`Status`] instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QodecError<E> {
    /// The backend below returned `Err`.
    Lower(E),
    /// The backend below broke the result contract: wrong number of `R`s, or an
    /// `R` with the wrong number of outcomes or flags. Also used when a decoder
    /// returns the wrong number of outcomes or flags.
    Protocol,
    /// Unknown id, bad operands or arguments, or a block that is not live. Also
    /// used when a decoder's correction targets such a block.
    InvalidInstruction,
    /// The layer cannot lower an instruction or a decoder's correction: no
    /// gadget or other handling applies.
    NoImplementation,
    /// An earlier `Err` in this shot came after the layer had changed state, so
    /// the layer may disagree with the layers below. An `Err` raised before any
    /// state change does not poison.
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
