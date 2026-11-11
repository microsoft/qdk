// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    read,
    spec::{self, compile_select, encode_arg, matches, signature, type_table},
};
use qodec::{BlockOperand, Instruction, InstructionSet, Qodec, SelectPattern};
use qodec_runtime_recipes::{
    Backend, BasicReadout, BlockRef, Blueprint, Call, CompiledSelect, Compiler, Correction, Design,
    Encoder, Fault, FaultKind, FrameDecoder, FrameDecoderPlan, FrameDecoding, Layer,
    LayerBlueprint, LayerModel, Lower, Never, OwnedParam, Passive, PlanError, QodecLayer,
    QodecPlan, QodecRuntimeError, Record, Reply, Resources, Rounds, Runtime, Selection,
    SpecEncoder, SpecEncoderPlan, Stack, Status, Step, TypeChecker, TypeTable, UntilReadyPlan,
    from_fn,
    parity::{Basis, BinarySystem, Inconsistent, Parity, Var},
    pauli::{Clifford, DensePauli, Pauli, PauliOp},
    resolver::{ResolveCache, Resolver},
    runtime as shot,
    select::accepts,
};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::{cell::RefCell, collections::VecDeque, sync::Arc};

// ── Spec helpers ────────────────────────────────────────────────────────────

fn compiled(flags: &[&str], patterns: &[&[(&str, u8)]]) -> CompiledSelect {
    let flags: Vec<String> = flags.iter().map(ToString::to_string).collect();
    let patterns: Vec<SelectPattern> = patterns
        .iter()
        .map(|p| p.iter().map(|(k, v)| ((*k).to_string(), *v)).collect())
        .collect();
    compile_select(&flags, &patterns).expect("select should compile")
}

#[test]
fn accepts_is_exact_when_every_completion_matches() {
    // A per-pattern scan would answer Undetermined here.
    let select = compiled(&["f0"], &[&[("f0", 0)], &[("f0", 1)]]);
    assert_eq!(accepts(&select, &[None]), Selection::Satisfied);
}

#[test]
fn accepts_is_three_valued() {
    let select = compiled(&["f0", "f1"], &[&[("f0", 1), ("flags[1]", 0)]]);
    assert_eq!(
        accepts(&select, &[Some(true), Some(false)]),
        Selection::Satisfied
    );
    assert_eq!(accepts(&select, &[Some(false), None]), Selection::Violated);
    assert_eq!(
        accepts(&select, &[Some(true), None]),
        Selection::Undetermined
    );
    assert_eq!(accepts(&Vec::new(), &[None]), Selection::Satisfied);
}

#[test]
fn compile_select_rejects_conflicts_and_unknown_flags() {
    let flags = vec!["f0".to_string()];
    let conflicting: SelectPattern = [("f0".to_string(), 0), ("flags[0]".to_string(), 1)].into();
    assert!(compile_select(&flags, &[conflicting]).is_err());
    let unknown: SelectPattern = [("nope".to_string(), 0)].into();
    assert!(compile_select(&flags, &[unknown]).is_err());
}

#[test]
fn binary_system_detects_inconsistency_with_insertion_order() {
    // A system that sorted its rows and pivoted on the largest variable would
    // wrongly accept {a⊕c=0, a⊕b=0, b=1, c=0}. Insertion order catches it.
    let (a, b, c) = (Var::Rec(0), Var::Rec(1), Var::Rec(2));
    let mut sys = BinarySystem::new();
    sys.add(Parity::var(a) ^ Parity::var(c)).expect("a ^ c = 0");
    sys.add(Parity::var(a) ^ Parity::var(b)).expect("a ^ b = 0");
    sys.add(Parity::var(b) ^ Parity::constant(true))
        .expect("b = 1");
    assert_eq!(sys.value(Parity::var(c)), Some(true));
    assert_eq!(sys.add(Parity::var(c)), Err(Inconsistent));
}

#[test]
fn binary_system_reports_unknown_values() {
    let mut sys = BinarySystem::new();
    sys.add(Parity::var(Var::Rec(0)) ^ Parity::var(Var::Rec(1)))
        .expect("consistent");
    assert_eq!(sys.value(Parity::var(Var::Rec(0))), None);
    assert_eq!(
        sys.value(Parity::var(Var::Rec(0)) ^ Parity::var(Var::Rec(1))),
        Some(false)
    );
}

fn instruction(inputs: &[(&str, bool)], outputs: &[(&str, bool)]) -> Instruction {
    let operands = |ops: &[(&str, bool)]| {
        ops.iter()
            .map(|(block, is_variadic)| BlockOperand {
                block: (*block).to_string(),
                is_variadic: *is_variadic,
            })
            .collect()
    };
    Instruction {
        mnemonic: "test".to_string(),
        description: String::new(),
        inputs: operands(inputs),
        outputs: operands(outputs),
        parameters: Vec::new(),
        action: Vec::new(),
        flags: Vec::new(),
        metadata: qodec::Metadata::default(),
    }
}

fn types(blocks: &[(&str, usize)]) -> TypeTable {
    type_table(&InstructionSet {
        name: "test".to_string(),
        description: String::new(),
        blocks: blocks
            .iter()
            .map(|(name, encodes)| qodec::Block {
                name: (*name).to_string(),
                encodes: *encodes,
            })
            .collect(),
        instructions: Vec::new(),
        metadata: qodec::Metadata::default(),
    })
}

#[test]
fn bind_operands_assigns_flat_offsets() {
    let types = types(&[("a", 2), ("b", 1)]);
    let i = instruction(&[("a", false), ("b", false)], &[("a", false)]);
    let (ins, outs) = signature(&i, &types)
        .expect("signature")
        .bind(&types, &[7u64, 8], &FxHashMap::default())
        .expect("binds");
    assert_eq!(
        ins.iter()
            .map(|b| (b.label, b.offset, b.width))
            .collect::<Vec<_>>(),
        vec![(7, 0, 2), (8, 2, 1)]
    );
    assert_eq!(outs.len(), 1);
}

#[test]
fn bind_operands_resolves_variadics_with_known_types() {
    let types = types(&[("a", 1), ("b", 1)]);
    let i = instruction(&[("a", true), ("b", true)], &[]);
    let targets = [1u64, 2, 3];
    let signature = signature(&i, &types).expect("signature");
    // Without types, the split between the two variadics is ambiguous.
    assert!(
        signature
            .bind(&types, &targets, &FxHashMap::default())
            .is_err()
    );
    let id = |name| types.get(name).expect("declared");
    let known: FxHashMap<u64, usize> = [(1, id("a")), (2, id("a")), (3, id("b"))]
        .into_iter()
        .collect();
    let (ins, _) = signature
        .bind(&types, &targets, &known)
        .expect("unique split");
    assert_eq!(
        ins.iter()
            .map(|b| types.names[b.block_type].as_str())
            .collect::<Vec<_>>(),
        vec!["a", "a", "b"]
    );
}

#[test]
fn bind_operands_requires_distinct_targets() {
    let types = types(&[("a", 1)]);
    let i = instruction(&[("a", false), ("a", false)], &[]);
    let signature = signature(&i, &types).expect("signature");
    assert!(
        signature
            .bind(&types, &[1u64, 1], &FxHashMap::default())
            .is_err()
    );
}

#[test]
fn named_cliffords_compose() {
    let named = |name, arity| Clifford::named(name, arity).expect("known Clifford");
    let (h, s, s_adj) = (named("h", 1), named("s", 1), named("s_adj", 1));
    assert_eq!(h.then(&h), Clifford::identity(1));
    assert_eq!(s.then(&s_adj), Clifford::identity(1));
    assert_eq!(s.then(&s), named("z", 1));
    // X then Z is Z·X = iY. A tableau has no global phase, so the Cliffords
    // are equal.
    assert_eq!(named("x", 1).then(&named("z", 1)), named("y", 1));
    // CY = S_t · CX · S_t†.
    let cy = s_adj
        .embed(&[1], 2)
        .then(&named("cx", 2))
        .then(&s.embed(&[1], 2));
    assert_eq!(cy, named("cy", 2));
}

#[test]
fn clifford_conjugates_signed_paulis() {
    let dense = |text, width| {
        DensePauli::from_pauli(&Pauli::parse(text).expect("pauli"), width).expect("fits")
    };
    let cx = Clifford::named("cx", 2).expect("cx");
    assert_eq!(cx.conjugate(&dense("X_0", 2)), dense("X_0 X_1", 2));
    let x = Clifford::named("x", 1).expect("x");
    assert_eq!(x.conjugate(&dense("Z", 1)), dense("-Z_0", 1));
}

