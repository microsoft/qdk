// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lower-ISA step resolver.
//!
//! An encoder applies [`Step`]s of its own: corrections from its decoder, and
//! the code's logical operator for an upper Pauli instruction without a
//! gadget. A step says what to do in the spec's own vocabulary of action steps
//! (a Pauli, a Clifford, a rotation), never by name: as the spec's
//! [design principles][design-principles] put it, "the action is
//! authoritative". So the resolver matches each step against the actions of
//! the LOWER ISA.
//!
//! [design-principles]: https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#design-principles

use super::{
    action::{Angle, PauliExpr, StepAction, action_clifford},
    encoder::Fixes,
    isa::{Isa, ParamKind, Signature, TypeTable},
    pauli::{Clifford, Pauli, PauliOp},
    protocol::{Call, Fault, OwnedParam, Step},
    select::CompiledSelect,
};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::sync::{Arc, Mutex, PoisonError};

/// Matches steps against the static semantics of a lower ISA.
#[derive(Debug)]
pub struct Resolver {
    semantics: Vec<Semantics>,
}

impl Resolver {
    /// Indexes the instructions of `isa` whose actions have static semantics,
    /// i.e. do not depend on a call's arguments or outcomes: no variadic
    /// operands, no `pauli` parameters and no guarded steps. Other instructions
    /// never match.
    #[must_use]
    pub fn new(isa: &Isa) -> Self {
        let semantics = isa
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| semantics(index, instruction, &isa.types))
            .collect();
        Self { semantics }
    }

    /// Resolves `step` to lower instructions. Qubit `i` of `step` is
    /// `slots[i]`, a (lower block id, logical index in that block) pair.
    ///
    /// The distinct blocks of `slots`, in first-appearance order, become the
    /// operands of the lower instruction, which must be in place on them. For
    /// example, a CX from step qubit 0 to step qubit 1 on slots
    /// `[(7, 1), (4, 0)]`, where blocks 7 and 4 each hold 2 logical qubits,
    /// needs an instruction on blocks `[7, 4]` whose action is a CX from flat
    /// index 1 to flat index 2.
    ///
    /// A Pauli that no single instruction implements is applied one qubit at a
    /// time, each on its own block, and a `Y` as an `X` then a `Z` (equal up to
    /// a global phase). Returns `None` if two slots are the same qubit, or if
    /// the lower ISA cannot implement `step` on them.
    pub fn resolve(
        &self,
        step: &Step,
        slots: &[(u64, usize)],
        type_of: impl Fn(u64) -> usize + Copy,
        lower: &TypeTable,
        cache: &mut ResolveCache,
    ) -> Option<Vec<LowerOp>> {
        if let Some(ops) = self.resolve_whole(step, slots, type_of, lower, cache) {
            return Some(ops);
        }
        let Step::Pauli(p) = step else {
            return None;
        };
        if let [(_, PauliOp::X | PauliOp::Z)] = p.terms() {
            return None; // Nothing smaller to try.
        }
        let mut ops = Vec::new();
        for &(i, op) in p.terms() {
            let factors: &[PauliOp] = match op {
                PauliOp::Y => &[PauliOp::X, PauliOp::Z],
                PauliOp::X => &[PauliOp::X],
                PauliOp::Z => &[PauliOp::Z],
            };
            for &factor in factors {
                let single = Step::Pauli(Pauli::single(0, factor));
                ops.extend(self.resolve_whole(&single, &[slots[i]], type_of, lower, cache)?);
            }
        }
        Some(ops)
    }

    /// Resolves `step` to one lower instruction.
    fn resolve_whole(
        &self,
        step: &Step,
        slots: &[(u64, usize)],
        type_of: impl Fn(u64) -> usize,
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
        let key = (StepKey::of(step), blocks, pos);
        let resolved = if let Some(resolved) = cache.0.get(&key) {
            resolved.clone()?
        } else {
            let resolved = self.bind(step, &key.1, &key.2, width).map(Arc::new);
            cache.0.insert(key, resolved.clone());
            resolved?
        };
        Some(vec![LowerOp {
            resolved: (*resolved).clone(),
            operands: labels,
        }])
    }

    /// Whether a single lower instruction applies `op` on logical index
    /// `index` of a block of type `block_type`.
    #[must_use]
    pub fn binds_directly(
        &self,
        op: PauliOp,
        block_type: usize,
        index: usize,
        lower: &TypeTable,
    ) -> bool {
        let step = Step::Pauli(Pauli::single(0, op));
        self.bind(&step, &[block_type], &[index], lower.caps[block_type])
            .is_some()
    }

    /// Finds the lower instruction that applies `step`, with its qubit `i` at
    /// flat position `pos[i]` of blocks of types `blocks`, which span `width`
    /// flat positions. Only in-place instructions match, with input and output
    /// types both equal to `blocks`, because the targets are live blocks that
    /// must stay live with their types.
    ///
    /// Any unflagged match beats a flagged one, then a match without
    /// parameters beats one with a parameterized angle. The spec makes
    /// instructions with equal actions equivalent, so among equal matches the
    /// one the ISA declares first wins.
    fn bind(&self, step: &Step, blocks: &[usize], pos: &[usize], width: usize) -> Option<Resolved> {
        let place = |p: &Pauli| {
            Pauli::new(
                p.negative,
                p.terms().iter().map(|&(i, op)| (pos[i], op)).collect(),
            )
            .ok()
        };
        let in_place = self
            .semantics
            .iter()
            .filter(|s| s.in_place_types.as_deref() == Some(blocks));
        let clifford = |gate: Clifford| {
            in_place
                .clone()
                .filter(|s| s.clifford.as_ref() == Some(&gate) && s.parameters == 0)
                .map(|s| ((s.flagged, false), s, Vec::new()))
                .min_by_key(|(rank, s, _)| (*rank, s.index))
        };
        let best = match step {
            Step::Pauli(p) => clifford(Clifford::from_pauli(&place(p)?, width).ok()?),
            Step::Clifford { targets, gate } => {
                let at: Vec<usize> = targets.iter().map(|&t| pos[t]).collect();
                clifford(gate.embed(&at, width))
            }
            Step::Rotate { pauli, angle } => {
                let pauli = place(pauli)?;
                in_place
                    .filter_map(|s| {
                        let (p, a) = s.rotation.as_ref()?;
                        if *p != pauli {
                            return None;
                        }
                        match a {
                            Angle::Param(0) if s.parameters == 1 => {
                                Some(((s.flagged, true), s, vec![OwnedParam::Float(*angle)]))
                            }
                            Angle::Literal(a) if s.parameters == 0 && is_close(*a, *angle) => {
                                Some(((s.flagged, false), s, Vec::new()))
                            }
                            _ => None,
                        }
                    })
                    .min_by_key(|(rank, s, _)| (*rank, s.index))
            }
        };
        let (_, s, args) = best?;
        let select = if s.flagged {
            vec![(0..s.flag_count).map(|f| (f, false)).collect()]
        } else {
            Vec::new()
        };
        Some(Resolved {
            index: s.index,
            args,
            select,
        })
    }
}

