// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The reference decoder: the Pauli-frame decoder.
//!
//! It evaluates a gadget's checks, readouts and frames as one system of parity
//! equations over GF(2), and carries every block's stabilizer and logical
//! signs from one gadget's `out` to the next gadget's `in`, as the spec's
//! [frames] section describes. It never pends and never corrects, so it is a
//! [`Decode`] that [`Passive`] turns into a decoder. A contradiction is
//! reported as the recoverable status `inconsistent`.
//!
//! [frames]: https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/gadget.md#frames

use super::{
    action::{ActionStep, CliffordMap, ParsedPauli, StepAction, action_clifford, action_extent},
    error::PlanError,
    layer::Blueprint,
    layer_model::{EncodingModel, FrameKey, GadgetModel, LayerModel},
    parity::{Basis, BinarySystem, Inconsistent, Parity, Side, Var},
    passive::{Decode, Passive, RecordLayout},
    pauli::{Clifford, DensePauli, Pauli, PauliOp},
    protocol::{BasicReadout, Bit, Status},
    protocol::{BlockRef, Invocation, Record},
    select::{self, SelectPolicy},
};
use rustc_hash::FxHashMap;
use std::rc::Rc;

/// Output logical sign -> parity of input signs it carries.
pub type Transport = Vec<(FrameKey, Parity)>;

/// What the frame decoder needs for one gadget. Its parities are over the
/// variables of [`Var`]: the gadget's record, its readouts, and the signs of
/// its encodings.
#[derive(Clone, Debug)]
pub struct FrameGadget {
    /// The implemented instruction's observe outcomes.
    pub outcomes: usize,
    /// The implemented instruction's flags.
    pub flags: usize,
    /// The gadget's checks: each parity is 0 without errors.
    pub checks: Vec<Parity>,
    /// The gadget's readouts: the outcomes, then the flags.
    pub readouts: Vec<Parity>,
    /// The gadget's `frames`: per output logical sign, the parity of circuit
    /// readouts that flips it.
    pub frames: FxHashMap<FrameKey, Parity>,
    /// The code of each `in` operand.
    pub inputs: Vec<CodeShape>,
    /// The code of each `out` operand.
    pub outputs: Vec<CodeShape>,
    /// The sign transport, when it is fixed for every invocation. Otherwise
    /// the decoder derives it per invocation from `action`.
    pub transport: Option<Transport>,
    /// The implemented instruction's action.
    pub action: Vec<ActionStep>,
    /// The largest flat index the action addresses through literal Paulis,
    /// plus one.
    pub action_extent: usize,
}

impl FrameGadget {
    /// What the frame decoder needs for `gadget`, of `layer`.
    pub fn plan(gadget: &GadgetModel, layer: &LayerModel) -> Result<Self, String> {
        let instruction = &layer.upper.instructions[gadget.instruction];
        if gadget.readouts.len() != instruction.outcomes + instruction.flags {
            return Err("needs one readout per observe outcome and flag".to_string());
        }
        let shape = |e: &EncodingModel| CodeShape {
            stabilizers: e.code.stabilizers.len(),
            logicals: e.code.x.len(),
        };
        Ok(Self {
            outcomes: instruction.outcomes,
            flags: instruction.flags,
            checks: gadget.checks.clone(),
            readouts: gadget.readouts.clone(),
            frames: gadget.frames.clone(),
            inputs: gadget.inputs.iter().map(shape).collect(),
            outputs: gadget.outputs.iter().map(shape).collect(),
            transport: circuit_transport(gadget, layer)?,
            action: instruction.action.clone(),
            action_extent: action_extent(&instruction.action),
        })
    }
}

// ── Logical-sign transport from a Clifford-only body ────────────────────────

