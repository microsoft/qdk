// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_circuit_store::{
    AngleOp, Expr, Literal, PackedOperation, ParamValue, Qubit, StandardGate, TimeUnit,
};

use crate::{AngleExpression, BodyKind, Circuit, CircuitError, RegisterSpec, combine};

fn append_x(circuit: &mut Circuit) {
    circuit
        .append(
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("append");
}

#[test]
fn full_document_retains_registers_and_non_finite_phase() {
    let mut circuit = Circuit::from_registers(
        [
            RegisterSpec::Quantum {
                name: "q".into(),
                size: 2,
            },
            RegisterSpec::Classical {
                name: "c".into(),
                size: 1,
            },
        ],
        1,
    )
    .expect("registers");
    circuit.store_mut().set_global_phase(f64::INFINITY);
    append_x(&mut circuit);
    let bytes = circuit.to_bytes().expect("serialize");
    let restored = Circuit::from_bytes(&bytes).expect("restore");
    assert_eq!(bytes, restored.to_bytes().expect("reserialize"));
    assert_eq!(restored.quantum_registers(), circuit.quantum_registers());
    assert_eq!(
        restored.classical_registers(),
        circuit.classical_registers()
    );
    assert_eq!(restored.store().clbit_count(), 2);
    assert_eq!(
        restored.store().global_phase().to_bits(),
        f64::INFINITY.to_bits()
    );
    let text = circuit.to_text().expect("text");
    assert_eq!(
        Circuit::from_text(&text)
            .expect("restore text")
            .to_bytes()
            .expect("encode"),
        bytes
    );
}

#[test]
fn document_text_keeps_register_names_with_quotes_and_nul() {
    let circuit = Circuit::from_registers(
        [RegisterSpec::Quantum {
            name: "q\"(\0)".into(),
            size: 2,
        }],
        0,
    )
    .expect("registers");
    let restored = Circuit::from_text(&circuit.to_text().expect("text")).expect("restore");
    assert_eq!(restored.quantum_registers(), circuit.quantum_registers());
    assert_eq!(
        restored.to_bytes().expect("bytes"),
        circuit.to_bytes().expect("bytes")
    );
}

#[test]
fn document_import_does_not_accept_a_bare_store() {
    let store = qsc_circuit_store::CircuitStore::new(1, 0);
    assert!(Circuit::from_bytes(&store.to_bytes().expect("bytes")).is_err());
}

#[test]
fn document_text_rejects_an_unknown_field() {
    let circuit = Circuit::new(1, 0);
    let text = circuit.to_text().expect("text");
    let changed = text.replace("(quantumRegisters", "(unknown 1) (quantumRegisters");
    assert_ne!(changed, text);
    assert!(
        matches!(Circuit::from_text(&changed), Err(CircuitError::Encoding(error)) if error.to_string().contains("unknown field"))
    );
}

#[test]
fn document_text_rejects_an_overlapping_register() {
    let circuit = Circuit::from_registers(
        [
            RegisterSpec::Quantum {
                name: "a".into(),
                size: 2,
            },
            RegisterSpec::Quantum {
                name: "b".into(),
                size: 2,
            },
        ],
        0,
    )
    .expect("registers");
    let text = circuit.to_text().expect("text");
    let changed = text.replace("(start 2)", "(start 1)");
    assert_ne!(changed, text);
    assert!(
        matches!(Circuit::from_text(&changed), Err(CircuitError::Invalid(message)) if message.contains("overlaps"))
    );
}

#[test]
fn angle_representation_survives_interning_and_full_binding() {
    let mut circuit = Circuit::new(1, 0);
    let theta = circuit.store_mut().declare_parameter("theta");
    let expression = combine(
        &AngleExpression::parameter(theta, "theta".into()),
        AngleOp::Mul,
        &AngleExpression::literal(2.0),
    );
    let angle = expression.intern(circuit.store_mut());
    circuit
        .append(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Expr(angle)])),
        )
        .expect("rotation");
    assert_eq!(
        AngleExpression::extract(circuit.store(), angle).text(),
        "(theta * 2)"
    );
    let bound = circuit.bind_parameters(&[(theta, 0.25)]).expect("bind");
    assert_eq!(
        bound
            .store()
            .instruction(0)
            .expect("instruction")
            .params
            .as_deref(),
        Some([ParamValue::Literal(0.5)].as_slice())
    );
    assert_eq!(circuit.store().referenced_parameters(), vec![theta]);
}

#[test]
fn cancelled_body_does_not_publish_or_reenter() {
    let mut circuit = Circuit::new(1, 0);
    let mut body = circuit.body(BodyKind::Box { label: None });
    body.enter(&mut circuit).expect("enter");
    append_x(&mut circuit);
    body.cancel(&mut circuit).expect("cancel");
    assert!(circuit.store().is_empty());
    assert!(body.enter(&mut circuit).is_err());
    assert!(body.finish(&mut circuit).is_err());
    circuit.require_closed("save").expect("closed");
}

#[test]
fn out_of_order_body_close_preserves_the_active_child() {
    let mut circuit = Circuit::new(1, 0);
    let mut outer = circuit.body(BodyKind::Box { label: None });
    outer.enter(&mut circuit).expect("outer");
    let mut inner = circuit.body(BodyKind::Moment);
    inner.enter(&mut circuit).expect("inner");
    assert!(outer.finish(&mut circuit).is_err());
    append_x(&mut circuit);
    inner.finish(&mut circuit).expect("finish inner");
    outer.finish(&mut circuit).expect("finish outer");
    assert_eq!(circuit.store().len(), 1);
    assert_eq!(
        circuit
            .store()
            .block(inner.block())
            .expect("inner block")
            .instructions
            .len(),
        1
    );
}

#[test]
fn drawing_uses_shared_gate_vocabulary_and_wire_order() {
    let mut circuit = Circuit::new(2, 0);
    circuit
        .append(
            PackedOperation::Standard(StandardGate::Cx),
            &[Qubit(0), Qubit(1)],
            &[],
            None,
        )
        .expect("cx");
    append_x(&mut circuit);
    let diagram = circuit.build_diagram();
    assert_eq!(diagram.component_grid.len(), 2);
    let qsc_circuit::Operation::Unitary(cx) = &diagram.component_grid[0].components[0] else {
        panic!("controlled unitary");
    };
    assert_eq!(cx.gate, "X");
    assert_eq!(cx.controls.len(), 1);
    assert_eq!(cx.targets.len(), 1);
    assert!(!diagram.display_no_locations().to_string().is_empty());
}

#[test]
fn graph_crossing_preserves_the_unsupported_loop_diagnostic() {
    let mut circuit = Circuit::new(1, 0);
    let condition = circuit
        .store_mut()
        .push_expr(Expr::Literal(Literal::Bool(true)));
    let mut body = circuit.body(BodyKind::While { condition });
    body.enter(&mut circuit).expect("enter");
    circuit
        .append(
            PackedOperation::Delay {
                duration: 1.0,
                unit: TimeUnit::Dt,
            },
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("delay");
    body.finish(&mut circuit).expect("finish");
    assert!(matches!(
        circuit.to_graph(),
        Err(CircuitError::Graph(
            qsc_codegen::circuit_qhdg::Error::Unsupported { .. }
        ))
    ));
}
