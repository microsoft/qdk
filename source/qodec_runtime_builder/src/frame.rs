// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Plans the [`FrameDecoder`](crate::FrameDecoder) of a layer: each gadget's
//! checks, readouts and frames as parities, and its sign transport when the
//! gadget's body fixes it.

use super::{
    actions::{action_clifford, action_extent, compile_action},
    frame_decoder::{CodeShape, FrameGadget, Transport},
    isa::bind_operands,
    parity::{Basis, Parity, Side, Var},
    pauli::{Clifford, DensePauli, Pauli},
    plan::{GadgetPlan, LayerPlan},
    spec::{CodePlan, EncodingPlan, RefScope, expr, signature},
};
use rustc_hash::FxHashMap;

/// What the frame decoder needs for `gp`, a gadget of `layer`.
pub fn frame_gadget(gp: &GadgetPlan, layer: &LayerPlan) -> Result<FrameGadget, String> {
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
    let shape = |code: &CodePlan| CodeShape {
        stabilizers: code.stabilizers.len(),
        logicals: code.x.len(),
    };
    Ok(FrameGadget {
        outcomes: instruction.observe_count(),
        flags: instruction.flags.len(),
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
        frames: gp.frames.clone(),
        inputs: gp.inputs.iter().map(|l| shape(&l.code)).collect(),
        outputs: gp.outputs.iter().map(|l| shape(&l.code)).collect(),
        transport: circuit_transport(gp, layer)?,
        action: compile_action(instruction),
        action_extent: action_extent(instruction),
    })
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
    // Give each gadget label a range of qubit positions, one per qubit of its
    // lower block.
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
        let (ins, _) = bind_operands(
            lower,
            &signature(instruction),
            &pc.operands,
            &FxHashMap::default(),
        )?;
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

    // The images under `u` of every input stabilizer and logical operator,
    // each tagged with its input sign variable in `vars`.
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
                // Find the input generators whose product is this output
                // logical operator `p`, up to sign.
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