/// The logical-sign transport of a gadget with a Clifford-only body: for every
/// output logical X and Z sign, the parity of input signs it carries.
///
/// Returns `None` unless the gadget has inputs and outputs and no instruction
/// parameters, and every circuit call is an in-place (in == out) instruction
/// whose action has only unguarded `clifford` and literal `pauli` steps.
/// Fails if some output logical operator is not, up to sign, a product of the
/// circuit's images of the input stabilizers and logical operators.
fn circuit_transport(
    gadget: &GadgetModel,
    layer: &LayerModel,
) -> Result<Option<Transport>, String> {
    let implemented = &layer.upper.instructions[gadget.instruction];
    if gadget.inputs.is_empty() || gadget.outputs.is_empty() || !implemented.parameters.is_empty() {
        return Ok(None);
    }
    let lower = &layer.lower;
    // Give each label a range of qubit positions, one per qubit of its lower
    // block.
    let mut start = Vec::with_capacity(gadget.labels.len());
    let mut width = 0;
    for &t in &gadget.label_types {
        start.push(width);
        width += lower.types.caps[t];
    }
    let mut u = Clifford::identity(width);
    for call in &gadget.body {
        let instruction = &lower.instructions[call.instruction];
        if instruction.inputs != instruction.outputs {
            return Ok(None);
        }
        let (ins, _) = instruction.bind(&lower.types, &call.operands, &FxHashMap::default())?;
        let positions: Vec<usize> = ins
            .iter()
            .flat_map(|b| (0..b.width).map(|i| start[b.label] + i))
            .collect();
        let Some(c) = action_clifford(&instruction.action, positions.len()) else {
            return Ok(None);
        };
        u = u.then(&c.embed(&positions, width));
    }

    let place = |p: &Pauli, encoding: &EncodingModel| -> Result<DensePauli, String> {
        let positions: Vec<usize> = encoding
            .labels
            .iter()
            .flat_map(|&label| {
                let (from, cap) = (start[label], lower.types.caps[gadget.label_types[label]]);
                (0..cap).map(move |i| from + i)
            })
            .collect();
        let mut d = DensePauli::identity(width);
        for &(q, op) in p.terms() {
            let pos = *positions.get(q).ok_or("code Pauli outside its encoding")?;
            d.x[pos] = op.has_x();
            d.z[pos] = op.has_z();
        }
        Ok(d)
    };

    // The images under `u` of every input stabilizer and logical operator,
    // each tagged with its input sign variable in `vars`.
    let mut gens = Gf2Basis::default();
    let mut vars = Vec::new();
    for (e, encoding) in gadget.inputs.iter().enumerate() {
        for basis in [Basis::Stabilizers, Basis::X, Basis::Z] {
            for (k, p) in encoding.code.paulis(basis).iter().enumerate() {
                let image = u.conjugate(&place(p, encoding)?);
                let mut combo = vec![false; vars.len() + 1];
                combo[vars.len()] = true;
                vars.push(Var::Enc {
                    side: Side::In,
                    e,
                    basis,
                    k,
                });
                gens.add(image.bits(), combo);
            }
        }
    }
    let mut transport = Vec::new();
    for (e, encoding) in gadget.outputs.iter().enumerate() {
        for basis in [Basis::X, Basis::Z] {
            for (k, p) in encoding.code.paulis(basis).iter().enumerate() {
                // Find the input generators whose product is this output
                // logical operator `p`, up to sign.
                let combo = gens
                    .solve(place(p, encoding)?.bits())
                    .ok_or("circuit does not preserve the code boundary")?;
                let mut t = Parity::default();
                for (i, &used) in combo.iter().enumerate() {
                    if used {
                        t ^= Parity::var(vars[i]);
                    }
                }
                transport.push(((e, basis, k), t));
            }
        }
    }
    Ok(Some(transport))
}

/// Incremental GF(2) row basis that tracks which inputs make up each row.
///
/// `add` reduces every new row by all earlier rows, so no row has a bit set at
/// an earlier row's pivot, and one pass over `rows` in insertion order fully
/// reduces a vector. `BinarySystem` relies on the same invariant.
#[derive(Default)]
struct Gf2Basis {
    /// (pivot, row, which inputs XOR to the row).
    rows: Vec<(usize, Vec<bool>, Vec<bool>)>,
}

impl Gf2Basis {
    fn add(&mut self, mut v: Vec<bool>, mut combo: Vec<bool>) {
        for (pivot, row, row_combo) in &self.rows {
            if v[*pivot] {
                xor_into(&mut v, row);
                xor_into(&mut combo, row_combo);
            }
        }
        if let Some(pivot) = v.iter().position(|&b| b) {
            self.rows.push((pivot, v, combo));
        }
    }

    fn solve(&self, mut target: Vec<bool>) -> Option<Vec<bool>> {
        let mut combo = Vec::new();
        for (pivot, row, row_combo) in &self.rows {
            if target[*pivot] {
                xor_into(&mut target, row);
                xor_into(&mut combo, row_combo);
            }
        }
        target.iter().all(|b| !b).then_some(combo)
    }
}

