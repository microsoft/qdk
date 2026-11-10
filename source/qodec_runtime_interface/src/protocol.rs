// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What flows between the components of a qodec runtime, and the roles the
//! components play: [`Encoder`], [`Decoder`] and [`Controller`].
//!
//! Every role is a [`Layer`] with fixed types: it takes a batch and returns
//! the batch's answers, or a [`Fault`]. Only the messages the qodec spec
//! defines are fixed for everyone: ISA calls ([`Call`]), ISA results
//! ([`Reply`]) and a gadget's readout record ([`Record`]). The rest are type
//! parameters agreed by the two neighbors that exchange them:
//!
//! - `N`, what a pending decoder asks its controller for ([`Reply::Pending`]);
//! - `C`, the corrections a decoder sends its encoder ([`Lower::Fix`]).
//!
//! A layer that never sends one of them uses [`Never`], so the type checker
//! proves it: `Reply<Never>` is a plain ISA result, and `Lower<Never>` a plain
//! ISA call.
//!
//! ```text
//!   │ Vec<Call>                          ▲ Result<Vec<Reply>, Fault>      (upper ISA)
//!  ┌▼────────────────────────────────────┴┐
//!  │ Controller<N>  (if a decoder pends)  │
//!  └┬────────────────────────────────────▲┘
//!   │ Vec<Call>                          │ Result<Vec<Reply<N>>, Fault>
//!  ┌▼────────────────────────────────────┴┐
//!  │ Decoder<N, C>                        │
//!  └┬────────────────────────────────────▲┘
//!   │ Vec<Lower<C>>                      │ Result<Vec<Record>, Fault>     (gadget level)
//!  ┌▼────────────────────────────────────┴┐
//!  │ Encoder<C>                           │
//!  └┬────────────────────────────────────▲┘
//!   │ Vec<Call>                          │ Result<Vec<Reply>, Fault>      (lower ISA)
//! ```

use super::{
    layer::Layer,
    pauli::{Clifford, Pauli},
};
use qodec::Argument;
use smallvec::SmallVec;
use std::{
    collections::BTreeMap,
    fmt,
    ops::{BitOr, BitOrAssign},
};

/// An ISA call.
///
/// Ids are per boundary: id 3 above a layer and id 3 below it name unrelated
/// instructions of two different ISAs.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    /// The instruction's position in this boundary's ISA.
    pub instruction_id: usize,
    /// Block operands first, then arguments in the order the instruction
    /// declares its parameters.
    pub parameters: SmallVec<[OwnedParam; 2]>,
}

/// The type of a message that is never sent. An enum variant that holds a
/// `Never` can't be built, so a layer whose output type has such a variant
/// proves it never returns it. For example, `Reply<Never>` is a result that is
/// never pending.
pub type Never = std::convert::Infallible;

/// An unrecoverable error of a layer, which loses the shot. Recoverable
/// conditions travel with the results instead (see [`Status`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fault {
    /// What went wrong.
    pub kind: FaultKind,
    /// Details for a human.
    pub detail: String,
    /// The index of the qodec layer that raised it, 0 being the top, if known.
    pub layer: Option<usize>,
}

/// The kind of a [`Fault`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultKind {
    /// Unknown instruction, bad operands or arguments, or a block that is not
    /// live.
    InvalidInstruction,
    /// No gadget or other handling applies.
    NoImplementation,
    /// A neighbor broke the protocol, for example a result of the wrong shape.
    Protocol,
    /// The backend at the bottom failed. Its error is reported separately.
    Bottom,
}

impl Fault {
    /// A fault of `kind`.
    pub fn new(kind: FaultKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            layer: None,
        }
    }

    /// An [`FaultKind::InvalidInstruction`] fault.
    pub fn invalid(detail: impl Into<String>) -> Self {
        Self::new(FaultKind::InvalidInstruction, detail)
    }

    /// A [`FaultKind::NoImplementation`] fault.
    pub fn no_implementation(detail: impl Into<String>) -> Self {
        Self::new(FaultKind::NoImplementation, detail)
    }

    /// A [`FaultKind::Protocol`] fault.
    pub fn protocol(detail: impl Into<String>) -> Self {
        Self::new(FaultKind::Protocol, detail)
    }

    /// Records the layer that raised the fault, unless a layer below already
    /// did.
    #[must_use]
    pub fn at_layer(mut self, layer: usize) -> Self {
        self.layer.get_or_insert(layer);
        self
    }
}

