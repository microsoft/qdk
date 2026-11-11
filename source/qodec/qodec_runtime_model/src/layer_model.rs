// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The model of a qodec: each layer resolved into plain data that pieces plan
//! from. It is shaped like the qodec spec, not like any piece: an ISA, codes,
//! and gadgets with their circuits, checks, readouts and frames. Names are
//! interned: block types, instructions and circuit labels are ids, and their
//! names stay in side tables for messages.

use super::{
    error::PlanError,
    isa::{Isa, ParamKind},
    parity::{Basis, Parity},
    pauli::Pauli,
    protocol::{GadgetId, OwnedParam},
    select::CompiledSelect,
};
use rustc_hash::FxHashMap;
use std::{collections::BTreeMap, sync::Arc};

/// A whole qodec: its layers, top first, and the ISAs at its ends.
#[derive(Clone, Debug)]
pub struct QodecModel {
    /// Every layer but the bottom one, top first: each lowers its ISA to the
    /// next one's. A qodec with one layer has none.
    pub layers: Vec<Arc<LayerModel>>,
    /// The top ISA, which runtimes accept.
    pub top: Arc<Isa>,
    /// The bottom ISA, which bottom backends run.
    pub bottom: Arc<Isa>,
}

impl QodecModel {
    /// The resources needed at every layer boundary to run a program that
    /// needs `top`: `[top, below layer 0, ..., below the last layer]`. The last
    /// entry sizes the bottom backend.
    pub fn required_resources(&self, top: Resources) -> Result<Vec<Resources>, PlanError> {
        let mut resources = vec![top];
        for (k, layer) in self.layers.iter().enumerate() {
            let lower = layer
                .required_resources(&resources[k])
                .map_err(|e| PlanError(format!("layer {k}: {e}")))?;
            resources.push(lower);
        }
        Ok(resources)
    }
}

/// One qodec layer: its ISA ("upper"), the next layer's ISA ("lower"), the
/// codes that encode its blocks, and the gadgets that implement its
/// instructions in the lower ISA.
#[derive(Clone, Debug)]
pub struct LayerModel {
    /// The upper ISA.
    pub upper: Arc<Isa>,
    /// The lower ISA.
    pub lower: Arc<Isa>,
    /// Per upper block type: the code that encodes it, if the layer binds one.
    pub codes: Vec<Option<Arc<CodeModel>>>,
    /// The gadgets, by [`GadgetId`].
    pub gadgets: Vec<Arc<GadgetModel>>,
    /// Per upper instruction: the gadget that implements it, if any.
    pub gadget_of: Vec<Option<GadgetId>>,
}

/// One gadget: an upper instruction implemented as a circuit of the lower
/// ISA.
///
/// Its circuit labels are interned as label ids: input supports first, then
/// labels the body introduces, then output supports, each in first-appearance
/// order.
#[derive(Clone, Debug)]
pub struct GadgetModel {
    /// The gadget's id in its layer.
    pub id: GadgetId,
    /// The gadget's name, for messages.
    pub name: String,
    /// The upper instruction it implements.
    pub instruction: usize,
    /// Each label's name, by label id, for messages.
    pub labels: Vec<String>,
    /// Each label's lower block type, by label id.
    pub label_types: Vec<usize>,
    /// One encoding per `in` operand.
    pub inputs: Vec<EncodingModel>,
    /// One encoding per `out` operand.
    pub outputs: Vec<EncodingModel>,
    /// The circuit's calls, in order.
    pub body: Vec<CallModel>,
    /// The length of the circuit's readout record: its calls' outcomes and
    /// flags, in order.
    pub record_len: usize,
    /// The gadget's checks: each parity is 0 without errors.
    pub checks: Vec<Parity>,
    /// The gadget's readouts: the implemented instruction's outcomes, then
    /// its flags.
    pub readouts: Vec<Parity>,
    /// The gadget's `frames`: per output logical sign, the parity of circuit
    /// readouts that flips it.
    pub frames: FxHashMap<FrameKey, Parity>,
}