// ── End-to-end ──────────────────────────────────────────────────────────────

/// Repetition-3 artifacts shared by the bundles below. Besides `R`, `M`, `CX`,
/// `X` and `Z`, the physical ISA has a non-destructive `MZ` and a classically
/// controlled `XIF`, which `measure_reset` uses to feed a readout into a `bit`
/// parameter.
const REPETITION3_ARTIFACTS: &str = r#"
---
repetition3.isa.yaml:
  name: repetition3
  blocks: {repetition3: 1}
  instructions:
    - {mnemonic: prepare_z, description: '', out: [repetition3], action: [stabilize: Z_0]}
    - {mnemonic: idle, description: '', in: [repetition3], out: [repetition3]}
    - {mnemonic: __quantum__qis__m__body, description: '', in: [repetition3], action: [observe: Z_0]}
    - {mnemonic: __quantum__qis__mresetz__body, description: '', in: [repetition3], out: [repetition3], action: [observe: Z_0, stabilize: Z_0]}
    - {mnemonic: measure_reset, description: '', in: [repetition3], out: [repetition3], action: [observe: Z_0, stabilize: Z_0]}
    - {mnemonic: __quantum__qis__x__body, description: '', in: [repetition3], out: [repetition3], action: [pauli: X_0]}
    - {mnemonic: __quantum__qis__z__body, description: '', in: [repetition3], out: [repetition3], action: [pauli: Z_0]}
    - {mnemonic: logical_x, description: '', in: [repetition3], out: [repetition3], action: [pauli: X_0]}
    - {mnemonic: discard, description: '', in: [repetition3]}
---
physical.isa.yaml:
  name: physical
  blocks: {qubit: 1}
  instructions:
    - {mnemonic: R, description: '', out: [qubit], action: [stabilize: Z_0]}
    - {mnemonic: M, description: '', in: [qubit], action: [observe: Z_0]}
    - {mnemonic: MZ, description: '', in: [qubit], out: [qubit], action: [observe: Z_0]}
    - {mnemonic: CX, description: '', in: [qubit, qubit], out: [qubit, qubit], action: [clifford: {X_0: X_0 X_1, Z_1: Z_0 Z_1}]}
    - {mnemonic: X, description: '', in: [qubit], out: [qubit], action: [pauli: X_0]}
    - {mnemonic: Z, description: '', in: [qubit], out: [qubit], action: [pauli: Z_0]}
    - {mnemonic: XIF, description: '', in: [qubit], out: [qubit], parameters: {c: bit}, action: [{pauli: X_0, if: [c]}]}
---
repetition3.code.yaml:
  name: repetition3
  stabilizers: [Z_0 Z_1, Z_1 Z_2]
  x: [X_0 X_1 X_2]
  z: [Z_0]
---
prepare_z.gadget.yaml:
  circuit: {source: [{R: [0]}, {R: [1]}, {R: [2]}]}
---
idle.gadget.yaml:
  circuit:
    source: [{R: [3]}, {R: [4]}, {CX: [0, 3]}, {CX: [1, 3]}, {CX: [1, 4]}, {CX: [2, 4]}, {M: [3]}, {M: [4]}]
  checks:
    - ["circuit.readouts[0]", "in[0].stabilizers[0]"]
    - ["circuit.readouts[1]", "in[0].stabilizers[1]"]
    - ["circuit.readouts[0]", "out[0].stabilizers[0]"]
    - ["circuit.readouts[1]", "out[0].stabilizers[1]"]
---
m.gadget.yaml:
  circuit: {source: [{M: [0]}, {M: [1]}, {M: [2]}]}
  checks:
    - ["circuit.readouts[0]", "circuit.readouts[1]", "in[0].stabilizers[0]"]
    - ["circuit.readouts[1]", "circuit.readouts[2]", "in[0].stabilizers[1]"]
  readouts: [["circuit.readouts[0]", "in[0].z[0]"]]
---
mresetz.gadget.yaml:
  circuit: {source: [{M: [0]}, {M: [1]}, {M: [2]}, {R: [0]}, {R: [1]}, {R: [2]}]}
  checks:
    - ["circuit.readouts[0:2]", "in[0].stabilizers[0]"]
    - ["circuit.readouts[1, 2]", "in[0].stabilizers[1]"]
  readouts: [["circuit.readouts[0]", "in[0].z[0]"]]
---
measure_reset.gadget.yaml:
  circuit:
    source:
      - MZ: [0]
      - MZ: [1]
      - MZ: [2]
      - XIF: [0, {c: "circuit.readouts[0]"}]
      - XIF: [1, {c: "circuit.readouts[1]"}]
      - XIF: [2, {c: "circuit.readouts[2]"}]
  checks:
    - ["circuit.readouts[0]", "circuit.readouts[1]", "in[0].stabilizers[0]"]
    - ["circuit.readouts[1]", "circuit.readouts[2]", "in[0].stabilizers[1]"]
  readouts: [["circuit.readouts[0]", "in[0].z[0]"]]
---
x.gadget.yaml:
  circuit: {source: [{X: [0]}, {X: [1]}, {X: [2]}]}
---
z.gadget.yaml:
  circuit: {source: [{Z: [0]}]}
"#;

/// The repetition3 layer. `logical_x` and `discard` have no gadget, so the
/// layer runs them as a logical Pauli and a release.
const REPETITION3_LAYER: &str = r"    - instruction_set: repetition3.isa.yaml
      codes: {repetition3: repetition3.code.yaml}
      gadgets:
        prepare_z: prepare_z.gadget.yaml
        idle: idle.gadget.yaml
        __quantum__qis__m__body: m.gadget.yaml
        __quantum__qis__mresetz__body: mresetz.gadget.yaml
        measure_reset: measure_reset.gadget.yaml
        __quantum__qis__x__body: x.gadget.yaml
        __quantum__qis__z__body: z.gadget.yaml
";

fn repetition3() -> Qodec {
    let bundle = format!(
        "qodec.yaml:\n  name: repetition3\n  layers:\n{REPETITION3_LAYER}    - instruction_set: physical.isa.yaml\n{REPETITION3_ARTIFACTS}"
    );
    Qodec::from_bundle_str(&bundle).expect("bundle should load")
}

/// A logical layer with a trivial one-qubit code, to put on top of
/// repetition3. `flip` and `phase` have no gadget.
const LOGICAL_LAYER: &str = r"    - instruction_set: logical.isa.yaml
      codes: {lq: trivial.code.yaml}
      gadgets:
        prepare: prepare.gadget.yaml
        measure: measure.gadget.yaml
";

const LOGICAL_ARTIFACTS: &str = r#"
---
logical.isa.yaml:
  name: logical
  blocks: {lq: 1}
  instructions:
    - {mnemonic: prepare, description: '', out: [lq], action: [stabilize: Z_0]}
    - {mnemonic: measure, description: '', in: [lq], action: [observe: Z_0]}
    - {mnemonic: flip, description: '', in: [lq], out: [lq], action: [pauli: X_0]}
    - {mnemonic: phase, description: '', in: [lq], out: [lq], action: [pauli: Z_0]}
---
trivial.code.yaml:
  name: trivial
  stabilizers: []
  x: [X_0]
  z: [Z_0]
---
prepare.gadget.yaml:
  circuit: {source: [{prepare_z: [0]}]}
---
measure.gadget.yaml:
  circuit: {source: [{__quantum__qis__m__body: [0]}]}
  readouts: [["circuit.readouts[0]", "in[0].z[0]"]]
"#;

/// A logical layer with a trivial one-qubit code on top of repetition3.
fn logical_over_repetition3() -> Qodec {
    let bundle = format!(
        "qodec.yaml:\n  name: logical\n  layers:\n{LOGICAL_LAYER}{REPETITION3_LAYER}    - instruction_set: physical.isa.yaml\n{LOGICAL_ARTIFACTS}{REPETITION3_ARTIFACTS}"
    );
    Qodec::from_bundle_str(&bundle).expect("bundle should load")
}

