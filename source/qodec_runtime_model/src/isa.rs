// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Instruction sets: block types, and instruction signatures with their
//! actions. And the qodec spec rules over them, which define what a call
//! means: parsing calls, binding operands, and matching arguments. Every piece
//! must read a call the same way, so these rules are part of the model.

use super::{action::ActionStep, protocol::OwnedParam};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, hash::Hash};

/// An instruction set: its block types and its instructions.
#[derive(Clone, Debug, Default)]
pub struct Isa {
    /// The instruction set's name.
    pub name: String,
    /// Its block types.
    pub types: TypeTable,
    /// Its instructions, by instruction id.
    pub instructions: Vec<Signature>,
}

impl Isa {
    /// The (observe count, flag count) of instruction `index`, if it exists
    /// and returns a result.
    #[inline]
    #[must_use]
    pub fn shape(&self, index: usize) -> Option<(usize, usize)> {
        let instruction = self.instructions.get(index)?;
        instruction
            .has_result()
            .then_some((instruction.outcomes, instruction.flags))
    }

    /// Parses `parameters` as a call of instruction `instruction_id`: the
    /// instruction must exist, and the arguments must supply every parameter
    /// with a value of the right kind. The operands are not checked here;
    /// binding them needs the blocks' types (see [`Signature::bind`]).
    #[inline]
    pub fn parse_call(
        &self,
        instruction_id: usize,
        parameters: &[OwnedParam],
    ) -> Result<ParsedCall, String> {
        let instruction = self
            .instructions
            .get(instruction_id)
            .ok_or_else(|| format!("unknown instruction id {instruction_id}"))?;
        let operands: Vec<u64> = parameters
            .iter()
            .map_while(|p| match p {
                OwnedParam::Block(b) => Some(*b),
                _ => None,
            })
            .collect();
        let rest = &parameters[operands.len()..];
        if rest.len() != instruction.parameters.len() {
            return Err(format!(
                "`{}` expects {} arguments, got {}",
                instruction.mnemonic,
                instruction.parameters.len(),
                rest.len()
            ));
        }
        let mut args = BTreeMap::new();
        for (parameter, p) in instruction.parameters.iter().zip(rest) {
            if let OwnedParam::Block(_) = p {
                return Err(format!("argument `{}` is not a value", parameter.name));
            }
            args.insert(parameter.name.clone(), p.clone());
        }
        instruction.validate_arguments(&args)?;
        Ok(ParsedCall {
            index: instruction_id,
            operands,
            args,
        })
    }
}

/// An instruction: what a call takes, returns and does.
#[derive(Clone, Debug, Default)]
pub struct Signature {
    /// The instruction's mnemonic.
    pub mnemonic: String,
    /// The blocks it consumes, in operand order.
    pub inputs: Vec<Operand>,
    /// The blocks it produces, in operand order.
    pub outputs: Vec<Operand>,
    /// Its classical parameters, in declaration order.
    pub parameters: Vec<Parameter>,
    /// Its observe outcomes per call.
    pub outcomes: usize,
    /// Its flags per call.
    pub flags: usize,
    /// What it does, step by step, on the flat indices of its operands.
    pub action: Vec<ActionStep>,
}

impl Signature {
    /// Whether a call returns a result: it does iff it has observe outcomes
    /// or flags.
    #[inline]
    #[must_use]
    pub fn has_result(&self) -> bool {
        self.outcomes + self.flags > 0
    }

    /// Requires `args` to supply every parameter, and nothing else, with
    /// values that match the parameter kinds.
    #[inline]
    pub fn validate_arguments(&self, args: &BTreeMap<String, OwnedParam>) -> Result<(), String> {
        if args.len() != self.parameters.len() {
            return Err(format!(
                "`{}` expects {} arguments, got {}",
                self.mnemonic,
                self.parameters.len(),
                args.len()
            ));
        }
        for parameter in &self.parameters {
            let value = args.get(&parameter.name).ok_or_else(|| {
                format!(
                    "`{}` is missing argument `{}`",
                    self.mnemonic, parameter.name
                )
            })?;
            if !parameter.kind.matches(value) {
                return Err(format!(
                    "argument `{}` of `{}` does not match its kind {:?}",
                    parameter.name, self.mnemonic, parameter.kind
                ));
            }
        }
        Ok(())
    }

