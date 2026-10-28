// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The plan domain: shot-independent data needed to build a qodec runtime.
//! Derived once from a qodec.
//!
//! Building a qodec runtime from a [`Qodec`] requires a non-trivial amount of
//! pre-processing. In order to cheaply instantiate a new qodec runtime for every
//! shot in a job requiring ~1M shots, we capture the data resulting from that
//! pre-processing in a [`QodecPlan`].

use super::{
    backend::{Backend, InstructionId, Param, PlanError, Readout},
    decoder::{Decoder, DecoderPlan},
    parity::Parity,
    resolver::Resolver,
    runtime::QodecRuntime,
    spec::{
        CodePlan, CompiledSelect, EncodingPlan, FrameKey, RefScope, TypeTable, bind_operands,
        compile_select, layout_boundary, prepare_frames,
    },
};
use qodec::{Argument, Circuit, Gadget, Instruction, InstructionSet, Layer, ParameterKind, Qodec};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, rc::Rc, sync::Arc};

/// Holds the data needed to instantiate a fresh qodec runtime.
pub struct QodecPlan<R> {
    layer_plans: Vec<Rc<LayerPlan>>,
    decoder_plans: Vec<Box<dyn DecoderPlan<R>>>,
}

impl<R: Readout + 'static> QodecPlan<R> {
    /// Plans every layer of `qodec` except the bottom one, and plans `decoder`
    /// for each of them.
    pub fn new(qodec: &Qodec, decoder: &dyn Decoder<R>) -> Result<Self, PlanError> {
        let layer_plans = build_layer_plans(qodec.layers(), decoder)?;
        let decoder_plans = build_decoder_plans(&layer_plans, decoder)?;

        Ok(Self {
            layer_plans,
            decoder_plans,
        })
    }

    /// The layer plans, top first.
    #[must_use]
    pub fn layer_plans(&self) -> &[Rc<LayerPlan>] {
        &self.layer_plans
    }

    /// Optional sizing, for a program that knows it needs `top_resources`: the
    /// resources at every boundary, `[top, below layer 0, ..., bottom backend]`,
    /// so the bottom backend can be built large enough. Runtimes do not
    /// need it.
    pub fn required_resources(
        &self,
        top_resources: Resources,
    ) -> Result<Vec<Resources>, PlanError> {
        // resources[k] is layer k's UPPER resources; it runs on resources[k+1].
        let mut resources = vec![top_resources];
        for (k, layer) in self.layer_plans.iter().enumerate() {
            let lower = layer
                .required_resources(&resources[k])
                .map_err(|e| PlanError(format!("layer {k}: {e}")))?;
            resources.push(lower);
        }
        Ok(resources)
    }

    /// Instantiates the runtime of one shot: fresh layer and decoder runtimes
    /// over `backend`, any [`Backend`] for the bottom ISA (a simulator, a
    /// profiler, ...), in its initial state. Layer runtimes allocate lower block
    /// ids on demand, densely from 0, so `backend` either holds enough blocks
    /// upfront (see [`Self::required_resources`]) or grows on demand. Pass
    /// `&mut backend` to reuse one backend across shots.
    pub fn instantiate<'a, Id, P, B>(&self, backend: B) -> QodecRuntime<'a, Id, P, R, B::Error>
    where
        Id: InstructionId + 'a,
        P: Param + 'a,
        B: Backend<Id, P, R> + 'a,
        B::Error: 'a,
    {
        QodecRuntime::new(&self.layer_plans, &self.decoder_plans, backend)
    }
}

