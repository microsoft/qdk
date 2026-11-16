// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ActionTranslator`]: a layer that runs the bottom ISA by its actions, on
//! an [`ActionBackend`] such as a simulator.

use super::{
    action::{ActionStep, Angle, PauliExpr, Predicate, StepAction},
    actions::{ActionOp, Generator, Guard, Qubit},
    backend::Backend,
    isa::{BoundOperand, Isa, ParsedCall, Signature},
    layer::Layer,
    pauli::Pauli,
    protocol::{BasicReadout, Call, OwnedParam, Status},
};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Answers a batch of bottom-ISA calls by translating each call's action into
/// [`ActionOp`]s on numbered qubits, and running the whole batch on the
/// [`ActionBackend`](crate::ActionBackend) below, once.
///
/// ```text
///   │ Vec<Call>   bottom ISA       ▲ Result<Vec<BasicReadout>, String>
///  ┌▼──────────────────────────────┴┐
///  │ ActionTranslator               │  live blocks: label → type, qubits
///  └┬──────────────────────────────▲┘
///   │ Vec<ActionOp>                │ Result<Vec<bool>, String>   outcomes
///   ▼ an ActionBackend
/// ```
///
/// Stacked on an action backend, it is a [`BatchBackend`](crate::BatchBackend)
/// that a [`Runtime`](crate::Runtime) can drive.
///
/// Each action step becomes the [`ActionOp`] of the same name. The qubits
/// follow the spec's [flat index space](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#instruction-structure):
///
/// - A boundary index (below the larger of the input and output widths) is
///   the input's qubit there, or a fresh qubit past the inputs.
/// - A temporary (any other index) is a fresh qubit that an unconditional
///   `stabilize` introduces, one index at a time; another step may only use
///   it after that.
/// - After the call, each output holds the qubits of its flat indices.
///   Every other qubit of the call is released (traced out), and so are the
///   qubits of a block that an output-only operand overwrites.
///
/// Flags are ideal: always 0. Results carry no status.
pub struct ActionTranslator {
    isa: Arc<Isa>,
    /// Live blocks: label → (type, qubits).
    blocks: FxHashMap<u64, (usize, Vec<Qubit>)>,
    /// Released qubits, to reuse.
    free: Vec<Qubit>,
    /// The number of qubits ever used.
    qubits: Qubit,
}

/// The qubits of one call: its boundary, then the temporaries introduced so
/// far, by flat index.
struct Flat {
    boundary: Vec<Qubit>,
    temporaries: FxHashMap<usize, Qubit>,
}

impl Flat {
    fn at(&self, i: usize) -> Result<Qubit, String> {
        self.boundary
            .get(i)
            .or_else(|| self.temporaries.get(&i))
            .copied()
            .ok_or_else(|| {
                format!(
                    "flat index {i} is past the boundary, and no earlier unconditional `stabilize` introduces it"
                )
            })
    }

    fn place(&self, p: &Pauli) -> Result<Pauli, String> {
        let terms = p
            .terms()
            .iter()
            .map(|&(i, op)| Ok((self.at(i)?, op)))
            .collect::<Result<Vec<_>, String>>()?;
        Pauli::new(p.negative, terms)
    }
}

impl ActionTranslator {
    /// A translator for `isa`, the bottom ISA, with no live block.
    #[must_use]
    pub fn new(isa: &Arc<Isa>) -> Self {
        Self {
            isa: Arc::clone(isa),
            blocks: FxHashMap::default(),
            free: Vec::new(),
            qubits: 0,
        }
    }

    /// The qubits of the live block `label`, if any.
    #[must_use]
    pub fn qubits_of(&self, label: u64) -> Option<&[Qubit]> {
        self.blocks.get(&label).map(|(_, q)| q.as_slice())
    }

    /// The number of qubits ever in use at once: the qubits an action
    /// backend needs.
    #[must_use]
    pub fn qubits_used(&self) -> usize {
        self.qubits
    }

    fn allocate(&mut self, ops: &mut Vec<ActionOp>) -> Qubit {
        let q = self.free.pop().unwrap_or_else(|| {
            self.qubits += 1;
            self.qubits - 1
        });
        ops.push(ActionOp::Allocate(q));
        q
    }

