// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Helpers that implement qodec spec rules: operand binding, arguments,
//! selection, parity references, frame deltas, and encoding layouts.

use super::{
    backend::{Bit, Param, ParamView, Status},
    parity::{Basis, Parity, Side, Var},
    pauli::Pauli,
};
use qodec::{
    Argument, BlockOperand, Code, Encoding, Gadget, Instruction, InstructionSet, ParameterKind,
    ParityEquation, ParityTerm, Reference, ReferenceSegment, SelectPattern,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, hash::Hash, rc::Rc, sync::Arc};

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
    /// Interns the block types of `isa`.
    #[must_use]
    pub fn new(isa: &InstructionSet) -> Self {
        let mut table = Self::default();
        for block in &isa.blocks {
            table.index.insert(block.name.clone(), table.names.len());
            table.names.push(block.name.clone());
            table.caps.push(block.encodes);
        }
        table
    }

    /// The index of the type named `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    /// Logical qubits per block of the type named `name`.
    #[must_use]
    pub fn cap(&self, name: &str) -> Option<usize> {
        self.get(name).map(|t| self.caps[t])
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

// ── Operand binding ─────────────────────────────────────────────────────────

/// One operand bound into the flat index space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundOperand<T> {
    /// The target bound to the operand.
    pub label: T,
    /// The operand's block type.
    pub block_type: String,
    /// The first flat index of the operand.
    pub offset: usize,
    /// The operand's logical qubit count.
    pub width: usize,
}

/// The bound inputs and outputs of one instruction.
pub type BoundOperands<T> = (Vec<BoundOperand<T>>, Vec<BoundOperand<T>>);

/// Binds `targets` against `instruction.inputs` and `instruction.outputs`.
///
/// Both sides are bound against the same operand list: operand `j` is `in[j]`
/// and/or `out[j]`, so targets past the bound inputs are output-only operands.
/// Each side numbers its qubits from 0, giving the spec's [flat index
/// space](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#instruction-structure).
/// `input_types` constrains only the input side, so type-changing
/// instructions bind.
#[allow(clippy::implicit_hasher)]
pub fn bind_operands<T: Eq + Hash + Clone>(
    types: &TypeTable,
    instruction: &Instruction,
    targets: &[T],
    input_types: &FxHashMap<T, &str>,
) -> Result<BoundOperands<T>, String> {
    let mut seen = FxHashSet::default();
    if !targets.iter().all(|t| seen.insert(t)) {
        return Err("operands must be distinct".to_string());
    }
    let inputs = bind_side(&instruction.inputs, targets, types, input_types)?;
    let outputs = bind_side(&instruction.outputs, targets, types, &FxHashMap::default())?;
    if targets.len() != inputs.len().max(outputs.len()) {
        return Err(format!(
            "`{}` expects {} operands, got {}",
            instruction.mnemonic,
            inputs.len().max(outputs.len()),
            targets.len()
        ));
    }
    Ok((inputs, outputs))
}

fn bind_side<T: Eq + Hash + Clone>(
    operands: &[BlockOperand],
    targets: &[T],
    types: &TypeTable,
    known: &FxHashMap<T, &str>,
) -> Result<Vec<BoundOperand<T>>, String> {
    let side_types: Vec<&str> = if operands.iter().any(|o| o.is_variadic) {
        unique_type_assignment(operands, targets, known)?
    } else {
        if operands.len() > targets.len() {
            return Err(format!(
                "expected at least {} operands, got {}",
                operands.len(),
                targets.len()
            ));
        }
        operands.iter().map(|o| o.block.as_str()).collect()
    };
    let mut offset = 0;
    let mut bound = Vec::with_capacity(side_types.len());
    for (t, ty) in targets.iter().zip(side_types) {
        let width = types
            .cap(ty)
            .ok_or_else(|| format!("unknown block type `{ty}`"))?;
        if known.get(t).is_some_and(|k| *k != ty) {
            return Err(format!("operand has type `{}`, expected `{ty}`", known[t]));
        }
        bound.push(BoundOperand {
            label: t.clone(),
            block_type: ty.to_string(),
            offset,
            width,
        });
        offset += width;
    }
    Ok(bound)
}