fn semantics(index: usize, instruction: &Signature, types: &TypeTable) -> Option<Semantics> {
    if instruction
        .inputs
        .iter()
        .chain(&instruction.outputs)
        .any(|o| o.variadic)
        || instruction
            .parameters
            .iter()
            .any(|p| p.kind == ParamKind::Pauli)
        || instruction.action.iter().any(|s| s.guarded)
    {
        return None;
    }
    let in_types: Vec<usize> = instruction.inputs.iter().map(|o| o.block).collect();
    let out_types: Vec<usize> = instruction.outputs.iter().map(|o| o.block).collect();
    let width_of = |ts: &[usize]| ts.iter().map(|&t| types.caps[t]).sum::<usize>();
    let width = width_of(&in_types).max(width_of(&out_types));

    let clifford = if instruction.action.is_empty() {
        None
    } else {
        action_clifford(&instruction.action, width)
    };
    let rotation = match instruction.action.as_slice() {
        [step] => match &step.action {
            StepAction::Rotate {
                pauli: PauliExpr::Literal(p),
                angle,
            } if p.max_qubit().is_some_and(|q| q < width) => Some((p.clone(), angle.clone())),
            _ => None,
        },
        _ => None,
    };
    Some(Semantics {
        index,
        flagged: instruction.flags > 0,
        flag_count: instruction.flags,
        in_place_types: (in_types == out_types).then_some(in_types),
        parameters: instruction.parameters.len(),
        clifford,
        rotation,
    })
}

