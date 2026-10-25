// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! State queries (`Evaluate`): the prototype's MPS14–MPS17 and MPS19.

use std::rc::Rc;

use tensornet::{MpsEngine, reference::ExactReference};

use super::{
    Bytecode, OPID_Z, TestDoubleEngine,
    cases::{expectation, expectations, expectations_are_the_hand_computed_ones, scrambled},
    pauli,
};
use crate::{
    Evaluate, MpsBatchSimulator, MpsCost, MpsStack, ProgramError, QueryError, StateQuery,
    StateQueryValue, TensorNetShots, mps_capacity,
};

#[test]
fn mps14_mps19_state_queries_give_the_hand_computed_expectations() {
    expectations_are_the_hand_computed_ones(&MpsStack::new(ExactReference::new()), 1e-12);
}

#[test]
fn mps15_expectations_agree_with_rotated_measurements_and_p1() {
    // ⟨X_q⟩ is ⟨Z_q⟩ after H, ⟨Y_q⟩ is ⟨Z_q⟩ after S† (S, Z) and H; and
    // ⟨Z_q⟩ = 1 − 2·P(One) from the state's own p1: three code paths.
    let stack = MpsStack::new(ExactReference::new());
    let to_z = |program: Bytecode, q: u64, p: char| match p {
        'X' => program.h(q),
        'Y' => program.s(q).gate(OPID_Z, q, 0, 0.0).h(q),
        _ => program,
    };
    for seed in 0..8 {
        let circuit = scrambled(4, seed);
        let program = circuit.clone().build();
        let engine = ExactReference::new();
        let mut state = engine
            .prepare(&mps_capacity(4, 64).expect("a chain"))
            .expect("prepares");
        let plan = crate::program::plan(&program).expect("a Base program");
        for operation in plan.operations() {
            crate::lowering::apply_operation(&engine, &mut state, operation).expect("applies");
        }
        for q in 0..4 {
            let z = expectation(&stack, &program, "Z", &[q]);
            let p1 = engine.probability_of_one(&state, q).expect("p1");
            assert!(
                (z - (1.0 - 2.0 * p1)).abs() < 1e-12,
                "seed {seed} qubit {q}"
            );
            for p in ['X', 'Y'] {
                let direct = expectation(&stack, &program, &p.to_string(), &[q]);
                let rotated = to_z(circuit.clone(), q as u64, p).build();
                let via_z = expectation(&stack, &rotated, "Z", &[q]);
                assert!(
                    (direct - via_z).abs() < 1e-12,
                    "seed {seed}: ⟨{p}{q}⟩ {direct} against {via_z}"
                );
            }
        }
        // ⟨X₀Y₂⟩ is ⟨Z₀Z₂⟩ after both rotations.
        let direct = expectation(&stack, &program, "XY", &[0, 2]);
        let rotated = to_z(to_z(circuit.clone(), 0, 'X'), 2, 'Y').build();
        let via_z = expectation(&stack, &rotated, "ZZ", &[0, 2]);
        assert!(
            (direct - via_z).abs() < 1e-12,
            "seed {seed}: {direct} against {via_z}"
        );
    }
}