/// A second trivial-code layer on top of [`logical_over_repetition3`], so that
/// two layers sit above the one that lowers to the physical ISA. `flip` has no
/// gadget; the logical layer applies it with its own `flip`, and it needs a
/// `z` Pauli there too, which `phase` provides.
fn outer_over_logical_over_repetition3() -> Qodec {
    let bundle = format!(
        r#"qodec.yaml:
  name: outer
  layers:
    - instruction_set: outer.isa.yaml
      codes: {{oq: trivial.code.yaml}}
      gadgets:
        prepare: outer_prepare.gadget.yaml
        measure: outer_measure.gadget.yaml
{LOGICAL_LAYER}{REPETITION3_LAYER}    - instruction_set: physical.isa.yaml
---
outer.isa.yaml:
  name: outer
  blocks: {{oq: 1}}
  instructions:
    - {{mnemonic: prepare, description: '', out: [oq], action: [stabilize: Z_0]}}
    - {{mnemonic: measure, description: '', in: [oq], action: [observe: Z_0]}}
    - {{mnemonic: flip, description: '', in: [oq], out: [oq], action: [pauli: X_0]}}
---
outer_prepare.gadget.yaml:
  circuit: {{source: [{{prepare: [0]}}]}}
---
outer_measure.gadget.yaml:
  circuit: {{source: [{{measure: [0]}}]}}
  readouts: [["circuit.readouts[0]", "in[0].z[0]"]]
{LOGICAL_ARTIFACTS}{REPETITION3_ARTIFACTS}"#
    );
    Qodec::from_bundle_str(&bundle).expect("bundle should load")
}

/// A classical simulator of computational-basis states, enough for the
/// repetition code's Z-basis circuits. It grows on demand, or models a fixed
/// device that fails ids it cannot hold. It logs every instruction it runs.
#[derive(Default)]
struct BitSim {
    mnemonics: Vec<String>,
    bits: Vec<bool>,
    fixed_size: Option<usize>,
    batches: usize,
    fail: bool,
    /// Qubits that suffer a bit-flip error right before their next measurement.
    flips: Vec<usize>,
    log: Vec<Call>,
}

impl BitSim {
    /// A simulator in its initial state, which grows on demand.
    fn new(isa: &InstructionSet) -> Self {
        Self {
            mnemonics: isa
                .instructions
                .iter()
                .map(|i| i.mnemonic.clone())
                .collect(),
            ..Self::default()
        }
    }

    /// A simulator with a fixed number of qubits.
    fn fixed(isa: &InstructionSet, qubits: usize) -> Self {
        Self {
            bits: vec![false; qubits],
            fixed_size: Some(qubits),
            ..Self::new(isa)
        }
    }

    /// Returns to the initial state, for reuse in another shot.
    fn reset(&mut self) {
        self.bits.fill(false);
    }

    /// Makes room for qubit `q`, or fails if the simulator has a fixed size.
    fn hold(&mut self, q: usize) -> Result<(), String> {
        if q < self.bits.len() {
            Ok(())
        } else if self.fixed_size.is_some() {
            Err(format!("qubit {q} is out of range"))
        } else {
            self.bits.resize(q + 1, false);
            Ok(())
        }
    }

    /// How many instructions named `mnemonic` it ran.
    fn count(&self, mnemonic: &str) -> usize {
        self.log
            .iter()
            .filter(|c| self.mnemonics[c.instruction_id] == mnemonic)
            .count()
    }
}

impl Backend<Vec<Call>, Result<Vec<BasicReadout>, String>> for BitSim {
    fn execute(&mut self, instructions: Vec<Call>) -> Result<Vec<BasicReadout>, String> {
        self.batches += 1;
        if self.fail {
            return Err("boom".to_string());
        }
        let mut results = Vec::new();
        for ins in &instructions {
            self.log.push(ins.clone());
            for p in &ins.parameters {
                if let OwnedParam::Block(b) = p {
                    self.hold(usize::try_from(*b).expect("small id"))?;
                }
            }
            let q = |i: usize| match ins.parameters[i] {
                OwnedParam::Block(b) => usize::try_from(b).expect("small id"),
                ref other => panic!("expected a block, got {other:?}"),
            };
            match self.mnemonics[ins.instruction_id].as_str() {
                "R" => self.bits[q(0)] = false,
                "M" | "MZ" => {
                    if let Some(i) = self.flips.iter().position(|&f| f == q(0)) {
                        self.flips.swap_remove(i);
                        self.bits[q(0)] ^= true;
                    }
                    results.push(BasicReadout::new(
                        &[Some(self.bits[q(0)])],
                        &[],
                        Status::default(),
                    ));
                }
                "CX" => {
                    if self.bits[q(0)] {
                        self.bits[q(1)] ^= true;
                    }
                }
                "X" => self.bits[q(0)] ^= true,
                "Z" => {}
                "XIF" => {
                    if ins.parameters[1] == OwnedParam::Bool(true) {
                        self.bits[q(0)] ^= true;
                    }
                }
                other => return Err(format!("unsupported instruction {other}")),
            }
        }
        Ok(results)
    }
}

/// The plan of the top layer of `qodec`.
fn top_layer_plan(qodec: &Qodec) -> Arc<LayerModel> {
    Arc::clone(&read(qodec).expect("qodec reads").layers[0])
}

/// `qodec`, read and assembled by `design`.
fn qodec_plan(qodec: &Qodec, design: &dyn Design) -> Result<QodecPlan, PlanError> {
    QodecPlan::new(&read(qodec)?, design)
}

/// `layer`, from the top ISA of `qodec` to its second ISA, over a fresh
/// simulator.
fn on_sim<L>(qodec: &Qodec, layer: L) -> Runtime<L, BitSim, String> {
    let layers = qodec.layers();
    let (top, lower) = (&layers[0].instruction_set, &layers[1].instruction_set);
    let isa = |isa| Arc::new(spec::isa(isa).expect("ISA reads"));
    Runtime::new(layer, BitSim::new(lower), &isa(top), &isa(lower))
}

/// The reference layer, assembled by hand from its pieces.
type FrameLayer = Stack<Passive<FrameDecoder>, SpecEncoder>;

/// One shot of the top layer of `qodec`, directly over a fresh simulator.
fn single_layer(qodec: &Qodec) -> Runtime<FrameLayer, BitSim, String> {
    let plan = top_layer_plan(qodec);
    let blueprint = Stack::new(
        FrameDecoderPlan::plan(&plan).expect("decoder plans"),
        SpecEncoderPlan::<Never>::plan(&plan).expect("encoder plans"),
    );
    on_sim(qodec, blueprint.instantiate())
}

fn support(layer: &Runtime<FrameLayer, BitSim, String>, label: u64) -> Vec<u64> {
    layer
        .layer()
        .lower
        .live_block(label)
        .expect("live")
        .support
        .clone()
}

fn op(isa: &Arc<InstructionSet>, mnemonic: &str, blocks: &[u64]) -> Call {
    Call {
        instruction_id: index_of(isa, mnemonic),
        parameters: blocks.iter().map(|&b| OwnedParam::Block(b)).collect(),
    }
}

fn index_of(isa: &InstructionSet, mnemonic: &str) -> usize {
    isa.instructions
        .iter()
        .position(|i| i.mnemonic == mnemonic)
        .unwrap_or_else(|| panic!("no instruction `{mnemonic}`"))
}

fn outcomes(batch: &[BasicReadout]) -> Vec<Option<bool>> {
    batch.iter().flat_map(|r| r.outcomes.clone()).collect()
}

fn one_block() -> Resources {
    Resources::blocks([("repetition3".to_string(), 1)])
}

const M: &str = "__quantum__qis__m__body";
const X: &str = "__quantum__qis__x__body";

#[test]
fn required_resources_include_scratch() {
    let qodec = repetition3();
    let plan = top_layer_plan(&qodec);
    // Three data qubits per block, plus the idle gadget's two ancillas.
    assert_eq!(
        plan.required_resources(&one_block()),
        Ok(Resources::qubits(5))
    );
    assert_eq!(
        plan.required_resources(&Resources::qubits(2)),
        Ok(Resources::qubits(8))
    );
}

#[test]
fn logical_measurements_follow_logical_gates() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);

    let batch = layer
        .execute(vec![op(&isa, "prepare_z", &[0]), op(&isa, M, &[0])])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(false)]);

    let batch = layer
        .execute(vec![
            op(&isa, "prepare_z", &[0]),
            op(&isa, X, &[0]),
            op(&isa, "idle", &[0]),
            op(&isa, "__quantum__qis__z__body", &[0]),
            op(&isa, "idle", &[0]),
            op(&isa, M, &[0]),
        ])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(true)]);
    assert_eq!(batch[0].status, Status::default());
}