/// `(output entry, basis, logical index)` of one output logical sign.
pub type FrameKey = (usize, Basis, usize);

/// Where one gadget operand's block lives in the circuit.
#[derive(Clone, Debug)]
pub struct EncodingModel {
    /// The upper block type.
    pub block_type: usize,
    /// The code the layer binds to the block type.
    pub code: Arc<CodeModel>,
    /// The labels that hold it, in code-qubit order.
    pub labels: Vec<usize>,
}

/// One call of a gadget's circuit.
#[derive(Clone, Debug)]
pub struct CallModel {
    /// The lower instruction.
    pub instruction: usize,
    /// The operands, as label ids.
    pub operands: Vec<usize>,
    /// The arguments, in the lower instruction's parameter order. `None` if
    /// they can never be valid. That is not an error of the model: a qodec
    /// whose broken gadget a program never invokes still runs.
    pub args: Option<Vec<Arg>>,
    /// The call's `select` hint, over its flags.
    pub select: CompiledSelect,
}

/// One argument of a [`CallModel`].
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    /// A literal value, already validated against its parameter.
    Value(OwnedParam),
    /// Forwarded from this parameter of the implemented instruction. Its value
    /// is only known per invocation.
    Forwarded {
        /// The parameter of the implemented instruction.
        name: String,
        /// The kind of the lower parameter it is passed to.
        kind: ParamKind,
    },
    /// `circuit.readouts[j]`, an earlier record bit.
    Readout(usize),
}

/// A code, with its operators parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeModel {
    /// The code's name.
    pub name: String,
    /// Stabilizer generators.
    pub stabilizers: Vec<Pauli>,
    /// Logical X operators, one per logical qubit.
    pub x: Vec<Pauli>,
    /// Logical Z operators, one per logical qubit.
    pub z: Vec<Pauli>,
    /// Physical qubit count.
    pub n: usize,
}

impl CodeModel {
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

// ── Resources ───────────────────────────────────────────────────────────────

/// The resources needed at one layer boundary: qubits, blocks per type, or
/// both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resources {
    /// Qubits, for a single-type ISA whose blocks hold one qubit.
    pub qubits: usize,
    /// Blocks per block type.
    pub blocks: BTreeMap<String, usize>,
}

impl Resources {
    /// Only qubits.
    #[must_use]
    pub fn qubits(qubits: usize) -> Self {
        Self {
            qubits,
            blocks: BTreeMap::new(),
        }
    }

    /// Only blocks, per block type.
    #[must_use]
    pub fn blocks(blocks: impl IntoIterator<Item = (String, usize)>) -> Self {
        Self {
            qubits: 0,
            blocks: blocks.into_iter().collect(),
        }
    }
}

impl LayerModel {
    /// The lower resources needed to run a program that needs `upper`
    /// resources on this layer: [`Self::requirements`] for every upper block,
    /// plus [`Self::scratch`] once, because invocations run one at a time and
    /// each releases its scratch blocks before the next starts. The result
    /// counts qubits when the lower ISA has one block type that holds one
    /// qubit, and blocks otherwise.
    pub fn required_resources(&self, upper: &Resources) -> Result<Resources, PlanError> {
        // `upper.qubits` is added as blocks of the upper ISA's only type. That
        // equals its qubit count because `Resources::qubits` is only meant for an
        // ISA whose blocks hold one qubit; the block capacity is not checked here.
        let types = &self.upper.types;
        let mut need = upper.blocks.clone();
        if upper.qubits > 0 {
            let only = types
                .only()
                .ok_or("qubit resources need an ISA with one block type")?;
            *need.entry(types.names[only].clone()).or_default() += upper.qubits;
        }
        let requirements = self.requirements();
        let mut lower = self.scratch();
        for (up_type, &count) in &need {
            if count == 0 {
                continue;
            }
            let per_block = types
                .get(up_type)
                .and_then(|t| requirements[t].as_ref())
                .ok_or_else(|| {
                    PlanError(format!(
                        "no gadget produces or consumes block type `{up_type}`"
                    ))
                })?;
            for (t, k) in per_block.iter().enumerate() {
                lower[t] += count * k;
            }
        }
        let lower_types = &self.lower.types;
        if lower_types.only().is_some() && lower_types.caps[0] == 1 {
            return Ok(Resources::qubits(lower.iter().sum()));
        }
        Ok(Resources::blocks(
            lower_types.names.iter().cloned().zip(lower),
        ))
    }