    fn release(&mut self, qubits: impl IntoIterator<Item = Qubit>, ops: &mut Vec<ActionOp>) {
        for q in qubits {
            ops.push(ActionOp::Release(q));
            self.free.push(q);
        }
    }

    /// Appends the operations of `call`, whose outcomes come after the
    /// `measured` outcomes of the batch so far, and returns how many it
    /// measures.
    fn translate(
        &mut self,
        call: &Call,
        measured: usize,
        ops: &mut Vec<ActionOp>,
    ) -> Result<usize, String> {
        let isa = Arc::clone(&self.isa);
        let parsed = isa.parse_call(call.instruction_id, &call.parameters)?;
        let sig = &isa.instructions[parsed.index];
        let known: FxHashMap<u64, usize> = parsed
            .operands
            .iter()
            .filter_map(|l| self.blocks.get(l).map(|(t, _)| (*l, *t)))
            .collect();
        let (ins, outs) = sig.bind(&isa.types, &parsed.operands, &known)?;

        // The boundary: the inputs' qubits, then fresh ones.
        let mut flat = Flat {
            boundary: Vec::new(),
            temporaries: FxHashMap::default(),
        };
        for b in &ins {
            match self.blocks.get(&b.label) {
                Some((_, qubits)) if qubits.len() == b.width => {
                    flat.boundary.extend(qubits);
                }
                _ => return Err(format!("block {} is not live", b.label)),
            }
        }
        let width = |side: &[BoundOperand<u64>]| side.last().map_or(0, |b| b.offset + b.width);
        while flat.boundary.len() < width(&outs) {
            let q = self.allocate(ops);
            flat.boundary.push(q);
        }

        let mut outcomes = 0;
        for step in &sig.action {
            outcomes += self.step(step, sig, &parsed, &mut flat, measured, ops)?;
        }
        if outcomes != sig.outcomes {
            return Err(format!(
                "`{}` measured {outcomes} outcomes, but declares {}",
                sig.mnemonic, sig.outcomes
            ));
        }

        // What ends: boundary qubits no output holds, temporaries, and
        // blocks that output-only operands overwrite.
        let mut kept = vec![false; flat.boundary.len()];
        let mut given = Vec::with_capacity(outs.len());
        for b in &outs {
            kept[b.offset..b.offset + b.width].fill(true);
            given.push((
                b.label,
                b.block_type,
                flat.boundary[b.offset..b.offset + b.width].to_vec(),
            ));
        }
        for b in &ins {
            self.blocks.remove(&b.label);
        }
        let mut temporaries: Vec<(usize, Qubit)> = flat.temporaries.into_iter().collect();
        temporaries.sort_unstable();
        let ended: Vec<Qubit> = flat
            .boundary
            .iter()
            .zip(&kept)
            .filter(|(_, kept)| !**kept)
            .map(|(&q, _)| q)
            .chain(temporaries.into_iter().map(|(_, q)| q))
            .collect();
        self.release(ended, ops);
        for (label, block_type, qubits) in given {
            if let Some((_, old)) = self.blocks.insert(label, (block_type, qubits)) {
                self.release(old, ops);
            }
        }
        Ok(outcomes)
    }

