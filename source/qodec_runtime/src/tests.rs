// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    Backend, BackendInstruction, BasicReadout, BatchResult, FrameDecoder, FrameDecoderPlan,
    LayerPlan, LayerRuntime, OwnedParam, QodecError, QodecPlan, QodecRuntimeError, Readout,
    Resources, Status,
    parity::{BinarySystem, Inconsistent, Parity, Var},
    pauli::{Clifford, DensePauli, Pauli},
    spec::{CompiledSelect, TypeTable, accepts, bind_operands, compile_select},
};
use qodec::{BlockOperand, Instruction, InstructionSet, Qodec, SelectPattern};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::{rc::Rc, sync::Arc};

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
    // A per-pattern scan would answer None here.
    let select = compiled(&["f0"], &[&[("f0", 0)], &[("f0", 1)]]);
    assert_eq!(accepts(&select, &[None]), Some(true));
}

#[test]
fn accepts_is_three_valued() {
    let select = compiled(&["f0", "f1"], &[&[("f0", 1), ("flags[1]", 0)]]);
    assert_eq!(accepts(&select, &[Some(true), Some(false)]), Some(true));
    assert_eq!(accepts(&select, &[Some(false), None]), Some(false));
    assert_eq!(accepts(&select, &[Some(true), None]), None);
    assert_eq!(accepts(&Vec::new(), &[None]), Some(true));
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
    TypeTable::new(&InstructionSet {
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
    let (ins, outs) = bind_operands(&types, &i, &[7u64, 8], &FxHashMap::default()).expect("binds");
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
    // Without types, the split between the two variadics is ambiguous.
    assert!(bind_operands(&types, &i, &targets, &FxHashMap::default()).is_err());
    let known: FxHashMap<u64, &str> = [(1, "a"), (2, "a"), (3, "b")].into_iter().collect();
    let (ins, _) = bind_operands(&types, &i, &targets, &known).expect("unique split");
    assert_eq!(
        ins.iter()
            .map(|b| b.block_type.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "a", "b"]
    );
}

#[test]
fn bind_operands_requires_distinct_targets() {
    let types = types(&[("a", 1)]);
    let i = instruction(&[("a", false), ("a", false)], &[]);
    assert!(bind_operands(&types, &i, &[1u64, 1], &FxHashMap::default()).is_err());
}

#[test]
fn named_cliffords_compose() {
    let named = |name, arity| Clifford::named(name, arity).expect("known Clifford");
    let (h, s, s_adj) = (named("h", 1), named("s", 1), named("s_adj", 1));
    assert_eq!(h.then(&h), Clifford::identity(1));
    assert_eq!(s.then(&s_adj), Clifford::identity(1));
    assert_eq!(s.then(&s), named("z", 1));
    // Z·X = iY, so these are equal only up to a global phase.
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

/// Repetition-3 artifacts shared by the bundles below. The physical ISA adds a
/// non-destructive `MZ` and a classically controlled `XIF`.
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

/// A logical layer with a trivial one-qubit code on top of repetition3.
fn logical_over_repetition3() -> Qodec {
    let bundle = format!(
        r#"qodec.yaml:
  name: logical
  layers:
    - instruction_set: logical.isa.yaml
      codes: {{lq: trivial.code.yaml}}
      gadgets:
        prepare: prepare.gadget.yaml
        measure: measure.gadget.yaml
{REPETITION3_LAYER}    - instruction_set: physical.isa.yaml
---
logical.isa.yaml:
  name: logical
  blocks: {{lq: 1}}
  instructions:
    - {{mnemonic: prepare, description: '', out: [lq], action: [stabilize: Z_0]}}
    - {{mnemonic: measure, description: '', in: [lq], action: [observe: Z_0]}}
    - {{mnemonic: flip, description: '', in: [lq], out: [lq], action: [pauli: X_0]}}
---
trivial.code.yaml:
  name: trivial
  stabilizers: []
  x: [X_0]
  z: [Z_0]
---
prepare.gadget.yaml:
  circuit: {{source: [{{prepare_z: [0]}}]}}
---
measure.gadget.yaml:
  circuit: {{source: [{{__quantum__qis__m__body: [0]}}]}}
  readouts: [["circuit.readouts[0]", "in[0].z[0]"]]
{REPETITION3_ARTIFACTS}"#
    );
    Qodec::from_bundle_str(&bundle).expect("bundle should load")
}

/// A classical simulator of computational-basis states, enough for the
/// repetition code's Z-basis circuits. It grows on demand, or models a fixed
/// device that fails ids it cannot hold.
#[derive(Default)]
struct BitSim {
    mnemonics: Vec<String>,
    bits: Vec<bool>,
    fixed_size: Option<usize>,
    batches: usize,
    fail: bool,
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
}

impl Backend<usize, OwnedParam, BasicReadout> for BitSim {
    type Error = String;

    fn execute_batch(
        &mut self,
        instructions: &[BackendInstruction<usize, OwnedParam>],
    ) -> Result<BatchResult<BasicReadout>, String> {
        self.batches += 1;
        if self.fail {
            return Err("boom".to_string());
        }
        let mut results = SmallVec::new();
        for ins in instructions {
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
                "M" | "MZ" => results.push(BasicReadout::new(
                    &[Some(self.bits[q(0)])],
                    &[],
                    Status::default(),
                )),
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
        Ok(BatchResult { results })
    }
}

type TestLayer = LayerRuntime<usize, OwnedParam, BasicReadout, BitSim>;

/// One shot of the top layer of `qodec`, directly over a fresh simulator.
fn single_layer(qodec: &Qodec) -> TestLayer {
    let layers = qodec.layers();
    let plan =
        Rc::new(LayerPlan::new(&layers[0], &layers[1].instruction_set, true).expect("plan builds"));
    let decoder_plan = FrameDecoderPlan::new(&plan).expect("decoder plans");
    LayerRuntime::new(plan, &decoder_plan, BitSim::new(&layers[1].instruction_set))
}

fn op(
    isa: &Arc<InstructionSet>,
    mnemonic: &str,
    blocks: &[u64],
) -> BackendInstruction<usize, OwnedParam> {
    BackendInstruction {
        instruction_id: isa
            .instructions
            .iter()
            .position(|i| i.mnemonic == mnemonic)
            .unwrap_or_else(|| panic!("no instruction `{mnemonic}`")),
        parameters: blocks.iter().map(|&b| OwnedParam::Block(b)).collect(),
    }
}

fn outcomes(batch: &BatchResult<BasicReadout>) -> Vec<Option<bool>> {
    batch
        .results
        .iter()
        .flat_map(|r| r.outcomes.clone())
        .collect()
}

fn one_block() -> Resources {
    Resources::blocks([("repetition3".to_string(), 1)])
}

const M: &str = "__quantum__qis__m__body";
const X: &str = "__quantum__qis__x__body";

#[test]
fn required_resources_include_scratch() {
    let qodec = repetition3();
    let layers = qodec.layers();
    let plan = LayerPlan::new(&layers[0], &layers[1].instruction_set, true).expect("plan");
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
        .execute_batch(&[op(&isa, "prepare_z", &[0]), op(&isa, M, &[0])])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(false)]);

    let batch = layer
        .execute_batch(&[
            op(&isa, "prepare_z", &[0]),
            op(&isa, X, &[0]),
            op(&isa, "idle", &[0]),
            op(&isa, "__quantum__qis__z__body", &[0]),
            op(&isa, "idle", &[0]),
            op(&isa, M, &[0]),
        ])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(true)]);
    assert_eq!(batch.results[0].status, Status::default());
}

