// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a gadget computes, and whether it does what its instruction says.
//!
//! The analysis runs the gadget's circuit symbolically, with the lower
//! instruction set's actions, on an input where every block is in its code
//! space with unknown stabilizer signs, and every logical qubit is entangled
//! with a reference qubit under an unknown frame. It also runs the implemented
//! instruction's action on bare logical qubits, entangled the same way. Then:
//!
//! - every measurement's value is a formula over the incoming signs, random
//!   bits and bit parameters;
//! - a check is valid when its terms sum to the constant 0;
//! - the gadget implements its instruction when every operator the ideal
//!   output holds is held by the actual output, encoded in the output code,
//!   with the sign the decoder predicts from the readouts and frames;
//! - solving that last condition for the readouts and frames derives them.
//!
//! This is the spec's [gadget verification](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/validation.md#gadget-verification),
//! made visible: every value is shown, not only whether it is right.

use crate::{
    gf2::{Basis as FormBasis, Bits, Form},
    stabilizer::{Found, Pauli, Signed, Tableau},
};
use qodec::{
    Action, Argument, Code, Gadget, Instruction, InstructionSet, ParameterKind, ParityEquation,
    ParityTerm, Reference, ReferenceSegment,
};
use qodec_runtime_recipes::pauli::{Clifford, Pauli as SparsePauli};
use std::{collections::BTreeMap, fmt::Write};

/// What a sign belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Basis {
    /// A code stabilizer.
    Stabilizer,
    /// A logical X.
    X,
    /// A logical Z.
    Z,
}

impl Basis {
    fn name(self) -> &'static str {
        match self {
            Basis::Stabilizer => "stabilizers",
            Basis::X => "x",
            Basis::Z => "z",
        }
    }
}

/// What a variable of a formula stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Var {
    /// An incoming sign: of a stabilizer, or of a logical operator (the
    /// incoming Pauli frame).
    In {
        /// The input operand.
        e: usize,
        /// Which operator.
        basis: Basis,
        /// Its index in its list.
        k: usize,
    },
    /// The outcome of a random measurement: the `index`-th outcome of a call.
    Random {
        /// The circuit call.
        call: usize,
        /// The outcome's position among the call's outcomes.
        index: usize,
    },
    /// A `bit` parameter of the implemented instruction.
    Param(String),
    /// The ideal outcome of the instruction's `i`-th observable.
    Outcome(usize),
}

impl Var {
    /// The variable as a reference or a short name.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Var::In { e, basis, k } => format!("in[{e}].{}[{k}]", basis.name()),
            Var::Random { call, index: 0 } => format!("random@{call}"),
            Var::Random { call, index } => format!("random@{call}.{index}"),
            Var::Param(name) => name.clone(),
            Var::Outcome(i) => format!("outcome {i}"),
        }
    }
}

/// The variables of an analysis.
#[derive(Clone, Debug, Default)]
pub struct Vars(Vec<Var>);

impl Vars {
    fn add(&mut self, var: Var) -> usize {
        if let Some(i) = self.0.iter().position(|v| *v == var) {
            return i;
        }
        self.0.push(var);
        self.0.len() - 1
    }

    /// The variable with index `i`.
    #[must_use]
    pub fn get(&self, i: usize) -> &Var {
        &self.0[i]
    }

    fn mask(&self, keep: impl Fn(&Var) -> bool) -> Bits {
        let mut bits = Bits::default();
        for (i, v) in self.0.iter().enumerate() {
            if keep(v) {
                bits.set(i, true);
            }
        }
        bits
    }

    /// `form` written out: `in[0].stabilizers[1] ⊕ random@3 ⊕ 1`, or `0`.
    #[must_use]
    pub fn show(&self, form: &Form) -> String {
        let mut parts: Vec<String> = form.vars.ones().map(|i| self.0[i].name()).collect();
        if form.constant || parts.is_empty() {
            parts.push(u8::from(form.constant).to_string());
        }
        parts.join(" ⊕ ")
    }
}

/// A term of a parity equation, as the spec writes it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Term {
    /// `circuit.readouts[i]`.
    Record(usize),
    /// `readouts[i]`.
    Readout(usize),
    /// `in[e].<basis>[k]` or `out[e].<basis>[k]`.
    Sign {
        /// Whether it is an output sign.
        out: bool,
        /// The operand.
        e: usize,
        /// Which operator.
        basis: Basis,
        /// Its index.
        k: usize,
    },
    /// The literal 1.
    One,
}

impl Term {
    /// The term as a YAML string, or `1`.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Term::Record(i) => format!("circuit.readouts[{i}]"),
            Term::Readout(i) => format!("readouts[{i}]"),
            Term::Sign { out, e, basis, k } => {
                format!(
                    "{}[{e}].{}[{k}]",
                    if *out { "out" } else { "in" },
                    basis.name()
                )
            }
            Term::One => "1".to_string(),
        }
    }
}

/// An equation as a list of terms, written as the spec's YAML flow list.
#[must_use]
pub fn equation_text(terms: &[Term]) -> String {
    let parts: Vec<String> = terms
        .iter()
        .map(|t| match t {
            Term::One => "1".to_string(),
            t => format!("\"{}\"", t.text()),
        })
        .collect();
    format!("[{}]", parts.join(", "))
}

/// One bit of the circuit's record.
#[derive(Clone, Debug)]
pub struct RecordBit {
    /// The call that reported it.
    pub call: usize,
    /// What it is: the observable it measures, or the flag's name.
    pub what: String,
    /// Its value.
    pub value: Found,
}

/// A declared equation, and what it comes to.
#[derive(Clone, Debug)]
pub struct Verdict {
    /// The equation, as written.
    pub text: String,
    /// What its terms sum to, or why it cannot be evaluated. It is right when
    /// this is the constant 0.
    pub residual: Result<Form, String>,
    /// Whether the analysis can't tell, because it could not follow a step
    /// the equation depends on.
    pub unknown: bool,
}

impl Verdict {
    /// Whether it holds.
    #[must_use]
    pub fn holds(&self) -> bool {
        self.residual.as_ref().is_ok_and(Form::is_zero)
    }

    /// Whether it is known not to hold.
    #[must_use]
    pub fn fails(&self) -> bool {
        !self.holds() && !self.unknown
    }
}

/// An output code stabilizer.
#[derive(Clone, Debug)]
pub struct OutStabilizer {
    /// The output operand.
    pub e: usize,
    /// The stabilizer's index.
    pub k: usize,
    /// Its sign in the output state, or `None` if the output does not hold it.
    pub sign: Option<Form>,
    /// Whether the declared checks determine its sign.
    pub determined: bool,
}

/// Whether the circuit implements the instruction, given the readouts and
/// frames.
#[derive(Clone, Debug, Default)]
pub struct ActionVerdict {
    /// What does not match: one line per ideal operator that the output does
    /// not hold, or holds with another sign.
    pub mismatches: Vec<String>,
    /// Whether the analysis could compare at all: it can't past a step it
    /// cannot follow, such as a rotation.
    pub compared: bool,
}