#[test]
fn mps16_cost_is_the_chain_of_psi_and_one_that_does_not_fit_fails() {
    let cost = |stack: &MpsStack<ExactReference>, program: Bytecode| {
        stack.evaluate(&program.build(), &[StateQuery::Cost])
    };
    let chi =
        |max_bond| MpsStack::new(ExactReference::new()).with_max_bond_dimension(Some(max_bond));
    // Bell: sites [2, 2] and [2, 2]: 8 elements of 16 bytes.
    assert_eq!(
        cost(&chi(64), Bytecode::new(2).h(0).cx(0, 1)),
        Ok(vec![StateQueryValue::Cost(MpsCost {
            max_bond_dimension: 2,
            state_bytes: 128
        })])
    );
    // A product of 4: sites [2, 1], [1, 2, 1], [1, 2, 1], [1, 2].
    assert_eq!(
        cost(&chi(64), Bytecode::new(4).h(0).h(3)),
        Ok(vec![StateQueryValue::Cost(MpsCost {
            max_bond_dimension: 1,
            state_bytes: 128
        })])
    );
    // Two pairs across the middle: bond 4, beyond a χ of 2.
    let crossing = || Bytecode::new(4).h(0).cx(0, 2).h(1).cx(1, 3);
    assert_eq!(
        cost(&chi(4), crossing()),
        Ok(vec![StateQueryValue::Cost(MpsCost {
            max_bond_dimension: 4,
            state_bytes: 2 * (2 * 2) * 16 + 2 * (2 * 4 * 2) * 16,
        })])
    );
    match cost(&chi(2), crossing()) {
        Err(error @ QueryError::Failed(_)) => {
            assert!(error.to_string().contains("bond 4"), "{error}");
            assert!(error.is_environment_error());
        }
        other => panic!("{other:?}"),
    }
    // A ψ the engine cannot hold: the reference holds 20 sites.
    let wide = Bytecode::new(21).h(0).build();
    match chi(2).evaluate(&wide, &[StateQuery::Cost]) {
        Err(error @ QueryError::Unavailable(_)) => assert!(error.is_environment_error()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn mps17_a_query_is_checked_before_psi_and_ignores_the_terminal_measurements() {
    let stack = MpsStack::new(TestDoubleEngine::counting());
    let prepared = || stack.engine().prepared();
    let xx = || StateQuery::Expectation(pauli(1.0, "XX", &[0, 1]));
    let bell = || Bytecode::new(2).h(0).cx(0, 1);
    assert_eq!(
        stack.evaluate(&bell().build(), &[]),
        Err(QueryError::NoQueries)
    );
    assert_eq!(
        stack.evaluate(
            &bell().build(),
            &[xx(), StateQuery::Expectation(pauli(1.0, "Z", &[2]))]
        ),
        Err(QueryError::Qubit {
            qubit: 2,
            qubits: 2
        })
    );
    // A measurement before the end, or a reset, is not a Base program.
    assert_eq!(
        stack.evaluate(&Bytecode::new(2).h(0).mz(0, 0).cx(0, 1).build(), &[xx()]),
        Err(QueryError::Program(ProgramError::RegionCount { found: 2 }))
    );
    assert_eq!(
        stack.evaluate(&bell().reset(1).build(), &[xx()]),
        Err(QueryError::Program(ProgramError::Reset { qubit: 1 }))
    );
    assert_eq!(prepared(), 0, "rejected on the host: no state prepared");
    // The terminal measurements are ignored: ψ is the state before them.
    for program in [bell(), bell().mz(0, 0).mz(1, 1).record(0).record(1)] {
        let before = prepared();
        assert!((expectation(&stack, &program.build(), "XX", &[0, 1]) - 1.0).abs() < 1e-12);
        assert_eq!(prepared() - before, 1);
    }
}

#[test]
fn queries_on_a_strategy_use_its_capacity() {
    let engine = Rc::new(ExactReference::new());
    let program = Bytecode::new(3).h(0).cx(0, 1).cx(1, 2).build();
    let observables = [pauli(1.0, "XXX", &[0, 1, 2])];
    let batch = MpsBatchSimulator::new(Rc::clone(&engine), 3, 2).expect("a chain");
    let shots = TensorNetShots::new(Rc::clone(&engine), 4, 2).expect("a chain");
    for values in [
        expectations(&batch, &program, &observables),
        expectations(&shots, &program, &observables),
    ] {
        assert!((values[0] - 1.0).norm() < 1e-12);
    }
    // A program wider than the capacity is refused before ψ.
    let narrow = MpsBatchSimulator::new(engine, 2, 2).expect("a chain");
    assert_eq!(
        narrow.evaluate(&program, &[StateQuery::Cost]),
        Err(QueryError::Sites {
            program: 3,
            sites: 2
        })
    );
}
