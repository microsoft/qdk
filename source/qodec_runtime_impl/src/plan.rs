// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Plans: shot-independent data needed to instantiate a qodec runtime.
//! Derived once from a qodec.
//!
//! Building a qodec runtime from a [`Qodec`] requires a non-trivial amount of
//! pre-processing: matching gadgets to instructions, parsing codes and circuits,
//! and indexing the lower ISAs.
//!
//! In order to cheaply instantiate a new qodec runtime for every shot in a job
//! requiring many shots (e.g.: ~1M shots), we capture the data resulting from
//! that pre-processing in a [`QodecPlan`].

use super::{
    design::{Design, LayerBlueprint},
    layer::{Chain, Link},
    parity::Parity,
    pauli::PauliOp,
    protocol::{Call, Fault, GadgetId, OwnedParam, Reply},
    resolver::Resolver,
    runtime::{AtLayer, BatchBackend, QodecRuntime, Runtime},
    spec::{
        CodePlan, CompiledSelect, EncodingPlan, FrameKey, RefScope, TypeTable, bind_operands,
        compile_select, encode_arg, layout_boundary, matches, prepare_frames,
    },
};
use qodec::{Argument, Circuit, Gadget, Instruction, InstructionSet, Layer, ParameterKind, Qodec};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, fmt, rc::Rc, sync::Arc};

/// The plan of a whole qodec, from which [`Self::instantiate`] builds a fresh
/// runtime per shot.
pub struct QodecPlan {
    layer_plans: Vec<Rc<LayerPlan>>,
    layers: Vec<Box<dyn LayerBlueprint>>,
    top: Arc<InstructionSet>,
    bottom: Arc<InstructionSet>,
}

impl QodecPlan {
    /// Plans every layer of `qodec` except the bottom one, and has `design`
    /// assemble each of them.
    pub fn new(qodec: &Qodec, design: &dyn Design) -> Result<Self, PlanError> {
        let layers_data = qodec.layers();
        let layer_plans = build_layer_plans(layers_data)?;
        let layers = layer_plans
            .iter()
            .enumerate()
            .map(|(k, layer)| {
                design
                    .layer(layer)
                    .map_err(|e| PlanError(format!("layer {k}: {e}")))
            })
            .collect::<Result<_, _>>()?;
        let isa = |layer: &Layer| Arc::clone(&layer.instruction_set);
        Ok(Self {
            layer_plans,
            layers,
            top: isa(&layers_data[0]),
            bottom: isa(&layers_data[layers_data.len() - 1]),
        })
    }

    /// The layer plans, top first.
    #[must_use]
    pub fn layer_plans(&self) -> &[Rc<LayerPlan>] {
        &self.layer_plans
    }

    /// The top ISA, which the runtime accepts.
    #[must_use]
    pub fn top(&self) -> &Arc<InstructionSet> {
        &self.top
    }

    /// The bottom ISA, which the bottom backend runs.
    #[must_use]
    pub fn bottom(&self) -> &Arc<InstructionSet> {
        &self.bottom
    }

    /// The resources needed at every layer boundary to run a program that needs
    /// `top_resources`: `[top_resources, below layer 0, ..., below the last
    /// layer]`. The last entry sizes the bottom backend. Calling this is
    /// optional, because encoders allocate lower blocks on demand (see
    /// [`Self::instantiate`]).
    pub fn required_resources(
        &self,
        top_resources: Resources,
    ) -> Result<Vec<Resources>, PlanError> {
        let mut resources = vec![top_resources];
        for (k, layer) in self.layer_plans.iter().enumerate() {
            let lower = layer
                .required_resources(&resources[k])
                .map_err(|e| PlanError(format!("layer {k}: {e}")))?;
            resources.push(lower);
        }
        Ok(resources)
    }

    /// Instantiates a qodec runtime: fresh layers, chained over `backend`,
    /// which runs batches of bottom-ISA calls (a simulator, a profiler, ...;
    /// see [`BatchBackend`]). Encoders allocate lower block ids
    /// on demand, densely from 0, so `backend` must either hold enough blocks
    /// upfront (see [`Self::required_resources`]) or grow on demand. Pass
    /// `&mut backend` to reuse one backend across shots.
    pub fn instantiate<'a, B, E>(&self, backend: B) -> QodecRuntime<'a, E>
    where
        B: BatchBackend<E> + 'a,
    {
        let links = self
            .layers
            .iter()
            .enumerate()
            .map(
                |(index, blueprint)| -> Link<Vec<Call>, Result<Vec<Reply>, Fault>> {
                    Box::new(AtLayer {
                        index,
                        layer: blueprint.instantiate_layer(),
                    })
                },
            )
            .collect();
        Runtime::new(
            Chain::new(links),
            Box::new(backend),
            &self.top,
            &self.bottom,
        )
    }
}