/// What the tool derives for a gadget, to adopt.
#[derive(Clone, Debug, Default)]
pub struct Suggestions {
    /// Valid checks the declared ones do not imply.
    pub checks: Vec<Vec<Term>>,
    /// One equation per readout (observables, then flags), if it can be
    /// derived.
    pub readouts: Vec<Option<Vec<Term>>>,
    /// Frames, by output logical sign, that the action needs.
    pub frames: Vec<((usize, Basis, usize), Vec<Term>)>,
    /// Why some could not be derived.
    pub problems: Vec<String>,
}

/// One call of the circuit, for drawing it.
#[derive(Clone, Debug, Default)]
pub struct CallInfo {
    /// The call, as the circuit writes it: `CX 0 3`.
    pub text: String,
    /// The mnemonic.
    pub mnemonic: String,
    /// The labels it takes.
    pub ins: Vec<String>,
    /// The labels it gives.
    pub outs: Vec<String>,
    /// The bits of the record it reports.
    pub record: std::ops::Range<usize>,
}

/// The state after one call, for showing it.
#[derive(Clone, Debug)]
pub struct Snapshot {
    /// The generators, as Paulis on [`Analysis::qubits`] and their signs.
    pub gens: Vec<Signed>,
}

/// Everything the analysis found.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// What each qubit position is: a circuit label and the qubit in it, or a
    /// reference qubit.
    pub qubits: Vec<String>,
    /// The variables.
    pub vars: Vars,
    /// The incoming signs, by operand, kind and index: a variable, or, for a
    /// stabilizer that the code's other stabilizers generate, their product.
    pub in_signs: BTreeMap<(usize, Basis, usize), Form>,
    /// Whether the analysis followed every step exactly. When it did not,
    /// verdicts that depend on what it lost are unknown, not wrong.
    pub followed: bool,
    /// The circuit's record.
    pub record: Vec<RecordBit>,
    /// The circuit's labels: input supports, then others as calls name them,
    /// then output supports.
    pub labels: Vec<String>,
    /// The circuit's calls.
    pub calls: Vec<CallInfo>,
    /// The labels each input operand sits on.
    pub in_supports: Vec<Vec<String>>,
    /// The labels each output operand sits on.
    pub out_supports: Vec<Vec<String>>,
    /// Per call, what it does not let the analysis follow exactly.
    pub notes: Vec<(usize, String)>,
    /// Per call, what looks wrong with it.
    pub warnings: Vec<(usize, String)>,
    /// The state before the first call, then after each call.
    pub snapshots: Vec<Snapshot>,
    /// The declared checks.
    pub checks: Vec<Verdict>,
    /// The declared readouts.
    pub readouts: Vec<Verdict>,
    /// Whether each declared readout is a flag.
    pub readout_is_flag: Vec<bool>,
    /// The declared frames.
    pub frames: Vec<Verdict>,
    /// The output code stabilizers.
    pub out_stabilizers: Vec<OutStabilizer>,
    /// Whether the circuit implements the action.
    pub action: ActionVerdict,
    /// What the tool derived.
    pub suggestions: Suggestions,
}

impl Analysis {
    /// The number of problems: declared equations known not to hold, output
    /// stabilizers that are missing or undetermined, and action mismatches.
    #[must_use]
    pub fn problems(&self) -> usize {
        self.checks.iter().filter(|v| v.fails()).count()
            + self.readouts.iter().filter(|v| v.fails()).count()
            + self.frames.iter().filter(|v| v.fails()).count()
            + if self.followed {
                self.out_stabilizers
                    .iter()
                    .filter(|s| s.sign.is_none() || !s.determined)
                    .count()
            } else {
                0
            }
            + self.action.mismatches.len()
    }
}

/// A declared frame: the output sign it fixes, and its terms.
type DeclaredFrame = (
    Result<(usize, Basis, usize), String>,
    Result<Vec<Term>, String>,
);