#[test]
fn logical_paulis_apply_the_code_operator() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    // `logical_x` has no gadget, so this applies the code's logical X, X on all
    // three data qubits, as Pauli corrections.
    let batch = layer
        .execute(vec![
            op(&isa, "prepare_z", &[0]),
            op(&isa, "logical_x", &[0]),
            op(&isa, "idle", &[0]),
            op(&isa, M, &[0]),
        ])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(true)]);
    assert_eq!(layer.backend().count("X"), 3);
}

#[test]
fn mresetz_reports_then_resets() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    let batch = layer
        .execute(vec![
            op(&isa, "prepare_z", &[0]),
            op(&isa, X, &[0]),
            op(&isa, "__quantum__qis__mresetz__body", &[0]),
            op(&isa, M, &[0]),
        ])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(true), Some(false)]);
}

#[test]
fn non_adaptive_batches_cost_one_lower_batch() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    layer
        .execute(vec![
            op(&isa, "prepare_z", &[0]),
            op(&isa, "idle", &[0]),
            op(&isa, M, &[0]),
        ])
        .expect("runs");
    assert_eq!(layer.backend().batches, 1);
}

#[test]
fn bit_parameters_thread_earlier_readouts() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    let batch = layer
        .execute(vec![
            op(&isa, "prepare_z", &[0]),
            op(&isa, X, &[0]),
            op(&isa, "measure_reset", &[0]),
            op(&isa, M, &[0]),
        ])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(true), Some(false)]);
    // `measure_reset` waits for `circuit.readouts[0]`, which takes a second
    // lower batch. The encoder holds the calls after it meanwhile.
    assert_eq!(layer.backend().batches, 2);
}

#[test]
fn contradicting_syndromes_are_inconsistent() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    layer
        .execute(vec![op(&isa, "prepare_z", &[0]), op(&isa, "idle", &[0])])
        .expect("runs");
    // A bit flip on the middle data qubit between two syndrome rounds.
    let data = usize::try_from(support(&layer, 0)[1]).expect("small id");
    layer.backend_mut().bits[data] ^= true;
    let batch = layer
        .execute(vec![op(&isa, "idle", &[0]), op(&isa, M, &[0])])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![None]);
    assert!(batch[0].status.inconsistent);
}

#[test]
fn lower_ids_are_allocated_on_demand_and_reused() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    layer
        .execute(vec![
            op(&isa, "prepare_z", &[0]),
            op(&isa, "prepare_z", &[1]),
        ])
        .expect("runs");
    // Fresh ids, densely from 0.
    assert_eq!(support(&layer, 0), vec![0, 1, 2]);
    assert_eq!(support(&layer, 1), vec![3, 4, 5]);

    // Released ids are reused, lowest first, before any fresh one.
    let batch = layer
        .execute(vec![
            op(&isa, "discard", &[0]),
            op(&isa, "prepare_z", &[2]),
            op(&isa, X, &[2]),
            op(&isa, M, &[2]),
        ])
        .expect("runs after release");
    assert_eq!(outcomes(&batch), vec![Some(true)]);
    assert_eq!(layer.backend().bits.len(), 6);
}

#[test]
fn malformed_calls_are_rejected_without_losing_the_shot() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    let err = layer
        .execute(vec![Call {
            instruction_id: 99,
            parameters: SmallVec::new(),
        }])
        .expect_err("unknown id");
    assert!(matches!(err, QodecRuntimeError::Rejected(_)));
    assert!(!layer.is_poisoned());
    let batch = layer
        .execute(vec![op(&isa, "prepare_z", &[0]), op(&isa, M, &[0])])
        .expect("the shot goes on");
    assert_eq!(outcomes(&batch), vec![Some(false)]);
}

#[test]
fn faults_lose_the_shot() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    let err = layer
        .execute(vec![op(&isa, "idle", &[0])])
        .expect_err("block 0 is not live");
    let QodecRuntimeError::Fault(fault) = err else {
        panic!("expected a fault, got {err:?}");
    };
    assert_eq!(fault.kind, FaultKind::InvalidInstruction);
    assert!(layer.is_poisoned());
    let err = layer
        .execute(vec![op(&isa, "prepare_z", &[0])])
        .expect_err("poisoned");
    assert_eq!(err, QodecRuntimeError::Poisoned);
}

#[test]
fn type_checkers_reject_ill_typed_batches_whole_without_losing_the_shot() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut checked = Stack::new(
        TypeChecker::new(&Arc::new(spec::isa(&isa).expect("ISA reads"))),
        single_layer(&qodec),
    );
    // `prepare_z 0` is fine, but block 1 is not live, so nothing runs.
    let err = checked
        .execute(vec![op(&isa, "prepare_z", &[0]), op(&isa, "idle", &[1])])
        .expect_err("block 1 is not live");
    let QodecRuntimeError::Rejected(fault) = err else {
        panic!("expected a rejection, got {err:?}");
    };
    assert_eq!(
        (fault.kind, fault.detail.as_str()),
        (FaultKind::InvalidInstruction, "block 1 is not live")
    );
    let runtime = &checked.lower;
    assert!(!runtime.is_poisoned());
    assert_eq!(runtime.backend().batches, 0);
    assert!(runtime.layer().lower.live_block(0).is_none());

    let batch = checked
        .execute(vec![op(&isa, "prepare_z", &[0]), op(&isa, M, &[0])])
        .expect("the shot goes on");
    assert_eq!(outcomes(&batch), vec![Some(false)]);
}

#[test]
fn type_checkers_track_live_blocks_and_undo_rejected_batches() {
    let isa = Arc::new(InstructionSet {
        name: "two types".to_string(),
        description: String::new(),
        blocks: vec![
            qodec::Block {
                name: "a".to_string(),
                encodes: 1,
            },
            qodec::Block {
                name: "b".to_string(),
                encodes: 1,
            },
        ],
        instructions: vec![
            instruction(&[], &[("a", false)]),
            instruction(&[("b", false)], &[("b", false)]),
            instruction(&[("a", false)], &[]),
        ],
        metadata: qodec::Metadata::default(),
    });
    let call = |id, block| Call {
        instruction_id: id,
        parameters: [OwnedParam::Block(block)].into_iter().collect(),
    };
    let mut checker = TypeChecker::<String>::new(&Arc::new(spec::isa(&isa).expect("ISA reads")));
    checker.check(&[call(0, 7)]).expect("prepares an `a`");
    assert_eq!(checker.live_type(7), Some("a"));

    let err = checker.check(&[call(1, 7)]).expect_err("7 is not a `b`");
    assert_eq!(err, "operand has type `a`, expected `b`");

    // The last call fails, so the earlier ones are undone too.
    let err = checker
        .check(&[call(2, 7), call(0, 8), call(1, 8)])
        .expect_err("8 is not a `b`");
    assert_eq!(err, "operand has type `a`, expected `b`");
    assert_eq!(checker.live_type(7), Some("a"));
    assert_eq!(checker.live_type(8), None);

    checker.check(&[call(2, 7)]).expect("measures 7");
    assert_eq!(checker.live_type(7), None);

    // A batch the runtime below rejects did not run: the checker forgets it.
    let reject = from_fn(
        |_: Vec<Call>| -> Result<Vec<BasicReadout>, QodecRuntimeError<String>> {
            Err(QodecRuntimeError::Rejected(Fault::invalid("no")))
        },
    );
    let mut over_rejecting = Stack::new(checker, reject);
    over_rejecting
        .execute(vec![call(0, 9)])
        .expect_err("rejected below");
    assert_eq!(over_rejecting.upper.live_type(9), None);
}

#[test]
fn lower_errors_lose_the_shot() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    layer.backend_mut().fail = true;
    let err = layer
        .execute(vec![op(&isa, "prepare_z", &[0])])
        .expect_err("lower fails");
    assert_eq!(err, QodecRuntimeError::Bottom("boom".to_string()));
    assert!(layer.is_poisoned());
    layer.backend_mut().fail = false;
    let err = layer
        .execute(vec![op(&isa, "prepare_z", &[0])])
        .expect_err("poisoned");
    assert_eq!(err, QodecRuntimeError::Poisoned);

    // The shot is lost; the next one starts clean.
    let mut layer = single_layer(&qodec);
    let batch = layer
        .execute(vec![op(&isa, "prepare_z", &[0]), op(&isa, M, &[0])])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(false)]);
}