    /// Appends the operation of one action step of `sig`, called as `call`
    /// on `flat`; the call's outcomes start at index `base` in the batch.
    /// Returns how many outcomes the step measures.
    fn step(
        &mut self,
        step: &ActionStep,
        sig: &Signature,
        call: &ParsedCall,
        flat: &mut Flat,
        base: usize,
        ops: &mut Vec<ActionOp>,
    ) -> Result<usize, String> {
        let guard = match &step.condition {
            None => None,
            Some(condition) => {
                let mut outcomes = Vec::new();
                let mut constant = false;
                for p in &condition.predicates {
                    match p {
                        Predicate::Outcome(i) => outcomes.push(base + i),
                        Predicate::Param(name) => constant ^= bit(call, name)?,
                    }
                }
                let invert = condition.invert ^ constant;
                if outcomes.is_empty() {
                    // Decided by the arguments alone.
                    if !invert {
                        return Ok(0);
                    }
                    None
                } else {
                    Some(Guard { outcomes, invert })
                }
            }
        };
        let resolve = |p: &PauliExpr| p.resolve(&call.args);
        let op = match &step.action {
            StepAction::Clifford(map) => {
                let mut images: Vec<(Generator, Pauli)> = map
                    .iter()
                    .map(|(&(i, is_x), image)| {
                        let q = flat.at(i)?;
                        let g = if is_x {
                            Generator::X(q)
                        } else {
                            Generator::Z(q)
                        };
                        Ok((g, flat.place(image)?))
                    })
                    .collect::<Result<_, String>>()?;
                images.sort_unstable_by_key(|(g, _)| *g);
                ActionOp::Clifford { images, guard }
            }
            StepAction::Pauli(p) => ActionOp::Pauli {
                pauli: flat.place(&resolve(p)?)?,
                guard,
            },
            StepAction::Observe(paulis) => {
                let paulis = paulis
                    .iter()
                    .map(|p| flat.place(&resolve(p)?))
                    .collect::<Result<Vec<_>, _>>()?;
                let n = paulis.len();
                ops.push(ActionOp::Observe(paulis));
                return Ok(n);
            }
            StepAction::Rotate { pauli, angle } => ActionOp::Rotate {
                pauli: flat.place(&resolve(pauli)?)?,
                angle: match angle {
                    Angle::Literal(a) => *a,
                    Angle::Param(i) => number(call, &sig.parameters[*i].name)?,
                },
                guard,
            },
            StepAction::Stabilize(paulis) => {
                let paulis = paulis.iter().map(resolve).collect::<Result<Vec<_>, _>>()?;
                // An unconditional `stabilize` introduces the temporaries it
                // names.
                if !step.guarded() {
                    for i in paulis.iter().flat_map(Pauli::support) {
                        if i >= flat.boundary.len() && !flat.temporaries.contains_key(&i) {
                            let q = self.allocate(ops);
                            flat.temporaries.insert(i, q);
                        }
                    }
                }
                ActionOp::Stabilize {
                    paulis: paulis
                        .iter()
                        .map(|p| flat.place(p))
                        .collect::<Result<_, _>>()?,
                    guard,
                }
            }
        };
        ops.push(op);
        Ok(0)
    }
}

/// The value of `bit` parameter `name`.
fn bit(call: &ParsedCall, name: &str) -> Result<bool, String> {
    match call.args.get(name) {
        Some(OwnedParam::Bool(b)) => Ok(*b),
        Some(OwnedParam::Int(i)) => Ok(*i != 0),
        _ => Err(format!("missing `bit` argument `{name}`")),
    }
}

/// The value of `number` or `integer` parameter `name`.
#[allow(clippy::cast_precision_loss)]
fn number(call: &ParsedCall, name: &str) -> Result<f64, String> {
    match call.args.get(name) {
        Some(OwnedParam::Float(f)) => Ok(*f),
        Some(OwnedParam::Int(i)) => Ok(*i as f64),
        _ => Err(format!("missing number argument `{name}`")),
    }
}

impl Layer for ActionTranslator {
    type Input = Vec<Call>;
    type Output = Result<Vec<BasicReadout>, String>;
    type LowerInput = Vec<ActionOp>;
    type LowerOutput = Result<Vec<bool>, String>;

    fn execute(
        &mut self,
        calls: Vec<Call>,
        below: &mut dyn Backend<Vec<ActionOp>, Result<Vec<bool>, String>>,
    ) -> Self::Output {
        let mut ops = Vec::new();
        // Per call with a result: its first outcome, outcomes and flags.
        let mut shapes = Vec::new();
        let mut measured = 0;
        for call in &calls {
            let n = self.translate(call, measured, &mut ops)?;
            if let Some((outcomes, flags)) = self.isa.shape(call.instruction_id) {
                shapes.push((measured, outcomes, flags));
            }
            measured += n;
        }
        let outcomes = if ops.is_empty() {
            Vec::new()
        } else {
            below.execute(ops)?
        };
        if outcomes.len() != measured {
            return Err(format!(
                "the action backend measured {} outcomes, not {measured}",
                outcomes.len()
            ));
        }
        Ok(shapes
            .into_iter()
            .map(|(at, n, flags)| {
                let bits: Vec<Option<bool>> =
                    outcomes[at..at + n].iter().map(|&b| Some(b)).collect();
                BasicReadout::new(&bits, &vec![Some(false); flags], Status::default())
            })
            .collect())
    }
}
