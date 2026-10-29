// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lower-ISA operation resolver.
//!
//! The qodec spec has no generic operations, and its mnemonics are for humans:
//! as its [design principles][design-principles] put it, "the action is
//! authoritative". A layer emits generic operations of its own: Pauli
//! corrections, decoder operations and releases. It matches them against the
//! LOWER ISA's actions. A mnemonic only breaks a tie between instructions whose
//! actions match equally well.
//!
//! [design-principles]: https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#design-principles

use super::{
    actions::{PauliExpr, action_clifford},
    pauli::{Clifford, Pauli, PauliOp},
    spec::{CompiledSelect, TypeTable},
};
use qodec::{Action, Argument, InstructionSet, ParameterKind, Scalar};
use rustc_hash::FxHashMap;
use std::{collections::BTreeMap, rc::Rc};

/// A matching lower instruction: ((flagged, parameterized), semantics, args).
type Candidate<'a> = ((bool, bool), &'a Semantics, BTreeMap<String, Argument>);

/// Matches generic operations against the static semantics of a lower ISA.
#[derive(Debug)]
pub struct Resolver {
    semantics: Vec<Semantics>,
}

impl Resolver {
    /// Indexes the instructions of `isa` that have static semantics: no
    /// variadic operands, no `pauli` parameters and no guarded steps.
    #[must_use]
    pub fn new(isa: &InstructionSet, types: &TypeTable) -> Self {
        let semantics = isa
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| semantics(index, instruction, types))
            .collect();
        Self { semantics }
    }

    /// Resolves a generic operation on `slots` (lower block, logical index).
    /// Returns `None` if the lower ISA cannot implement it.
    pub fn resolve(
        &self,
        op: &str,
        slots: &[(u64, usize)],
        type_of: impl Fn(u64) -> usize,
        angle: Option<f64>,
        lower: &TypeTable,
        cache: &mut ResolveCache,
    ) -> Option<Vec<LowerOp>> {
        let mut labels: Vec<u64> = Vec::new();
        let mut offsets: Vec<usize> = Vec::new();
        let mut width = 0;
        for &(block, _) in slots {
            if !labels.contains(&block) {
                labels.push(block);
                offsets.push(width);
                width += lower.caps[type_of(block)];
            }
        }
        let blocks: Vec<usize> = labels.iter().map(|&b| type_of(b)).collect();
        let pos: Vec<usize> = slots
            .iter()
            .map(|(block, index)| {
                let at = labels.iter().position(|b| b == block).unwrap_or_default();
                offsets[at] + index
            })
            .collect();
        let mut distinct = pos.clone();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() != pos.len() {
            return None;
        }
        let key = (op.to_string(), blocks, pos, angle.map(f64::to_bits));
        let parts = if let Some(parts) = cache.0.get(&key) {
            parts.clone()?
        } else {
            let parts = self
                .resolve_parts(op, &key.1, &key.2, angle, lower)
                .map(Rc::new);
            cache.0.insert(key, parts.clone());
            parts?
        };
        Some(
            parts
                .iter()
                .map(|resolved| LowerOp {
                    resolved: resolved.clone(),
                    operands: labels.clone(),
                })
                .collect(),
        )
    }

    fn resolve_parts(
        &self,
        op: &str,
        blocks: &[usize],
        pos: &[usize],
        angle: Option<f64>,
        lower: &TypeTable,
    ) -> Option<Vec<Resolved>> {
        match self.bind_one(op, blocks, pos, angle, lower) {
            Ok(resolved) => Some(vec![resolved]),
            Err(BindError::Ambiguous) => None,
            Err(BindError::Unbound) => decompose(op, angle)?
                .into_iter()
                .map(|(part, part_angle)| self.bind_one(part, blocks, pos, part_angle, lower).ok())
                .collect(),
        }
    }

    /// Whether `op` binds directly (without decomposition) on one logical index
    /// of a block of type `block_type`.
    pub(crate) fn binds_directly(&self, op: &str, block_type: usize, lower: &TypeTable) -> bool {
        self.bind_one(op, &[block_type], &[0], None, lower).is_ok()
    }

    fn bind_one(
        &self,
        op: &str,
        blocks: &[usize],
        pos: &[usize],
        angle: Option<f64>,
        lower: &TypeTable,
    ) -> Result<Resolved, BindError> {
        let width: usize = blocks.iter().map(|&t| lower.caps[t]).sum();
        let gate = Clifford::named(op, pos.len()).map(|g| g.embed(pos, width));
        let mut matches: Vec<Candidate<'_>> = Vec::new();
        for s in &self.semantics {
            if s.in_place_types.as_deref() != Some(blocks) {
                continue;
            }
            if let Some(gate) = &gate {
                if s.clifford.as_ref() == Some(gate) && s.parameters.is_empty() {
                    matches.push(((s.flagged, false), s, BTreeMap::new()));
                }
                continue;
            }
            if op == "prepare" || op == "measure" {
                continue;
            }
            let [
                SemOp::Op {
                    name,
                    targets,
                    angle: a,
                },
            ] = s.steps.as_slice()
            else {
                continue;
            };
            if name != op || targets != pos {
                continue;
            }
            match (a, angle) {
                (Some(Scalar::Parameter(param)), Some(angle))
                    if s.parameters.len() == 1 && &s.parameters[0] == param =>
                {
                    let args = BTreeMap::from([(param.clone(), Argument::Number(angle))]);
                    matches.push(((s.flagged, true), s, args));
                }
                (None, None) if s.parameters.is_empty() => {
                    matches.push(((s.flagged, false), s, BTreeMap::new()));
                }
                (Some(Scalar::Literal(a)), Some(angle))
                    if s.parameters.is_empty() && is_close(*a, angle) =>
                {
                    matches.push(((s.flagged, false), s, BTreeMap::new()));
                }
                _ => {}
            }
        }
        // Rank (flagged, parameterized), lexicographic, lowest wins. Ties: prefer
        // "__quantum__qis__<op>__body", else ambiguous. ISAs that QIR programs
        // target name each instruction after the QIR intrinsic it implements,
        // so that mnemonic marks the canonical instruction for `op`.
        let best = matches
            .iter()
            .map(|(rank, _, _)| *rank)
            .min()
            .ok_or(BindError::Unbound)?;
        let mut tied: Vec<_> = matches.into_iter().filter(|(r, _, _)| *r == best).collect();
        if tied.len() > 1 {
            let preferred = format!("__quantum__qis__{op}__body");
            tied.retain(|(_, s, _)| s.mnemonic == preferred);
        }
        if tied.len() != 1 {
            return Err(BindError::Ambiguous);
        }
        let (_, s, args) = tied.pop().ok_or(BindError::Ambiguous)?;
        let select = if s.flagged {
            vec![(0..s.flag_count).map(|f| (f, false)).collect()]
        } else {
            Vec::new()
        };
        Ok(Resolved {
            index: s.index,
            args,
            select,
        })
    }
}

