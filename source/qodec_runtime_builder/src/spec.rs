// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Helpers that implement qodec spec rules on the `qodec` model: converting
//! instruction sets and arguments for the pieces, selection patterns, frame
//! deltas, parity references, encoding layouts, and codes. The rules that need
//! no `qodec` model, such as binding operands, are in
//! [`qodec_runtime_pieces::isa`].

use super::{
    FrameKey,
    isa::{self, CompiledSelect, Isa, Operand, ParamKind, Parameter, Signature, TypeTable},
    parity::{Basis, Parity, Side, Var},
    pauli::Pauli,
    protocol::OwnedParam,
};
use qodec::{
    Argument, BlockOperand, Code, Encoding, Gadget, Instruction, InstructionSet, ParameterKind,
    ParityEquation, ParityTerm, Reference, ReferenceSegment, SelectPattern,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, rc::Rc, sync::Arc};

// ── Instruction sets ────────────────────────────────────────────────────────

/// `isa` as the pieces see it: its block types and its instructions'
/// signatures.
#[must_use]
pub fn isa(isa: &InstructionSet) -> Isa {
    Isa {
        name: isa.name.clone(),
        types: type_table(isa),
        instructions: isa.instructions.iter().map(signature).collect(),
    }
}

/// The block types of `isa`.
#[must_use]
pub fn type_table(isa: &InstructionSet) -> TypeTable {
    TypeTable::new(isa.blocks.iter().map(|b| (b.name.clone(), b.encodes)))
}

/// What a call of `instruction` takes and returns.
#[must_use]
pub fn signature(instruction: &Instruction) -> Signature {
    let operands = |operands: &[BlockOperand]| {
        operands
            .iter()
            .map(|o| Operand {
                block: o.block.clone(),
                variadic: o.is_variadic,
            })
            .collect()
    };
    Signature {
        mnemonic: instruction.mnemonic.clone(),
        inputs: operands(&instruction.inputs),
        outputs: operands(&instruction.outputs),
        parameters: instruction
            .parameters
            .iter()
            .map(|p| Parameter {
                name: p.name.clone(),
                kind: param_kind(p.kind),
            })
            .collect(),
        outcomes: instruction.observe_count(),
        flags: instruction.flags.len(),
    }
}

/// The pieces' name for `kind`.
#[must_use]
pub fn param_kind(kind: ParameterKind) -> ParamKind {
    match kind {
        ParameterKind::Bit => ParamKind::Bit,
        ParameterKind::Number => ParamKind::Number,
        ParameterKind::Integer => ParamKind::Integer,
        ParameterKind::Boolean => ParamKind::Boolean,
        ParameterKind::String => ParamKind::String,
        ParameterKind::Pauli => ParamKind::Pauli,
    }
}

// ── Arguments ───────────────────────────────────────────────────────────────

/// Whether `value` is a valid argument for a parameter of `kind`.
///
/// A list never matches: it is a type error. This deliberately deviates from
/// the spec, which is inconsistent here.
/// - The spec declares no list [parameter kind](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#instruction-structure),
///   and the `qodec` crate documents its list of integers
///   (`Argument::QubitList`) as appearing only in the operand role.
/// - Yet the YAML representation accepts lists of non-negative integers and
///   lists of strings as [argument values](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/representations/yaml.md#inline-yaml-calls),
///   and validation [checks them "by element type"](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/validation.md#circuit-sources),
///   so `[1, 2]` would supply an `integer` parameter.
///
/// A parameter's declared kind is its contract, and no action step can use a
/// list, so a gadget that passes one fails with `InvalidInstruction` when it
/// runs.
///
/// `Argument::Qubit` never matches either; the qodec parser never produces
/// it. Neither does `Argument::Readout`: the plan replaces readout arguments
/// with bits before this check (`ArgumentPlan::Readout` in `plan.rs`).
#[must_use]
pub fn matches(kind: ParameterKind, value: &Argument) -> bool {
    encode_arg(value).is_some_and(|v| isa::matches(param_kind(kind), &v))
}

/// Converts a spec argument value to a parameter. Returns `None` for the
/// values no parameter accepts (see [`matches`](fn@matches)): lists,
/// `Argument::Qubit` and `Argument::Readout`.
#[must_use]
pub fn encode_arg(value: &Argument) -> Option<OwnedParam> {
    Some(match value {
        Argument::Boolean(b) => OwnedParam::Bool(*b),
        Argument::Integer(i) => OwnedParam::Int(*i),
        Argument::Number(f) => OwnedParam::Float(*f),
        Argument::Text(s) => OwnedParam::Text(s.clone()),
        Argument::QubitList(_)
        | Argument::StringList(_)
        | Argument::Qubit(_)
        | Argument::Readout(_) => return None,
    })
}