/// A [variadic](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#variadic-blocks)
/// side consumes ALL targets. Each variadic operand takes any number of
/// consecutive targets, including none, and each fixed operand exactly one.
/// Finds the splittings whose total is `len(targets)` and whose types agree
/// with `known`, and requires exactly one resulting type list. Each state keeps
/// at most two distinct type lists, which is enough to detect ambiguity.
fn unique_type_assignment<'a, T: Eq + Hash>(
    operands: &'a [BlockOperand],
    targets: &[T],
    known: &FxHashMap<T, &str>,
) -> Result<Vec<&'a str>, String> {
    fn push<'a>(states: &mut Vec<Vec<&'a str>>, candidate: Vec<&'a str>) {
        if states.len() < 2 && !states.contains(&candidate) {
            states.push(candidate);
        }
    }
    let n = targets.len();
    let compatible = |j: usize, ty: &str| known.get(&targets[j]).is_none_or(|k| *k == ty);
    let mut states: Vec<Vec<Vec<&str>>> = vec![Vec::new(); n + 1];
    states[0].push(Vec::new());
    for op in operands {
        let ty = op.block.as_str();
        let mut next: Vec<Vec<Vec<&str>>> = vec![Vec::new(); n + 1];
        for (j, prefixes) in states.iter().enumerate() {
            for prefix in prefixes {
                if op.is_variadic {
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

// ── Arguments ───────────────────────────────────────────────────────────────

/// Requires `args` to supply every parameter of `instruction`, and nothing else,
/// with values that match the parameter kinds.
pub fn validate_arguments(
    instruction: &Instruction,
    args: &BTreeMap<String, Argument>,
) -> Result<(), String> {
    if args.len() != instruction.parameters.len() {
        return Err(format!(
            "`{}` expects {} arguments, got {}",
            instruction.mnemonic,
            instruction.parameters.len(),
            args.len()
        ));
    }
    for parameter in &instruction.parameters {
        let value = args.get(&parameter.name).ok_or_else(|| {
            format!(
                "`{}` is missing argument `{}`",
                instruction.mnemonic, parameter.name
            )
        })?;
        if !matches(parameter.kind, value) {
            return Err(format!(
                "argument `{}` of `{}` does not match its kind {:?}",
                parameter.name, instruction.mnemonic, parameter.kind
            ));
        }
    }
    Ok(())
}

/// Whether `value` is a valid argument for a parameter of `kind`. A list is all
/// non-negative Ints or all Strings (possibly empty), and each item must match.
#[must_use]
pub fn matches(kind: ParameterKind, value: &Argument) -> bool {
    match value {
        Argument::QubitList(items) => items.iter().all(|&i| matches_int(kind, i as i128)),
        Argument::StringList(items) => items.iter().all(|s| matches_text(kind, s)),
        Argument::Integer(i) => matches_int(kind, i128::from(*i)),
        Argument::Number(f) => kind == ParameterKind::Number && f.is_finite(),
        Argument::Boolean(_) => matches!(kind, ParameterKind::Bit | ParameterKind::Boolean),
        Argument::Text(s) => matches_text(kind, s),
        Argument::Qubit(_) | Argument::Readout(_) => false,
    }
}

fn matches_int(kind: ParameterKind, value: i128) -> bool {
    match kind {
        ParameterKind::Number | ParameterKind::Integer => true,
        ParameterKind::Bit => value == 0 || value == 1,
        _ => false,
    }
}

/// Whether `value` is a valid text argument for a parameter of `kind`. A
/// `circuit.readouts[...]` reference is not text: the spec lets only `bit`
/// parameters take [readout references](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/representations/yaml.md#threading-readouts-into-bit-parameters),
/// and supplying one to another parameter is an error.
fn matches_text(kind: ParameterKind, value: &str) -> bool {
    matches!(kind, ParameterKind::String | ParameterKind::Pauli)
        && !value.starts_with("circuit.readouts[")
}

/// Converts a parameter view to a spec argument value. `Block` entries are not
/// arguments.
#[must_use]
pub fn value_of(view: ParamView<'_>) -> Option<Argument> {
    Some(match view {
        ParamView::Block(_) => return None,
        ParamView::Bool(b) => Argument::Boolean(b),
        ParamView::Int(i) => Argument::Integer(i),
        ParamView::Float(f) => Argument::Number(f),
        ParamView::Text(s) => Argument::Text(s.to_string()),
        ParamView::IntList(items) => Argument::QubitList(
            items
                .iter()
                .map(|&i| usize::try_from(i).ok())
                .collect::<Option<_>>()?,
        ),
        ParamView::TextList(items) => Argument::StringList(items.to_vec()),
    })
}

/// Converts a spec argument value to a parameter. It never wraps: values that do
/// not fit fail.
#[must_use]
pub fn encode_arg<P: Param>(value: &Argument) -> Option<P> {
    Some(match value {
        Argument::Boolean(b) => P::from_view(ParamView::Bool(*b)),
        Argument::Integer(i) => P::from_view(ParamView::Int(*i)),
        Argument::Number(f) => P::from_view(ParamView::Float(*f)),
        Argument::Text(s) => P::from_view(ParamView::Text(s)),
        Argument::QubitList(items) => {
            let items = items
                .iter()
                .map(|&i| u64::try_from(i).ok())
                .collect::<Option<Vec<_>>>()?;
            P::from_view(ParamView::IntList(&items))
        }
        Argument::StringList(items) => P::from_view(ParamView::TextList(items)),
        Argument::Qubit(_) | Argument::Readout(_) => return None,
    })
}

// ── Selection ───────────────────────────────────────────────────────────────

/// Patterns of `(flag index, expected value)`. AND inside a pattern, OR across.
pub type CompiledSelect = Vec<Vec<(usize, bool)>>;

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

/// The largest number of unknown flags [`accepts`] enumerates. Past it,
/// `accepts` answers `None` instead of enumerating. Picked arbitrarily, not
/// measured: it bounds the worst case at 2^24 (about 16.8 million)
/// assignments.
const MAX_ENUMERATED_FLAGS: usize = 24;
// `accepts` enumerates assignments as the bits of a `u32`.
const _: () = assert!(MAX_ENUMERATED_FLAGS < u32::BITS as usize);

/// Three-valued answer: `Some(true)` iff EVERY completion of the unknown flags
/// matches some pattern, `Some(false)` iff NO completion does, `None`
/// otherwise. (A per-pattern scan is not enough: `[{f0:0},{f0:1}]` with `f0`
/// unknown is accepted by every completion.)
///
/// The answer is exact except in one case: when deciding it would require
/// enumerating more than `MAX_ENUMERATED_FLAGS` (24) unknown flags, it is
/// `None` even if every completion matches.
///
/// An empty `compiled` returns `Some(true)`: the spec defines an omitted or
/// empty [`select`](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/representations/yaml.md#per-call-select)
/// as no constraint, not as a list that nothing matches.
#[must_use]
pub fn accepts(compiled: &CompiledSelect, flags: &[Bit]) -> Option<bool> {
    if compiled.is_empty() {
        return Some(true);
    }
    // Keep patterns consistent with the known flags, restricted to unknown ones.
    let live: Vec<Vec<(usize, bool)>> = compiled
        .iter()
        .filter(|p| p.iter().all(|&(i, v)| flags[i].is_none_or(|f| f == v)))
        .map(|p| {
            p.iter()
                .copied()
                .filter(|&(i, _)| flags[i].is_none())
                .collect()
        })
        .collect();
    if live.is_empty() {
        return Some(false);
    }
    if live.iter().any(Vec::is_empty) {
        return Some(true);
    }
    // Some completion matches. Do all? Enumerate the unknown flags that the live
    // patterns mention; others cannot affect the result. The cost is
    // exponential in their number, hence the cap.
    let mut unknown: Vec<usize> = live.iter().flatten().map(|&(i, _)| i).collect();
    unknown.sort_unstable();
    unknown.dedup();
    if unknown.len() > MAX_ENUMERATED_FLAGS {
        return None;
    }
    for assignment in 0u32..(1 << unknown.len()) {
        let value = |flag: usize| {
            let position = unknown.binary_search(&flag).unwrap_or_default();
            assignment & (1 << position) != 0
        };
        if !live.iter().any(|p| p.iter().all(|&(i, v)| value(i) == v)) {
            return None;
        }
    }
    Some(true)
}

/// The spec makes [`select`](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/representations/yaml.md#per-call-select)
/// a hint for downstream tools. This runtime turns it into a recoverable
/// status, never an error.
#[must_use]
pub fn select_status(compiled: &CompiledSelect, flags: &[Bit]) -> Status {
    match accepts(compiled, flags) {
        Some(true) => Status::default(),
        Some(false) => Status::REJECTED,
        None => Status::UNRESOLVED,
    }
}

// ── Parity references ───────────────────────────────────────────────────────

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

/// Expands a reference into single variables, in order and with duplicates.
/// Only gadget-local references are supported, which is all that `qodec`
/// accepts in parity equations.
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

/// The parity of an equation. Duplicate references cancel.
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

// ── Frame deltas ────────────────────────────────────────────────────────────

/// `(output entry, basis, logical index)` of one output logical sign.
pub type FrameKey = (usize, Basis, usize);

/// Parses a gadget's [`frames`](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/gadget.md#frames)
/// into parities of circuit readouts.
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

/// Frame values may use circuit readouts, literals, and `readouts[...]` aliases
/// that resolve (acyclically) to those. Never `in[...]` or `out[...]`, even if
/// they would cancel. Each alias is expanded once (memo); without the memo,
/// chains such as `readouts[i] = [readouts[i-1], readouts[i-1]]` take 2^depth time.
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

// ── Encodings and code-qubit addresses ─────────────────────────────────────

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
                .map(Pauli::from_text)
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
    /// Lower type per label.
    pub lower_types: Vec<usize>,
}

impl EncodingPlan {
    /// Lower blocks per lower type.
    #[must_use]
    pub fn resources(&self, lower: &TypeTable) -> Vec<usize> {
        let mut counts = vec![0; lower.len()];
        for &t in &self.lower_types {
            counts[t] += 1;
        }
        counts
    }
}

/// Plans one side of a gadget boundary: one [`EncodingPlan`] per operand. The
/// layer binds the code, never the gadget.
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
        // Distinct operands on the same side must not share lower blocks;
        // otherwise two live blocks would own the same LowerId (double free).
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

/// Code-qubit position k → (lower block, logical index inside it).
#[must_use]
#[allow(clippy::implicit_hasher)]
pub fn code_qubits(
    layout: &EncodingPlan,
    placement: &FxHashMap<String, u64>,
    lower: &TypeTable,
) -> Vec<(u64, usize)> {
    layout
        .labels
        .iter()
        .zip(&layout.lower_types)
        .flat_map(|(label, &t)| {
            let block = placement[label];
            (0..lower.caps[t]).map(move |i| (block, i))
        })
        .collect()
}
