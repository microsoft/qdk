//! One Qodec instruction set as an `InstructionSet` of the protocol, and the
//! conversions of single instructions between the Qodec runtime's
//! `BackendInstruction` and the protocol's `InstructionCall`.
//!
//! `[GLUE]` Every adapter that wraps a foreign runtime needs a mapping like
//! this one, but its content is Qodec's: no facility can write it.
//!
//! The set takes the Qodec ISA's name and declares no action: two sets of
//! one name are matched by mnemonic, so the set a Qodec layer sends matches
//! the one an executor of the same ISA accepts, whatever each instruction
//! does. That covers the instructions whose action the protocol's `Action`
//! cannot say: `idle` (no action) and a conditional step such as `X_if`'s
//! `pauli: X_0, if: [correction]`. Between sets of other names, matching
//! needs both to declare actions, so such instructions do not match there.

use std::sync::Arc;

use executor_protocol::{
    InstructionCall, InstructionSet, Opcode, Operand, OperandKind, ReadoutFailure, ShotConditions,
    ShotId, ShotOutcome, Value,
};
use qodec::ParameterKind;
use qodec_runtime::{BackendInstruction, Bit, OwnedParam, Status};
use smallvec::SmallVec;

/// The Qodec runtime's instruction, by index in its ISA.
pub type Ins = BackendInstruction<usize, OwnedParam>;

/// What a call of one instruction holds, in this crate's convention: blocks,
/// then parameters, then one tag per readout (observes, then flags).
#[derive(Clone, Debug)]
pub struct Layout {
    /// The kinds of its arguments other than blocks, in order.
    pub parameters: Vec<ParameterKind>,
    /// How many of its readouts observe; the others are flags.
    pub observes: usize,
    /// How many readouts it has, so how many tags a call of it takes:
    /// every observe, then every flag.
    pub readouts: usize,
}

/// A Qodec ISA as a set of the protocol, with the layout of each instruction
/// by index, which is its opcode in the set.
#[derive(Debug)]
pub struct Boundary {
    /// The ISA as the protocol's set, of the same name, each instruction
    /// at its index as opcode ([`opcode`]).
    pub set: Arc<InstructionSet>,
    /// The layout of each instruction, by its index in the ISA.
    pub layouts: Vec<Layout>,
}

impl Boundary {
    /// `isa` as a set of the protocol, of the same name.
    ///
    /// # Errors
    ///
    /// Each instruction the protocol's operands cannot express: a variadic
    /// block operand, among its inputs or its outputs (an operand list is
    /// fixed), or a `string`/`pauli` argument (there is no text operand).
    pub fn new(isa: &qodec::InstructionSet) -> Result<Self, Vec<String>> {
        let mut set = InstructionSet::new(&isa.name);
        let mut layouts = Vec::new();
        let mut faults = Vec::new();
        for decl in &isa.instructions {
            if decl
                .inputs
                .iter()
                .chain(&decl.outputs)
                .any(|o| o.is_variadic)
            {
                faults.push(format!("{}: variadic block operand", decl.mnemonic));
            }
            let blocks = decl.inputs.len().max(decl.outputs.len());
            let mut kinds = vec![OperandKind::Block; blocks];
            for p in &decl.parameters {
                kinds.push(match p.kind {
                    ParameterKind::Bit | ParameterKind::Boolean | ParameterKind::Integer => {
                        OperandKind::Integer
                    }
                    ParameterKind::Number => OperandKind::Real,
                    ParameterKind::String | ParameterKind::Pauli => {
                        faults.push(format!("{}: text argument `{}`", decl.mnemonic, p.name));
                        OperandKind::Integer
                    }
                });
            }
            let observes = decl.observe_count();
            let readouts = observes + decl.flags.len();
            kinds.extend(std::iter::repeat_n(OperandKind::Tag, readouts));
            set = set.with(&decl.mnemonic, kinds);
            layouts.push(Layout {
                parameters: decl.parameters.iter().map(|p| p.kind).collect(),
                observes,
                readouts,
            });
        }
        if faults.is_empty() {
            Ok(Self {
                set: Arc::new(set),
                layouts,
            })
        } else {
            Err(faults)
        }
    }