#[test]
fn logical_paulis_apply_the_code_operator() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    let batch = layer
        .execute_batch(&[
            op(&isa, "prepare_z", &[0]),
            op(&isa, "logical_x", &[0]),
            op(&isa, "idle", &[0]),
            op(&isa, M, &[0]),
        ])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(true)]);
}

#[test]
fn mresetz_reports_then_resets() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    let batch = layer
        .execute_batch(&[
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
        .execute_batch(&[
            op(&isa, "prepare_z", &[0]),
            op(&isa, "idle", &[0]),
            op(&isa, M, &[0]),
        ])
        .expect("runs");
    assert_eq!(layer.lower().batches, 1);
}

#[test]
fn bit_parameters_thread_earlier_readouts() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    let batch = layer
        .execute_batch(&[
            op(&isa, "prepare_z", &[0]),
            op(&isa, X, &[0]),
            op(&isa, "measure_reset", &[0]),
            op(&isa, M, &[0]),
        ])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(true), Some(false)]);
    // Forcing `circuit.readouts[0]` flushes once mid-batch.
    assert_eq!(layer.lower().batches, 2);
}

#[test]
fn contradicting_syndromes_are_inconsistent() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    layer
        .execute_batch(&[op(&isa, "prepare_z", &[0]), op(&isa, "idle", &[0])])
        .expect("runs");
    // A bit flip on the middle data qubit between two syndrome rounds.
    let data = layer.live_block(0).expect("live").support[1];
    let data = usize::try_from(data).expect("small id");
    layer.lower_mut().bits[data] ^= true;
    let batch = layer
        .execute_batch(&[op(&isa, "idle", &[0]), op(&isa, M, &[0])])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![None]);
    assert!(batch.results[0].status.inconsistent);
}

