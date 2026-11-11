// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Reads a [`Qodec`] into a [`QodecModel`]: checks every layer once against
//! the qodec spec, and resolves it into plain data on interned ids.

use super::spec::{
    self, CodePlan, EncodingPlan, RefScope, compile_select, encode_arg, expr, layout_boundary,
    matches, param_kind, prepare_frames, signature,
};
use qodec::{Argument, Circuit, Gadget, Instruction, InstructionSet, Layer, ParameterKind, Qodec};
use qodec_runtime_model::{
    Arg, CallModel, CodeModel, CompiledSelect, EncodingModel, GadgetId, GadgetModel, Isa,
    LayerModel, PlanError, QodecModel, TypeTable,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, sync::Arc};

/// Reads `qodec`: every layer but the bottom one becomes a [`LayerModel`],
/// which lowers it to the next layer's ISA. A qodec with one layer has no
/// layer models; its ISA is both the top and the bottom one.
pub fn read(qodec: &Qodec) -> Result<QodecModel, PlanError> {
    let layers = qodec.layers();
    if layers.is_empty() {
        return Err(PlanError("a qodec needs at least one layer".into()));
    }
    let isas: Vec<Arc<Isa>> = layers
        .iter()
        .enumerate()
        .map(|(k, layer)| {
            spec::isa(&layer.instruction_set)
                .map(Arc::new)
                .map_err(|e| PlanError(format!("layer {k}: {e}")))
        })
        .collect::<Result<_, _>>()?;
    let models = (0..layers.len() - 1)
        .map(|k| {
            read_layer(
                &layers[k],
                &layers[k + 1].instruction_set,
                &isas[k],
                &isas[k + 1],
            )
            .map(Arc::new)
            .map_err(|e| PlanError(format!("layer {k}: {e}")))
        })
        .collect::<Result<_, _>>()?;
    Ok(QodecModel {
        layers: models,
        top: Arc::clone(&isas[0]),
        bottom: Arc::clone(&isas[isas.len() - 1]),
    })
}

/// Reads `layer`, whose gadgets lower it to `lower_isa`. `upper` and `lower`
/// are the two ISAs in the model.
fn read_layer(
    layer: &Layer,
    lower_isa: &Arc<InstructionSet>,
    upper: &Arc<Isa>,
    lower: &Arc<Isa>,
) -> Result<LayerModel, PlanError> {
    let codes = plan_codes(layer)?;
    let code_models: BTreeMap<String, Arc<CodeModel>> = codes
        .iter()
        .map(|(block, code)| (block.clone(), Arc::new(code.model())))
        .collect();
    let context = GadgetContext {
        isa: &layer.instruction_set,
        lower_isa,
        codes: &codes,
        code_models: &code_models,
        upper: &upper.types,
        lower: &lower.types,
    };
    let mut gadgets = Vec::with_capacity(layer.gadgets.len());
    let mut gadget_of = vec![None; upper.instructions.len()];
    for (id, (name, g)) in layer.gadgets.iter().enumerate() {
        let gadget = context
            .read_gadget(GadgetId(id), name, g)
            .map_err(|e| PlanError(format!("gadget `{name}`: {e}")))?;
        gadget_of[gadget.instruction] = Some(GadgetId(id));
        gadgets.push(Arc::new(gadget));
    }
    Ok(LayerModel {
        upper: Arc::clone(upper),
        lower: Arc::clone(lower),
        codes: upper
            .types
            .names
            .iter()
            .map(|name| code_models.get(name).cloned())
            .collect(),
        gadgets,
        gadget_of,
    })
}

/// Plans the layer's code bindings: the layer's `codes` for block types its
/// ISA declares, plus, for each operand type the layer leaves unbound, the code
/// of its gadget encodings. [`layout_boundary`] later rejects any encoding
/// whose code differs from the result.
fn plan_codes(layer: &Layer) -> Result<BTreeMap<String, Arc<CodePlan>>, PlanError> {
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
            Ok((block, Arc::new(bound)))
        })
        .collect()
}

/// What reading one gadget needs from its layer.
struct GadgetContext<'a> {
    isa: &'a InstructionSet,
    lower_isa: &'a InstructionSet,
    codes: &'a BTreeMap<String, Arc<CodePlan>>,
    code_models: &'a BTreeMap<String, Arc<CodeModel>>,
    upper: &'a TypeTable,
    lower: &'a TypeTable,
}

