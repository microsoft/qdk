// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Whole qodecs, read and planned, run down to `qdk_simulators`' action
//! simulators through the `ActionTranslator`.

use qdk_simulators::action_simulators::{FullStateActionSimulator, StabilizerActionSimulator};
use qodec_runtime_recipes::{
    ActionOp, ActionTranslator, Backend, BasicReadout, Call, FrameDecoding, Isa, OwnedParam,
    QodecPlan, QodecRuntimeError, Stack, runtime,
};
use smallvec::SmallVec;
use std::path::PathBuf;

type Engine = Box<dyn Backend<Vec<ActionOp>, Result<Vec<bool>, String>>>;

/// Both action simulators, seeded with `seed`, for `qubits` qubits at most.
fn engines(qubits: usize, seed: u32) -> [(&'static str, Engine); 2] {
    [
        (
            "stabilizer",
            Box::new(StabilizerActionSimulator::new(seed.into())),
        ),
        (
            "full state",
            Box::new(FullStateActionSimulator::new(qubits, seed)),
        ),
    ]
}

fn example(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples")
        .join(path)
}

fn call(isa: &Isa, mnemonic: &str, operands: &[u64]) -> Call {
    let instruction_id = isa
        .instructions
        .iter()
        .position(|i| i.mnemonic == mnemonic)
        .unwrap_or_else(|| panic!("no `{mnemonic}`"));
    let parameters: SmallVec<[OwnedParam; 2]> =
        operands.iter().map(|&b| OwnedParam::Block(b)).collect();
    Call {
        instruction_id,
        parameters,
    }
}

/// Runs `program`, top-ISA calls by mnemonic and operands, on the qodec at
/// `path` with `engine` at the bottom.
fn run(
    path: &str,
    program: &[(&str, &[u64])],
    engine: Engine,
) -> Result<Vec<BasicReadout>, QodecRuntimeError<String>> {
    let qodec = qodec::Qodec::load(example(path)).expect("loads");
    let model = crate::read(&qodec).expect("reads");
    let plan = QodecPlan::new(&model, &FrameDecoding).expect("plans");
    let calls: Vec<Call> = program
        .iter()
        .map(|(m, ops)| call(plan.top(), m, ops))
        .collect();
    let bottom = Stack::new(ActionTranslator::new(plan.bottom()), engine);
    runtime(&plan, bottom).execute(calls)
}

fn outcomes(results: &[BasicReadout]) -> Vec<Option<bool>> {
    results.iter().flat_map(|r| r.outcomes.clone()).collect()
}

#[test]
fn a_steane_memory_keeps_its_state_on_both_simulators() {
    let program: &[(&str, &[u64])] = &[
        ("prepare_z", &[0]),
        ("idle", &[0]),
        ("idle", &[0]),
        ("measure_z", &[0]),
    ];
    for seed in 0..3 {
        for (name, engine) in engines(14, seed) {
            let results = run("steane/steane.qodec.yaml", program, engine)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(outcomes(&results), [Some(false)], "{name}, seed {seed}");
            assert!(
                results.iter().all(|r| r.status == Default::default()),
                "{name}: {results:?}"
            );
        }
    }
}

#[test]
fn steane_gates_act_on_the_logical_qubit() {
    let h: &[(&str, &[u64])] = &[("prepare_z", &[0]), ("h", &[0]), ("measure_x", &[0])];
    for (name, engine) in engines(14, 1) {
        let results =
            run("steane/steane.qodec.yaml", h, engine).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(outcomes(&results), [Some(false)], "{name}");
    }
    // Two blocks need more qubits than a state vector should hold here.
    let bell: &[(&str, &[u64])] = &[
        ("prepare_x", &[0]),
        ("prepare_z", &[1]),
        ("cnot", &[0, 1]),
        ("measure_z", &[0]),
        ("measure_z", &[1]),
    ];
    let mut seen = [false; 2];
    for seed in 0..10 {
        let results = run(
            "steane/steane.qodec.yaml",
            bell,
            Box::new(StabilizerActionSimulator::new(seed)),
        )
        .expect("runs");
        let o = outcomes(&results);
        assert_eq!(o[0], o[1]);
        seen[usize::from(o[0] == Some(true))] = true;
    }
    assert_eq!(seen, [true, true]);
}

#[test]
fn every_example_memory_keeps_its_state_on_a_stabilizer_simulator() {
    let program: &[(&str, &[u64])] = &[
        ("prepare_z", &[0]),
        ("idle", &[0]),
        ("idle", &[0]),
        ("measure_z", &[0]),
    ];
    for path in [
        "bacon-shor/bacon-shor.qodec.yaml",
        "five_qubit/five_qubit.qodec.yaml",
        "reed-muller-15/reed-muller-15.qodec.yaml",
        "repetition3/repetition3.qodec.yaml",
        "steane/steane.qodec.yaml",
        "surface/surface.qodec.yaml",
    ] {
        for seed in 0..3 {
            let results = run(
                path,
                program,
                Box::new(StabilizerActionSimulator::new(seed)),
            )
            .unwrap_or_else(|e| panic!("{path}: {e}"));
            let o = outcomes(&results);
            assert!(
                !o.is_empty() && o.iter().all(|b| *b == Some(false)),
                "{path}, seed {seed}: {o:?}"
            );
            assert!(
                results.iter().all(|r| r.status == Default::default()),
                "{path}: {results:?}"
            );
        }
    }
}