#[test]
fn required_resources_reject_unknown_block_types() {
    let qodec = logical_over_repetition3();
    let model = read(&qodec).expect("qodec reads");
    let unknown = Resources::blocks([("nope".to_string(), 1)]);
    assert!(model.required_resources(unknown).is_err());
}

// ── Arguments ───────────────────────────────────────────────────────────────

#[test]
fn list_arguments_are_type_errors() {
    use qodec::{Argument, ParameterKind};
    // The spec's YAML would let `[1, 2]` supply an `integer` parameter, element
    // by element. The runtime rejects it (see `spec::matches`).
    assert!(!matches(
        ParameterKind::Integer,
        &Argument::QubitList(vec![1, 2])
    ));
    assert!(!matches(
        ParameterKind::Integer,
        &Argument::QubitList(Vec::new())
    ));
    let names = Argument::StringList(vec!["a".to_string()]);
    assert!(!matches(ParameterKind::String, &names));
    assert_eq!(encode_arg(&names), None);

    // A qodec whose gadget passes a list still loads and plans; running the
    // gadget fails.
    let bundle = r"qodec.yaml:
  name: lists
  layers:
    - instruction_set: logical.isa.yaml
      codes: {lq: trivial.code.yaml}
      gadgets:
        prepare: prepare.gadget.yaml
        tagged: tagged.gadget.yaml
    - instruction_set: physical.isa.yaml
---
logical.isa.yaml:
  name: logical
  blocks: {lq: 1}
  instructions:
    - {mnemonic: prepare, description: '', out: [lq], action: [stabilize: Z_0]}
    - {mnemonic: tagged, description: '', in: [lq], out: [lq]}
---
physical.isa.yaml:
  name: physical
  blocks: {qubit: 1}
  instructions:
    - {mnemonic: R, description: '', out: [qubit], action: [stabilize: Z_0]}
    - {mnemonic: tag, description: '', in: [qubit], out: [qubit], parameters: {targets: integer}}
---
trivial.code.yaml:
  name: trivial
  stabilizers: []
  x: [X_0]
  z: [Z_0]
---
prepare.gadget.yaml:
  circuit: {source: [{R: [0]}]}
---
tagged.gadget.yaml:
  circuit: {source: [{tag: [0, targets: [1, 2]]}]}
";
    let qodec = Qodec::from_bundle_str(bundle).expect("bundle should load");
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let bottom = Arc::clone(&qodec.layers()[1].instruction_set);
    let qodec_plan = qodec_plan(&qodec, &FrameDecoding).expect("qodec plans");
    let err = shot(&qodec_plan, BitSim::new(&bottom))
        .execute(vec![op(&isa, "prepare", &[0]), op(&isa, "tagged", &[0])])
        .expect_err("`tag` takes one integer");
    let QodecRuntimeError::Fault(fault) = err else {
        panic!("expected a fault, got {err:?}");
    };
    assert_eq!(fault.kind, FaultKind::InvalidInstruction);
}

// ── Corrections ─────────────────────────────────────────────────────────────

/// A logical layer over a physical ISA with no Pauli instructions. With `flip`,
/// the logical layer has a Pauli-only instruction without a gadget.
fn logical_over_pauli_free_isa(flip: bool) -> Qodec {
    let flip = if flip {
        "    - {mnemonic: flip, description: '', in: [lq], out: [lq], action: [pauli: X_0]}\n"
    } else {
        ""
    };
    let bundle = format!(
        r#"qodec.yaml:
  name: pauli_free
  layers:
    - instruction_set: logical.isa.yaml
      codes: {{lq: trivial.code.yaml}}
      gadgets:
        prepare: prepare.gadget.yaml
        measure: measure.gadget.yaml
    - instruction_set: physical.isa.yaml
---
logical.isa.yaml:
  name: logical
  blocks: {{lq: 1}}
  instructions:
    - {{mnemonic: prepare, description: '', out: [lq], action: [stabilize: Z_0]}}
    - {{mnemonic: measure, description: '', in: [lq], action: [observe: Z_0]}}
{flip}---
physical.isa.yaml:
  name: physical
  blocks: {{qubit: 1}}
  instructions:
    - {{mnemonic: R, description: '', out: [qubit], action: [stabilize: Z_0]}}
    - {{mnemonic: M, description: '', in: [qubit], action: [observe: Z_0]}}
---
trivial.code.yaml:
  name: trivial
  stabilizers: []
  x: [X_0]
  z: [Z_0]
---
prepare.gadget.yaml:
  circuit: {{source: [{{R: [0]}}]}}
---
measure.gadget.yaml:
  circuit: {{source: [{{M: [0]}}]}}
  readouts: [["circuit.readouts[0]", "in[0].z[0]"]]
"#
    );
    Qodec::from_bundle_str(&bundle).expect("bundle should load")
}

/// The backend below a wrapped decoder that never corrects: passes its calls
/// on, and keeps a copy of the records that come back.
struct Tap<'a, C> {
    below: &'a mut dyn Backend<Vec<Lower<C>>, Result<Vec<Record>, Fault>>,
    records: Vec<Record>,
}

impl<C> Backend<Vec<Lower>, Result<Vec<Record>, Fault>> for Tap<'_, C> {
    fn execute(&mut self, batch: Vec<Lower>) -> Result<Vec<Record>, Fault> {
        let batch = batch
            .into_iter()
            .map(|m| match m {
                Lower::Call(call) => Lower::Call(call),
                Lower::Fix(never) => match never {},
            })
            .collect();
        let records = self.below.execute(batch)?;
        self.records.extend(records.iter().cloned());
        Ok(records)
    }
}

/// A decoder that corrects: after every `prepare_z`, it flips each code qubit
/// of the new block with an `X` correction. The decoder it wraps doesn't know,
/// so a measurement then reads 1. It sends one call at a time, so that its
/// corrections reach a block before any later call uses it (see
/// [`Decoder`](crate::Decoder)).
struct FlipPrepared<D> {
    inner: D,
    layer: Arc<LayerModel>,
    prepare: usize,
}

impl<D> Layer for FlipPrepared<D>
where
    D: Layer<
            Input = Vec<Call>,
            Output = Result<Vec<Reply>, Fault>,
            LowerInput = Vec<Lower>,
            LowerOutput = Result<Vec<Record>, Fault>,
        >,
{
    type Input = Vec<Call>;
    type Output = Result<Vec<Reply>, Fault>;
    type LowerInput = Vec<Lower<Correction>>;
    type LowerOutput = Result<Vec<Record>, Fault>;

    fn execute(
        &mut self,
        calls: Vec<Call>,
        below: &mut dyn Backend<Vec<Lower<Correction>>, Result<Vec<Record>, Fault>>,
    ) -> Result<Vec<Reply>, Fault> {
        let mut replies = Vec::new();
        for call in calls {
            let mut tap = Tap {
                below: &mut *below,
                records: Vec::new(),
            };
            replies.extend(self.inner.execute(vec![call], &mut tap)?);
            for record in tap.records {
                let Some(inv) = record
                    .invocation
                    .filter(|_| record.instruction == self.prepare)
                else {
                    continue;
                };
                let n = self.layer.gadgets[inv.gadget.0].outputs[0].code.n;
                let x_all = Pauli::new(false, (0..n).map(|q| (q, PauliOp::X)).collect())
                    .expect("distinct qubits");
                below.execute(vec![Lower::Fix(Correction {
                    blocks: vec![inv.output_refs[0].clone()],
                    step: Step::Pauli(x_all),
                })])?;
            }
        }
        Ok(replies)
    }
}

#[test]
fn decoders_correct_live_blocks_through_their_encoder() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let plan = top_layer_plan(&qodec);
    let decoder = FlipPrepared {
        inner: FrameDecoderPlan::plan(&plan)
            .expect("decoder plans")
            .instantiate(),
        layer: Arc::clone(&plan),
        prepare: index_of(&isa, "prepare_z"),
    };
    let encoder = SpecEncoderPlan::<Correction>::plan(&plan)
        .expect("the lower ISA has `x` and `z`")
        .instantiate();
    let mut layer = on_sim(&qodec, Stack::new(decoder, encoder));
    let batch = layer
        .execute(vec![op(&isa, "prepare_z", &[0]), op(&isa, M, &[0])])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(true)]);
    assert_eq!(layer.backend().count("X"), 3);
}