/// A configuration error: a qodec, a plan, a decoder or a resource requirement
/// is not supported or not well formed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanError(pub String);

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PlanError {}

impl From<String> for PlanError {
    fn from(message: String) -> Self {
        PlanError(message)
    }
}

impl From<&str> for PlanError {
    fn from(message: &str) -> Self {
        PlanError(message.to_string())
    }
}

fn build_layer_plans(layers: &[Layer]) -> Result<Vec<Rc<LayerPlan>>, PlanError> {
    if layers.is_empty() {
        return Err(PlanError("a qodec needs at least one layer".into()));
    }

    // Every layer except the last has gadgets that lower it to the next layer's
    // ISA; the last layer only supplies the bottom ISA. With a single layer
    // there are no layer plans, and the runtime passes instructions straight
    // to the bottom backend.
    let n = layers.len() - 1;
    let layer_plans = (0..n)
        .map(|k| {
            LayerPlan::new(&layers[k], &layers[k + 1].instruction_set)
                .map(Rc::new)
                .map_err(|e| PlanError(format!("layer {k}: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(layer_plans)
}

/// The plan of one qodec layer: its ISAs, codes and gadgets, checked and
/// indexed once. Encoders, decoders and controllers plan from it.
///
/// "Upper" refers to this layer's ISA, whose instructions the layer accepts,
/// and "lower" to the next layer's ISA, whose instructions it emits.
#[derive(Debug)]
pub struct LayerPlan {
    /// The upper ISA.
    pub isa: Arc<InstructionSet>,
    /// The next layer's ISA.
    pub lower_isa: Arc<InstructionSet>,
    /// Upper block types.
    pub upper: TypeTable,
    /// Lower block types.
    pub lower: TypeTable,
    /// Upper block type -> the plan of its code.
    pub codes: BTreeMap<String, Rc<CodePlan>>,
    /// A map from an upper block type to the number of lower blocks of each type
    /// needed to encode it in the lower ISA.
    ///
    /// Each value has one count per lower type, in the order of [`Self::lower`].
    /// Different gadgets may encode the same upper type on different lower blocks,
    /// and sizing must fit all of them, so each count is
    /// the largest that any gadget encoding (`in` or `out`) of that upper type
    /// needs. Counts are maximized separately per lower type, so they can
    /// overcount: encodings needing `[2, 0]` and `[0, 2]` give `[2, 2]`.
    ///
    /// For example, with lower types `q` and `anc`, in that order:
    ///
    /// ```text
    /// { "lq": [3, 0], "pair": [6, 1] }
    /// ```
    ///
    /// One `lq` block needs 3 `q` blocks and no `anc` block, and one `pair`
    /// block needs 6 `q` blocks and 1 `anc` block. [`Self::required_resources`]
    /// sizes 2 `lq` blocks as 2 × 3 = 6 `q` blocks, plus the blocks gadgets use
    /// beyond their operands' encodings ([`Self::scratch`]).
    pub requirements: BTreeMap<String, Vec<usize>>,
    /// Lower blocks per lower type that a gadget uses beyond its boundary
    /// encodings, maximized over gadgets (see `gadget_scratch`). Only used for
    /// sizing, by [`Self::required_resources`].
    pub scratch: Vec<usize>,
    /// Per upper instruction: how the layer handles it.
    pub instructions: Vec<InstructionPlan>,
    /// The layer's gadgets, indexed by [`GadgetId`].
    pub gadgets: Vec<Rc<GadgetPlan>>,
    /// Matches the layer's own generic operations against the lower ISA.
    pub resolver: Resolver,
    /// Per lower type `t`: the index of the first lower instruction with
    /// `in = [t]` (not variadic), `out = []`, no observe outcomes and no flags.
    /// If a type has none, releasing a block of that type emits nothing: the
    /// block stays live below until the layer reuses its id. A measuring
    /// instruction is never used as a fallback release, because below a qodec
    /// layer it would run a whole measurement gadget.
    pub lower_release: Vec<Option<usize>>,
    /// Per upper instruction: whether it returns a result, which it does iff it
    /// has observe outcomes or flags.
    pub upper_needs_result: Vec<bool>,
    /// Per lower instruction: (observe count, flag count) of its result.
    pub lower_shapes: Vec<(usize, usize)>,
}

impl LayerPlan {
    /// Plans `layer`, whose gadgets lower it to `lower_isa`.
    ///
    /// If an upper instruction is planned as
    /// [`InstructionPlan::LogicalPauli`], the encoder applies it as Pauli
    /// corrections, so this also runs [`Self::check_pauli_corrections`]. An
    /// encoder that accepts corrections from its decoder runs the check when it
    /// is planned (see [`PlannedFix::check`](crate::encoder::PlannedFix::check)).
    pub fn new(layer: &Layer, lower_isa: &Arc<InstructionSet>) -> Result<Self, PlanError> {
        let isa = Arc::clone(&layer.instruction_set);
        let upper = TypeTable::new(&isa);
        let lower = TypeTable::new(lower_isa);
        let codes = plan_codes(layer)?;
        let mut requirements: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut scratch = vec![0; lower.len()];
        let mut instructions = vec![InstructionPlan::Unsupported; isa.instructions.len()];
        let mut gadgets = Vec::with_capacity(layer.gadgets.len());

        let context = GadgetContext {
            isa: &isa,
            lower_isa,
            codes: &codes,
            upper: &upper,
            lower: &lower,
        };
        for (id, (name, g)) in layer.gadgets.iter().enumerate() {
            let gp = context
                .plan_gadget(GadgetId(id), name, g)
                .map_err(|e| PlanError(format!("gadget `{name}`: {e}")))?;
            for layout in gp.inputs.iter().chain(&gp.outputs) {
                let entry = requirements
                    .entry(layout.block_type.clone())
                    .or_insert_with(|| vec![0; lower.len()]);
                max_merge(entry, &layout.resources(&lower));
            }
            max_merge(&mut scratch, &gadget_scratch(&gp, &lower));
            let index = gp.index;
            let gp = Rc::new(gp);
            gadgets.push(Rc::clone(&gp));
            instructions[index] = InstructionPlan::Gadget(gp);
        }

        // Upper instructions without a gadget.
        for (instruction_plan, instruction) in instructions.iter_mut().zip(&isa.instructions) {
            if matches!(instruction_plan, InstructionPlan::Unsupported) {
                if is_pauli_only(instruction) {
                    *instruction_plan = InstructionPlan::LogicalPauli;
                } else if is_release(instruction) {
                    *instruction_plan = InstructionPlan::Release;
                }
            }
        }

        let resolver = Resolver::new(lower_isa, &lower);
        let lower_release = lower
            .names
            .iter()
            .map(|t| {
                lower_isa.instructions.iter().position(|i| {
                    matches!(i.inputs.as_slice(), [op] if &op.block == t && !op.is_variadic)
                        && i.outputs.is_empty()
                        && i.observe_count() == 0
                        && i.flags.is_empty()
                })
            })
            .collect();

        let upper_needs_result = isa
            .instructions
            .iter()
            .map(|i| i.observe_count() + i.flags.len() > 0)
            .collect();
        let lower_shapes = lower_isa
            .instructions
            .iter()
            .map(|i| (i.observe_count(), i.flags.len()))
            .collect();
        let plan = Self {
            isa,
            lower_isa: Arc::clone(lower_isa),
            upper,
            lower,
            codes,
            requirements,
            scratch,
            instructions,
            gadgets,
            resolver,
            lower_release,
            upper_needs_result,
            lower_shapes,
        };
        if plan
            .instructions
            .iter()
            .any(|h| matches!(h, InstructionPlan::LogicalPauli))
        {
            plan.check_pauli_corrections()?;
        }
        Ok(plan)
    }

    /// Checks that the lower ISA can carry this layer's Pauli corrections: one
    /// instruction must apply `X`, and one `Z`, on logical index 0 of every
    /// lower type. A `Y` falls back to them (see
    /// [`Resolver::resolve`](crate::resolver::Resolver::resolve)).
    ///
    /// Run it only for a layer that issues Pauli corrections: one with a
    /// [`InstructionPlan::LogicalPauli`] instruction, or whose encoder accepts
    /// [`Correction`](crate::protocol::Correction)s. Other layers need no Pauli
    /// instructions below them. The check catches a lower ISA without Pauli
    /// instructions at plan time. It doesn't cover the other logical indices
    /// of a multi-qubit lower type; a correction the resolver can't bind there
    /// fails when it is issued, with `NoImplementation`.
    pub fn check_pauli_corrections(&self) -> Result<(), PlanError> {
        for (t, name) in self.lower.names.iter().enumerate() {
            for op in [PauliOp::X, PauliOp::Z] {
                if !self.resolver.binds_directly(op, t, &self.lower) {
                    return Err(PlanError(format!(
                        "the lower ISA has no `{op:?}` Pauli for block type `{name}`"
                    )));
                }
            }
        }
        Ok(())
    }

    /// The lower resources needed to run a program that needs `upper` resources
    /// on this layer: [`Self::requirements`] for every upper block, plus
    /// [`Self::scratch`] once, because invocations run one at a time and each
    /// releases its scratch blocks before the next starts. The result counts
    /// qubits when the lower ISA has one block type that holds one qubit, and
    /// blocks otherwise.
    pub fn required_resources(&self, upper: &Resources) -> Result<Resources, PlanError> {
        // `upper.qubits` is added as blocks of the upper ISA's only type. That
        // equals its qubit count because `Resources::qubits` is only meant for an
        // ISA whose blocks hold one qubit; the block capacity is not checked here.
        let mut need = upper.blocks.clone();
        if upper.qubits > 0 {
            let only = self
                .upper
                .only()
                .ok_or("qubit resources need an ISA with one block type")?;
            *need.entry(self.upper.names[only].clone()).or_default() += upper.qubits;
        }
        let mut lower = self.scratch.clone();
        for (up_type, &count) in &need {
            if count == 0 {
                continue;
            }
            let per_block = self.requirements.get(up_type).ok_or_else(|| {
                PlanError(format!(
                    "no gadget produces or consumes block type `{up_type}`"
                ))
            })?;
            for (t, k) in per_block.iter().enumerate() {
                lower[t] += count * k;
            }
        }
        if self.lower.only().is_some() && self.lower.caps[0] == 1 {
            return Ok(Resources::qubits(lower.iter().sum()));
        }
        Ok(Resources::blocks(
            self.lower.names.iter().cloned().zip(lower),
        ))
    }
}

/// Plans the layer's code bindings: the layer's `codes` for block types its
/// ISA declares, plus, for each operand type the layer leaves unbound, the code
/// of its gadget encodings. [`layout_boundary`] later rejects any encoding
/// whose code differs from the result.
fn plan_codes(layer: &Layer) -> Result<BTreeMap<String, Rc<CodePlan>>, PlanError> {
    let mut raw: BTreeMap<String, Arc<qodec::Code>> = layer
        .codes
        .iter()
        .filter(|(block, _)| {
            layer
                .instruction_set
                .blocks
                .iter()
                .any(|b| &b.name == *block)
        })
        .map(|(block, code)| (block.clone(), Arc::clone(code)))
        .collect();
    for g in layer.gadgets.values() {
        let operands = g.implements.inputs.iter().chain(&g.implements.outputs);
        for (encoding, operand) in g.inputs.iter().chain(&g.outputs).zip(operands) {
            raw.entry(operand.block.clone())
                .or_insert_with(|| Arc::clone(&encoding.code));
        }
    }
    raw.into_iter()
        .map(|(block, code)| {
            let bound =
                CodePlan::new(&code).map_err(|e| PlanError(format!("code for `{block}`: {e}")))?;
            Ok((block, Rc::new(bound)))
        })
        .collect()
}

/// What planning one gadget needs from its layer.
struct GadgetContext<'a> {
    isa: &'a InstructionSet,
    lower_isa: &'a InstructionSet,
    codes: &'a BTreeMap<String, Rc<CodePlan>>,
    upper: &'a TypeTable,
    lower: &'a TypeTable,
}

impl GadgetContext<'_> {
    fn plan_gadget(&self, id: GadgetId, name: &str, g: &Gadget) -> Result<GadgetPlan, String> {
        let lower = self.lower;
        let index = self
            .isa
            .instructions
            .iter()
            .position(|i| i.mnemonic == name)
            .ok_or("implements no instruction of the layer")?;
        let instruction = &g.implements;
        if *instruction != self.isa.instructions[index] {
            return Err("`implements` differs from the layer's instruction".into());
        }
        if *g.circuit.instruction_set != *self.lower_isa {
            return Err("the circuit must target the next layer's ISA".into());
        }
        for param in g.parameter_bindings.keys() {
            if instruction.parameters.iter().all(|p| &p.name != param) {
                return Err(format!("binds unknown parameter `{param}`"));
            }
        }

        let layout = |operands, encodings| {
            layout_boundary(operands, encodings, self.codes, self.upper, lower)
        };
        let inputs = layout(&instruction.inputs, &g.inputs)?;
        let outputs = layout(&instruction.outputs, &g.outputs)?;
        let body = CircuitPlan::new(&g.circuit, lower, &g.parameter_bindings)?;

        let mut labels: Vec<String> = Vec::new();
        let mut seen = FxHashSet::default();
        for label in inputs
            .iter()
            .flat_map(|l| &l.labels)
            .chain(&body.labels)
            .chain(outputs.iter().flat_map(|l| &l.labels))
        {
            if seen.insert(label.as_str()) {
                labels.push(label.clone());
            }
        }

        let mut label_type = body.block_types.clone();
        for layout in inputs.iter().chain(&outputs) {
            for (label, &t) in layout.labels.iter().zip(&layout.lower_types) {
                // Encodings and the circuit body must agree: no silent
                // retyping of a lower block.
                if *label_type.entry(label.clone()).or_insert(t) != t {
                    return Err(format!("label `{label}` has conflicting types"));
                }
            }
        }
        if let Some(only) = lower.only() {
            for label in &labels {
                label_type.entry(label.clone()).or_insert(only);
            }
        }
        if let Some(label) = labels.iter().find(|l| !label_type.contains_key(*l)) {
            return Err(format!("label `{label}` has no lower type"));
        }

        let scope = RefScope {
            record_len: body.record_len,
            readouts_len: g.readouts.len(),
            inputs: &inputs,
            outputs: &outputs,
        };
        let frames = prepare_frames(g, &scope)?;
        let slots = SlotPlan::new(
            &labels,
            &label_type,
            &inputs,
            &outputs,
            &body,
            self.lower_isa,
        );

        Ok(GadgetPlan {
            id,
            name: name.to_string(),
            index,
            gadget: g.clone(),
            body,
            labels,
            inputs,
            outputs,
            label_type,
            frames,
            slots,
        })
    }
}

/// Lower blocks per lower type that a gadget uses beyond its boundary: per
/// type, its labels minus the larger of its input and output encodings, or 0
/// (see `LayerPlan::scratch`).
///
/// The boundary is already counted per upper block: the caller's resources
/// count each upper block once, and [`LayerPlan::requirements`] gives it, per
/// lower type, as many lower blocks as the largest of its encodings uses. That
/// covers the larger side of an invocation, not both. When a gadget's inputs and outputs use different
/// labels, as in a teleportation gadget, both sides are live during the
/// invocation, so the smaller side is counted here as scratch. For example, a
/// gadget with 4 input labels, 4 different output labels, and 2 ancillas has
/// 10 labels and a boundary of 4, so it needs 6 scratch blocks.
fn gadget_scratch(gp: &GadgetPlan, lower: &TypeTable) -> Vec<usize> {
    let sum = |layouts: &[EncodingPlan]| {
        let mut total = vec![0; lower.len()];
        for l in layouts {
            for (t, n) in l.resources(lower).into_iter().enumerate() {
                total[t] += n;
            }
        }
        total
    };
    let mut boundary = sum(&gp.inputs);
    max_merge(&mut boundary, &sum(&gp.outputs));
    let mut used = vec![0usize; lower.len()];
    for label in &gp.labels {
        used[gp.label_type[label]] += 1;
    }
    used.iter()
        .zip(&boundary)
        .map(|(u, b)| u.saturating_sub(*b))
        .collect()
}

fn max_merge(into: &mut [usize], from: &[usize]) {
    for (a, b) in into.iter_mut().zip(from) {
        *a = (*a).max(*b);
    }
}

/// Whether `instruction` can be planned as [`InstructionPlan::LogicalPauli`]:
/// the action is non-empty and has only unguarded `pauli` steps, the
/// instruction has no flags, and its boundary is in place: at least one input,
/// and outputs identical to the inputs.
fn is_pauli_only(instruction: &Instruction) -> bool {
    !instruction.action.is_empty()
        && instruction
            .action
            .iter()
            .all(|s| s.condition.is_none() && matches!(s.action, qodec::Action::Pauli(_)))
        && instruction.flags.is_empty()
        && !instruction.inputs.is_empty()
        && instruction.inputs == instruction.outputs
}

/// Whether `instruction` can be planned as [`InstructionPlan::Release`]:
/// `in = [t]` with `t` not variadic, `out = []`, empty action, no flags.
fn is_release(instruction: &Instruction) -> bool {
    matches!(instruction.inputs.as_slice(), [op] if !op.is_variadic)
        && instruction.outputs.is_empty()
        && instruction.action.is_empty()
        && instruction.flags.is_empty()
}

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

/// How a layer handles one upper instruction.
#[derive(Clone, Debug)]
pub enum InstructionPlan {
    /// Runs this gadget.
    Gadget(Rc<GadgetPlan>),
    /// A Pauli-only instruction without a gadget: applied as the code's logical
    /// operator, through Pauli corrections.
    LogicalPauli,
    /// Releases the block.
    Release,
    /// Fails with `NoImplementation`.
    Unsupported,
}

/// How a layer runs one gadget.
#[derive(Debug)]
pub struct GadgetPlan {
    /// The gadget's id in its layer.
    pub id: GadgetId,
    /// The gadget's name: the mnemonic it implements.
    pub name: String,
    /// Index of the implemented instruction in the upper ISA.
    pub index: usize,
    /// The gadget as loaded.
    pub gadget: Gadget,
    /// The plan of its circuit.
    pub body: CircuitPlan,
    /// Every circuit label of the gadget, without duplicates: input supports
    /// first, then body labels, then output supports, each in first-appearance
    /// order. A label's position here is its slot (see [`SlotPlan`]).
    pub labels: Vec<String>,
    /// One encoding plan per `in` operand.
    pub inputs: Vec<EncodingPlan>,
    /// One encoding plan per `out` operand.
    pub outputs: Vec<EncodingPlan>,
    /// Circuit label -> its lower type, which is the same everywhere in the
    /// gadget.
    pub label_type: FxHashMap<String, usize>,
    /// The gadget's `frames`, parsed by [`prepare_frames`]: per output logical
    /// sign, the parity of circuit readouts that flips it.
    pub frames: FxHashMap<FrameKey, Parity>,
    /// The same gadget as `labels`, `inputs`, `outputs`, and `body`, with every
    /// label replaced by its slot (see [`SlotPlan`]), derived from them at plan
    /// time. The encoder uses slots wherever the body or an encoding names
    /// a block, and never looks a label up. The label-based fields remain for
    /// decoders (`frame_decoder.rs` reads them at plan time and through
    /// `Invocation::gadget`) and for sizing resources.
    pub slots: SlotPlan,
}

/// The gadget with every label replaced by its slot: the label's position in
/// [`GadgetPlan::labels`]. Labels are resolved to slots once, when the plan is
/// built, so the encoder never looks up a label by name while running an
/// invocation. Instead, it places each invocation as a `Vec<u64>` indexed by
/// slot, holding the lower block id of each label (`PreparedInvocation::placement`).
///
/// For example, with `labels = ["0", "1", "anc"]`, the body call `CX: [0, anc]`
/// compiles to slot operands `[0, 2]`. If an invocation places the labels on
/// lower blocks `[5, 6, 2]`, the layer emits `CX` on lower blocks 5 and 2.
///
/// `types`, `retained`, and `consumed` have exactly one entry per slot.
#[derive(Clone, Debug)]
pub struct SlotPlan {
    /// Lower type per slot.
    pub types: Vec<usize>,
    /// Per `in` operand: the slots of its encoding, in code-qubit order.
    pub inputs: Vec<Vec<usize>>,
    /// Per `out` operand: the slots of its encoding, in code-qubit order.
    pub outputs: Vec<Vec<usize>>,
    /// Per slot: whether an output encoding holds it.
    pub retained: Vec<bool>,
    /// Per slot: whether the body itself consumes it.
    pub consumed: Vec<bool>,
    /// One per call of [`CircuitPlan::calls`], in the same order, so the layer
    /// runtime zips the two.
    pub calls: Vec<CompiledCall>,
}

impl SlotPlan {
    fn new(
        labels: &[String],
        label_type: &FxHashMap<String, usize>,
        inputs: &[EncodingPlan],
        outputs: &[EncodingPlan],
        body: &CircuitPlan,
        lower_isa: &InstructionSet,
    ) -> Self {
        let index: FxHashMap<&str, usize> = labels
            .iter()
            .enumerate()
            .map(|(slot, label)| (label.as_str(), slot))
            .collect();
        let encodings = |layouts: &[EncodingPlan]| -> Vec<Vec<usize>> {
            layouts
                .iter()
                .map(|l| l.labels.iter().map(|label| index[label.as_str()]).collect())
                .collect()
        };
        let outputs = encodings(outputs);
        let mut retained = vec![false; labels.len()];
        for &slot in outputs.iter().flatten() {
            retained[slot] = true;
        }
        Self {
            types: labels.iter().map(|label| label_type[label]).collect(),
            inputs: encodings(inputs),
            outputs,
            retained,
            consumed: labels
                .iter()
                .map(|label| body.consumed.contains(label))
                .collect(),
            calls: body
                .calls
                .iter()
                .map(|pc| compile_call(pc, &lower_isa.instructions[pc.instruction], &index))
                .collect(),
        }
    }
}

/// Compiles a call for the encoder: its operands become slots, and
/// its arguments are put in the lower instruction's parameter order, with
/// literal values validated and encoded once.
fn compile_call(
    cp: &CallPlan,
    instruction: &Instruction,
    index: &FxHashMap<&str, usize>,
) -> CompiledCall {
    let operands = cp.operands.iter().map(|o| index[o.as_str()]).collect();
    let args = (cp.args.len() == instruction.parameters.len())
        .then(|| {
            instruction
                .parameters
                .iter()
                .map(|parameter| {
                    let (_, arg) = cp.args.iter().find(|(name, _)| *name == parameter.name)?;
                    Some(match arg {
                        ArgumentPlan::Value(value) => {
                            if !matches(parameter.kind, value) {
                                return None;
                            }
                            CompiledArg::Value(encode_arg(value)?)
                        }
                        ArgumentPlan::Forwarded(name) => CompiledArg::Forwarded {
                            name: name.clone(),
                            kind: parameter.kind,
                        },
                        ArgumentPlan::Readout(j) => {
                            if !matches(parameter.kind, &Argument::Boolean(false)) {
                                return None;
                            }
                            CompiledArg::Readout(*j)
                        }
                    })
                })
                .collect::<Option<Vec<_>>>()
        })
        .flatten();
    CompiledCall { operands, args }
}

/// A gadget-body call, ready to emit.
#[derive(Clone, Debug)]
pub struct CompiledCall {
    /// The operands, as slots.
    pub operands: Vec<usize>,
    /// The arguments, in the lower instruction's parameter order. `None` if
    /// they can never be valid, so running the call fails with
    /// `InvalidInstruction`. This is not a plan error, so that a qodec whose
    /// broken gadget a program never invokes still plans and runs.
    pub args: Option<Vec<CompiledArg>>,
}

/// One argument of a [`CompiledCall`].
#[derive(Clone, Debug, PartialEq)]
pub enum CompiledArg {
    /// A literal value, already validated and encoded.
    Value(OwnedParam),
    /// Forwarded from this parameter of the implemented instruction. Its value
    /// is only known per invocation, so it is validated against `kind` then.
    Forwarded {
        /// The parameter of the implemented instruction.
        name: String,
        /// The kind of the lower parameter it is passed to.
        kind: ParameterKind,
    },
    /// `circuit.readouts[j]`, an earlier record bit.
    Readout(usize),
}

impl GadgetPlan {
    /// The implemented instruction.
    #[must_use]
    pub fn instruction(&self) -> &Instruction {
        &self.gadget.implements
    }
}

/// The plan of a gadget body: its parsed circuit, checked against the lower
/// ISA.
#[derive(Clone, Debug)]
pub struct CircuitPlan {
    /// Circuit labels, in first-appearance order.
    pub labels: Vec<String>,
    /// Circuit label -> lower type.
    pub block_types: FxHashMap<String, usize>,
    /// The calls, in program order.
    pub calls: Vec<CallPlan>,
    /// Labels the body itself consumes: their last call takes them as an input
    /// but does not output them, as a destructive measurement does. They are
    /// already gone below, so the layer must not release them again.
    pub consumed: FxHashSet<String>,
    /// Length of the circuit readout record.
    pub record_len: usize,
}

impl CircuitPlan {
    /// Plans a gadget body: parses its circuit and checks every call against the
    /// lower ISA.
    fn new(
        circuit: &Circuit,
        lower: &TypeTable,
        parameter_bindings: &BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let isa = &circuit.instruction_set;
        let calls = circuit.calls()?;
        // Circuit source name -> forwarded instruction parameter.
        let sources: FxHashMap<&str, &str> = parameter_bindings
            .iter()
            .map(|(param, source)| (source.as_str(), param.as_str()))
            .collect();

        let mut labels: Vec<String> = Vec::new();
        let mut seen_labels: FxHashSet<String> = FxHashSet::default();
        let mut block_types: FxHashMap<String, usize> = FxHashMap::default();
        let mut call_plans = Vec::with_capacity(calls.len());
        // Label -> (whether it is an input of its last call, whether it is an
        // output of its last call).
        let mut last_use: FxHashMap<String, (bool, bool)> = FxHashMap::default();
        let mut offset = 0;

        for call in &calls {
            let index = isa
                .instructions
                .iter()
                .position(|i| i.mnemonic == call.mnemonic)
                .ok_or_else(|| format!("unknown instruction `{}`", call.mnemonic))?;
            let instruction = &isa.instructions[index];
            let operands: Vec<String> = call.operands.iter().map(qodec::Operand::label).collect();
            let (ins, outs) = bind_operands(lower, instruction, &operands, &FxHashMap::default())
                .map_err(|e| format!("call `{}`: {e}", call.mnemonic))?;

            let mut args = Vec::with_capacity(call.arguments.len());
            for (name, value) in &call.arguments {
                let kind = instruction
                    .parameters
                    .iter()
                    .find(|p| &p.name == name)
                    .map(|p| p.kind)
                    .ok_or_else(|| format!("call `{}` has no parameter `{name}`", call.mnemonic))?;
                let arg = match value {
                    Argument::Readout(j) => {
                        if kind != ParameterKind::Bit || *j >= offset {
                            return Err(format!(
                                "call `{}`: `circuit.readouts[{j}]` must be an earlier bit for a `bit` parameter",
                                call.mnemonic
                            ));
                        }
                        ArgumentPlan::Readout(*j)
                    }
                    Argument::Text(s) if sources.contains_key(s.as_str()) => {
                        ArgumentPlan::Forwarded(sources[s.as_str()].to_string())
                    }
                    other => ArgumentPlan::Value(other.clone()),
                };
                args.push((name.clone(), arg));
            }

            let observe_count = instruction.observe_count();
            let flag_count = instruction.flags.len();
            let select = compile_select(&instruction.flags, &call.select)
                .map_err(|e| format!("call `{}`: {e}", call.mnemonic))?;
            offset += observe_count + flag_count;

            for label in &operands {
                if seen_labels.insert(label.clone()) {
                    labels.push(label.clone());
                }
                let is_in = ins.iter().any(|b| &b.label == label);
                let is_out = outs.iter().any(|b| &b.label == label);
                last_use.insert(label.clone(), (is_in, is_out));
            }
            for bound in ins.iter().chain(&outs) {
                let t = lower
                    .get(&bound.block_type)
                    .ok_or_else(|| format!("unknown block type `{}`", bound.block_type))?;
                block_types.insert(bound.label.clone(), t);
            }
            call_plans.push(CallPlan {
                instruction: index,
                operands,
                args,
                observe_count,
                flag_count,
                select,
            });
        }

        let consumed = last_use
            .into_iter()
            .filter(|(_, (is_in, is_out))| *is_in && !*is_out)
            .map(|(label, _)| label)
            .collect();
        Ok(CircuitPlan {
            labels,
            block_types,
            calls: call_plans,
            consumed,
            record_len: offset,
        })
    }
}

/// The plan of one call of a gadget body, checked against the lower ISA.
#[derive(Clone, Debug)]
pub struct CallPlan {
    /// Index into the lower ISA.
    pub instruction: usize,
    /// Circuit labels, in operand order.
    pub operands: Vec<String>,
    /// Arguments, by parameter name.
    pub args: Vec<(String, ArgumentPlan)>,
    /// Observe outcomes the call adds to the record.
    pub observe_count: usize,
    /// Flags the call adds to the record, after its outcomes.
    pub flag_count: usize,
    /// The call's `select` patterns, compiled by [`compile_select`].
    pub select: CompiledSelect,
}

/// The plan of one argument of a gadget-body call.
#[derive(Clone, Debug, PartialEq)]
pub enum ArgumentPlan {
    /// A literal value.
    Value(Argument),
    /// Forwarded from this parameter of the implemented instruction, through
    /// the gadget's `parameter_bindings`.
    Forwarded(String),
    /// `circuit.readouts[j]`, an earlier record bit.
    Readout(usize),
}