#[test]
fn lower_ids_are_allocated_on_demand_and_reused() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    let support = |layer: &TestLayer, label| layer.live_block(label).expect("live").support.clone();
    layer
        .execute_batch(&[op(&isa, "prepare_z", &[0]), op(&isa, "prepare_z", &[1])])
        .expect("runs");
    // Fresh ids, densely from 0.
    assert_eq!(support(&layer, 0), vec![0, 1, 2]);
    assert_eq!(support(&layer, 1), vec![3, 4, 5]);

    // Released ids are reused, lowest first, before any fresh one.
    let batch = layer
        .execute_batch(&[
            op(&isa, "discard", &[0]),
            op(&isa, "prepare_z", &[2]),
            op(&isa, X, &[2]),
            op(&isa, M, &[2]),
        ])
        .expect("runs after release");
    assert_eq!(outcomes(&batch), vec![Some(true)]);
    assert_eq!(layer.lower().bits.len(), 6);
}

#[test]
fn invalid_instructions_are_rejected_without_poisoning() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    let err = layer
        .execute_batch(&[op(&isa, "idle", &[0])])
        .expect_err("block 0 is not live");
    assert_eq!(err, QodecError::InvalidInstruction);
    let err = layer
        .execute_batch(&[BackendInstruction {
            instruction_id: 99,
            parameters: SmallVec::new(),
        }])
        .expect_err("unknown id");
    assert_eq!(err, QodecError::InvalidInstruction);
    assert!(!layer.is_poisoned());
}

#[test]
fn lower_errors_poison_the_shot() {
    let qodec = repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let mut layer = single_layer(&qodec);
    layer.lower_mut().fail = true;
    let err = layer
        .execute_batch(&[op(&isa, "prepare_z", &[0])])
        .expect_err("lower fails");
    assert_eq!(err, QodecError::Lower("boom".to_string()));
    assert!(layer.is_poisoned());
    layer.lower_mut().fail = false;
    let err = layer
        .execute_batch(&[op(&isa, "prepare_z", &[0])])
        .expect_err("poisoned");
    assert_eq!(err, QodecError::Poisoned);

    // The shot is lost; the next one starts clean.
    let mut layer = single_layer(&qodec);
    let batch = layer
        .execute_batch(&[op(&isa, "prepare_z", &[0]), op(&isa, M, &[0])])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(false)]);
}