fn xor_into(a: &mut Vec<bool>, b: &[bool]) {
    if a.len() < b.len() {
        a.resize(b.len(), false);
    }
    for (x, y) in a.iter_mut().zip(b) {
        *x ^= *y;
    }
}

/// How many operators of each kind a code has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeShape {
    /// Stabilizer generators.
    pub stabilizers: usize,
    /// Logical qubits: logical X operators, and as many logical Z ones.
    pub logicals: usize,
}

impl CodeShape {
    /// The number of operators of a kind.
    #[must_use]
    pub fn len(&self, basis: Basis) -> usize {
        match basis {
            Basis::Stabilizers => self.stabilizers,
            Basis::X | Basis::Z => self.logicals,
        }
    }
}

/// The blueprint of the frame decoder of one layer. It instantiates a fresh
/// [`FrameDecoder`] per shot, wrapped in [`Passive`].
pub struct FrameDecoderPlan {
    layout: Rc<RecordLayout>,
    /// Per gadget, indexed by [`GadgetId`](crate::GadgetId).
    gadgets: Rc<Vec<FrameGadget>>,
    policy: SelectPolicy,
}

impl FrameDecoderPlan {
    /// The frame decoder of the layer whose records `layout` describes, with
    /// `gadgets` indexed by [`GadgetId`](crate::GadgetId).
    #[must_use]
    pub fn new(layout: Rc<RecordLayout>, gadgets: Vec<FrameGadget>) -> Self {
        Self {
            layout,
            gadgets: Rc::new(gadgets),
            policy: select::recoverable,
        }
    }