// ── Selection ───────────────────────────────────────────────────────────────

/// Compiles [`select`](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/representations/yaml.md#per-call-select)
/// patterns against an instruction's flag names.
pub fn compile_select(
    flag_names: &[String],
    patterns: &[SelectPattern],
) -> Result<CompiledSelect, String> {
    let mut compiled = Vec::with_capacity(patterns.len());
    for pattern in patterns {
        let mut entries: Vec<(usize, bool)> = Vec::with_capacity(pattern.len());
        for (key, &value) in pattern {
            let index = match key
                .strip_prefix("flags[")
                .and_then(|rest| rest.strip_suffix(']'))
            {
                Some(i) => i
                    .parse::<usize>()
                    .map_err(|_| format!("invalid flag reference `{key}`"))?,
                None => flag_names
                    .iter()
                    .position(|f| f == key)
                    .ok_or_else(|| format!("unknown flag `{key}`"))?,
            };
            if index >= flag_names.len() {
                return Err(format!("flag reference `{key}` is out of range"));
            }
            if value > 1 {
                return Err(format!("select value for `{key}` must be 0 or 1"));
            }
            let value = value == 1;
            match entries.iter().find(|(i, _)| *i == index) {
                Some((_, v)) if *v != value => {
                    return Err(format!("conflicting select values for `{key}`"));
                }
                Some(_) => {}
                None => entries.push((index, value)),
            }
        }
        compiled.push(entries);
    }
    Ok(compiled)
}

// ── Frame deltas ────────────────────────────────────────────────────────────

/// Parses a gadget's [`frames`](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/gadget.md#frames)
/// into parities of circuit readouts, keyed by the output logical sign each
/// one flips.
///
/// For example, the spec's `"out[0].z[0]": ["circuit.readouts[6]",
/// "circuit.readouts[8]"]` becomes key `(0, Basis::Z, 0)` with parity
/// `Rec(6) XOR Rec(8)`: the first logical Z sign of output 0 flips when bits 6
/// and 8 differ.
pub fn prepare_frames(
    gadget: &Gadget,
    scope: &RefScope<'_>,
) -> Result<FxHashMap<FrameKey, Parity>, String> {
    let mut frames = FxHashMap::default();
    let mut memo = FxHashMap::default();
    let mut active = vec![false; gadget.readouts.len()];
    for (key, equation) in &gadget.frames {
        let key = match reference_vars(key, scope)?.as_slice() {
            [
                Var::Enc {
                    side: Side::Out,
                    e,
                    basis: basis @ (Basis::X | Basis::Z),
                    k,
                },
            ] => (*e, *basis, *k),
            _ => {
                return Err(format!(
                    "frame key `{key}` must name exactly one output logical sign"
                ));
            }
        };
        if frames.contains_key(&key) {
            return Err(format!("duplicate frame for `{key:?}`"));
        }
        let parity = local_parity(gadget, equation, scope, &mut memo, &mut active)?;
        frames.insert(key, parity);
    }
    Ok(frames)
}

/// The parity of a frame value. The spec lets [frame values](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/gadget.md#frames)
/// use circuit readouts, literals, and `readouts[...]` aliases that resolve
/// (acyclically) to those. Never `in[...]` or `out[...]`, even if they would
/// cancel. Each alias is expanded once (memo); without the memo, chains such as
/// `readouts[i] = [readouts[i-1], readouts[i-1]]` take 2^depth time.
fn local_parity(
    gadget: &Gadget,
    equation: &ParityEquation,
    scope: &RefScope<'_>,
    memo: &mut FxHashMap<usize, Parity>,
    active: &mut [bool],
) -> Result<Parity, String> {
    let mut acc = Parity::default();
    for term in equation {
        let reference = match term {
            ParityTerm::Bit(b) => {
                acc ^= Parity::constant(*b);
                continue;
            }
            ParityTerm::Reference(r) => r,
        };
        for var in reference_vars(reference, scope)? {
            match var {
                Var::Rec(_) => acc ^= Parity::var(var),
                Var::Ro(i) => {
                    if active[i] {
                        return Err(format!("cyclic readout alias `readouts[{i}]`"));
                    }
                    if !memo.contains_key(&i) {
                        active[i] = true;
                        let parity = local_parity(
                            gadget,
                            &gadget.readouts[i].equation,
                            scope,
                            memo,
                            active,
                        )?;
                        active[i] = false;
                        memo.insert(i, parity);
                    }
                    acc ^= &memo[&i];
                }
                Var::Enc { .. } => {
                    return Err("frame values cannot reference encoding signs".to_string());
                }
            }
        }
    }
    Ok(acc)
}