#[derive(Debug)]
struct Semantics {
    index: usize,
    flagged: bool,
    flag_count: usize,
    /// Operand types, when the instruction is in place (in types == out types).
    in_place_types: Option<Vec<usize>>,
    /// How many parameters it has.
    parameters: usize,
    /// The Clifford its action composes to, if the action is not empty and
    /// has only unguarded `clifford` and literal `pauli` steps inside its
    /// width.
    clifford: Option<Clifford>,
    /// The Pauli and angle of its action, if the action is one `rotate` step.
    rotation: Option<(Pauli, Angle)>,
}

/// A step, hashable: a rotation angle is keyed by its bits.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum StepKey {
    Pauli(Pauli),
    Clifford(Vec<usize>, Clifford),
    Rotate(Pauli, u64),
}

impl StepKey {
    fn of(step: &Step) -> Self {
        match step {
            Step::Pauli(p) => StepKey::Pauli(p.clone()),
            Step::Clifford { targets, gate } => StepKey::Clifford(targets.clone(), gate.clone()),
            Step::Rotate { pauli, angle } => StepKey::Rotate(pauli.clone(), angle.to_bits()),
        }
    }
}

type CacheKey = (StepKey, Vec<usize>, Vec<usize>);

/// Memoizes single-instruction resolutions by `(step, operand block types,
/// flat target positions)`, which determine the result regardless of the
/// block ids. Failures are cached too.
#[derive(Debug, Default)]
pub struct ResolveCache(FxHashMap<CacheKey, Option<Arc<Resolved>>>);

/// A resolved lower instruction with its operand blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct LowerOp {
    /// The lower instruction.
    pub resolved: Resolved,
    /// Its lower block operands, in order.
    pub operands: Vec<u64>,
}

/// One lower instruction that implements (part of) a step.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// Index into the lower ISA.
    pub index: usize,
    /// Its arguments, in parameter order (the angle of a parameterized
    /// rotation).
    pub args: Vec<OwnedParam>,
    /// The layer cannot act on a raised flag for steps it applies itself, so
    /// it requires all flags to be zero. A raised flag sets the recoverable
    /// `rejected` status instead of failing.
    pub select: CompiledSelect,
}

/// Whether two angles are equal up to floating-point rounding, with Python's
/// `math.isclose` defaults: relative tolerance `1e-9`, no absolute tolerance,
/// so `0.0` only matches `0.0`. The tolerance is a convention, not measured or
/// required by the spec; it only needs to absorb floating-point rounding
/// between a requested angle and a literal angle in a lower action.
fn is_close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}

/// The [`Fixes`] of a layer's encoders: a [`Resolver`] for the lower ISA,
/// whose results become lower calls. Results are cached for the life of the
/// plan: they depend only on the step and the lower types, never on the shot.
/// The cache is shared by every shot of the plan, on any thread.
#[derive(Debug)]
pub struct ResolverFixes {
    resolver: Arc<Resolver>,
    lower: Arc<Isa>,
    cache: Mutex<ResolveCache>,
}

impl ResolverFixes {
    /// Applies corrections with `resolver`, which indexes `lower`.
    #[must_use]
    pub fn new(resolver: Arc<Resolver>, lower: Arc<Isa>) -> Self {
        Self {
            resolver,
            lower,
            cache: Mutex::default(),
        }
    }
}

impl Fixes for ResolverFixes {
    fn resolve(
        &self,
        step: &Step,
        qubits: &[(u64, usize)],
        lower_type: &dyn Fn(u64) -> usize,
    ) -> Result<Vec<(Call, CompiledSelect)>, Fault> {
        // A thread that panicked while holding the lock can only have left
        // complete entries behind: each is inserted whole, after it is computed.
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        let parts = self
            .resolver
            .resolve(step, qubits, lower_type, &self.lower.types, &mut cache)
            .ok_or_else(|| {
                Fault::no_implementation(format!("the lower ISA cannot apply {step} there"))
            })?;
        Ok(parts
            .into_iter()
            .map(|part| {
                let mut parameters: SmallVec<[OwnedParam; 2]> = part
                    .operands
                    .iter()
                    .map(|&o| OwnedParam::Block(o))
                    .collect();
                parameters.extend(part.resolved.args);
                let call = Call {
                    instruction_id: part.resolved.index,
                    parameters,
                };
                (call, part.resolved.select)
            })
            .collect())
    }
}