    /// `ins` as a call at `opcode`, its readouts tagged from `*next_tag`.
    ///
    /// FRICTION: the protocol's blocks are `u32`, Qodec's `u64`; a bit
    /// argument is an `Integer` (0 or 1), so a bit forced from a `None`
    /// readout is already 0 here, its unresolvedness kept only in the
    /// layer's status.
    ///
    /// A `number` argument is a `Real`, also when Qodec holds it as an
    /// integer (`theta=1` parses as one), the nearest `Real` to it: the
    /// runtime accepts an integer for a `number`, and the set declares a
    /// `Real` there
    /// (`an_integer_given_for_a_number_is_sent_as_the_nearest_real`).
    /// Each argument is sent as its own parameter declares, whatever
    /// parameters come before it
    /// (`each_argument_is_sent_as_its_own_parameter_declares`).
    pub fn call(&self, ins: &Ins, opcode: Opcode, next_tag: &mut u32) -> InstructionCall {
        let layout = &self.layouts[ins.instruction_id];
        let mut kinds = layout.parameters.iter();
        let operands = ins.parameters.iter().map(move |p| {
            let kind = match p {
                OwnedParam::Block(_) => None,
                _ => kinds.next(),
            };
            match *p {
                OwnedParam::Block(b) => {
                    Operand::Block(u32::try_from(b).expect("fewer than 2^32 blocks"))
                }
                OwnedParam::Bool(b) => Operand::Integer(i64::from(b)),
                OwnedParam::Int(i) if kind == Some(&ParameterKind::Number) => {
                    Operand::Real(i as f64)
                }
                OwnedParam::Int(i) => Operand::Integer(i),
                OwnedParam::Float(x) => Operand::Real(x),
                ref other => unreachable!("refused by `Boundary::new`: {other:?}"),
            }
        });
        let first = *next_tag;
        *next_tag += u32::try_from(layout.readouts).expect("fewer than 2^32 readouts");
        InstructionCall::new(opcode, operands.chain((first..*next_tag).map(Operand::Tag)))
    }

    /// `call` as the Qodec runtime's instruction, and the tags of its
    /// readouts. Each operand is taken as the parameter at its place
    /// declares: a `Bool` at a `bit` or a `boolean`, an `Int` at an
    /// `integer`, a `Float` at a `number`. So an instruction whose
    /// arguments have these forms comes back from its call as it was
    /// (`a_call_comes_back_as_the_instruction_it_was_made_from`). It does
    /// not check `call`: the caller checks it against the set first
    /// ([`InstructionSet::check_batch`]), as the layer's check does at each
    /// call.
    ///
    /// # Panics
    ///
    /// When `call`'s opcode is beyond the set, or it holds an operand of a
    /// kind no set of this crate declares (the protocol's operand kinds may
    /// grow). A call the set refuses for its operands gives a wrong
    /// instruction instead.
    pub fn instruction(&self, call: &InstructionCall) -> (Ins, SmallVec<[u32; 4]>) {
        let index = call.opcode.index();
        let layout = &self.layouts[index];
        let mut parameters = SmallVec::new();
        let mut tags = SmallVec::new();
        let mut params = layout.parameters.iter();
        for operand in call.operands.iter() {
            match *operand {
                Operand::Block(b) => parameters.push(OwnedParam::Block(u64::from(b))),
                Operand::Integer(i) => parameters.push(match params.next() {
                    Some(ParameterKind::Bit | ParameterKind::Boolean) => OwnedParam::Bool(i != 0),
                    _ => OwnedParam::Int(i),
                }),
                Operand::Real(x) => {
                    params.next();
                    parameters.push(OwnedParam::Float(x));
                }
                Operand::Tag(t) => tags.push(t),
                ref other => unreachable!("a kind no Qodec set declares: {other:?}"),
            }
        }
        (
            Ins {
                instruction_id: index,
                parameters,
            },
            tags,
        )
    }
}

/// The opcode of the instruction at `index` in a Qodec ISA, in the set
/// [`Boundary::new`] makes of it.
///
/// # Panics
///
/// When `index` is 2^32 or beyond.
#[must_use]
pub fn opcode(index: usize) -> Opcode {
    Opcode::new(u32::try_from(index).expect("fewer than 2^32 instructions"))
}

/// A readout bit as a value: `None` (unresolved) has no value.
///
/// FRICTION (QD5): `Value` has no "unresolved", only `Missing(_)`.
pub fn value(bit: Bit) -> Value {
    match bit {
        Some(false) => Value::Zero,
        Some(true) => Value::One,
        None => Value::Missing(ReadoutFailure::Other),
    }
}

/// A value as a readout bit.
pub fn bit(value: Value) -> Bit {
    match value {
        Value::Zero => Some(false),
        Value::One => Some(true),
        Value::Missing(_) => None,
    }
}

/// The Qodec runtime's status as the protocol's conditions: field for
/// field, since both name the same three conditions.
#[must_use]
pub fn conditions(status: Status) -> ShotConditions {
    let mut conditions = ShotConditions::NONE;
    if status.rejected {
        conditions |= ShotConditions::REJECTED;
    }
    if status.inconsistent {
        conditions |= ShotConditions::INCONSISTENT;
    }
    if status.unresolved {
        conditions |= ShotConditions::UNRESOLVED;
    }
    conditions
}

/// The protocol's conditions as the Qodec runtime's status: the inverse of
/// [`conditions`].
#[must_use]
pub fn status(conditions: ShotConditions) -> Status {
    Status {
        rejected: conditions.contains(ShotConditions::REJECTED),
        inconsistent: conditions.contains(ShotConditions::INCONSISTENT),
        unresolved: conditions.contains(ShotConditions::UNRESOLVED),
    }
}

/// The outcomes of one result: its bits (observes then flags) under `tags`.
pub fn outcomes<'a>(
    shot: ShotId,
    tags: &'a [u32],
    bits: impl Iterator<Item = Bit> + 'a,
) -> impl Iterator<Item = ShotOutcome> + 'a {
    tags.iter()
        .zip(bits)
        .map(move |(&tag, b)| ShotOutcome::new(shot, tag, value(b)))
}