// ── Parity references ───────────────────────────────────────────────────────

/// The parity of an equation. Duplicate references cancel. For example,
/// `["circuit.readouts[0:2]", "circuit.readouts[1]", 1]` expands to
/// `Rec(0) XOR Rec(1) XOR Rec(1) XOR 1`, which is `Rec(0) XOR 1`.
pub fn expr(equation: &ParityEquation, scope: &RefScope<'_>) -> Result<Parity, String> {
    let mut acc = Parity::default();
    for term in equation {
        match term {
            ParityTerm::Bit(b) => acc ^= Parity::constant(*b),
            ParityTerm::Reference(r) => {
                for var in reference_vars(r, scope)? {
                    acc ^= Parity::var(var);
                }
            }
        }
    }
    Ok(acc)
}

/// Expands a reference into single variables, in order and with duplicates.
/// Only gadget-local references are supported, which is all that `qodec`
/// accepts in parity equations. `Reference::expand` also shortens the alias
/// `in[e].code.x[k]` to `in[e].x[k]` (likewise for `out`, `z` and
/// `stabilizers`), so aliases need no case here.
pub fn reference_vars(reference: &Reference, scope: &RefScope<'_>) -> Result<Vec<Var>, String> {
    use ReferenceSegment::{Field, Index};
    let mut vars = Vec::new();
    for single in reference.expand() {
        let out_of_range = || format!("reference `{reference}` is out of range");
        let var = match single.segments() {
            [Field(c), Field(r), Index(i)] if c == "circuit" && r == "readouts" => (*i
                < scope.record_len)
                .then_some(Var::Rec(*i))
                .ok_or_else(out_of_range)?,
            [Field(r), Index(i)] if r == "readouts" => (*i < scope.readouts_len)
                .then_some(Var::Ro(*i))
                .ok_or_else(out_of_range)?,
            [Field(side), Index(e), Field(basis), Index(k)] => {
                let (side, layouts) = match side.as_str() {
                    "in" => (Side::In, scope.inputs),
                    "out" => (Side::Out, scope.outputs),
                    _ => return Err(format!("unsupported reference `{reference}`")),
                };
                let basis = match basis.as_str() {
                    "stabilizers" => Basis::Stabilizers,
                    "x" => Basis::X,
                    "z" => Basis::Z,
                    _ => return Err(format!("unsupported reference `{reference}`")),
                };
                let layout = layouts.get(*e).ok_or_else(out_of_range)?;
                if *k >= layout.code.len(basis) {
                    return Err(out_of_range());
                }
                Var::Enc {
                    side,
                    e: *e,
                    basis,
                    k: *k,
                }
            }
            _ => return Err(format!("unsupported reference `{reference}`")),
        };
        vars.push(var);
    }
    Ok(vars)
}

/// What a gadget-local reference may address.
pub struct RefScope<'a> {
    /// Length of the circuit readout record.
    pub record_len: usize,
    /// Number of gadget readouts.
    pub readouts_len: usize,
    /// The input encodings.
    pub inputs: &'a [EncodingPlan],
    /// The output encodings.
    pub outputs: &'a [EncodingPlan],
}

// ── Encodings and code-qubit addresses ─────────────────────────────────────