#[test]
fn only_encoders_that_take_corrections_need_lower_paulis() {
    let qodec = logical_over_pauli_free_isa(false);
    assert!(qodec_plan(&qodec, &FrameDecoding).is_ok());
    let plan = top_layer_plan(&qodec);
    assert!(SpecEncoderPlan::<Never>::plan(&plan).is_ok());
    assert_eq!(
        SpecEncoderPlan::<Correction>::plan(&plan).err(),
        Some(PlanError(
            "the lower ISA has no `X` Pauli for block type `qubit`".to_string()
        ))
    );
}

#[test]
fn logical_paulis_need_lower_paulis_whatever_the_decoder() {
    let qodec = logical_over_pauli_free_isa(true);
    let err = qodec_plan(&qodec, &FrameDecoding)
        .err()
        .expect("`flip` is applied through `x` below");
    assert_eq!(
        err,
        PlanError("layer 0: the lower ISA has no `X` Pauli for block type `qubit`".to_string())
    );
}

// ── Controllers ─────────────────────────────────────────────────────────────

/// A decoder that holds each result until `rounds` QEC rounds ran on the
/// blocks its call left live, and asks for them at the end of a batch.
/// Wrapped around any decoder that never pends, it shows the `Pending`
/// protocol.
struct Delayed<D> {
    inner: D,
    layer: Arc<LayerModel>,
    rounds: usize,
    held: VecDeque<Held>,
    /// The live block of each label.
    live: FxHashMap<u64, BlockRef>,
}

struct Held {
    readout: BasicReadout,
    labels: Vec<u64>,
    /// Records on `labels` since this result.
    seen: usize,
}

impl<D> Layer for Delayed<D>
where
    D: Layer<
            Input = Vec<Call>,
            Output = Result<Vec<Reply>, Fault>,
            LowerInput = Vec<Lower>,
            LowerOutput = Result<Vec<Record>, Fault>,
        >,
{
    type Input = Vec<Call>;
    type Output = Result<Vec<Reply<Rounds>>, Fault>;
    type LowerInput = Vec<Lower>;
    type LowerOutput = Result<Vec<Record>, Fault>;

    fn execute(
        &mut self,
        calls: Vec<Call>,
        below: &mut dyn Backend<Vec<Lower>, Result<Vec<Record>, Fault>>,
    ) -> Result<Vec<Reply<Rounds>>, Fault> {
        let mut tap = Tap {
            below,
            records: Vec::new(),
        };
        let mut replies = self.inner.execute(calls, &mut tap)?.into_iter();
        for record in &tap.records {
            let (operands, outputs) = record
                .invocation
                .as_ref()
                .map(|inv| (inv.operands.clone(), inv.output_refs.clone()))
                .unwrap_or_default();
            for held in &mut self.held {
                if held.labels.iter().any(|l| operands.contains(l)) {
                    held.seen += 1;
                }
            }
            for r in record.ended() {
                if self.live.get(&r.label) == Some(r) {
                    self.live.remove(&r.label);
                }
            }
            for r in &outputs {
                self.live.insert(r.label, r.clone());
            }
            if self.layer.upper.instructions[record.instruction].has_result() {
                let reply = replies
                    .next()
                    .ok_or_else(|| Fault::protocol("a missing result"))?;
                self.held.push_back(Held {
                    readout: reply.into_ready(),
                    labels: outputs.iter().map(|r| r.label).collect(),
                    seen: 0,
                });
            }
        }

        let mut out = Vec::new();
        while let Some(front) = self.held.front() {
            let live: Vec<BlockRef> = front
                .labels
                .iter()
                .filter_map(|l| self.live.get(l).cloned())
                .collect();
            if front.seen < self.rounds && !live.is_empty() {
                out.push(Reply::Pending(Rounds { blocks: live }));
                break;
            }
            if let Some(held) = self.held.pop_front() {
                out.push(Reply::Ready(held.readout));
            }
        }
        Ok(out)
    }
}

struct DelayedPlan {
    layer: Arc<LayerModel>,
    frame: FrameDecoderPlan,
    rounds: usize,
}

impl Blueprint for DelayedPlan {
    type Instance = Delayed<Passive<FrameDecoder>>;

    fn instantiate(&self) -> Self::Instance {
        Delayed {
            inner: self.frame.instantiate(),
            layer: Arc::clone(&self.layer),
            rounds: self.rounds,
            held: VecDeque::new(),
            live: FxHashMap::default(),
        }
    }
}

/// The repetition3 layer with a [`Delayed`] frame decoder and an `UntilReady`
/// controller that finds `idle` by its action.
fn delayed_layer(
    qodec: &Qodec,
    rounds: usize,
    max_rounds: usize,
) -> impl Blueprint<Instance: QodecLayer> {
    let plan = top_layer_plan(qodec);
    Stack::new(
        UntilReadyPlan::plan(&plan, max_rounds).expect("`idle` is a QEC round"),
        Stack::new(
            DelayedPlan {
                layer: Arc::clone(&plan),
                frame: FrameDecoderPlan::plan(&plan).expect("decoder plans"),
                rounds,
            },
            SpecEncoderPlan::<Never>::plan(&plan).expect("encoder plans"),
        ),
    )
}

#[test]
fn controllers_add_rounds_until_the_decoder_is_ready() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = on_sim(&qodec, delayed_layer(&qodec, 2, 5).instantiate());

    // `mresetz` leaves block 0 live, so its result waits for two rounds, which
    // nothing else in the batch runs: the controller adds two `idle`s.
    let batch = layer
        .execute(vec![
            op(&isa, "prepare_z", &[0]),
            op(&isa, X, &[0]),
            op(&isa, "__quantum__qis__mresetz__body", &[0]),
        ])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(true)]);
    assert_eq!(layer.backend().count("CX"), 2 * 4);
    assert_eq!(layer.backend().batches, 3);

    // A destructive measurement leaves no block to wait on.
    let batch = layer.execute(vec![op(&isa, M, &[0])]).expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(false)]);
    assert_eq!(layer.backend().count("CX"), 2 * 4);
}

#[test]
fn controllers_give_up_on_decoders_that_never_get_ready() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = on_sim(&qodec, delayed_layer(&qodec, 3, 2).instantiate());
    let err = layer
        .execute(vec![
            op(&isa, "prepare_z", &[0]),
            op(&isa, "__quantum__qis__mresetz__body", &[0]),
        ])
        .expect_err("three rounds needed, two allowed");
    let QodecRuntimeError::Fault(fault) = err else {
        panic!("expected a fault, got {err:?}");
    };
    assert_eq!(fault.kind, FaultKind::Protocol);
}

#[test]
fn rounds_are_found_by_their_action() {
    // The logical layer has no identity instruction on `lq`.
    let qodec = logical_over_repetition3();
    let err = UntilReadyPlan::plan(&top_layer_plan(&qodec), 1).expect_err("no round");
    assert_eq!(
        err,
        PlanError("no instruction runs a QEC round on `lq`".to_string())
    );
}

// ── deq decoders ────────────────────────────────────────────────────────────