impl fmt::Display for FaultKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FaultKind::InvalidInstruction => "invalid instruction",
            FaultKind::NoImplementation => "instruction has no implementation",
            FaultKind::Protocol => "protocol violated",
            FaultKind::Bottom => "the bottom backend failed",
        })
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(layer) = self.layer {
            write!(f, "layer {layer}: ")?;
        }
        write!(f, "{}", self.kind)?;
        if !self.detail.is_empty() {
            write!(f, ": {}", self.detail)?;
        }
        Ok(())
    }
}

impl std::error::Error for Fault {}

/// A block operand or a classical argument of a call. Every parameter kind
/// takes a single value, so there are no list arguments.
#[derive(Clone, Debug, PartialEq)]
pub enum OwnedParam {
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
    Text(String),
}

/// The result of one call: its observe outcomes, then its flags, in the order
/// of a gadget's [`readouts`](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/gadget.md#readouts),
/// and a [`Status`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BasicReadout {
    /// The observe outcomes, in action order.
    pub outcomes: Vec<Bit>,
    /// The flags, in declaration order.
    pub flags: Vec<Bit>,
    /// The recoverable conditions reported with this result.
    pub status: Status,
}

impl BasicReadout {
    /// A result with these outcomes, flags and status.
    #[must_use]
    pub fn new(outcomes: &[Bit], flags: &[Bit], status: Status) -> Self {
        Self {
            outcomes: outcomes.to_vec(),
            flags: flags.to_vec(),
            status,
        }
    }
}

/// A result bit, or `None` if it could not be determined.
pub type Bit = Option<bool>;

/// Recoverable conditions. Several can hold at once, so this is a set of flags
/// rather than an enum, and statuses merge with field-wise OR.
///
/// Conditions travel up with the results: an encoder adds those it raises or
/// receives from below to the call's [`Record`], and a decoder passes them on
/// in its results.
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

/// The result of an ISA call that has observe outcomes or flags, or a request
/// for more information before that result can be given.
///
/// A batch's answer holds the results of its calls that have outcomes or
/// flags, in call order. A decoder that needs more information first gives
/// the results it can, then one `Pending(N)` for the rest. Its caller then
/// executes another batch, such as a QEC round, whose answer starts with the
/// results still owed.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply<N = Never> {
    /// The result.
    Ready(BasicReadout),
    /// The decoder needs `N` before it can give the next result.
    Pending(N),
}

impl Reply<Never> {
    /// The result. A `Reply<Never>` is always ready.
    #[must_use]
    pub fn into_ready(self) -> BasicReadout {
        match self {
            Reply::Ready(readout) => readout,
            Reply::Pending(never) => match never {},
        }
    }
}

/// What a decoder sends its encoder: the calls it passes on, and corrections.
#[derive(Clone, Debug, PartialEq)]
pub enum Lower<C = Never> {
    /// An ISA call to lower.
    Call(Call),
    /// A correction to apply to live blocks.
    Fix(C),
}

/// What an encoder reports for one call, once every bit of its readout record
/// is known. An encoder answers a batch with one record per [`Lower::Call`],
/// in call order; corrections produce none.
#[derive(Clone, Debug)]
pub struct Record {
    /// The index of the call's instruction in the upper ISA.
    pub instruction: usize,
    /// The gadget invocation, if a gadget implemented the call.
    pub invocation: Option<Invocation>,
    /// The gadget's [circuit readout record](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/gadget.md#circuit-object):
    /// each lower call's observe outcomes, then its flags. Empty without a
    /// gadget.
    pub bits: Vec<Bit>,
    /// The blocks the call ended besides a gadget's inputs, which always end
    /// (its outputs are new blocks): released blocks, and blocks overwritten by
    /// output-only operands. See [`Self::ended`].
    pub released: Vec<BlockRef>,
    /// Recoverable conditions raised below or while lowering the call.
    pub status: Status,
}