/// Analyzes `gadget`, which implements an instruction of `upper` with a circuit
/// of `lower`. Fails only when the circuit cannot be read at all; everything
/// else it can't follow is a note or a verdict.
#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
pub fn analyze(gadget: &Gadget, lower: &InstructionSet) -> Result<Analysis, String> {
    let calls = gadget.circuit.calls()?;
    let implemented = &gadget.implements;
    let mut a = Analysis::default();

    // ── Qubit positions ────────────────────────────────────────────────────
    let caps: BTreeMap<&str, usize> = lower
        .blocks
        .iter()
        .map(|b| (b.name.as_str(), b.encodes))
        .collect();
    let single = (lower.blocks.len() == 1).then(|| lower.blocks[0].name.clone());
    let mut label_type: BTreeMap<String, String> = BTreeMap::new();
    let mut learn = |label: &str, block: &str| -> Result<(), String> {
        match label_type.get(label) {
            Some(t) if t != block => Err(format!("label `{label}` is both `{t}` and `{block}`")),
            Some(_) => Ok(()),
            None => {
                label_type.insert(label.to_string(), block.to_string());
                Ok(())
            }
        }
    };
    for encoding in gadget.inputs.iter().chain(&gadget.outputs) {
        for (i, label) in encoding.support.iter().enumerate() {
            match encoding.block_types.get(i).or(single.as_ref()) {
                Some(t) => learn(label, t)?,
                None => return Err(format!("label `{label}` has no block type")),
            }
        }
    }
    let mut instructions = Vec::with_capacity(calls.len());
    for call in &calls {
        let instruction = lower
            .instructions
            .iter()
            .find(|i| i.mnemonic == call.mnemonic)
            .ok_or_else(|| {
                format!(
                    "`{}` is not an instruction of `{}`",
                    call.mnemonic, lower.name
                )
            })?;
        for (j, operand) in call.operands.iter().enumerate() {
            let declared = instruction
                .inputs
                .get(j)
                .or(instruction.outputs.get(j))
                .or_else(|| instruction.inputs.last().filter(|o| o.is_variadic))
                .or_else(|| instruction.outputs.last().filter(|o| o.is_variadic))
                .ok_or_else(|| format!("`{}` takes fewer operands", call.mnemonic))?;
            learn(&operand.label(), &declared.block)?;
        }
        instructions.push(instruction);
    }
    for encoding in &gadget.inputs {
        for l in &encoding.support {
            if !a.labels.contains(l) {
                a.labels.push(l.clone());
            }
        }
    }
    for call in &calls {
        for o in &call.operands {
            let l = o.label();
            if !a.labels.contains(&l) {
                a.labels.push(l);
            }
        }
    }
    for encoding in &gadget.outputs {
        for l in &encoding.support {
            if !a.labels.contains(l) {
                a.labels.push(l.clone());
            }
        }
    }
    a.in_supports = gadget.inputs.iter().map(|e| e.support.clone()).collect();
    a.out_supports = gadget.outputs.iter().map(|e| e.support.clone()).collect();
    let mut positions: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (label, t) in &label_type {
        let cap = *caps
            .get(t.as_str())
            .ok_or_else(|| format!("unknown block type `{t}`"))?;
        let start = a.qubits.len();
        for i in 0..cap {
            a.qubits.push(if cap == 1 {
                label.clone()
            } else {
                format!("{label}.{i}")
            });
        }
        positions.insert(label.clone(), (start..start + cap).collect());
    }
    let code_qubits = |support: &[String]| -> Vec<usize> {
        support.iter().flat_map(|l| positions[l].clone()).collect()
    };
    // One reference qubit per input logical qubit.
    let mut refs: Vec<usize> = Vec::new();
    for (e, encoding) in gadget.inputs.iter().enumerate() {
        for k in 0..encoding.code.x.len() {
            refs.push(a.qubits.len());
            a.qubits.push(format!("ref{e}.{k}"));
        }
    }
    let scratch = a.qubits.len();

    // ── The input state ────────────────────────────────────────────────────
    let mut t = Tableau::default();
    let mut live: Vec<String> = Vec::new();
    let mut r = 0;
    for (e, encoding) in gadget.inputs.iter().enumerate() {
        let qubits = code_qubits(&encoding.support);
        let code = parse_code(&encoding.code)?;
        for (k, s) in code.stabilizers.iter().enumerate() {
            let (p, _) = Pauli::place(s, |i| qubits.get(i).copied())?;
            // A stabilizer the others generate has the sign of their product.
            let sign = if let Some(product) = t.sign_of(&p) {
                product
            } else {
                let v = Form::var(a.vars.add(Var::In {
                    e,
                    basis: Basis::Stabilizer,
                    k,
                }));
                t.add(p, v.clone());
                v
            };
            a.in_signs.insert((e, Basis::Stabilizer, k), sign);
        }
        for k in 0..code.x.len() {
            for (basis, op, ref_x) in [(Basis::X, &code.x[k], true), (Basis::Z, &code.z[k], false)]
            {
                let (mut p, _) = Pauli::place(op, |i| qubits.get(i).copied())?;
                p.x.set(refs[r], ref_x);
                p.z.set(refs[r], !ref_x);
                let v = Form::var(a.vars.add(Var::In { e, basis, k }));
                a.in_signs.insert((e, basis, k), v.clone());
                t.add(p, v);
            }
            r += 1;
        }
        live.extend(encoding.support.iter().cloned());
    }
    a.snapshots.push(Snapshot {
        gens: t.gens.clone(),
    });

    // ── The circuit ────────────────────────────────────────────────────────
    let bit_param = |name: &str| -> Option<String> {
        // A source parameter name, bound to an instruction parameter, or the
        // instruction parameter's own name.
        let bound = gadget
            .parameter_bindings
            .iter()
            .find(|(_, source)| source.as_str() == name)
            .map(|(p, _)| p.clone());
        bound.or_else(|| Some(name.to_string())).filter(|p| {
            implemented
                .parameters
                .iter()
                .any(|q| &q.name == p && q.kind == ParameterKind::Bit)
        })
    };
    for (c, (call, instruction)) in calls.iter().zip(&instructions).enumerate() {
        let labels: Vec<String> = call.operands.iter().map(qodec::Operand::label).collect();
        // Operand j is in[j] and/or out[j]; a variadic side takes them all.
        let count = |operands: &[qodec::BlockOperand]| {
            if operands.iter().any(|o| o.is_variadic) {
                labels.len()
            } else {
                operands.len().min(labels.len())
            }
        };
        let (n_in, n_out) = (count(&instruction.inputs), count(&instruction.outputs));
        let flat = |n: usize| -> Vec<usize> {
            labels[..n]
                .iter()
                .flat_map(|l| positions[l].clone())
                .collect()
        };
        let (in_pos, out_pos) = (flat(n_in), flat(n_out));
        for label in &labels[..n_in] {
            if !live.contains(label) {
                a.warnings.push((
                    c,
                    format!("`{label}` is used before any call gives it a state"),
                ));
                live.push(label.clone());
            }
        }
        // Output-only operands start fresh: a live block there is overwritten.
        for label in labels.get(n_in..n_out).unwrap_or_default() {
            if live.contains(label) {
                t.trace_out(&positions[label]);
            } else {
                live.push(label.clone());
            }
        }
        let bound = in_pos.len().max(out_pos.len());
        let temporaries = extent(&instruction.action).saturating_sub(bound);
        let place = |i: usize| {
            in_pos
                .get(i)
                .or(out_pos.get(i))
                .copied()
                .or((i < bound + temporaries).then(|| scratch + i - bound))
        };
        let mut env = Env::default();
        for parameter in &instruction.parameters {
            let Some(value) = call.arguments.get(&parameter.name) else {
                a.warnings
                    .push((c, format!("argument `{}` is missing", parameter.name)));
                continue;
            };
            match (parameter.kind, value) {
                (ParameterKind::Bit | ParameterKind::Boolean, Argument::Readout(j)) => {
                    match a.record.get(*j) {
                        Some(bit) => {
                            env.bits
                                .insert(parameter.name.clone(), bit.value.form().clone());
                        }
                        None => a.warnings.push((
                            c,
                            format!("`circuit.readouts[{j}]` is not reported before this call"),
                        )),
                    }
                }
                (ParameterKind::Bit | ParameterKind::Boolean, Argument::Integer(v)) => {
                    env.bits
                        .insert(parameter.name.clone(), Form::constant(*v == 1));
                }
                (ParameterKind::Bit | ParameterKind::Boolean, Argument::Boolean(v)) => {
                    env.bits.insert(parameter.name.clone(), Form::constant(*v));
                }
                (ParameterKind::Bit | ParameterKind::Boolean, Argument::Text(name)) => {
                    match bit_param(name) {
                        Some(p) => {
                            let v = a.vars.add(Var::Param(p));
                            env.bits.insert(parameter.name.clone(), Form::var(v));
                        }
                        None => a.warnings.push((
                            c,
                            format!(
                                "`{name}` is not a bit parameter of `{}`",
                                implemented.mnemonic
                            ),
                        )),
                    }
                }
                (ParameterKind::Pauli, Argument::Text(text)) => {
                    env.paulis.insert(parameter.name.clone(), text.clone());
                }
                _ => {}
            }
        }
        let mut text = call.mnemonic.clone();
        for l in &labels {
            text.push(' ');
            text.push_str(l);
        }
        for (name, value) in &call.arguments {
            let v = match value {
                Argument::Readout(j) => format!("circuit.readouts[{j}]"),
                Argument::Integer(i) => i.to_string(),
                Argument::Number(x) => x.to_string(),
                Argument::Boolean(b) => b.to_string(),
                Argument::Text(t) => t.clone(),
                other => format!("{other:?}"),
            };
            write!(text, " {name}={v}").expect("writing to a string succeeds");
        }
        let record_start = a.record.len();
        let mut outcomes = Vec::new();
        let mut notes = Vec::new();
        let mut fresh = |index: usize| a.vars.add(Var::Random { call: c, index });
        apply(
            &mut t,
            &instruction.action,
            &place,
            &env,
            &mut fresh,
            &mut outcomes,
            &mut notes,
        );
        let observables = observables(instruction);
        for (i, found) in outcomes.into_iter().enumerate() {
            a.record.push(RecordBit {
                call: c,
                what: observables.get(i).cloned().unwrap_or_default(),
                value: found,
            });
        }
        for flag in &instruction.flags {
            a.record.push(RecordBit {
                call: c,
                what: flag.clone(),
                value: Found::Determined(Form::default()),
            });
        }
        a.notes.extend(notes.into_iter().map(|n| (c, n)));
        a.calls.push(CallInfo {
            text,
            mnemonic: call.mnemonic.clone(),
            ins: labels[..n_in].to_vec(),
            outs: labels[..n_out].to_vec(),
            record: record_start..a.record.len(),
        });

        // Input-only operands end here, and so do temporary qubits.
        for label in labels.get(n_out..n_in).unwrap_or_default() {
            t.trace_out(&positions[label]);
            live.retain(|l| l != label);
        }
        t.trace_out(&(scratch..scratch + temporaries).collect::<Vec<_>>());
        a.snapshots.push(Snapshot {
            gens: t.gens.clone(),
        });
    }

    // ── The output state ───────────────────────────────────────────────────
    let out_labels: Vec<&String> = gadget.outputs.iter().flat_map(|e| &e.support).collect();
    let dropped: Vec<usize> = live
        .iter()
        .filter(|l| !out_labels.contains(l))
        .flat_map(|l| positions[l].clone())
        .collect();
    t.trace_out(&dropped);
    let mut out_codes = Vec::new();
    for (e, encoding) in gadget.outputs.iter().enumerate() {
        let qubits = code_qubits(&encoding.support);
        let code = parse_code(&encoding.code)?;
        for (k, s) in code.stabilizers.iter().enumerate() {
            let (p, _) = Pauli::place(s, |i| qubits.get(i).copied())?;
            a.out_stabilizers.push(OutStabilizer {
                e,
                k,
                sign: t.sign_of(&p),
                determined: false,
            });
        }
        out_codes.push((qubits, code));
    }

    // ── The ideal action, and the decoder's transport ──────────────────────
    let upper_caps = |operands: &[qodec::BlockOperand], codes: &[qodec::Encoding]| -> Vec<usize> {
        operands
            .iter()
            .zip(codes)
            .map(|(_, enc)| enc.code.x.len())
            .collect()
    };
    let in_logicals = upper_caps(&implemented.inputs, &gadget.inputs);
    let out_logicals = upper_caps(&implemented.outputs, &gadget.outputs);
    let n_in: usize = in_logicals.iter().sum();
    let n_out: usize = out_logicals.iter().sum();
    let width = n_in.max(n_out).max(extent(&implemented.action));
    let mut ideal = Tableau::default();
    for q in 0..n_in {
        let rq = width + q;
        let mut xx = Pauli::single(q, true, false);
        xx.x.set(rq, true);
        let mut zz = Pauli::single(q, false, true);
        zz.z.set(rq, true);
        ideal.add(xx, Form::default());
        ideal.add(zz, Form::default());
    }
    let mut env = Env::default();
    for p in &implemented.parameters {
        if p.kind == ParameterKind::Bit {
            let v = a.vars.add(Var::Param(p.name.clone()));
            env.bits.insert(p.name.clone(), Form::var(v));
        }
    }
    let mut ideal_outcomes = Vec::new();
    let mut ideal_notes = Vec::new();
    let mut fresh = |index: usize| a.vars.add(Var::Outcome(index));
    apply(
        &mut ideal,
        &implemented.action,
        &|i| Some(i),
        &env,
        &mut fresh,
        &mut ideal_outcomes,
        &mut ideal_notes,
    );
    ideal.trace_out(&(n_out..width).collect::<Vec<_>>());
    let transport = transport(implemented, &in_logicals, &out_logicals, &mut a.vars);
    a.followed = a.notes.is_empty() && ideal_notes.is_empty();

    // ── Declared equations ─────────────────────────────────────────────────
    let record_len = a.record.len();
    let scope = Scope {
        record_len,
        readouts: gadget.readouts.len(),
        inputs: gadget.inputs.iter().map(|e| shape(&e.code)).collect(),
        outputs: gadget.outputs.iter().map(|e| shape(&e.code)).collect(),
    };
    let declared_readouts: Vec<Result<Vec<Term>, String>> = gadget
        .readouts
        .iter()
        .map(|r| terms(&r.equation, &scope))
        .collect();
    let frames_declared: Vec<DeclaredFrame> = gadget
        .frames
        .iter()
        .map(|(key, eq)| (frame_key(key, &scope), terms(eq, &scope)))
        .collect();
    let delta = |key: (usize, Basis, usize)| -> Vec<Term> {
        frames_declared
            .iter()
            .find(|(k, _)| k.as_ref().is_ok_and(|k| *k == key))
            .and_then(|(_, t)| t.as_ref().ok().cloned())
            .unwrap_or_default()
    };
    let mut eval = Evaluator {
        record: a.record.iter().map(|b| b.value.form().clone()).collect(),
        out_stabilizers: &a.out_stabilizers,
        readouts: &declared_readouts,
        transport: &transport,
        delta: &delta,
        in_signs: &a.in_signs,
        vars: &mut a.vars,
        depth: 0,
    };
    let checks: Vec<(String, Result<Vec<Term>, String>)> = gadget
        .checks
        .iter()
        .map(|eq| (equation_text_of(eq), terms(eq, &scope)))
        .collect();
    a.checks = checks
        .iter()
        .map(|(text, t)| Verdict {
            text: text.clone(),
            residual: t.clone().and_then(|t| eval.sum(&t)),
            unknown: false,
        })
        .collect();
    a.readout_is_flag = (0..gadget.readouts.len())
        .map(|i| i >= implemented.observe_count())
        .collect();
    let readout_forms: Vec<Result<Form, String>> = declared_readouts
        .iter()
        .map(|t| t.clone().and_then(|t| eval.sum(&t)))
        .collect();
    a.readouts = gadget
        .readouts
        .iter()
        .enumerate()
        .map(|(i, r)| Verdict {
            text: equation_text_of(&r.equation),
            // A flag must be 0; an observable is judged with the action.
            residual: if a.readout_is_flag[i] {
                readout_forms[i].clone()
            } else {
                readout_forms[i]
                    .as_ref()
                    .map(|_| Form::default())
                    .map_err(Clone::clone)
            },
            unknown: false,
        })
        .collect();
    a.frames = frames_declared
        .iter()
        .zip(&gadget.frames)
        .map(|((key, t), (reference, eq))| Verdict {
            text: format!("{reference}: {}", equation_text_of(eq)),
            residual: match (key, t) {
                (Err(e), _) | (_, Err(e)) => Err(e.clone()),
                (Ok(_), Ok(t)) if t.iter().any(|t| matches!(t, Term::Sign { .. })) => {
                    Err("a frame cannot use encoding signs".to_string())
                }
                (Ok(_), Ok(_)) => Ok(Form::default()),
            },
            unknown: false,
        })
        .collect();
    // Past a step it could not follow, a failure may be the analysis's own.
    if !a.followed {
        for v in a
            .checks
            .iter_mut()
            .chain(&mut a.readouts)
            .chain(&mut a.frames)
        {
            v.unknown = !v.holds();
        }
    }

    // Output stabilizers the declared checks determine.
    let mut unknown = FormBasis::default();
    for (_, t) in &checks {
        if let Ok(t) = t {
            let mut row = Form::default();
            for term in t {
                if let Term::Sign {
                    out: true,
                    e,
                    basis: Basis::Stabilizer,
                    k,
                } = term
                {
                    row.vars.flip(stabilizer_index(&a.out_stabilizers, *e, *k));
                }
            }
            unknown.add(&row);
        }
    }
    for (i, s) in a.out_stabilizers.iter_mut().enumerate() {
        let (_, rest) = unknown.express(&Form::var(i));
        s.determined = s.sign.is_some() && rest.vars.is_zero();
    }

    // ── The action ─────────────────────────────────────────────────────────
    a.action.compared = a.followed;
    let n_observables = implemented.observe_count();
    let outcome_vars: Vec<usize> = (0..ideal_outcomes.len())
        .map(|i| a.vars.add(Var::Outcome(i)))
        .collect();
    // Per ideal generator: (which outcomes and frame keys it involves, the rest
    // of the condition).
    let mut conditions: Vec<(Bits, Form, String)> = Vec::new();
    let frame_keys: Vec<(usize, Basis, usize)> = (0..out_logicals.len())
        .flat_map(|e| (0..out_logicals[e]).flat_map(move |k| [(e, Basis::X, k), (e, Basis::Z, k)]))
        .collect();
    if a.action.compared {
        for g in &ideal.gens {
            let (encoded, minus, keys) =
                match encode(g, n_out, width, &out_logicals, &out_codes, &refs) {
                    Ok(x) => x,
                    Err(e) => {
                        a.action.mismatches.push(e);
                        continue;
                    }
                };
            let shown = describe_ideal(g, n_out, width);
            let Some(mut actual) = t.sign_of(&encoded) else {
                a.action
                    .mismatches
                    .push(format!("the output does not hold {shown}"));
                continue;
            };
            if minus {
                actual.flip();
            }
            // actual = ideal sign, with outcomes read from the readouts, plus
            // the frame the decoder predicts for the logical operators in it.
            let mut unknowns = Bits::default();
            let mut rest = actual.clone();
            for v in g.sign.vars.ones() {
                match outcome_vars.iter().position(|&o| o == v) {
                    Some(i) => unknowns.flip(i),
                    None => rest.xor(&Form::var(v)),
                }
            }
            if g.sign.constant {
                rest.flip();
            }
            for key in &keys {
                let f = frame_keys
                    .iter()
                    .position(|k| k == key)
                    .expect("a known key");
                unknowns.flip(ideal_outcomes.len() + f);
                rest.xor(transport.get(key).unwrap_or(&Form::default()));
            }
            conditions.push((unknowns, rest, shown));
        }
        // Check the declared readouts and frames against the conditions.
        let declared: Option<Vec<Form>> = (0..ideal_outcomes.len())
            .map(|i| readout_forms.get(i).and_then(|r| r.as_ref().ok()).cloned())
            .chain(frame_keys.iter().map(|key| {
                let mut eval = Evaluator {
                    record: a.record.iter().map(|b| b.value.form().clone()).collect(),
                    out_stabilizers: &a.out_stabilizers,
                    readouts: &declared_readouts,
                    transport: &transport,
                    delta: &delta,
                    in_signs: &a.in_signs,
                    vars: &mut a.vars,
                    depth: 0,
                };
                eval.sum(&delta(*key)).ok()
            }))
            .collect();
        if ideal_outcomes.len() < n_observables {
            a.action
                .mismatches
                .push("the action observes fewer outcomes than it declares".to_string());
        }
        match declared {
            Some(values) => {
                for (unknowns, rest, shown) in &conditions {
                    let mut lhs = Form::default();
                    for u in unknowns.ones() {
                        lhs.xor(&values[u]);
                    }
                    let diff = lhs.plus(rest);
                    if !diff.is_zero() {
                        a.action.mismatches.push(format!(
                            "{shown} comes out with sign off by {}",
                            a.vars.show(&diff)
                        ));
                    }
                }
            }
            None => a
                .action
                .mismatches
                .push("some readouts are missing or do not evaluate".to_string()),
        }
        a.suggestions = derive(
            &a,
            &conditions,
            ideal_outcomes.len(),
            &frame_keys,
            n_observables + implemented.flags.len(),
            n_observables,
        );
    } else {
        for note in ideal_notes {
            a.notes
                .push((usize::MAX, format!("the instruction's action: {note}")));
        }
        a.suggestions = derive(
            &a,
            &[],
            0,
            &[],
            n_observables + implemented.flags.len(),
            n_observables,
        );
    }
    // Checks not implied by the declared ones.
    let declared_span = {
        let mut b = FormBasis::default();
        for (_, t) in &checks {
            if let Ok(t) = t {
                b.add(&term_vector(t, &scope));
            }
        }
        b
    };
    a.suggestions.checks.retain(|c| {
        let (_, rest) = declared_span.express(&term_vector(c, &scope));
        !rest.vars.is_zero()
    });
    Ok(a)
}