/// Plans one side of a gadget boundary: one [`EncodingPlan`] per operand. The
/// code comes from the layer (`codes`, by block type), never from the gadget;
/// the gadget's encoding must name the same code.
pub fn layout_boundary(
    operands: &[BlockOperand],
    encodings: &[Encoding],
    codes: &BTreeMap<String, Rc<CodePlan>>,
    upper: &TypeTable,
    lower: &TypeTable,
) -> Result<Vec<EncodingPlan>, String> {
    if operands.len() != encodings.len() {
        return Err("one encoding is required per operand".to_string());
    }
    let mut layouts = Vec::with_capacity(operands.len());
    let mut all_labels = FxHashSet::default();
    for (op, enc) in operands.iter().zip(encodings) {
        let code = codes
            .get(&op.block)
            .ok_or_else(|| format!("the layer binds no code to block type `{}`", op.block))?;
        if *enc.code != *code.code {
            return Err(format!(
                "encoding code differs from the layer's code for `{}`",
                op.block
            ));
        }
        if code.x.len() != code.z.len() || Some(code.x.len()) != upper.cap(&op.block) {
            return Err(format!(
                "code for `{}` does not encode its block's logical qubits",
                op.block
            ));
        }
        let lower_types: Vec<usize> = if enc.block_types.is_empty() {
            let only = lower.only().ok_or_else(|| {
                "encodings need block_types when the lower ISA has several block types".to_string()
            })?;
            vec![only; enc.support.len()]
        } else {
            enc.block_types
                .iter()
                .map(|t| {
                    lower
                        .get(t)
                        .ok_or_else(|| format!("unknown lower block type `{t}`"))
                })
                .collect::<Result<_, _>>()?
        };
        if lower_types.len() != enc.support.len() {
            return Err("different support and block-type lengths".to_string());
        }
        if lower_types.iter().map(|&t| lower.caps[t]).sum::<usize>() != code.n {
            return Err(format!(
                "encoding support for `{}` does not hold its code's {} qubits",
                op.block, code.n
            ));
        }
        // A support label names one lower block of the gadget's circuit, so it
        // may appear only once per side, within and across encodings.
        // Otherwise live blocks would share a lower block id and release it
        // twice.
        for label in &enc.support {
            if !all_labels.insert(label.as_str()) {
                return Err(format!("support label `{label}` is used twice on one side"));
            }
        }
        layouts.push(EncodingPlan {
            block_type: op.block.clone(),
            code: Rc::clone(code),
            labels: enc.support.clone(),
            lower_types,
        });
    }
    Ok(layouts)
}

/// The address of each code qubit of `layout`, in code-qubit order:
/// `(slot, logical index inside that slot's lower block)`.
///
/// `slots[i]` is the slot of `layout.labels[i]`. For example, with two labels
/// of a lower type that encodes 2 qubits, in slots 3 and 4, code qubits 0 to 3
/// are `(3, 0), (3, 1), (4, 0), (4, 1)`.
#[must_use]
pub fn code_qubits(
    layout: &EncodingPlan,
    slots: &[usize],
    lower: &TypeTable,
) -> Vec<(usize, usize)> {
    slots
        .iter()
        .zip(&layout.lower_types)
        .flat_map(|(&slot, &t)| (0..lower.caps[t]).map(move |i| (slot, i)))
        .collect()
}

/// The plan of one gadget-boundary [encoding](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/gadget.md#the-in--out-boundary-encodings):
/// where the block lives in the gadget's circuit.
#[derive(Clone, Debug)]
pub struct EncodingPlan {
    /// The upper block type.
    pub block_type: String,
    /// The code the layer binds to the block type.
    pub code: Rc<CodePlan>,
    /// Circuit labels, in code-qubit order.
    pub labels: Vec<String>,
    /// The lower block type of each label, as an index into the lower
    /// [`TypeTable`].
    pub lower_types: Vec<usize>,
}

impl EncodingPlan {
    /// The number of lower blocks of each lower type, indexed like `lower`.
    #[must_use]
    pub fn resources(&self, lower: &TypeTable) -> Vec<usize> {
        let mut counts = vec![0; lower.len()];
        for &t in &self.lower_types {
            counts[t] += 1;
        }
        counts
    }
}

/// The plan of a code: a [`Code`] with its Pauli text parsed.
#[derive(Debug)]
pub struct CodePlan {
    /// The code as loaded.
    pub code: Arc<Code>,
    /// Stabilizer generators.
    pub stabilizers: Vec<Pauli>,
    /// Logical X operators, one per logical qubit.
    pub x: Vec<Pauli>,
    /// Logical Z operators, one per logical qubit.
    pub z: Vec<Pauli>,
    /// Physical qubit count.
    pub n: usize,
}

impl CodePlan {
    /// Parses the operators of `code`.
    pub fn new(code: &Arc<Code>) -> Result<Self, String> {
        let parse = |paulis: &[qodec::PauliString]| {
            paulis
                .iter()
                .map(|p| Pauli::parse(&p.0))
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(Self {
            code: Arc::clone(code),
            stabilizers: parse(&code.stabilizers)?,
            x: parse(&code.x)?,
            z: parse(&code.z)?,
            n: code.physical_qubit_count(),
        })
    }

    /// The number of operators of a kind.
    #[must_use]
    pub fn len(&self, basis: Basis) -> usize {
        self.paulis(basis).len()
    }

    /// The operators of a kind.
    #[must_use]
    pub fn paulis(&self, basis: Basis) -> &[Pauli] {
        match basis {
            Basis::Stabilizers => &self.stabilizers,
            Basis::X => &self.x,
            Basis::Z => &self.z,
        }
    }
}