/// Assembles the layer that lowers to `bottom` with a [`DeqPlan`] decoder, and
/// every other layer as [`FrameDecoding`] does. Records each choice by the
/// layer's upper ISA.
struct DeqAtBottom {
    bottom: Arc<InstructionSet>,
    picked: RefCell<Vec<(String, &'static str)>>,
}

impl DeqAtBottom {
    fn new(bottom: &Arc<InstructionSet>) -> Self {
        Self {
            bottom: Arc::clone(bottom),
            picked: RefCell::default(),
        }
    }
}

impl Design for DeqAtBottom {
    fn layer(&self, layer: &Arc<LayerModel>) -> Result<Box<dyn LayerBlueprint>, PlanError> {
        let deq = layer.lower.name == self.bottom.name;
        self.picked
            .borrow_mut()
            .push((layer.upper.name.clone(), if deq { "deq" } else { "frame" }));
        if deq {
            Ok(Box::new(Stack::new(
                DeqPlan::new(layer)?,
                SpecEncoderPlan::<Never>::plan(layer)?,
            )))
        } else {
            FrameDecoding.layer(layer)
        }
    }
}

mod deq;

use deq::DeqPlan;

#[test]
fn designs_mix_decoders_per_layer() {
    let qodec = outer_over_logical_over_repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let bottom_isa = Arc::clone(&qodec.layers()[3].instruction_set);
    let design = DeqAtBottom::new(&bottom_isa);
    let qodec_plan = qodec_plan(&qodec, &design).expect("qodec plans");
    assert_eq!(
        design.picked.take(),
        [
            ("outer".to_string(), "frame"),
            ("logical".to_string(), "frame"),
            ("repetition3".to_string(), "deq"),
        ]
    );

    // `measure` runs the repetition3 measurement gadget, which deq decodes.
    for flip in [false, true] {
        let mut ops = vec![op(&isa, "prepare", &[0])];
        if flip {
            ops.push(op(&isa, "flip", &[0]));
        }
        ops.push(op(&isa, "measure", &[0]));
        let batch = shot(&qodec_plan, BitSim::new(&bottom_isa))
            .execute(ops)
            .expect("runs");
        assert_eq!(outcomes(&batch), vec![Some(flip)]);
        assert_eq!(batch[0].status, Status::default());
    }
}

#[test]
fn deq_corrects_a_bit_flip_that_the_frame_decoder_misses() {
    let qodec = outer_over_logical_over_repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let bottom_isa = Arc::clone(&qodec.layers()[3].instruction_set);
    let program = [
        op(&isa, "prepare", &[0]),
        op(&isa, "flip", &[0]),
        op(&isa, "measure", &[0]),
    ];
    let noisy = |qubit| BitSim {
        flips: vec![qubit],
        ..BitSim::new(&bottom_isa)
    };

    // `prepare` puts the repetition code on physical qubits 0 to 2. Flip each
    // in turn right before the final measurement: deq corrects it.
    let with_deq = qodec_plan(&qodec, &DeqAtBottom::new(&bottom_isa)).expect("qodec plans");
    for qubit in 0..3 {
        let batch = shot(&with_deq, noisy(qubit))
            .execute(&program[..])
            .expect("runs");
        assert_eq!(outcomes(&batch), vec![Some(true)], "flip on qubit {qubit}");
        assert_eq!(batch[0].status, Status::default());
    }

    // The frame decoder alone misses the flip of qubit 0, the one the logical
    // readout reads. No gadget's checks determine the stabilizer signs that
    // `prepare_z` leaves, so the measurement's checks only solve for them.
    let frame_only = qodec_plan(&qodec, &FrameDecoding).expect("qodec plans");
    let batch = shot(&frame_only, noisy(0))
        .execute(&program[..])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(false)]);
    assert_eq!(batch[0].status, Status::default());
}

// ── Whole qodecs ────────────────────────────────────────────────────────────

#[test]
fn single_layer_qodecs_pass_through_to_the_bottom_backend() {
    let bundle = r"qodec.yaml:
  name: physical_only
  layers:
    - instruction_set: physical.isa.yaml
---
physical.isa.yaml:
  name: physical
  blocks: {qubit: 1}
  instructions:
    - {mnemonic: R, description: '', out: [qubit], action: [stabilize: Z_0]}
    - {mnemonic: M, description: '', in: [qubit], action: [observe: Z_0]}
    - {mnemonic: X, description: '', in: [qubit], out: [qubit], action: [pauli: X_0]}
";
    let qodec = Qodec::from_bundle_str(bundle).expect("bundle should load");
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let model = read(&qodec).expect("qodec reads");
    assert!(model.layers.is_empty());
    let top = Resources::qubits(2);
    assert_eq!(model.required_resources(top.clone()), Ok(vec![top]));
    let qodec_plan = QodecPlan::new(&model, &FrameDecoding).expect("qodec plans");

    // Instructions reach the backend unchanged, with the caller's block ids.
    let mut runtime = shot(&qodec_plan, BitSim::new(&isa));
    let batch = runtime
        .execute(vec![
            op(&isa, "R", &[0]),
            op(&isa, "R", &[1]),
            op(&isa, "X", &[1]),
            op(&isa, "M", &[0]),
            op(&isa, "M", &[1]),
        ])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(false), Some(true)]);
}

#[test]
fn qodec_plans_instantiate_a_fresh_runtime_per_shot() {
    let qodec = logical_over_repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let bottom_isa = Arc::clone(&qodec.layers()[2].instruction_set);
    let qodec_plan = qodec_plan(&qodec, &FrameDecoding).expect("qodec plans");

    // A simulator built for the shot, owned by the runtime. `flip` has no gadget:
    // the top layer applies the logical X as a correction, which the middle
    // layer runs through its X gadget.
    let mut runtime = shot(&qodec_plan, BitSim::new(&bottom_isa));
    let batch = runtime
        .execute(vec![
            op(&isa, "prepare", &[0]),
            op(&isa, "flip", &[0]),
            op(&isa, "measure", &[0]),
        ])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(true)]);
    let err = runtime
        .execute(vec![op(&isa, "measure", &[0])])
        .expect_err("block 0 was consumed");
    let QodecRuntimeError::Fault(fault) = err else {
        panic!("expected a fault, got {err:?}");
    };
    assert_eq!(
        (fault.kind, fault.layer),
        (FaultKind::InvalidInstruction, Some(0))
    );
    drop(runtime);

    // One simulator lent to two shots and reset by its owner in between. Block
    // ids and decoder state start over in every shot.
    let mut sim = BitSim::new(&bottom_isa);
    for flip in [true, false] {
        let mut ops = vec![op(&isa, "prepare", &[0])];
        if flip {
            ops.push(op(&isa, "flip", &[0]));
        }
        ops.push(op(&isa, "measure", &[0]));
        let batch = shot(&qodec_plan, &mut sim).execute(ops).expect("runs");
        assert_eq!(outcomes(&batch), vec![Some(flip)]);
        sim.reset();
    }
    // Three layers, one bottom batch per shot.
    assert_eq!(sim.batches, 2);
}

#[test]
fn plans_are_shared_by_shots_on_many_threads() {
    let qodec = logical_over_repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let bottom_isa = Arc::clone(&qodec.layers()[2].instruction_set);
    let qodec_plan = qodec_plan(&qodec, &FrameDecoding).expect("qodec plans");
    let (plan, isa, bottom_isa) = (&qodec_plan, &isa, &bottom_isa);

    // `flip` has no gadget, so every shot that flips applies it as a
    // correction, through the resolver cache that all shots share.
    std::thread::scope(|scope| {
        let shots: Vec<_> = (0..8)
            .map(|t| {
                let flip = t % 2 == 1;
                scope.spawn(move || {
                    let mut ops = vec![op(isa, "prepare", &[0])];
                    if flip {
                        ops.push(op(isa, "flip", &[0]));
                    }
                    ops.push(op(isa, "measure", &[0]));
                    let batch = shot(plan, BitSim::new(bottom_isa))
                        .execute(ops)
                        .expect("runs");
                    (flip, outcomes(&batch))
                })
            })
            .collect();
        for handle in shots {
            let (flip, outcomes) = handle.join().expect("the shot's thread does not panic");
            assert_eq!(outcomes, vec![Some(flip)]);
        }
    });
}

#[test]
fn programs_need_not_know_their_size() {
    let qodec = logical_over_repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let bottom_isa = Arc::clone(&qodec.layers()[2].instruction_set);
    let qodec_plan = qodec_plan(&qodec, &FrameDecoding).expect("qodec plans");
    let program = [
        op(&isa, "prepare", &[0]),
        op(&isa, "prepare", &[1]),
        op(&isa, "flip", &[1]),
        op(&isa, "measure", &[0]),
        op(&isa, "measure", &[1]),
    ];

    // The same plan runs a two-block program on a backend that grows on demand.
    let mut sim = BitSim::new(&bottom_isa);
    let batch = shot(&qodec_plan, &mut sim)
        .execute(&program[..])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(false), Some(true)]);

    // A program that knows its size can size the backend upfront: three data
    // qubits per logical block, plus the idle gadget's two ancillas. Dense ids
    // keep the actual use within it.
    let required = read(&qodec)
        .expect("qodec reads")
        .required_resources(Resources::blocks([("lq".to_string(), 2)]))
        .expect("known block types");
    assert_eq!(
        required,
        vec![
            Resources::blocks([("lq".to_string(), 2)]),
            Resources::qubits(2),
            Resources::qubits(8),
        ]
    );
    assert!(sim.bits.len() <= required[2].qubits);

    // A fixed backend too small for the program fails the shot itself.
    let mut runtime = shot(&qodec_plan, BitSim::fixed(&bottom_isa, 3));
    let err = runtime.execute(&program[..]).expect_err("too small");
    assert_eq!(
        err,
        QodecRuntimeError::Bottom("qubit 3 is out of range".to_string())
    );
}