/// The arguments of one call.
#[derive(Default)]
struct Env {
    bits: BTreeMap<String, Form>,
    paulis: BTreeMap<String, String>,
}

/// Applies `action` to `t`, its flat qubit `i` being `place(i)`. Pushes the
/// outcomes of its `observe` steps, and a note for each step it cannot follow
/// exactly.
fn apply(
    t: &mut Tableau,
    action: &[qodec::ActionStep],
    place: &dyn Fn(usize) -> Option<usize>,
    env: &Env,
    fresh: &mut dyn FnMut(usize) -> usize,
    outcomes: &mut Vec<Found>,
    notes: &mut Vec<String>,
) {
    let pauli = |text: &str| -> Result<(Pauli, bool), String> {
        let text = env.paulis.get(text).map_or(text, String::as_str);
        let sparse = SparsePauli::parse(text)?;
        Pauli::place(&sparse, place)
    };
    for step in action {
        let condition = match &step.condition {
            None => Form::constant(true),
            Some(c) => {
                let mut f = Form::constant(c.invert);
                for p in &c.predicates {
                    let value = if let Some(i) = p
                        .strip_prefix("outcomes[")
                        .and_then(|r| r.strip_suffix(']'))
                        .and_then(|i| i.parse::<usize>().ok())
                    {
                        outcomes.get(i).map(|o| o.form().clone())
                    } else {
                        env.bits.get(p).cloned()
                    };
                    match value {
                        Some(v) => f.xor(&v),
                        None => notes.push(format!("the condition `{p}` is unknown")),
                    }
                }
                f
            }
        };
        if condition.is_zero() {
            continue;
        }
        let conditional = !condition.is_constant();
        let result: Result<(), String> = (|| {
            match &step.action {
                Action::Stabilize(paulis) => {
                    if conditional {
                        return Err("a conditional `stabilize` is not followed".to_string());
                    }
                    for p in paulis {
                        let (p, negative) = pauli(&p.0)?;
                        t.stabilize(&p, negative)?;
                    }
                }
                Action::Clifford(map) => {
                    if conditional {
                        return Err("a conditional `clifford` is not followed".to_string());
                    }
                    let mut width = 0;
                    for (k, v) in map {
                        for p in [&k.0, &v.0] {
                            if let Some(m) = SparsePauli::parse(p)?.max_qubit() {
                                width = width.max(m + 1);
                            }
                        }
                    }
                    let c = Clifford::from_images(
                        map.iter().map(|(k, v)| (k.0.as_str(), v.0.as_str())),
                        width,
                    )?;
                    let positions: Vec<usize> = (0..width)
                        .map(|i| place(i).ok_or("the Clifford acts outside its qubits"))
                        .collect::<Result<_, _>>()?;
                    t.conjugate(&positions, &c);
                }
                Action::Pauli(p) => {
                    let (p, _) = pauli(&p.0)?;
                    t.apply_pauli(&p, &condition);
                }
                Action::Observe(observables) => {
                    if conditional {
                        return Err("a conditional `observe` is not followed".to_string());
                    }
                    for o in observables {
                        let (p, negative) = pauli(&o.pauli.0)?;
                        let index = outcomes.len();
                        outcomes.push(t.measure(&p, negative, || fresh(index)));
                    }
                }
                Action::Rotate { pauli: axis, .. } => {
                    let (p, _) = pauli(&axis.0)?;
                    if t.keep_commuting(&p) > 0 {
                        return Err(format!(
                            "a rotation about `{}` leaves the stabilizer formalism; what it rotates is no longer followed",
                            axis.0
                        ));
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            notes.push(e);
        }
    }
}

/// The observables an instruction measures, in outcome order.
fn observables(instruction: &Instruction) -> Vec<String> {
    instruction
        .action
        .iter()
        .flat_map(|s| match &s.action {
            Action::Observe(os) => os.iter().map(|o| o.pauli.0.clone()).collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// The largest flat index an action names, plus one.
fn extent(action: &[qodec::ActionStep]) -> usize {
    let mut n = 0;
    let mut see = |text: &str| {
        if let Ok(p) = SparsePauli::parse(text)
            && let Some(m) = p.max_qubit()
        {
            n = n.max(m + 1);
        }
    };
    for step in action {
        match &step.action {
            Action::Stabilize(ps) => ps.iter().for_each(|p| see(&p.0)),
            Action::Clifford(map) => map.iter().for_each(|(k, v)| {
                see(&k.0);
                see(&v.0);
            }),
            Action::Pauli(p) | Action::Rotate { pauli: p, .. } => see(&p.0),
            Action::Observe(os) => os.iter().for_each(|o| see(&o.pauli.0)),
        }
    }
    n
}

/// A code with its operators parsed.
struct ParsedCode {
    stabilizers: Vec<SparsePauli>,
    x: Vec<SparsePauli>,
    z: Vec<SparsePauli>,
}

fn parse_code(code: &Code) -> Result<ParsedCode, String> {
    let parse = |ps: &[qodec::PauliString]| {
        ps.iter()
            .map(|p| SparsePauli::parse(&p.0))
            .collect::<Result<Vec<_>, _>>()
    };
    Ok(ParsedCode {
        stabilizers: parse(&code.stabilizers)?,
        x: parse(&code.x)?,
        z: parse(&code.z)?,
    })
}

/// How many operators of each kind a code has: stabilizers, logicals.
fn shape(code: &Code) -> (usize, usize) {
    (code.stabilizers.len(), code.x.len())
}

/// The decoder's transport through the action: per output logical sign
/// `(e, basis, k)`, the input signs it carries. It mirrors the frame
/// decoder's rule: Cliffords move the frame, other steps leave it.
fn transport(
    implemented: &Instruction,
    in_logicals: &[usize],
    out_logicals: &[usize],
    vars: &mut Vars,
) -> BTreeMap<(usize, Basis, usize), Form> {
    let width = in_logicals
        .iter()
        .sum::<usize>()
        .max(out_logicals.iter().sum())
        .max(extent(&implemented.action));
    // Per flat qubit: (sign of Z, flipped by an X frame; sign of X).
    let mut s: Vec<(Form, Form)> = vec![Default::default(); width];
    let mut q = 0;
    for (e, &n) in in_logicals.iter().enumerate() {
        for k in 0..n {
            s[q] = (
                Form::var(vars.add(Var::In {
                    e,
                    basis: Basis::Z,
                    k,
                })),
                Form::var(vars.add(Var::In {
                    e,
                    basis: Basis::X,
                    k,
                })),
            );
            q += 1;
        }
    }
    for step in &implemented.action {
        if step.condition.is_some() {
            continue;
        }
        if let Action::Clifford(map) = &step.action {
            let mut next: Vec<(Form, Form)> = vec![Default::default(); width];
            for (q, (z_sign, x_sign)) in s.iter().enumerate() {
                for (v, is_x) in [(z_sign, true), (x_sign, false)] {
                    if v.is_zero() {
                        continue;
                    }
                    let generator = format!("{}_{q}", if is_x { "X" } else { "Z" });
                    let image = map
                        .iter()
                        .find(|(k, _)| {
                            SparsePauli::parse(&k.0).ok() == SparsePauli::parse(&generator).ok()
                        })
                        .map_or(generator.clone(), |(_, v)| v.0.clone());
                    if let Ok(image) = SparsePauli::parse(&image) {
                        for &(t, op) in image.terms() {
                            if let Some(target) = next.get_mut(t) {
                                if op.has_x() {
                                    target.0.xor(v);
                                }
                                if op.has_z() {
                                    target.1.xor(v);
                                }
                            }
                        }
                    }
                }
            }
            s = next;
        }
    }
    let mut out = BTreeMap::new();
    let mut q = 0;
    for (e, &n) in out_logicals.iter().enumerate() {
        for k in 0..n {
            if let Some((z, x)) = s.get(q) {
                out.insert((e, Basis::Z, k), z.clone());
                out.insert((e, Basis::X, k), x.clone());
            }
            q += 1;
        }
    }
    out
}

/// What equation terms may name.
struct Scope {
    record_len: usize,
    readouts: usize,
    inputs: Vec<(usize, usize)>,
    outputs: Vec<(usize, usize)>,
}

/// The terms of `equation`, each reference expanded.
fn terms(equation: &ParityEquation, scope: &Scope) -> Result<Vec<Term>, String> {
    let mut out = Vec::new();
    for term in equation {
        match term {
            ParityTerm::Bit(true) => out.push(Term::One),
            ParityTerm::Bit(false) => {}
            ParityTerm::Reference(r) => out.extend(reference_terms(r, scope)?),
        }
    }
    Ok(out)
}

fn reference_terms(reference: &Reference, scope: &Scope) -> Result<Vec<Term>, String> {
    use ReferenceSegment::{Field, Index};
    let mut out = Vec::new();
    for single in reference.expand() {
        let range = || format!("`{reference}` is out of range");
        out.push(match single.segments() {
            [Field(c), Field(r), Index(i)] if c == "circuit" && r == "readouts" => (*i
                < scope.record_len)
                .then_some(Term::Record(*i))
                .ok_or_else(range)?,
            [Field(r), Index(i)] if r == "readouts" => (*i < scope.readouts)
                .then_some(Term::Readout(*i))
                .ok_or_else(range)?,
            [Field(side), Index(e), Field(basis), Index(k)] => {
                let (out, shapes) = match side.as_str() {
                    "in" => (false, &scope.inputs),
                    "out" => (true, &scope.outputs),
                    _ => return Err(format!("`{reference}` is not a reference the spec allows")),
                };
                let basis = match basis.as_str() {
                    "stabilizers" => Basis::Stabilizer,
                    "x" => Basis::X,
                    "z" => Basis::Z,
                    _ => return Err(format!("`{reference}` is not a reference the spec allows")),
                };
                let &(stabilizers, logicals) = shapes.get(*e).ok_or_else(range)?;
                let len = if basis == Basis::Stabilizer {
                    stabilizers
                } else {
                    logicals
                };
                if *k >= len {
                    return Err(range());
                }
                Term::Sign {
                    out,
                    e: *e,
                    basis,
                    k: *k,
                }
            }
            _ => return Err(format!("`{reference}` is not a reference the spec allows")),
        });
    }
    Ok(out)
}

fn frame_key(reference: &Reference, scope: &Scope) -> Result<(usize, Basis, usize), String> {
    match reference_terms(reference, scope)?.as_slice() {
        [
            Term::Sign {
                out: true,
                e,
                basis: basis @ (Basis::X | Basis::Z),
                k,
            },
        ] => Ok((*e, *basis, *k)),
        _ => Err(format!(
            "frame key `{reference}` must name one output logical sign"
        )),
    }
}

fn equation_text_of(equation: &ParityEquation) -> String {
    let parts: Vec<String> = equation
        .iter()
        .map(|t| match t {
            ParityTerm::Bit(b) => u8::from(*b).to_string(),
            ParityTerm::Reference(r) => format!("\"{r}\""),
        })
        .collect();
    format!("[{}]", parts.join(", "))
}

/// A check as a vector over its terms, to compare checks by what they say.
fn term_vector(terms: &[Term], scope: &Scope) -> Form {
    let mut f = Form::default();
    let offset_in = scope.record_len + scope.readouts;
    for t in terms {
        let i = match t {
            Term::Record(i) => *i,
            Term::Readout(i) => scope.record_len + i,
            Term::Sign { out, e, basis, k } => {
                let shapes = if *out { &scope.outputs } else { &scope.inputs };
                let base: usize = shapes[..*e].iter().map(|(s, l)| s + 2 * l).sum();
                let within = match basis {
                    Basis::Stabilizer => *k,
                    Basis::X => shapes[*e].0 + k,
                    Basis::Z => shapes[*e].0 + shapes[*e].1 + k,
                };
                let side = if *out {
                    scope.inputs.iter().map(|(s, l)| s + 2 * l).sum::<usize>()
                } else {
                    0
                };
                offset_in + side + base + within
            }
            Term::One => {
                f.flip();
                continue;
            }
        };
        f.vars.flip(i);
    }
    f
}

fn stabilizer_index(out: &[OutStabilizer], e: usize, k: usize) -> usize {
    out.iter()
        .position(|s| s.e == e && s.k == k)
        .unwrap_or(usize::MAX / 2)
}

/// Evaluates terms to formulas.
struct Evaluator<'a> {
    record: Vec<Form>,
    out_stabilizers: &'a [OutStabilizer],
    readouts: &'a [Result<Vec<Term>, String>],
    transport: &'a BTreeMap<(usize, Basis, usize), Form>,
    delta: &'a dyn Fn((usize, Basis, usize)) -> Vec<Term>,
    in_signs: &'a BTreeMap<(usize, Basis, usize), Form>,
    vars: &'a mut Vars,
    depth: usize,
}

impl Evaluator<'_> {
    fn sum(&mut self, terms: &[Term]) -> Result<Form, String> {
        if self.depth > 16 {
            return Err("the readouts refer to each other in a cycle".to_string());
        }
        let mut f = Form::default();
        for t in terms {
            match t {
                Term::One => f.flip(),
                Term::Record(i) => f.xor(&self.record[*i]),
                Term::Readout(i) => {
                    let ts = self.readouts[*i].clone()?;
                    self.depth += 1;
                    let v = self.sum(&ts);
                    self.depth -= 1;
                    f.xor(&v?);
                }
                Term::Sign {
                    out: false,
                    e,
                    basis,
                    k,
                } => match self.in_signs.get(&(*e, *basis, *k)) {
                    Some(s) => f.xor(s),
                    None => f.xor(&Form::var(self.vars.add(Var::In {
                        e: *e,
                        basis: *basis,
                        k: *k,
                    }))),
                },
                Term::Sign {
                    out: true,
                    e,
                    basis: Basis::Stabilizer,
                    k,
                } => {
                    let s = self
                        .out_stabilizers
                        .iter()
                        .find(|s| s.e == *e && s.k == *k)
                        .and_then(|s| s.sign.clone())
                        .ok_or_else(|| {
                            format!("the output does not hold out[{e}].stabilizers[{k}]")
                        })?;
                    f.xor(&s);
                }
                Term::Sign {
                    out: true,
                    e,
                    basis,
                    k,
                } => {
                    let key = (*e, *basis, *k);
                    if let Some(t) = self.transport.get(&key) {
                        f.xor(t);
                    }
                    let ts = (self.delta)(key);
                    self.depth += 1;
                    let v = self.sum(&ts);
                    self.depth -= 1;
                    f.xor(&v?);
                }
            }
        }
        Ok(f)
    }
}

/// `g`, an operator of the ideal output on logical qubits and references,
/// encoded on the actual output: the Pauli, whether it picks up a `-1`, and
/// the output logical signs that a frame on it flips.
#[allow(clippy::type_complexity)]
fn encode(
    g: &Signed,
    n_out: usize,
    width: usize,
    out_logicals: &[usize],
    out_codes: &[(Vec<usize>, ParsedCode)],
    refs: &[usize],
) -> Result<(Pauli, bool, Vec<(usize, Basis, usize)>), String> {
    let mut acc = Pauli::identity();
    let mut phase = 0usize; // power of i, beyond the Hermitian form
    let mut keys = Vec::new();
    let mut q = 0;
    for (e, &n) in out_logicals.iter().enumerate() {
        let (qubits, code) = &out_codes[e];
        for k in 0..n {
            let place = |p: &SparsePauli| Pauli::place(p, |i| qubits.get(i).copied());
            let letter = g.pauli.letter(q);
            if matches!(letter, Some('X' | 'Y')) {
                let (x, neg) = place(&code.x[k])?;
                phase += mul_phase(&mut acc, &x) + 2 * usize::from(neg);
                keys.push((e, Basis::X, k));
            }
            if matches!(letter, Some('Z' | 'Y')) {
                let (z, neg) = place(&code.z[k])?;
                phase += mul_phase(&mut acc, &z) + 2 * usize::from(neg);
                keys.push((e, Basis::Z, k));
            }
            if letter == Some('Y') {
                // Y = i X Z.
                phase += 1;
            }
            q += 1;
        }
    }
    debug_assert_eq!(q, n_out);
    for (i, &r) in refs.iter().enumerate() {
        if let Some(l) = g.pauli.letter(width + i) {
            let p = Pauli::single(r, l != 'Z', l != 'X');
            phase += mul_phase(&mut acc, &p);
        }
    }
    // `acc` is now i^phase · (Hermitian acc) up to the Hermitian phases
    // `mul_phase` already folded in.
    let minus = match phase % 4 {
        0 => false,
        2 => true,
        _ => return Err("an encoded operator is not Hermitian".to_string()),
    };
    Ok((acc, minus, keys))
}

/// `acc = acc · p`, both Hermitian: returns the power of `i` the product
/// picks up beyond the Hermitian form of the result.
fn mul_phase(acc: &mut Pauli, p: &Pauli) -> usize {
    let phase = acc.x.and_count(&acc.z) + p.x.and_count(&p.z) + 2 * acc.z.and_count(&p.x);
    let mut x = acc.x.clone();
    x.xor(&p.x);
    let mut z = acc.z.clone();
    z.xor(&p.z);
    let hermitian = x.and_count(&z);
    *acc = Pauli { x, z };
    (phase + 4 * (hermitian / 4 + 1) - hermitian) % 4
}

/// An ideal operator in the instruction's own terms: `Z_0 ⊗ Z_ref0`.
fn describe_ideal(g: &Signed, n_out: usize, width: usize) -> String {
    let mut parts = Vec::new();
    for q in 0..n_out {
        if let Some(l) = g.pauli.letter(q) {
            parts.push(format!("{l}_{q}"));
        }
    }
    let mut refs = Vec::new();
    for r in g
        .pauli
        .x
        .ones()
        .chain(g.pauli.z.ones())
        .filter(|&r| r >= width)
    {
        if !refs.contains(&r) {
            refs.push(r);
        }
    }
    refs.sort_unstable();
    for r in refs {
        if let Some(l) = g.pauli.letter(r) {
            parts.push(format!("{l}_ref{}", r - width));
        }
    }
    if parts.is_empty() {
        "the identity".to_string()
    } else {
        format!("`{}`", parts.join(" "))
    }
}

/// Derives checks, readouts and frames.
#[allow(clippy::too_many_lines)]
fn derive(
    a: &Analysis,
    conditions: &[(Bits, Form, String)],
    outcomes: usize,
    frame_keys: &[(usize, Basis, usize)],
    readouts: usize,
    observables: usize,
) -> Suggestions {
    let mut s = Suggestions::default();
    // Checks: every way the terms' formulas cancel.
    let mut basis = FormBasis::default();
    let mut terms: Vec<Term> = Vec::new();
    let mut forms: Vec<Form> = Vec::new();
    for (i, bit) in a.record.iter().enumerate() {
        terms.push(Term::Record(i));
        forms.push(bit.value.form().clone());
    }
    for (&(e, basis, k), sign) in &a.in_signs {
        if basis == Basis::Stabilizer {
            terms.push(Term::Sign {
                out: false,
                e,
                basis,
                k,
            });
            forms.push(sign.clone());
        }
    }

    for st in &a.out_stabilizers {
        if let Some(sign) = &st.sign {
            terms.push(Term::Sign {
                out: true,
                e: st.e,
                basis: Basis::Stabilizer,
                k: st.k,
            });
            forms.push(sign.clone());
        }
    }
    for (i, form) in forms.iter().enumerate() {
        if let Some((combo, constant)) = basis.add(form) {
            let mut check: Vec<Term> = combo.ones().map(|j| terms[j].clone()).collect();
            check.push(terms[i].clone());
            if constant {
                check.push(Term::One);
            }
            s.checks.push(check);
        }
    }

    // Readouts and frames: solve the action's conditions for them.
    let mut rows: Vec<(usize, Bits, Form)> = Vec::new();
    for (unknowns, rest, shown) in conditions {
        let mut u = unknowns.clone();
        let mut r = rest.clone();
        for (pivot, pu, pr) in &rows {
            if u.get(*pivot) {
                u.xor(pu);
                r.xor(pr);
            }
        }
        match u.first() {
            Some(pivot) => {
                for (_, ou, or) in &mut rows {
                    if ou.get(pivot) {
                        ou.xor(&u);
                        or.xor(&r);
                    }
                }
                rows.push((pivot, u, r));
            }
            None if !r.is_zero() => s.problems.push(format!(
                "no readouts or frames make the circuit implement the action: {shown} is off by {}",
                a.vars.show(&r)
            )),
            None => {}
        }
    }
    let solution = |i: usize| -> Form {
        rows.iter()
            .find(|(p, _, _)| *p == i)
            .map_or_else(Form::default, |(_, _, r)| r.clone())
    };
    // What the decoder can read: the record, and the incoming signs.
    let mut record_basis = FormBasis::default();
    let in_mask = a.vars.mask(|v| matches!(v, Var::In { .. }));
    let project = |f: &Form| {
        let mut g = f.clone();
        for i in in_mask.ones() {
            g.vars.set(i, false);
        }
        g
    };
    for bit in &a.record {
        record_basis.add(&project(bit.value.form()));
    }
    let express = |target: &Form, signs: bool| -> Result<Vec<Term>, String> {
        let (combo, rest) = record_basis.express(&project(target));
        if !rest.vars.is_zero() {
            return Err(format!(
                "it depends on {}, which no bit of the circuit reports",
                a.vars.show(&Form {
                    constant: false,
                    vars: rest.vars
                })
            ));
        }
        let mut terms: Vec<Term> = combo.ones().map(Term::Record).collect();
        let mut remaining = target.clone();
        for j in combo.ones() {
            remaining.xor(a.record[j].value.form());
        }
        for v in remaining.vars.ones() {
            match a.vars.get(v) {
                Var::In { e, basis, k } if signs => terms.push(Term::Sign {
                    out: false,
                    e: *e,
                    basis: *basis,
                    k: *k,
                }),
                other => return Err(format!("it depends on {}", other.name())),
            }
        }
        if remaining.constant {
            terms.push(Term::One);
        }
        Ok(terms)
    };
    s.readouts = (0..readouts)
        .map(|i| {
            if i < observables && i < outcomes {
                match express(&solution(i), true) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        s.problems.push(format!("readout {i}: {e}"));
                        None
                    }
                }
            } else if i >= observables {
                // A flag: the tool cannot know which zero parity is meant.
                None
            } else {
                None
            }
        })
        .collect();
    for (f, key) in frame_keys.iter().enumerate() {
        let value = solution(outcomes + f);
        if value.is_zero() {
            continue;
        }
        match express(&value, false) {
            Ok(t) => s.frames.push((*key, t)),
            Err(e) => s.problems.push(format!(
                "frame for out[{}].{}[{}]: {e}",
                key.0,
                key.1.name(),
                key.2
            )),
        }
    }
    s
}