impl Record {
    /// Every block the call ended: the gadget's inputs, then
    /// [`Self::released`]. A decoder can drop what it keeps for them.
    pub fn ended(&self) -> impl Iterator<Item = &BlockRef> {
        self.invocation
            .iter()
            .flat_map(|inv| &inv.input_refs)
            .chain(&self.released)
    }
}

/// One gadget invocation.
#[derive(Clone, Debug)]
pub struct Invocation {
    /// Sequence number within the shot, from 0.
    pub id: u64,
    /// The gadget that ran.
    pub gadget: GadgetId,
    /// The upper block operands, in order.
    pub operands: Vec<u64>,
    /// The call's arguments, by parameter name.
    pub args: BTreeMap<String, Argument>,
    /// The input blocks, one per `in` operand.
    pub input_refs: Vec<BlockRef>,
    /// The output blocks, one per `out` operand, each with a new generation.
    pub output_refs: Vec<BlockRef>,
}

/// Identifies a gadget of a layer: its position among the gadgets the layer
/// declares, in declaration order. An instruction may have several gadgets,
/// so this is not the instruction's index; decoders look up what they planned
/// for a gadget by this id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GadgetId(pub usize);

/// One binding of an upper block label. Every gadget output binds its label
/// with a new generation, so state keyed by an older reference never applies
/// to the new block.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockRef {
    /// The upper block id chosen by the caller.
    pub label: u64,
    /// Incremented every time the label is (re)bound to an output.
    pub generation: u64,
    /// The upper block type.
    pub block_type: String,
}

/// A step applied to the code qubits of live blocks, which an encoder resolves
/// against the actions of its lower ISA.
#[derive(Clone, Debug, PartialEq)]
pub struct Correction {
    /// The targeted blocks. They must be distinct and live, with their current
    /// generation.
    pub blocks: Vec<BlockRef>,
    /// What to apply. Its qubits index the concatenated code qubits of
    /// `blocks`.
    pub step: Step,
}

/// What to do to some qubits, in the vocabulary of the spec's
/// [action steps](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#actions).
/// It says what the operation is, never what it is called: an encoder finds a
/// lower instruction whose action matches.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// A Pauli, as a `pauli` step applies one. Its sign is a global phase.
    Pauli(Pauli),
    /// A Clifford, as a `clifford` step applies one: qubit `i` of `gate` acts
    /// on qubit `targets[i]`. [`Step::named`] builds the common ones.
    Clifford {
        /// The qubits `gate` acts on, in its qubit order.
        targets: Vec<usize>,
        /// The Clifford, on `targets.len()` qubits.
        gate: Clifford,
    },
    /// A rotation `exp(-i angle/2 P)` about a Pauli `P`, as a `rotate` step
    /// applies one.
    Rotate {
        /// The rotation axis.
        pauli: Pauli,
        /// The angle, in radians.
        angle: f64,
    },
}

impl Step {
    /// A Clifford by its usual name, such as `x`, `h`, `cx` or `swap`, on
    /// `targets`. `None` if no Clifford of that name acts on that many qubits.
    #[must_use]
    pub fn named(name: &str, targets: Vec<usize>) -> Option<Self> {
        let gate = Clifford::named(name, targets.len())?;
        Some(Step::Clifford { targets, gate })
    }

    /// The qubits it acts on, and the same step on qubits `0..n` instead,
    /// qubit `i` standing for `support[i]`.
    pub fn localize(&self) -> Result<(Vec<usize>, Step), String> {
        let localize_pauli = |p: &Pauli| {
            let local = Pauli::new(
                p.negative,
                p.terms()
                    .iter()
                    .enumerate()
                    .map(|(i, &(_, op))| (i, op))
                    .collect(),
            )?;
            Ok::<_, String>((p.support().collect(), local))
        };
        match self {
            Step::Pauli(p) => {
                let (support, local) = localize_pauli(p)?;
                Ok((support, Step::Pauli(local)))
            }
            Step::Rotate { pauli, angle } => {
                let (support, local) = localize_pauli(pauli)?;
                Ok((
                    support,
                    Step::Rotate {
                        pauli: local,
                        angle: *angle,
                    },
                ))
            }
            Step::Clifford { targets, gate } => {
                let mut distinct = targets.clone();
                distinct.sort_unstable();
                distinct.dedup();
                if distinct.len() != targets.len() || gate.width() != targets.len() {
                    return Err("a Clifford needs one distinct target per qubit".to_string());
                }
                Ok((
                    targets.clone(),
                    Step::Clifford {
                        targets: (0..targets.len()).collect(),
                        gate: gate.clone(),
                    },
                ))
            }
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Pauli(p) => write!(f, "Pauli `{p}`"),
            Step::Clifford { targets, .. } => write!(f, "a Clifford on qubits {targets:?}"),
            Step::Rotate { pauli, angle } => write!(f, "a rotation about `{pauli}` by {angle}"),
        }
    }
}