    /// Plans the checks, readouts and sign transports of every gadget of
    /// `layer`.
    pub fn plan(layer: &LayerModel) -> Result<Self, PlanError> {
        let gadgets = layer
            .gadgets
            .iter()
            .map(|gadget| {
                FrameGadget::plan(gadget, layer)
                    .map_err(|e| PlanError(format!("gadget `{}`: {e}", gadget.name)))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self::new(Rc::new(RecordLayout::plan(layer)), gadgets))
    }

    /// Reads `select` hints with `policy` instead (see [`Passive`]).
    #[must_use]
    pub fn with_select_policy(mut self, policy: SelectPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// A fresh frame decoder for one shot, without the [`Passive`] wrapper,
    /// for decoders that build on it.
    #[must_use]
    pub fn decoder(&self) -> FrameDecoder {
        FrameDecoder {
            gadgets: Rc::clone(&self.gadgets),
            signs: FxHashMap::default(),
        }
    }
}

impl Blueprint for FrameDecoderPlan {
    type Instance = Passive<FrameDecoder>;

    fn instantiate(&self) -> Passive<FrameDecoder> {
        Passive::new(Rc::clone(&self.layout), self.decoder()).with_select_policy(self.policy)
    }
}

/// The per-shot state of the frame decoder.
pub struct FrameDecoder {
    gadgets: Rc<Vec<FrameGadget>>,
    /// Per output block not yet released: the signs its decode determined,
    /// keyed by (basis, index in that basis).
    signs: FxHashMap<BlockRef, FxHashMap<(Basis, usize), bool>>,
}

impl Decode for FrameDecoder {
    fn decode(&mut self, record: &Record) -> BasicReadout {
        let decoded = match &record.invocation {
            Some(inv) => self.decode_invocation(inv, &record.bits),
            None => BasicReadout::default(),
        };
        for r in record.ended() {
            self.signs.remove(r);
        }
        decoded
    }
}

impl FrameDecoder {
    fn decode_invocation(&mut self, inv: &Invocation, raw: &[Bit]) -> BasicReadout {
        let gadgets = Rc::clone(&self.gadgets);
        let Some(p) = gadgets.get(inv.gadget.0) else {
            return BasicReadout {
                status: Status::UNRESOLVED,
                ..BasicReadout::default()
            };
        };
        let n = p.outcomes;
        let unknown = |status| BasicReadout {
            outcomes: vec![None; n],
            flags: vec![None; p.flags],
            status,
        };
        let mut sys = BinarySystem::new();
        let mut st = Status::default();
        if self.build_system(p, inv, raw, &mut sys, &mut st).is_err() {
            // Recoverable: every bit and every output sign becomes unknown.
            return unknown(st | Status::INCONSISTENT);
        }

        for (e, block_ref) in inv.output_refs.iter().enumerate() {
            let code = &p.outputs[e];
            let mut signs = FxHashMap::default();
            for basis in [Basis::Stabilizers, Basis::X, Basis::Z] {
                for k in 0..code.len(basis) {
                    let var = Var::Enc {
                        side: Side::Out,
                        e,
                        basis,
                        k,
                    };
                    if let Some(v) = sys.value(Parity::var(var)) {
                        signs.insert((basis, k), v);
                    }
                }
            }
            self.signs.insert(block_ref.clone(), signs);
        }

        let mut values: Vec<Bit> = (0..p.readouts.len())
            .map(|i| sys.value(Parity::var(Var::Ro(i))))
            .collect();
        let flags = values.split_off(n);
        BasicReadout {
            outcomes: values,
            flags,
            status: st,
        }
    }

    fn build_system(
        &self,
        p: &FrameGadget,
        inv: &Invocation,
        raw: &[Bit],
        sys: &mut BinarySystem,
        st: &mut Status,
    ) -> Result<(), Inconsistent> {
        for check in &p.checks {
            sys.add(check.clone())?;
        }
        for (i, v) in raw.iter().enumerate() {
            if let Some(v) = v {
                sys.add(Parity::var(Var::Rec(i)) ^ Parity::constant(*v))?;
            }
        }
        // Signs that the decodes of earlier gadgets determined for the inputs.
        // The order of `add` calls doesn't change any result, because the system
        // has the same solutions in any order. Sorting only makes the rows
        // independent of the hash map's iteration order.
        for (e, block_ref) in inv.input_refs.iter().enumerate() {
            if let Some(signs) = self.signs.get(block_ref) {
                let mut signs: Vec<_> = signs.iter().collect();
                signs.sort_unstable();
                for (&(basis, k), &v) in signs {
                    let var = Var::Enc {
                        side: Side::In,
                        e,
                        basis,
                        k,
                    };
                    sys.add(Parity::var(var) ^ Parity::constant(v))?;
                }
            }
        }
        // Define readouts[i].
        for (i, eq) in p.readouts.iter().enumerate() {
            sys.add(Parity::var(Var::Ro(i)) ^ eq)?;
        }
        let computed;
        let transport = if let Some(t) = &p.transport {
            t
        } else {
            computed = if let Ok(t) = action_transport(p, inv, sys) {
                t
            } else {
                // Recoverable: output logical signs stay unknown.
                *st |= Status::UNRESOLVED;
                Vec::new()
            };
            &computed
        };
        // Each output logical sign is the XOR of the input signs it carries
        // (`t`) and the gadget's frame for it, if any. A missing frame entry
        // adds nothing; it never resets the carried sign.
        for (key @ (e, basis, k), t) in transport {
            let mut row = Parity::var(Var::Enc {
                side: Side::Out,
                e: *e,
                basis: *basis,
                k: *k,
            }) ^ t;
            if let Some(frame) = p.frames.get(key) {
                row ^= frame;
            }
            sys.add(row)?;
        }
        Ok(())
    }
}

// ── Logical-sign transport from the instruction action ─────────────────────

/// The logical-sign transport of one invocation, derived from the implemented
/// instruction's action instead of the gadget body. Fails if a guarded or
/// `stabilize` step touches a qubit whose frame `sys` does not determine to be
/// zero, if `sys` does not determine that a `rotate` step's Pauli commutes with
/// the frame, or if a Pauli of the action cannot be resolved.
///
/// Each logical qubit q carries a pair `(z_sign, x_sign)` of parities over the
/// input sign variables. `z_sign` is flipped by an X-type frame on q; `x_sign`
/// by a Z-type frame.
fn action_transport(
    gadget: &FrameGadget,
    inv: &Invocation,
    sys: &BinarySystem,
) -> Result<Transport, Unresolved> {
    let logical = |codes: &[CodeShape]| codes.iter().map(|c| c.logicals).sum::<usize>();
    let width = logical(&gadget.inputs)
        .max(logical(&gadget.outputs))
        .max(gadget.action_extent);
    // Fresh and temporary qubits start at (0, 0).
    let mut s: Vec<(Parity, Parity)> = vec![Default::default(); width];
    let mut q = 0;
    for (e, code) in gadget.inputs.iter().enumerate() {
        for k in 0..code.logicals {
            let var = |basis| {
                Parity::var(Var::Enc {
                    side: Side::In,
                    e,
                    basis,
                    k,
                })
            };
            s[q] = (var(Basis::Z), var(Basis::X));
            q += 1;
        }
    }
    let known_zero = |p: &Parity| sys.value(p.clone()) == Some(false);
    let resolve = |pauli: &ParsedPauli| {
        pauli
            .as_ref()
            .map_err(|_| Unresolved)?
            .resolve(&inv.args)
            .map_err(|_| Unresolved)
    };

    for step in &gadget.action {
        if step.guarded {
            // The step may or may not apply; that is harmless only when the
            // qubits it touches carry no frame.
            let touched = step_support(&step.action, &resolve)?;
            if touched
                .iter()
                .any(|&q| q >= width || !known_zero(&s[q].0) || !known_zero(&s[q].1))
            {
                return Err(Unresolved);
            }
            continue;
        }
        match &step.action {
            StepAction::Clifford { map, .. } => {
                let map = map.as_ref().map_err(|_| Unresolved)?;
                s = conjugate_frame(&s, map)?;
            }
            StepAction::Stabilize(paulis) => {
                for pauli in paulis {
                    for q in resolve(pauli)?.support() {
                        let (z_sign, x_sign) = s.get(q).ok_or(Unresolved)?;
                        if !known_zero(z_sign) || !known_zero(x_sign) {
                            return Err(Unresolved);
                        }
                    }
                }
            }
            StepAction::Rotate { pauli, .. } => {
                // A frame that anticommutes with the rotation's Pauli would
                // flip the sign of its angle. `acc` is that anticommutation.
                let mut acc = Parity::default();
                for &(q, op) in resolve(pauli)?.terms() {
                    let (z_sign, x_sign) = s.get(q).ok_or(Unresolved)?;
                    if op.has_z() {
                        acc ^= z_sign;
                    }
                    if op.has_x() {
                        acc ^= x_sign;
                    }
                }
                if !known_zero(&acc) {
                    return Err(Unresolved);
                }
            }
            StepAction::Pauli(_) | StepAction::Observe(_) => {}
        }
    }

    let mut transport = Vec::new();
    let mut offset = 0;
    for (e, code) in gadget.outputs.iter().enumerate() {
        for k in 0..code.logicals {
            let (z_sign, x_sign) = &s[offset + k];
            transport.push(((e, Basis::Z, k), z_sign.clone()));
            transport.push(((e, Basis::X, k), x_sign.clone()));
        }
        offset += code.logicals;
    }
    Ok(transport)
}

/// Moves a frame through a Clifford: an X-type frame on q (which flips
/// `z_sign`) becomes `C(X_q)`, and a Z-type frame becomes `C(Z_q)`.
fn conjugate_frame(
    s: &[(Parity, Parity)],
    map: &CliffordMap,
) -> Result<Vec<(Parity, Parity)>, Unresolved> {
    let mut next: Vec<(Parity, Parity)> = vec![Default::default(); s.len()];
    for (q, (z_sign, x_sign)) in s.iter().enumerate() {
        for (v, is_x) in [(z_sign, true), (x_sign, false)] {
            if *v == Parity::default() {
                continue;
            }
            let identity = Pauli::single(q, if is_x { PauliOp::X } else { PauliOp::Z });
            let image = map.get(&(q, is_x)).unwrap_or(&identity);
            for &(t, op) in image.terms() {
                let target = next.get_mut(t).ok_or(Unresolved)?;
                if op.has_x() {
                    target.0 ^= v;
                }
                if op.has_z() {
                    target.1 ^= v;
                }
            }
        }
    }
    Ok(next)
}

/// The flat indices an action step touches.
fn step_support(
    action: &StepAction,
    resolve: &impl Fn(&ParsedPauli) -> Result<Pauli, Unresolved>,
) -> Result<Vec<usize>, Unresolved> {
    let mut support = Vec::new();
    match action {
        StepAction::Clifford { images, .. } => {
            for (generator, image) in images {
                support.extend(resolve(generator)?.support());
                support.extend(resolve(image)?.support());
            }
        }
        StepAction::Pauli(p) | StepAction::Rotate { pauli: p, .. } => {
            support.extend(resolve(p)?.support());
        }
        StepAction::Observe(paulis) | StepAction::Stabilize(paulis) => {
            for p in paulis {
                support.extend(resolve(p)?.support());
            }
        }
    }
    Ok(support)
}

/// An output logical sign cannot be transported.
struct Unresolved;