fn semantics(
    index: usize,
    instruction: &qodec::Instruction,
    types: &TypeTable,
) -> Option<Semantics> {
    if instruction
        .inputs
        .iter()
        .chain(&instruction.outputs)
        .any(|o| o.is_variadic)
        || instruction
            .parameters
            .iter()
            .any(|p| p.kind == ParameterKind::Pauli)
        || instruction.action.iter().any(|s| s.condition.is_some())
    {
        return None;
    }
    let side_types = |operands: &[qodec::BlockOperand]| {
        operands
            .iter()
            .map(|o| types.get(&o.block))
            .collect::<Option<Vec<_>>>()
    };
    let in_types = side_types(&instruction.inputs)?;
    let out_types = side_types(&instruction.outputs)?;
    let width_of = |ts: &[usize]| ts.iter().map(|&t| types.caps[t]).sum::<usize>();
    let width = width_of(&in_types).max(width_of(&out_types));

    let steps = sem_steps(instruction, width)?;
    let is_clifford = !steps.is_empty()
        && steps.iter().all(|s| match s {
            SemOp::Clifford => true,
            SemOp::Op { name, .. } => matches!(name.as_str(), "x" | "y" | "z"),
        });
    let clifford = if is_clifford {
        action_clifford(instruction, width)
    } else {
        None
    };
    Some(Semantics {
        index,
        mnemonic: instruction.mnemonic.clone(),
        flagged: !instruction.flags.is_empty(),
        flag_count: instruction.flags.len(),
        in_place_types: (in_types == out_types).then_some(in_types),
        parameters: instruction
            .parameters
            .iter()
            .map(|p| p.name.clone())
            .collect(),
        steps,
        clifford,
    })
}

