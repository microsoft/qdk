// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The reference decoder: the Pauli-frame decoder.
//!
//! It evaluates a gadget's checks, readouts and frames as one GF(2) system, and
//! carries every block's stabilizer and logical signs from one gadget's `out`
//! to the next gadget's `in`, as the spec's [frames] section describes. It
//! never yields corrections, so it is deferrable. A contradiction is reported
//! as the recoverable status `inconsistent`.
//!
//! [frames]: https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/gadget.md#frames

use super::{
    actions::{CliffordMap, PauliExpr, action_clifford, action_extent, clifford_map},
    backend::{Bit, PlanError, Status},
    decoder::{BlockRef, Decoded, Decoder, DecoderPlan, DecoderRuntime, Invocation, Step},
    parity::{Basis, BinarySystem, Inconsistent, Parity, Side, Var},
    pauli::{Clifford, DensePauli, Pauli, PauliOp},
    plan::{GadgetPlan, InstructionPlan, LayerPlan},
    spec::{EncodingPlan, FrameKey, RefScope, bind_operands, expr},
};
use qodec::Action;
use rustc_hash::FxHashMap;
use std::rc::Rc;

/// The Pauli-frame [`Decoder`].
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameDecoder;

impl<R: 'static> Decoder<R> for FrameDecoder {
    fn deferrable(&self) -> bool {
        true
    }

    fn plan(&self, layer: &Rc<LayerPlan>) -> Result<Box<dyn DecoderPlan<R>>, PlanError> {
        Ok(Box::new(FrameDecoderPlan::new(layer)?))
    }
}

/// The frame decoder planned for one layer. It instantiates a
/// [`FrameDecoderRuntime`] per shot.
pub struct FrameDecoderPlan {
    /// Per upper instruction index; `None` for instructions without a gadget.
    gadgets: Rc<Vec<Option<FrameGadgetPlan>>>,
}

impl FrameDecoderPlan {
    /// Plans the checks, readouts and transports of every gadget of `layer`.
    pub fn new(layer: &LayerPlan) -> Result<Self, PlanError> {
        let mut gadgets = Vec::with_capacity(layer.instructions.len());
        for instruction_plan in &layer.instructions {
            gadgets.push(match instruction_plan {
                InstructionPlan::Gadget(gp) => Some(
                    FrameGadgetPlan::new(gp, layer)
                        .map_err(|e| PlanError(format!("gadget `{}`: {e}", gp.name)))?,
                ),
                _ => None,
            });
        }
        Ok(Self {
            gadgets: Rc::new(gadgets),
        })
    }
}

impl<R: 'static> DecoderPlan<R> for FrameDecoderPlan {
    fn deferrable(&self) -> bool {
        true
    }

    fn instantiate(&self) -> Box<dyn DecoderRuntime<R>> {
        Box::new(FrameDecoderRuntime {
            gadgets: Rc::clone(&self.gadgets),
            signs: FxHashMap::default(),
        })
    }
}

/// The per-shot state of the frame decoder.
pub struct FrameDecoderRuntime {
    gadgets: Rc<Vec<Option<FrameGadgetPlan>>>,
    /// Known boundary signs.
    signs: FxHashMap<BlockRef, FxHashMap<(Basis, usize), bool>>,
}