    /// Per upper block type: the number of lower blocks of each lower type
    /// needed to encode one block of it, or `None` if no gadget encodes it.
    ///
    /// Different gadgets may encode the same upper type on different lower
    /// blocks, and sizing must fit all of them, so each count is the largest
    /// that any gadget encoding (`in` or `out`) of that upper type needs.
    /// Counts are maximized separately per lower type, so they can overcount:
    /// encodings needing `[2, 0]` and `[0, 2]` give `[2, 2]`.
    #[must_use]
    pub fn requirements(&self) -> Vec<Option<Vec<usize>>> {
        let mut requirements = vec![None; self.upper.types.len()];
        for gadget in &self.gadgets {
            for encoding in gadget.inputs.iter().chain(&gadget.outputs) {
                let entry: &mut Vec<usize> = requirements[encoding.block_type]
                    .get_or_insert_with(|| vec![0; self.lower.types.len()]);
                max_merge(entry, &self.encoding_blocks(gadget, encoding));
            }
        }
        requirements
    }

    /// Lower blocks per lower type that a gadget uses beyond its boundary
    /// encodings, maximized over gadgets.
    ///
    /// The boundary is already counted per upper block (see
    /// [`Self::requirements`]): the largest of an upper type's encodings, which
    /// covers the larger side of an invocation, not both. When a gadget's
    /// inputs and outputs use different labels, as in a teleportation gadget,
    /// both sides are live during the invocation, so the smaller side is
    /// counted here as scratch. For example, a gadget with 4 input labels, 4
    /// different output labels, and 2 ancillas has 10 labels and a boundary of
    /// 4, so it needs 6 scratch blocks.
    #[must_use]
    pub fn scratch(&self) -> Vec<usize> {
        let n = self.lower.types.len();
        let mut scratch = vec![0; n];
        for gadget in &self.gadgets {
            let side = |encodings: &[EncodingModel]| {
                let mut total = vec![0; n];
                for encoding in encodings {
                    for (t, k) in self
                        .encoding_blocks(gadget, encoding)
                        .into_iter()
                        .enumerate()
                    {
                        total[t] += k;
                    }
                }
                total
            };
            let mut boundary = side(&gadget.inputs);
            max_merge(&mut boundary, &side(&gadget.outputs));
            let mut used = vec![0usize; n];
            for &t in &gadget.label_types {
                used[t] += 1;
            }
            let extra: Vec<usize> = used
                .iter()
                .zip(&boundary)
                .map(|(u, b)| u.saturating_sub(*b))
                .collect();
            max_merge(&mut scratch, &extra);
        }
        scratch
    }

    /// The number of lower blocks of each lower type that `encoding`, of
    /// `gadget`, uses.
    fn encoding_blocks(&self, gadget: &GadgetModel, encoding: &EncodingModel) -> Vec<usize> {
        let mut counts = vec![0; self.lower.types.len()];
        for &label in &encoding.labels {
            counts[gadget.label_types[label]] += 1;
        }
        counts
    }
}

fn max_merge(into: &mut [usize], from: &[usize]) {
    for (a, b) in into.iter_mut().zip(from) {
        *a = (*a).max(*b);
    }
}