#[test]
fn required_resources_reject_unknown_block_types() {
    let qodec = logical_over_repetition3();
    let qodec_plan = QodecPlan::<BasicReadout>::new(&qodec, &FrameDecoder).expect("qodec plans");
    let unknown = Resources::blocks([("nope".to_string(), 1)]);
    assert!(qodec_plan.required_resources(unknown).is_err());
}

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
    let qodec_plan = QodecPlan::new(&qodec, &FrameDecoder).expect("qodec plans");
    assert!(qodec_plan.layer_plans().is_empty());
    let top = Resources::qubits(2);
    assert_eq!(qodec_plan.required_resources(top.clone()), Ok(vec![top]));

    // Instructions reach the backend unchanged, with the caller's block ids.
    let mut runtime = qodec_plan.instantiate(BitSim::new(&isa));
    let batch = runtime
        .execute_batch(&[
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
    let qodec_plan = QodecPlan::new(&qodec, &FrameDecoder).expect("qodec plans");

    // A simulator built for the shot, owned by the runtime. `flip` has no gadget:
    // the top layer applies the logical X as a correction, which the middle
    // layer runs through its X gadget.
    let mut runtime = qodec_plan.instantiate(BitSim::new(&bottom_isa));
    let batch = runtime
        .execute_batch(&[
            op(&isa, "prepare", &[0]),
            op(&isa, "flip", &[0]),
            op(&isa, "measure", &[0]),
        ])
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(true)]);
    let err = runtime
        .execute_batch(&[op(&isa, "measure", &[0])])
        .expect_err("block 0 was consumed");
    assert_eq!(
        err,
        QodecRuntimeError::Layer {
            layer: 0,
            error: QodecError::InvalidInstruction
        }
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
        let batch = qodec_plan
            .instantiate(&mut sim)
            .execute_batch(&ops)
            .expect("runs");
        assert_eq!(outcomes(&batch), vec![Some(flip)]);
        sim.reset();
    }
    assert_eq!(sim.batches, 2);
}

#[test]
fn programs_need_not_know_their_size() {
    let qodec = logical_over_repetition3();
    let isa = Arc::clone(&qodec.layers()[0].instruction_set);
    let bottom_isa = Arc::clone(&qodec.layers()[2].instruction_set);
    let qodec_plan = QodecPlan::new(&qodec, &FrameDecoder).expect("qodec plans");
    let program = [
        op(&isa, "prepare", &[0]),
        op(&isa, "prepare", &[1]),
        op(&isa, "flip", &[1]),
        op(&isa, "measure", &[0]),
        op(&isa, "measure", &[1]),
    ];

    // The same plan runs a two-block program on a backend that grows on demand.
    let mut sim = BitSim::new(&bottom_isa);
    let batch = qodec_plan
        .instantiate(&mut sim)
        .execute_batch(&program)
        .expect("runs");
    assert_eq!(outcomes(&batch), vec![Some(false), Some(true)]);

    // A program that knows its size can size the backend upfront: three data
    // qubits per logical block, plus the idle gadget's two ancillas. Dense ids
    // keep the actual use within it.
    let required = qodec_plan
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
    let mut runtime = qodec_plan.instantiate(BitSim::fixed(&bottom_isa, 3));
    let err = runtime.execute_batch(&program).expect_err("too small");
    assert_eq!(
        err,
        QodecRuntimeError::Backend("qubit 3 is out of range".to_string())
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
    let layers = qodec.layers();
    let plan = LayerPlan::new(&layers[0], &layers[1].instruction_set, true).expect("plan builds");
    let super::plan::InstructionPlan::Gadget(gp) = &plan.instructions[0] else {
        panic!("chain has a gadget");
    };
    // An even number of copies of readouts[0] cancels.
    let key = (0, super::parity::Basis::X, 0);
    assert_eq!(gp.frames[&key], Parity::default());
}