impl GadgetContext<'_> {
    fn read_gadget(&self, id: GadgetId, name: &str, g: &Gadget) -> Result<GadgetModel, String> {
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

        let (labels, label_type) = intern_labels(&inputs, &body, &outputs, lower)?;

        let scope = RefScope {
            record_len: body.record_len,
            readouts_len: g.readouts.len(),
            inputs: &inputs,
            outputs: &outputs,
        };
        let frames = prepare_frames(g, &scope)?;
        let checks = g
            .checks
            .iter()
            .map(|c| expr(c, &scope))
            .collect::<Result<_, _>>()?;
        let readouts = g
            .readouts
            .iter()
            .map(|r| expr(&r.equation, &scope))
            .collect::<Result<_, _>>()?;

        let id_of: FxHashMap<&str, usize> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| (label.as_str(), i))
            .collect();
        let encoding = |layout: &EncodingPlan| EncodingModel {
            block_type: self
                .upper
                .get(&layout.block_type)
                .expect("the boundary layout checked the block type"),
            code: Arc::clone(&self.code_models[&layout.block_type]),
            labels: layout.labels.iter().map(|l| id_of[l.as_str()]).collect(),
        };
        let calls = body
            .calls
            .iter()
            .map(|cp| CallModel {
                instruction: cp.instruction,
                operands: cp.operands.iter().map(|o| id_of[o.as_str()]).collect(),
                args: compile_args(cp, &self.lower_isa.instructions[cp.instruction]),
                select: cp.select.clone(),
            })
            .collect();

        let inputs = inputs.iter().map(encoding).collect();
        let outputs = outputs.iter().map(encoding).collect();
        let label_types = labels.iter().map(|l| label_type[l]).collect();
        Ok(GadgetModel {
            id,
            name: name.to_string(),
            instruction: index,
            label_types,
            labels,
            inputs,
            outputs,
            body: calls,
            record_len: body.record_len,
            checks,
            readouts,
            frames,
        })
    }
}

/// The labels of a gadget, in label-id order: input supports first, then body
/// labels, then output supports, each in first-appearance order. And each
/// label's lower type, which the encodings and the body must agree on.
fn intern_labels(
    inputs: &[EncodingPlan],
    body: &CircuitPlan,
    outputs: &[EncodingPlan],
    lower: &TypeTable,
) -> Result<(Vec<String>, FxHashMap<String, usize>), String> {
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
    for layout in inputs.iter().chain(outputs) {
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
    Ok((labels, label_type))
}

/// A call's arguments in the lower instruction's parameter order, with
/// literal values validated and encoded once, or `None` if they can never be
/// valid. Invalid arguments are not an error of the model, so that a qodec
/// whose broken gadget a program never invokes still runs.
fn compile_args(cp: &CallPlan, instruction: &Instruction) -> Option<Vec<Arg>> {
    (cp.args.len() == instruction.parameters.len())
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
                            Arg::Value(encode_arg(value)?)
                        }
                        ArgumentPlan::Forwarded(name) => Arg::Forwarded {
                            name: name.clone(),
                            kind: param_kind(parameter.kind),
                        },
                        ArgumentPlan::Readout(j) => {
                            if !matches(parameter.kind, &Argument::Boolean(false)) {
                                return None;
                            }
                            Arg::Readout(*j)
                        }
                    })
                })
                .collect::<Option<Vec<_>>>()
        })
        .flatten()
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
        let mut offset = 0;

        for call in &calls {
            let index = isa
                .instructions
                .iter()
                .position(|i| i.mnemonic == call.mnemonic)
                .ok_or_else(|| format!("unknown instruction `{}`", call.mnemonic))?;
            let instruction = &isa.instructions[index];
            let operands: Vec<String> = call.operands.iter().map(qodec::Operand::label).collect();
            let (ins, outs) = signature(instruction, lower)?
                .bind(lower, &operands, &FxHashMap::default())
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

            let select = compile_select(&instruction.flags, &call.select)
                .map_err(|e| format!("call `{}`: {e}", call.mnemonic))?;
            offset += instruction.observe_count() + instruction.flags.len();

            for label in &operands {
                if seen_labels.insert(label.clone()) {
                    labels.push(label.clone());
                }
            }
            for bound in ins.iter().chain(&outs) {
                block_types.insert(bound.label.clone(), bound.block_type);
            }
            call_plans.push(CallPlan {
                instruction: index,
                operands,
                args,
                select,
            });
        }

        Ok(CircuitPlan {
            labels,
            block_types,
            calls: call_plans,
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