fn build_layer_plans<R: Readout + 'static>(
    layers: &[Layer],
    decoder: &dyn Decoder<R>,
) -> Result<Vec<Rc<LayerPlan>>, PlanError> {
    if layers.is_empty() {
        return Err(PlanError("a qodec needs at least one layer".into()));
    }

    // Layers 0..n-1 have gadgets. With a single layer there are none, and
    // the runtime passes instructions straight to the bottom backend.
    let n = layers.len() - 1;
    let layer_plans = (0..n)
        .map(|k| {
            LayerPlan::new(
                &layers[k],
                &layers[k + 1].instruction_set,
                decoder.deferrable(),
            )
            .map(Rc::new)
            .map_err(|e| PlanError(format!("layer {k}: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(layer_plans)
}

fn build_decoder_plans<R: Readout + 'static>(
    layer_plans: &[Rc<LayerPlan>],
    decoder: &dyn Decoder<R>,
) -> Result<Vec<Box<dyn DecoderPlan<R>>>, PlanError> {
    let decoder_plans = layer_plans
        .iter()
        .map(|layer| decoder.plan(layer))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(decoder_plans)
}

/// Qubits (for a single-type ISA whose blocks hold one qubit) and blocks per type.
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

/// The plan of one argument of a gadget-body call.
#[derive(Clone, Debug, PartialEq)]
pub enum ArgumentPlan {
    /// A literal value.
    Value(Argument),
    /// Forwarded from this instruction parameter of the implemented instruction.
    Forwarded(String),
    /// `circuit.readouts[j]`, an earlier record bit.
    Readout(usize),
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
    /// The call's selection hint.
    pub select: CompiledSelect,
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
    /// Labels the body itself consumes. They are already gone below, so the
    /// layer must not release them again.
    pub consumed: FxHashSet<String>,
    /// Length of the circuit readout record.
    pub record_len: usize,
}

/// How a layer runs one gadget.
#[derive(Debug)]
pub struct GadgetPlan {
    /// The gadget's name: the mnemonic it implements.
    pub name: String,
    /// Index of the implemented instruction in the upper ISA.
    pub index: usize,
    /// The gadget as loaded.
    pub gadget: Gadget,
    /// The plan of its circuit.
    pub body: CircuitPlan,
    /// Input support, body, and output support labels, in that order, without
    /// duplicates.
    pub labels: Vec<String>,
    /// One encoding plan per `in` operand.
    pub inputs: Vec<EncodingPlan>,
    /// One encoding plan per `out` operand.
    pub outputs: Vec<EncodingPlan>,
    /// A label has ONE lower type for the whole gadget.
    pub label_type: FxHashMap<String, usize>,
    /// Output logical-sign corrections.
    pub frames: FxHashMap<FrameKey, Parity>,
}

impl GadgetPlan {
    /// The implemented instruction.
    #[must_use]
    pub fn instruction(&self) -> &Instruction {
        &self.gadget.implements
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

/// The plan of one qodec layer: everything needed to instantiate its runtime, built once.
#[derive(Debug)]
pub struct LayerPlan {
    /// Upper ISA: decodes incoming Ids.
    pub isa: Arc<InstructionSet>,
    /// The next layer's ISA.
    pub lower_isa: Arc<InstructionSet>,
    /// Upper block types.
    pub upper: TypeTable,
    /// Lower block types.
    pub lower: TypeTable,
    /// Upper block type -> the plan of its code.
    pub codes: BTreeMap<String, Rc<CodePlan>>,
    /// Upper block type -> lower blocks per lower type.
    pub requirements: BTreeMap<String, Vec<usize>>,
    /// Lower blocks per lower type reserved for gadget scratch.
    pub scratch: Vec<usize>,
    /// Per upper instruction: how the layer handles it.
    pub instructions: Vec<InstructionPlan>,
    /// Matches the layer's own generic operations against the lower ISA.
    pub resolver: Resolver,
    /// Per lower type `t`: an instruction with `in = [t]` (not variadic),
    /// `out = []`, no observe and no flags. If a type has none, releases of
    /// that type are lazy.
    pub lower_release: Vec<Option<usize>>,
    /// Per upper instruction: whether it returns a result, which it does iff it
    /// has observe outcomes or flags.
    pub upper_needs_result: Vec<bool>,
    /// Per lower instruction: (observe count, flag count) of its result.
    pub lower_shapes: Vec<(usize, usize)>,
}

fn max_merge(into: &mut [usize], from: &[usize]) {
    for (a, b) in into.iter_mut().zip(from) {
        *a = (*a).max(*b);
    }
}

impl LayerPlan {
    /// Plans `layer`, whose gadgets lower it to `lower_isa`.
    ///
    /// `decoder_deferrable` is whether the chosen [`Decoder`] is deferrable. A
    /// decoder that is not deferrable may issue Pauli corrections, so planning
    /// then requires the lower ISA to bind `x` and `z` directly on every lower
    /// type. The same holds when any upper instruction is planned as
    /// [`InstructionPlan::LogicalPauli`].
    pub fn new(
        layer: &Layer,
        lower_isa: &Arc<InstructionSet>,
        decoder_deferrable: bool,
    ) -> Result<Self, PlanError> {
        let isa = Arc::clone(&layer.instruction_set);
        let upper = TypeTable::new(&isa);
        let lower = TypeTable::new(lower_isa);
        let codes = plan_codes(layer)?;
        let mut requirements: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut scratch = vec![0; lower.len()];
        let mut instructions = vec![InstructionPlan::Unsupported; isa.instructions.len()];

        let context = GadgetContext {
            isa: &isa,
            lower_isa,
            codes: &codes,
            upper: &upper,
            lower: &lower,
        };
        for (name, g) in &layer.gadgets {
            let gp = context
                .plan_gadget(name, g)
                .map_err(|e| PlanError(format!("gadget `{name}`: {e}")))?;
            for layout in gp.inputs.iter().chain(&gp.outputs) {
                let entry = requirements
                    .entry(layout.block_type.clone())
                    .or_insert_with(|| vec![0; lower.len()]);
                max_merge(entry, &layout.resources(&lower));
            }
            max_merge(&mut scratch, &gadget_scratch(&gp, &lower));
            let index = gp.index;
            instructions[index] = InstructionPlan::Gadget(Rc::new(gp));
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

        // Pauli corrections are checked only where they can occur.
        if instructions
            .iter()
            .any(|h| matches!(h, InstructionPlan::LogicalPauli))
            || !decoder_deferrable
        {
            for (t, name) in lower.names.iter().enumerate() {
                for op in ["x", "z"] {
                    if !resolver.binds_directly(op, t, &lower) {
                        return Err(PlanError(format!(
                            "the lower ISA has no `{op}` Pauli for block type `{name}`"
                        )));
                    }
                }
            }
        }

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
        Ok(Self {
            isa,
            lower_isa: Arc::clone(lower_isa),
            upper,
            lower,
            codes,
            requirements,
            scratch,
            instructions,
            resolver,
            lower_release,
            upper_needs_result,
            lower_shapes,
        })
    }

    /// The lower resources needed to run `upper` resources on this layer.
    pub fn required_resources(&self, upper: &Resources) -> Result<Resources, PlanError> {
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

/// What planning one gadget needs from its layer.
struct GadgetContext<'a> {
    isa: &'a InstructionSet,
    lower_isa: &'a InstructionSet,
    codes: &'a BTreeMap<String, Rc<CodePlan>>,
    upper: &'a TypeTable,
    lower: &'a TypeTable,
}

impl GadgetContext<'_> {
    fn plan_gadget(&self, name: &str, g: &Gadget) -> Result<GadgetPlan, String> {
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

        Ok(GadgetPlan {
            name: name.to_string(),
            index,
            gadget: g.clone(),
            body,
            labels,
            inputs,
            outputs,
            label_type,
            frames,
        })
    }
}

/// Lower blocks per type a gadget needs beyond its boundary.
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

/// Plans the layer's code bindings: explicit layer codes, completed by gadget
/// encodings (which `qodec` allows to supply omitted bindings).
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

/// The action is non-empty and has only unguarded `pauli` steps, the
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

/// `in = [t]` with `t` not variadic, `out = []`, empty action, no flags.
fn is_release(instruction: &Instruction) -> bool {
    matches!(instruction.inputs.as_slice(), [op] if !op.is_variadic)
        && instruction.outputs.is_empty()
        && instruction.action.is_empty()
        && instruction.flags.is_empty()
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