impl<R> DecoderRuntime<R> for FrameDecoderRuntime {
    fn decode(&mut self, inv: &Invocation, raw: &[Bit]) -> Step<Decoded> {
        let instruction = inv.gadget.instruction();
        let n = instruction.observe_count();
        let unknown = |status| Decoded {
            outcomes: vec![None; n],
            flags: vec![None; instruction.flags.len()],
            status,
        };
        let gadgets = Rc::clone(&self.gadgets);
        let Some(Some(p)) = gadgets.get(inv.gadget.index) else {
            return Step::Done(unknown(Status::UNRESOLVED));
        };
        let mut sys = BinarySystem::new();
        let mut st = Status::default();
        if self.build_system(p, inv, raw, &mut sys, &mut st).is_err() {
            // Recoverable: every bit and every output sign becomes unknown.
            return Step::Done(unknown(st | Status::INCONSISTENT));
        }

        for (e, block_ref) in inv.output_refs.iter().enumerate() {
            let code = &inv.gadget.outputs[e].code;
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
        Step::Done(Decoded {
            outcomes: values,
            flags,
            status: st,
        })
    }

    /// Never called: this decoder never yields corrections.
    fn resume_decode(&mut self, _last: Option<&R>) -> Step<Decoded> {
        Step::Done(Decoded::default())
    }

    fn discarded(&mut self, refs: &[BlockRef]) {
        for r in refs {
            self.signs.remove(r);
        }
    }
}

impl FrameDecoderRuntime {
    fn build_system(
        &self,
        p: &FrameGadgetPlan,
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
        // Incoming frame.
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
            computed = if let Ok(t) = action_transport(inv, sys) {
                t
            } else {
                // Recoverable: output logical signs stay unknown.
                *st |= Status::UNRESOLVED;
                Vec::new()
            };
            &computed
        };
        // Carry logical signs. Missing frame entries add nothing; they never
        // reset the transported sign.
        for (key @ (e, basis, k), t) in transport {
            let mut row = Parity::var(Var::Enc {
                side: Side::Out,
                e: *e,
                basis: *basis,
                k: *k,
            }) ^ t;
            if let Some(frame) = inv.gadget.frames.get(key) {
                row ^= frame;
            }
            sys.add(row)?;
        }
        Ok(())
    }
}

/// Output logical sign -> parity of input signs it carries.
type Transport = Vec<(FrameKey, Parity)>;

/// What the frame decoder needs for one gadget, fixed once per layer.
#[derive(Debug)]
struct FrameGadgetPlan {
    checks: Vec<Parity>,
    readouts: Vec<Parity>,
    /// Fixed per gadget when the body is Clifford-only (see
    /// `circuit_transport`); otherwise derived per invocation from the
    /// instruction's action (see `action_transport`).
    transport: Option<Transport>,
}