// ── Resolving steps ─────────────────────────────────────────────────────────

/// A one-qubit-block ISA whose actions cover every kind of step.
fn steps_isa() -> Arc<InstructionSet> {
    let bundle = r"qodec.yaml:
  name: steps
  layers:
    - instruction_set: steps.isa.yaml
---
steps.isa.yaml:
  name: steps
  blocks: {qubit: 1}
  instructions:
    - {mnemonic: X, description: '', in: [qubit], out: [qubit], action: [pauli: X_0]}
    - {mnemonic: Z, description: '', in: [qubit], out: [qubit], action: [pauli: Z_0]}
    - {mnemonic: XX, description: '', in: [qubit, qubit], out: [qubit, qubit], action: [pauli: X_0 X_1]}
    - {mnemonic: CX, description: '', in: [qubit, qubit], out: [qubit, qubit], action: [clifford: {X_0: X_0 X_1, Z_1: Z_0 Z_1}]}
    - {mnemonic: RZ, description: '', in: [qubit], out: [qubit], parameters: {theta: number}, action: [rotate: {pauli: Z_0, angle: theta}]}
    - {mnemonic: T, description: '', in: [qubit], out: [qubit], action: [rotate: {pauli: Z_0, angle: 0.7853981633974483}]}
";
    let qodec = Qodec::from_bundle_str(bundle).expect("bundle should load");
    Arc::clone(&qodec.layers()[0].instruction_set)
}

/// Lower instructions as (mnemonic, operands, arguments).
type Applied = Vec<(String, Vec<u64>, Vec<String>)>;

/// The lower instructions that apply `step` on `slots`.
fn resolved(isa: &InstructionSet, step: &Step, slots: &[(u64, usize)]) -> Option<Applied> {
    let model = spec::isa(isa).expect("ISA reads");
    let resolver = Resolver::new(&model);
    let ops = resolver.resolve(
        step,
        slots,
        |_| 0,
        &model.types,
        &mut ResolveCache::default(),
    )?;
    Some(
        ops.into_iter()
            .map(|op| {
                let parameters = &model.instructions[op.resolved.index].parameters;
                let args = parameters
                    .iter()
                    .zip(&op.resolved.args)
                    .map(|(parameter, value)| format!("{}={value:?}", parameter.name))
                    .collect();
                (
                    isa.instructions[op.resolved.index].mnemonic.clone(),
                    op.operands,
                    args,
                )
            })
            .collect(),
    )
}

fn pauli(text: &str) -> Pauli {
    Pauli::parse(text).expect("valid Pauli")
}

#[test]
fn steps_resolve_by_their_action() {
    let isa = steps_isa();
    let named = |name, targets| Step::named(name, targets).expect("known Clifford");
    let plain = |m: &str, operands: &[u64]| (m.to_string(), operands.to_vec(), Vec::new());

    assert_eq!(
        resolved(&isa, &named("cx", vec![0, 1]), &[(4, 0), (2, 0)]),
        Some(vec![plain("CX", &[4, 2])])
    );
    // A whole Pauli first, then one qubit at a time, `Y` as `X` then `Z`.
    assert_eq!(
        resolved(&isa, &Step::Pauli(pauli("X_0 X_1")), &[(0, 0), (1, 0)]),
        Some(vec![plain("XX", &[0, 1])])
    );
    assert_eq!(
        resolved(&isa, &Step::Pauli(pauli("X_0 Y_1")), &[(0, 0), (1, 0)]),
        Some(vec![plain("X", &[0]), plain("X", &[1]), plain("Z", &[1])])
    );
    // A literal angle beats a parameter; other angles fill the parameter.
    let rotate = |p, angle| Step::Rotate {
        pauli: pauli(p),
        angle,
    };
    assert_eq!(
        resolved(&isa, &rotate("Z_0", std::f64::consts::FRAC_PI_4), &[(3, 0)]),
        Some(vec![plain("T", &[3])])
    );
    assert_eq!(
        resolved(&isa, &rotate("Z_0", 0.5), &[(3, 0)]),
        Some(vec![(
            "RZ".to_string(),
            vec![3],
            vec!["theta=Float(0.5)".to_string()]
        )])
    );
    assert_eq!(resolved(&isa, &rotate("X_0", 0.5), &[(3, 0)]), None);
}

// ── Compilers ───────────────────────────────────────────────────────────────

#[test]
fn compilers_lower_like_runtimes_without_decoders() {
    let qodec = logical_over_repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let bottom_isa = Arc::clone(&qodec.layers()[2].instruction_set);
    let qodec_plan = qodec_plan(&qodec, &FrameDecoding).expect("qodec plans");
    let program = [
        op(&isa, "prepare", &[0]),
        op(&isa, "flip", &[0]),
        op(&isa, "measure", &[0]),
    ];
    let compiled = Compiler::for_model(&read(&qodec).expect("qodec reads"))
        .expect("encoders plan")
        .lower(program.iter().cloned())
        .expect("not adaptive");

    let mut sim = BitSim::new(&bottom_isa);
    shot(&qodec_plan, &mut sim)
        .execute(&program[..])
        .expect("runs");
    assert_eq!(compiled, sim.log);
}

#[test]
fn encoders_expecting_no_results_keep_nothing_for_them() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut encoder = SpecEncoderPlan::<Never>::plan(&top_layer_plan(&qodec))
        .expect("encoder plans")
        .instantiate();
    encoder.expect_no_results();
    let mut lowered = 0;
    let mut collect = from_fn(|calls: Vec<Call>| -> Result<Vec<Reply>, Fault> {
        lowered += calls.len();
        Ok(Vec::new())
    });
    let records = encoder
        .execute(
            vec![
                Lower::Call(op(&isa, "prepare_z", &[0])),
                Lower::Call(op(&isa, M, &[0])),
            ],
            &mut collect,
        )
        .expect("lowers");
    // The measurement's record comes back with its bits unknown.
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].bits, vec![None; 3]);
    // Three `R`s and three `M`s.
    assert_eq!(lowered, 6);
}

#[test]
fn compilers_reject_adaptive_programs() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let err = Compiler::for_model(&read(&qodec).expect("qodec reads"))
        .expect("encoders plan")
        .lower([op(&isa, "prepare_z", &[0]), op(&isa, "measure_reset", &[0])])
        .expect_err("`measure_reset` feeds a readout forward");
    assert_eq!(
        (err.kind, err.layer),
        (FaultKind::NoImplementation, Some(0))
    );
}

#[test]
fn frame_aliases_are_memoized() {
    // readouts[i] = [readouts[i-1], readouts[i-1]] takes 2^depth time to expand
    // without the memo.
    let depth = 64;
    let mut readouts = vec!["    - [\"circuit.readouts[0]\"]".to_string()];
    for i in 1..depth {
        readouts.push(format!(
            "    - [\"readouts[{}]\", \"readouts[{}]\"]",
            i - 1,
            i - 1
        ));
    }
    let readouts = readouts.join("\n");
    let observes = vec!["Z_0"; depth].join(", ");
    let last = depth - 1;
    let bundle = format!(
        r#"qodec.yaml:
  name: chain
  layers:
    - instruction_set: chain.isa.yaml
      codes: {{repetition3: repetition3.code.yaml}}
      gadgets: {{chain: chain.gadget.yaml}}
    - instruction_set: physical.isa.yaml
---
chain.isa.yaml:
  name: chain
  blocks: {{repetition3: 1}}
  instructions:
    - {{mnemonic: chain, description: '', in: [repetition3], out: [repetition3], action: [observe: [{observes}]]}}
---
chain.gadget.yaml:
  circuit: {{source: [{{MZ: [0]}}]}}
  readouts:
{readouts}
  frames:
    "out[0].x[0]": ["readouts[{last}]"]
{REPETITION3_ARTIFACTS}"#
    );
    let qodec = Qodec::from_bundle_str(&bundle).expect("bundle should load");
    let layer = top_layer_plan(&qodec);
    let gadget = &layer.gadgets[layer.gadget_of[0].expect("chain has a gadget").0];
    // An even number of copies of readouts[0] cancels.
    let key = (0, Basis::X, 0);
    assert_eq!(gadget.frames[&key], Parity::default());
}