/// A correction type an encoder can apply: a [`Correction`], or [`Never`] for
/// a decoder that never corrects.
pub trait Fix {
    /// The correction to apply.
    fn into_correction(self) -> Correction;
}

impl Fix for Never {
    fn into_correction(self) -> Correction {
        match self {}
    }
}

impl Fix for Correction {
    fn into_correction(self) -> Correction {
        self
    }
}

// ── Roles ───────────────────────────────────────────────────────────────────

/// Lowers a batch of upper calls, and corrections of type `C`, to lower calls,
/// and answers with each call's readout record.
///
/// It calls the backend below whenever a gadget needs a readout to go on, and
/// once at the end of the batch.
pub trait Encoder<C = Never>:
    Layer<
        Input = Vec<Lower<C>>,
        Output = Result<Vec<Record>, Fault>,
        LowerInput = Vec<Call>,
        LowerOutput = Result<Vec<Reply>, Fault>,
    >
{
    /// Tells it that no result will ever come back, as when compiling ahead of
    /// time. It then sends each batch's lower calls below once and keeps
    /// nothing for their results: its records have every bit unknown, and a
    /// gadget that needs a readout to go on fails with
    /// [`FaultKind::NoImplementation`].
    fn expect_no_results(&mut self);
}

impl<C, E: Encoder<C> + ?Sized> Encoder<C> for Box<E> {
    fn expect_no_results(&mut self) {
        (**self).expect_no_results();
    }
}

/// Turns readout records into upper results. It may ask for more information
/// with `Reply::Pending(N)`, and correct live blocks with `Lower::Fix(C)`.
///
/// It decides how to split its batch below: for example, a decoder that
/// corrects sends the calls up to a gadget it must decode, decodes, sends its
/// corrections, and goes on, so that no later call uses a block before it is
/// corrected.
pub trait Decoder<N = Never, C = Never>:
    Layer<
        Input = Vec<Call>,
        Output = Result<Vec<Reply<N>>, Fault>,
        LowerInput = Vec<Lower<C>>,
        LowerOutput = Result<Vec<Record>, Fault>,
    >
{
}

impl<N, C, L> Decoder<N, C> for L where
    L: Layer<
            Input = Vec<Call>,
            Output = Result<Vec<Reply<N>>, Fault>,
            LowerInput = Vec<Lower<C>>,
            LowerOutput = Result<Vec<Record>, Fault>,
        > + ?Sized
{
}

/// Answers a decoder's `Pending(N)` requests, for example by adding QEC
/// rounds, and gives only final results above.
pub trait Controller<N>:
    Layer<
        Input = Vec<Call>,
        Output = Result<Vec<Reply>, Fault>,
        LowerInput = Vec<Call>,
        LowerOutput = Result<Vec<Reply<N>>, Fault>,
    >
{
}

impl<N, L> Controller<N> for L where
    L: Layer<
            Input = Vec<Call>,
            Output = Result<Vec<Reply>, Fault>,
            LowerInput = Vec<Call>,
            LowerOutput = Result<Vec<Reply<N>>, Fault>,
        > + ?Sized
{
}

/// A whole qodec layer: batches of ISA calls and their results on both sides.
/// A qodec runtime chains one per layer.
pub type BoxedLayer = Box<
    dyn Layer<
            Input = Vec<Call>,
            Output = Result<Vec<Reply>, Fault>,
            LowerInput = Vec<Call>,
            LowerOutput = Result<Vec<Reply>, Fault>,
        >,
>;