impl FrameGadgetPlan {
    fn new(gp: &GadgetPlan, layer: &LayerPlan) -> Result<Self, String> {
        let instruction = gp.instruction();
        if gp.gadget.readouts.len() != instruction.observe_count() + instruction.flags.len() {
            return Err("needs one readout per observe outcome and flag".to_string());
        }
        let scope = RefScope {
            record_len: gp.body.record_len,
            readouts_len: gp.gadget.readouts.len(),
            inputs: &gp.inputs,
            outputs: &gp.outputs,
        };
        Ok(Self {
            checks: gp
                .gadget
                .checks
                .iter()
                .map(|c| expr(c, &scope))
                .collect::<Result<_, _>>()?,
            readouts: gp
                .gadget
                .readouts
                .iter()
                .map(|r| expr(&r.equation, &scope))
                .collect::<Result<_, _>>()?,
            transport: circuit_transport(gp, layer)?,
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
fn circuit_transport(gp: &GadgetPlan, layer: &LayerPlan) -> Result<Option<Transport>, String> {
    if gp.inputs.is_empty() || gp.outputs.is_empty() || !gp.instruction().parameters.is_empty() {
        return Ok(None);
    }
    let lower = &layer.lower;
    // Assign each circuit label a range of positions.
    let mut start: FxHashMap<&str, usize> = FxHashMap::default();
    let mut width = 0;
    for label in &gp.labels {
        start.insert(label, width);
        width += lower.caps[gp.label_type[label]];
    }
    let mut u = Clifford::identity(width);
    for pc in &gp.body.calls {
        let instruction = &layer.lower_isa.instructions[pc.instruction];
        if instruction.inputs != instruction.outputs {
            return Ok(None);
        }
        let (ins, _) = bind_operands(lower, instruction, &pc.operands, &FxHashMap::default())?;
        let positions: Vec<usize> = ins
            .iter()
            .flat_map(|b| (0..b.width).map(|i| start[b.label.as_str()] + i))
            .collect();
        let Some(c) = action_clifford(instruction, positions.len()) else {
            return Ok(None);
        };
        u = u.then(&c.embed(&positions, width));
    }

    let place = |p: &Pauli, layout: &EncodingPlan| -> Result<DensePauli, String> {
        let positions: Vec<usize> = layout
            .labels
            .iter()
            .zip(&layout.lower_types)
            .flat_map(|(label, &t)| (0..lower.caps[t]).map(|i| start[label.as_str()] + i))
            .collect();
        let mut d = DensePauli::identity(width);
        for &(q, op) in p.terms() {
            let pos = *positions.get(q).ok_or("code Pauli outside its encoding")?;
            d.x[pos] = op.has_x();
            d.z[pos] = op.has_z();
        }
        Ok(d)
    };

    // Input generator -> image under U.
    let mut gens = Gf2Basis::default();
    let mut vars = Vec::new();
    for (e, layout) in gp.inputs.iter().enumerate() {
        for basis in [Basis::Stabilizers, Basis::X, Basis::Z] {
            for (k, p) in layout.code.paulis(basis).iter().enumerate() {
                let image = u.conjugate(&place(p, layout)?);
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
    for (e, layout) in gp.outputs.iter().enumerate() {
        for basis in [Basis::X, Basis::Z] {
            for (k, p) in layout.code.paulis(basis).iter().enumerate() {
                // Which input generators multiply, up to sign, to this output
                // logical operator `p`?
                let combo = gens
                    .solve(place(p, layout)?.bits())
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
#[derive(Default)]
struct Gf2Basis {
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

// ── Logical-sign transport from the instruction action ─────────────────────

/// Each logical qubit q carries a pair `(z_sign, x_sign)` of parities. `z_sign`
/// is flipped by an X-type frame on q; `x_sign` by a Z-type frame.
fn action_transport(inv: &Invocation, sys: &BinarySystem) -> Result<Transport, Unresolved> {
    let instruction = inv.gadget.instruction();
    let logical = |layouts: &[EncodingPlan]| layouts.iter().map(|l| l.code.x.len()).sum::<usize>();
    let width = logical(&inv.gadget.inputs)
        .max(logical(&inv.gadget.outputs))
        .max(action_extent(instruction));
    // Fresh and temporary qubits start at (0, 0).
    let mut s: Vec<(Parity, Parity)> = vec![Default::default(); width];
    let mut q = 0;
    for (e, layout) in inv.gadget.inputs.iter().enumerate() {
        for k in 0..layout.code.x.len() {
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
    let resolve = |text| {
        PauliExpr::parse(instruction, text)
            .and_then(|e| e.resolve(&inv.args))
            .map_err(|_| Unresolved)
    };

    for step in &instruction.action {
        if step.condition.is_some() {
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
            Action::Clifford(images) => {
                let map = clifford_map(images).map_err(|_| Unresolved)?;
                s = conjugate_frame(&s, &map)?;
            }
            Action::Stabilize(paulis) => {
                for text in paulis {
                    for q in resolve(text)?.support() {
                        let (z_sign, x_sign) = s.get(q).ok_or(Unresolved)?;
                        if !known_zero(z_sign) || !known_zero(x_sign) {
                            return Err(Unresolved);
                        }
                    }
                }
            }
            Action::Rotate { pauli, .. } => {
                // A non-Clifford rotation needs a realized frame.
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
            Action::Pauli(_) | Action::Observe(_) => {}
        }
    }

    let mut transport = Vec::new();
    let mut offset = 0;
    for (e, layout) in inv.gadget.outputs.iter().enumerate() {
        for k in 0..layout.code.x.len() {
            let (z_sign, x_sign) = &s[offset + k];
            transport.push(((e, Basis::Z, k), z_sign.clone()));
            transport.push(((e, Basis::X, k), x_sign.clone()));
        }
        offset += layout.code.x.len();
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
fn step_support<'a>(
    action: &'a Action,
    resolve: &impl Fn(&'a qodec::PauliString) -> Result<Pauli, Unresolved>,
) -> Result<Vec<usize>, Unresolved> {
    let mut support = Vec::new();
    match action {
        Action::Clifford(images) => {
            for (generator, image) in images {
                support.extend(resolve(generator)?.support());
                support.extend(resolve(image)?.support());
            }
        }
        Action::Pauli(p) | Action::Rotate { pauli: p, .. } => support.extend(resolve(p)?.support()),
        Action::Observe(observables) => {
            for o in observables {
                support.extend(resolve(&o.pauli)?.support());
            }
        }
        Action::Stabilize(paulis) => {
            for p in paulis {
                support.extend(resolve(p)?.support());
            }
        }
    }
    Ok(support)
}

/// An output logical sign cannot be transported.
struct Unresolved;