    /// Binds `targets` against the inputs and outputs; `types` are the block
    /// types of the instruction's ISA.
    ///
    /// Both sides are bound against the same operand list: operand `j` is
    /// `in[j]` and/or `out[j]`, so targets past the bound inputs are
    /// output-only operands. Each side numbers its qubits from 0, giving the
    /// spec's [flat index space](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#instruction-structure).
    /// For example, if block type `pair` encodes 2 qubits, an instruction with
    /// `in: [pair, pair]` and `out: [pair, pair, pair]` called on targets
    /// `[7, 8, 9]` binds inputs `7` and `8` at offsets 0 and 2, and outputs
    /// `7`, `8` and `9` at offsets 0, 2 and 4. Target `9` is output-only.
    ///
    /// `input_types` (the targets' known block types) constrains only the
    /// input side, so type-changing instructions bind.
    #[inline]
    #[allow(clippy::implicit_hasher)]
    pub fn bind<T: Eq + Hash + Clone>(
        &self,
        types: &TypeTable,
        targets: &[T],
        input_types: &FxHashMap<T, usize>,
    ) -> Result<BoundOperands<T>, String> {
        let mut seen = FxHashSet::default();
        if !targets.iter().all(|t| seen.insert(t)) {
            return Err("operands must be distinct".to_string());
        }
        let inputs = bind_side(&self.inputs, targets, types, input_types)?;
        let outputs = bind_side(&self.outputs, targets, types, &FxHashMap::default())?;
        if targets.len() != inputs.len().max(outputs.len()) {
            return Err(format!(
                "`{}` expects {} operands, got {}",
                self.mnemonic,
                inputs.len().max(outputs.len()),
                targets.len()
            ));
        }
        Ok((inputs, outputs))
    }
}

/// A block operand of a [`Signature`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operand {
    /// The block type, an index into its ISA's [`TypeTable`].
    pub block: usize,
    /// Whether it takes any number of consecutive blocks, including none.
    pub variadic: bool,
}

/// A classical parameter of a [`Signature`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    /// The parameter's name.
    pub name: String,
    /// What values it takes.
    pub kind: ParamKind,
}

/// The kind of a parameter: what values it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamKind {
    /// A bit: 0 or 1, or a boolean.
    Bit,
    /// A real number.
    Number,
    /// An integer.
    Integer,
    /// A boolean.
    Boolean,
    /// Text.
    String,
    /// A Pauli operator, as text.
    Pauli,
}

impl ParamKind {
    /// Whether `value` is a valid argument for a parameter of this kind. A
    /// block is an operand, never an argument.
    #[inline]
    #[must_use]
    pub fn matches(self, value: &OwnedParam) -> bool {
        match value {
            OwnedParam::Int(i) => self.matches_int(i128::from(*i)),
            OwnedParam::Float(f) => self == ParamKind::Number && f.is_finite(),
            OwnedParam::Bool(_) => matches!(self, ParamKind::Bit | ParamKind::Boolean),
            OwnedParam::Text(s) => self.matches_text(s),
            OwnedParam::Block(_) => false,
        }
    }

    /// The spec lets [integers supply `number` parameters](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/validation.md#circuit-sources)
    /// as well as `integer` ones.
    fn matches_int(self, value: i128) -> bool {
        match self {
            ParamKind::Number | ParamKind::Integer => true,
            ParamKind::Bit => value == 0 || value == 1,
            _ => false,
        }
    }

    /// Whether `value` is a valid text argument. A `circuit.readouts[...]`
    /// reference is not text: the spec lets only `bit` parameters take
    /// [readout references](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/representations/yaml.md#threading-readouts-into-bit-parameters),
    /// and supplying one to another parameter is an error.
    fn matches_text(self, value: &str) -> bool {
        matches!(self, ParamKind::String | ParamKind::Pauli)
            && !value.starts_with("circuit.readouts[")
    }
}

// ── Block type tables ───────────────────────────────────────────────────────

/// The block types of one ISA, interned as indices.
#[derive(Clone, Debug, Default)]
pub struct TypeTable {
    /// Type names, in declaration order.
    pub names: Vec<String>,
    /// Logical qubits per block of each type.
    pub caps: Vec<usize>,
    index: FxHashMap<String, usize>,
}

impl TypeTable {
    /// Interns `blocks`: (type name, logical qubits per block), in declaration
    /// order.
    #[must_use]
    pub fn new(blocks: impl IntoIterator<Item = (String, usize)>) -> Self {
        let mut table = Self::default();
        for (name, cap) in blocks {
            table.index.insert(name.clone(), table.names.len());
            table.names.push(name);
            table.caps.push(cap);
        }
        table
    }