#[derive(Debug)]
struct Semantics {
    index: usize,
    mnemonic: String,
    flagged: bool,
    flag_count: usize,
    /// Operand types, when the instruction is in place (in types == out types).
    in_place_types: Option<Vec<usize>>,
    parameters: Vec<String>,
    steps: Vec<SemOp>,
    /// Clifford composition of ALL steps; `None` if the action is empty or has
    /// any prepare, measure or rotate step.
    clifford: Option<Clifford>,
}

/// The static semantics of an action on `width` flat indices, or `None` if any
/// step has none.
fn sem_steps(instruction: &qodec::Instruction, width: usize) -> Option<Vec<SemOp>> {
    let single_positive_z = |p: &Pauli| match p.terms() {
        [(q, PauliOp::Z)] if !p.negative => Some(*q),
        _ => None,
    };
    let op = |name: &str, q: usize| SemOp::Op {
        name: name.to_string(),
        targets: vec![q],
        angle: None,
    };
    let mut steps = Vec::new();
    for step in &instruction.action {
        match &step.action {
            Action::Clifford(_) => steps.push(SemOp::Clifford),
            Action::Pauli(p) => {
                let p = PauliExpr::parse(instruction, p).ok()?.literal()?.clone();
                steps.extend(p.terms().iter().map(|&(q, pauli)| op(pauli.lower(), q)));
            }
            Action::Stabilize(paulis) => {
                for p in paulis {
                    let q = single_positive_z(&Pauli::from_text(p).ok()?)?;
                    if q >= width {
                        return None;
                    }
                    steps.push(op("prepare", q));
                }
            }
            Action::Observe(observables) => {
                for o in observables {
                    let q = single_positive_z(&Pauli::from_text(&o.pauli).ok()?)?;
                    steps.push(op("measure", q));
                }
            }
            Action::Rotate { pauli, angle } => {
                let p = Pauli::from_text(pauli).ok()?;
                let axis = p.terms().first()?.1;
                if p.negative
                    || !(1..=2).contains(&p.terms().len())
                    || p.terms().iter().any(|(_, op)| *op != axis)
                {
                    return None;
                }
                steps.push(SemOp::Op {
                    name: format!("r{}", axis.lower().repeat(p.terms().len())),
                    targets: p.support().collect(),
                    angle: Some(angle.clone()),
                });
            }
        }
    }
    Some(steps)
}

#[derive(Clone, Debug, PartialEq)]
enum SemOp {
    Clifford,
    Op {
        name: String,
        targets: Vec<usize>,
        angle: Option<Scalar>,
    },
}

type CacheKey = (String, Vec<usize>, Vec<usize>, Option<u64>);

/// Caches resolved `(op, slot types, positions, angle)` results, so repeated
/// operations cost O(1).
#[derive(Debug, Default)]
pub struct ResolveCache(FxHashMap<CacheKey, Option<Rc<Vec<Resolved>>>>);

/// A resolved lower instruction with its operand blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct LowerOp {
    /// The lower instruction.
    pub resolved: Resolved,
    /// Its lower block operands, in order.
    pub operands: Vec<u64>,
}

/// One lower instruction that implements (part of) a generic operation.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// Index into the lower ISA.
    pub index: usize,
    /// Arguments, by parameter name (the angle of a parameterized rotation).
    pub args: BTreeMap<String, Argument>,
    /// The layer cannot act on a raised flag for operations it issues itself,
    /// so it requires all flags to be zero.
    pub select: CompiledSelect,
}

/// Decompositions used when an operation does not bind directly. `y` is
/// `x·z` up to a global phase.
fn decompose(op: &str, angle: Option<f64>) -> Option<Vec<(&'static str, Option<f64>)>> {
    match (op, angle) {
        ("y", None) => Some(vec![("x", None), ("z", None)]),
        _ => None,
    }
}

#[derive(Debug)]
enum BindError {
    Unbound,
    Ambiguous,
}

/// Python's `math.isclose` defaults: relative tolerance `1e-9`, no absolute
/// tolerance, so `0.0` only matches `0.0`. The tolerance is a convention, not
/// measured or required by the spec; it only needs to absorb floating-point
/// rounding between a requested angle and a literal angle in a lower action.
fn is_close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}