    /// Logical qubits per block of the type named `name`.
    #[must_use]
    pub fn cap(&self, name: &str) -> Option<usize> {
        self.get(name).map(|t| self.caps[t])
    }

    /// The index of the type named `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    /// The number of types.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the ISA declares no types.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The only type, if there is exactly one.
    #[must_use]
    pub fn only(&self) -> Option<usize> {
        (self.names.len() == 1).then_some(0)
    }
}

// ── Calls ───────────────────────────────────────────────────────────────────

/// A call split into its block operands and its arguments by parameter name.
pub struct ParsedCall {
    /// The index of the call's instruction.
    pub index: usize,
    /// The leading `Block` parameters.
    pub operands: Vec<u64>,
    /// The other parameters, by parameter name.
    pub args: BTreeMap<String, OwnedParam>,
}

/// The bound inputs and outputs of one instruction.
pub type BoundOperands<T> = (Vec<BoundOperand<T>>, Vec<BoundOperand<T>>);

/// One operand bound into the flat index space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundOperand<T> {
    /// The target bound to the operand.
    pub label: T,
    /// The operand's block type.
    pub block_type: usize,
    /// The first flat index of the operand.
    pub offset: usize,
    /// The operand's logical qubit count.
    pub width: usize,
}

fn bind_side<T: Eq + Hash + Clone>(
    operands: &[Operand],
    targets: &[T],
    types: &TypeTable,
    known: &FxHashMap<T, usize>,
) -> Result<Vec<BoundOperand<T>>, String> {
    let side_types: Vec<usize> = if operands.iter().any(|o| o.variadic) {
        unique_type_assignment(operands, targets, known)?
    } else {
        if operands.len() > targets.len() {
            return Err(format!(
                "expected at least {} operands, got {}",
                operands.len(),
                targets.len()
            ));
        }
        operands.iter().map(|o| o.block).collect()
    };
    let mut offset = 0;
    let mut bound = Vec::with_capacity(side_types.len());
    for (t, ty) in targets.iter().zip(side_types) {
        if let Some(&k) = known.get(t)
            && k != ty
        {
            return Err(format!(
                "operand has type `{}`, expected `{}`",
                types.names[k], types.names[ty]
            ));
        }
        let width = types.caps[ty];
        bound.push(BoundOperand {
            label: t.clone(),
            block_type: ty,
            offset,
            width,
        });
        offset += width;
    }
    Ok(bound)
}

/// The one type list that binds a [variadic](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#variadic-blocks)
/// side to ALL of `targets`.
///
/// Each variadic operand takes any number of consecutive targets, including
/// none, and each fixed operand exactly one. Of the splittings that use all
/// targets and agree with `known`, exactly one type list must result; more is
/// ambiguous.
///
/// `states[j]` holds the type lists of the prefixes that use the first `j`
/// targets, and keeps at most two. That is enough: how a prefix can be
/// completed depends only on `j`, so two distinct lists in one state either
/// both complete, which is ambiguous, or neither does.
fn unique_type_assignment<T: Eq + Hash>(
    operands: &[Operand],
    targets: &[T],
    known: &FxHashMap<T, usize>,
) -> Result<Vec<usize>, String> {
    fn push(states: &mut Vec<Vec<usize>>, candidate: Vec<usize>) {
        if states.len() < 2 && !states.contains(&candidate) {
            states.push(candidate);
        }
    }
    let n = targets.len();
    let compatible = |j: usize, ty: usize| known.get(&targets[j]).is_none_or(|&k| k == ty);
    let mut states: Vec<Vec<Vec<usize>>> = vec![Vec::new(); n + 1];
    states[0].push(Vec::new());
    for op in operands {
        let ty = op.block;
        let mut next: Vec<Vec<Vec<usize>>> = vec![Vec::new(); n + 1];
        for (j, prefixes) in states.iter().enumerate() {
            for prefix in prefixes {
                if op.variadic {
                    let mut current = prefix.clone();
                    push(&mut next[j], current.clone());
                    for m in j..n {
                        if !compatible(m, ty) {
                            break;
                        }
                        current.push(ty);
                        push(&mut next[m + 1], current.clone());
                    }
                } else if j < n && compatible(j, ty) {
                    let mut current = prefix.clone();
                    current.push(ty);
                    push(&mut next[j + 1], current);
                }
            }
        }
        states = next;
    }
    let mut finals = std::mem::take(&mut states[n]);
    match finals.len() {
        0 => Err("operands do not match the variadic signature".to_string()),
        1 => Ok(finals.pop().unwrap_or_default()),
        _ => Err("ambiguous variadic types".to_string()),
    }
}
