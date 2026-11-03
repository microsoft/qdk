// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    BinaryOp, CircuitStore, Clbit, ControlFlow, CustomOperation, Expr, IfElse, Literal,
    PackedOperation, ParamValue, Qubit, StandardGate, StoreError, TimeUnit, UnitaryOperation,
    VarType, WhileLoop, gate::STANDARD_GATE_COUNT,
};

fn store_with(qubits: u32, clbits: u32) -> CircuitStore {
    CircuitStore::new(qubits, clbits)
}

fn push_standard(store: &mut CircuitStore, gate: StandardGate, qubits: &[Qubit]) {
    store
        .push(PackedOperation::Standard(gate), qubits, &[], None)
        .expect("standard gate should append");
}

#[test]
fn standard_gate_tables_cover_every_variant() {
    let gates = StandardGate::all();
    assert_eq!(gates.len(), STANDARD_GATE_COUNT);
    for (index, gate) in gates.iter().enumerate() {
        assert_eq!(*gate as usize, index, "discriminant order must match all()");
        assert!(!gate.name().is_empty());
        assert!(gate.qubit_arity() >= 1);
    }
}

#[test]
fn instruction_ids_are_distinct_except_for_documented_aliases() {
    let mut seen = Vec::new();
    for gate in StandardGate::all() {
        let id = gate.instruction_id();
        assert!(
            !seen.contains(&id),
            "{} reuses instruction id {id}",
            gate.name()
        );
        seen.push(id);
    }
}

#[test]
fn standard_gates_are_stored_inline() {
    for gate in StandardGate::all() {
        let operation = PackedOperation::Standard(gate);
        assert!(operation.is_inline(), "{} should be inline", gate.name());
    }
}

#[test]
fn custom_and_unitary_operations_are_not_inline() {
    let custom = PackedOperation::Custom(Box::new(CustomOperation {
        name: "my_op".to_string(),
        qubit_arity: 2,
        param_arity: 0,
    }));
    let unitary = PackedOperation::Unitary(Box::new(UnitaryOperation {
        qubit_arity: 1,
        matrix: vec![(1.0, 0.0), (0.0, 0.0), (0.0, 0.0), (1.0, 0.0)],
    }));
    assert!(!custom.is_inline());
    assert!(!unitary.is_inline());
}

#[test]
fn unitary_operations_map_onto_generic_estimator_identifiers() {
    let one = PackedOperation::Unitary(Box::new(UnitaryOperation {
        qubit_arity: 1,
        matrix: Vec::new(),
    }));
    let two = PackedOperation::Unitary(Box::new(UnitaryOperation {
        qubit_arity: 2,
        matrix: Vec::new(),
    }));
    let three = PackedOperation::Unitary(Box::new(UnitaryOperation {
        qubit_arity: 3,
        matrix: Vec::new(),
    }));
    assert_eq!(one.instruction_id(), 1280);
    assert_eq!(two.instruction_id(), 1281);
    assert_eq!(three.instruction_id(), 65535);
}

#[test]
fn custom_operations_map_to_generic_rather_than_a_specific_gate() {
    let custom = PackedOperation::Custom(Box::new(CustomOperation {
        name: "h".to_string(),
        qubit_arity: 1,
        param_arity: 0,
    }));
    assert_eq!(
        custom.instruction_id(),
        65535,
        "a custom operation named like a standard gate must not be treated as one"
    );
}

#[test]
fn every_operation_kind_survives_storage_and_retrieval() {
    let mut store = store_with(3, 1);
    let operations = vec![
        PackedOperation::Standard(StandardGate::H),
        PackedOperation::Standard(StandardGate::Cx),
        PackedOperation::Custom(Box::new(CustomOperation {
            name: "my_op".to_string(),
            qubit_arity: 2,
            param_arity: 1,
        })),
        PackedOperation::Unitary(Box::new(UnitaryOperation {
            qubit_arity: 1,
            matrix: vec![(0.0, 1.0)],
        })),
        PackedOperation::Barrier { qubit_arity: 3 },
        PackedOperation::Reset,
        PackedOperation::Delay {
            duration: 4.0,
            unit: TimeUnit::Ns,
        },
    ];

    for operation in &operations {
        let arity = usize::from(operation.qubit_arity());
        let qubits: Vec<Qubit> = (0..arity)
            .map(|i| Qubit(u32::try_from(i).expect("small")))
            .collect();
        store
            .push(operation.clone(), &qubits, &[], None)
            .expect("operation should append");
    }

    assert_eq!(store.len(), operations.len());
    for (index, expected) in operations.iter().enumerate() {
        let stored = store.instruction(index).expect("instruction should exist");
        assert_eq!(&stored.operation, expected);
        let qubits = store.qubits_of(stored).expect("operands should resolve");
        assert_eq!(qubits.len(), usize::from(expected.qubit_arity()));
    }
}

#[test]
fn parameters_are_absent_unless_supplied() {
    let mut store = store_with(1, 0);
    push_standard(&mut store, StandardGate::H, &[Qubit(0)]);
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Literal(0.5)])),
        )
        .expect("rotation should append");

    let plain = store.instruction(0).expect("h should exist");
    let rotation = store.instruction(1).expect("rx should exist");
    assert!(plain.params.is_none());
    assert!(plain.is_inline());
    assert_eq!(
        rotation.params.as_deref(),
        Some([ParamValue::Literal(0.5)].as_slice())
    );
    assert!(!rotation.is_inline());
}

#[test]
fn arity_mismatch_is_rejected() {
    let mut store = store_with(2, 0);
    let error = store
        .push(
            PackedOperation::Standard(StandardGate::Cx),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect_err("single operand for a two-qubit gate should fail");
    assert_eq!(
        error,
        StoreError::QubitArity {
            expected: 2,
            actual: 1
        }
    );
}

#[test]
fn out_of_range_operands_are_rejected() {
    let mut store = store_with(2, 1);
    let qubit_error = store
        .push(
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(5)],
            &[],
            None,
        )
        .expect_err("qubit beyond width should fail");
    assert_eq!(
        qubit_error,
        StoreError::QubitOutOfRange { index: 5, width: 2 }
    );

    let clbit_error = store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(9)],
            None,
        )
        .expect_err("clbit beyond width should fail");
    assert_eq!(
        clbit_error,
        StoreError::ClbitOutOfRange { index: 9, width: 1 }
    );
}

#[test]
fn instructions_retain_order() {
    let mut store = store_with(2, 0);
    push_standard(&mut store, StandardGate::H, &[Qubit(0)]);
    push_standard(&mut store, StandardGate::Cx, &[Qubit(0), Qubit(1)]);
    push_standard(&mut store, StandardGate::Z, &[Qubit(1)]);

    let names: Vec<&str> = store.instructions().map(|i| i.operation.name()).collect();
    assert_eq!(names, vec!["h", "cx", "z"]);
}

#[test]
fn operand_storage_is_independent_of_instruction_count() {
    // The property that matters at scale: a million gates over one operand
    // pattern must not store a million operand lists.
    let mut counts = Vec::new();
    for instruction_count in [1usize, 10, 1_000, 50_000] {
        let mut store = store_with(2, 0);
        store.reserve(instruction_count);
        for _ in 0..instruction_count {
            push_standard(&mut store, StandardGate::Cx, &[Qubit(0), Qubit(1)]);
        }
        assert_eq!(store.len(), instruction_count);
        counts.push(store.qubit_operand_list_count());
    }
    assert!(
        counts.windows(2).all(|pair| pair[0] == pair[1]),
        "operand list count varied with instruction count: {counts:?}"
    );
}

#[test]
fn equal_operand_lists_share_one_handle() {
    let mut store = store_with(2, 0);
    push_standard(&mut store, StandardGate::Cx, &[Qubit(0), Qubit(1)]);
    push_standard(&mut store, StandardGate::Cz, &[Qubit(0), Qubit(1)]);

    let first = store.instruction(0).expect("cx should exist");
    let second = store.instruction(1).expect("cz should exist");
    assert_eq!(
        first.qubits, second.qubits,
        "different gates over the same operands should share a handle"
    );
}

#[test]
fn distinct_operand_lists_get_distinct_handles() {
    let mut store = store_with(3, 0);
    push_standard(&mut store, StandardGate::Cx, &[Qubit(0), Qubit(1)]);
    push_standard(&mut store, StandardGate::Cx, &[Qubit(1), Qubit(0)]);
    push_standard(&mut store, StandardGate::Cx, &[Qubit(0), Qubit(2)]);

    let handles: Vec<_> = store.instructions().map(|i| i.qubits).collect();
    assert_eq!(
        handles[0].0 as usize, 1,
        "first pattern follows the empty list"
    );
    assert_ne!(handles[0], handles[1], "operand order is significant");
    assert_ne!(handles[0], handles[2]);
    assert_ne!(handles[1], handles[2]);
    assert_eq!(
        store.qubit_operand_list_count(),
        4,
        "empty plus three patterns"
    );
}

#[test]
fn interning_preserves_operand_values() {
    let mut store = store_with(4, 0);
    let patterns = [
        vec![Qubit(0), Qubit(1)],
        vec![Qubit(2), Qubit(3)],
        vec![Qubit(0), Qubit(1)],
        vec![Qubit(3), Qubit(2)],
    ];
    for pattern in &patterns {
        push_standard(&mut store, StandardGate::Cx, pattern);
    }
    for (index, expected) in patterns.iter().enumerate() {
        let stored = store.instruction(index).expect("instruction should exist");
        let operands = store.qubits_of(stored).expect("operands should resolve");
        assert_eq!(operands, expected.as_slice());
    }
}

#[test]
fn classical_operands_are_interned_separately() {
    let mut store = store_with(2, 2);
    for _ in 0..100 {
        store
            .push(
                PackedOperation::Standard(StandardGate::MeasureZ),
                &[Qubit(0)],
                &[Clbit(0)],
                None,
            )
            .expect("measurement should append");
    }
    assert_eq!(
        store.clbit_operand_list_count(),
        2,
        "empty plus one pattern"
    );
}

#[test]
fn blocks_share_the_circuit_operand_table() {
    // The reason bodies are blocks rather than nested circuits: a loop body
    // over a repeated pattern must not re-intern it.
    let mut store = store_with(2, 0);
    let body = store.new_block();
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("top-level gate should append");
    for _ in 0..50 {
        store
            .push_into_block(
                body,
                PackedOperation::Standard(StandardGate::Cx),
                &[Qubit(0), Qubit(1)],
                &[],
                None,
            )
            .expect("block gate should append");
    }
    assert_eq!(
        store.qubit_operand_list_count(),
        2,
        "the block body must reuse the operand pattern already interned outside it"
    );
}

#[test]
fn control_flow_references_blocks_and_conditions() {
    let mut store = store_with(2, 1);
    let then_block = store.new_block();
    store
        .push_into_block(
            then_block,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("block gate should append");

    let measurement = store.push_expr(Expr::Clbit(0));
    let one = store.push_expr(Expr::Literal(Literal::Int(1)));
    let condition = store.push_expr(Expr::Binary(BinaryOp::Eq, measurement, one));

    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: None,
        }))
        .expect("conditional should append");

    assert_eq!(store.len(), 1);
    assert_eq!(store.block_count(), 1);
    let stored = store.instruction(0).expect("conditional should exist");
    assert_eq!(stored.operation.name(), "if_else");
}

#[test]
fn control_flow_rejects_unknown_blocks_and_expressions() {
    let mut store = store_with(1, 1);
    let condition = store.push_expr(Expr::Clbit(0));

    let block_error = store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block: crate::BlockId(7),
            else_block: None,
        }))
        .expect_err("unknown block should fail");
    assert_eq!(block_error, StoreError::UnknownBlock(7));

    let block = store.new_block();
    let expr_error = store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition: crate::ExprId(99),
            then_block: block,
            else_block: None,
        }))
        .expect_err("unknown expression should fail");
    assert_eq!(expr_error, StoreError::UnknownExpr(99));
}

#[test]
fn nested_control_flow_is_stored_in_blocks() {
    let mut store = store_with(1, 1);
    let condition = store.push_expr(Expr::Clbit(0));
    let inner_body = store.new_block();
    let outer_body = store.new_block();

    store
        .push_into_block(
            outer_body,
            PackedOperation::ControlFlow(Box::new(ControlFlow::While(WhileLoop {
                condition,
                body: inner_body,
            }))),
            &[],
            &[],
            None,
        )
        .expect("nested loop should append");
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block: outer_body,
            else_block: None,
        }))
        .expect("conditional should append");

    let outer = store.block(outer_body).expect("outer block should exist");
    assert_eq!(outer.instructions.len(), 1);
    assert_eq!(outer.instructions[0].operation.name(), "while");
}

#[test]
fn parameters_and_variables_are_separate_declarations() {
    let mut store = store_with(1, 0);
    let theta = store.declare_parameter("theta");
    let count = store.declare_var("count", VarType::Int);

    assert_eq!(store.parameters().len(), 1);
    assert_eq!(store.vars().len(), 1);
    assert_eq!(store.parameters()[theta.0 as usize].name, "theta");
    assert_eq!(store.vars()[count.0 as usize].ty, VarType::Int);
}

#[test]
fn literal_only_conditions_are_not_runtime() {
    let mut store = store_with(1, 1);
    let left = store.push_expr(Expr::Literal(Literal::Int(1)));
    let right = store.push_expr(Expr::Literal(Literal::Int(2)));
    let static_condition = store.push_expr(Expr::Binary(BinaryOp::Lt, left, right));

    let measurement = store.push_expr(Expr::Clbit(0));
    let runtime_condition = store.push_expr(Expr::Binary(BinaryOp::Eq, measurement, left));

    assert!(
        !store.exprs().is_runtime(static_condition),
        "a condition over literals alone needs no adaptive profile"
    );
    assert!(store.exprs().is_runtime(runtime_condition));
}

#[test]
fn symbolic_parameters_survive_storage() {
    let mut store = store_with(1, 0);
    let theta = store.declare_parameter("theta");
    store
        .push_parameterized(
            StandardGate::Rx,
            &[Qubit(0)],
            &[ParamValue::Parameter(theta)],
        )
        .expect("rotation should append");

    let stored = store.instruction(0).expect("rotation should exist");
    assert_eq!(
        stored.params.as_deref(),
        Some([ParamValue::Parameter(theta)].as_slice())
    );
}

#[test]
fn unknown_parameter_handles_are_rejected() {
    let mut store = store_with(1, 0);
    let error = store
        .push_parameterized(
            StandardGate::Rx,
            &[Qubit(0)],
            &[ParamValue::Parameter(crate::ParameterId(3))],
        )
        .expect_err("undeclared parameter should fail");
    assert_eq!(error, StoreError::UnknownParameter(3));
}

#[test]
fn only_referenced_parameters_are_reported() {
    let mut store = store_with(1, 0);
    let used = store.declare_parameter("used");
    let _unused = store.declare_parameter("unused");
    store
        .push_parameterized(
            StandardGate::Rz,
            &[Qubit(0)],
            &[ParamValue::Parameter(used)],
        )
        .expect("rotation should append");

    assert_eq!(
        store.referenced_parameters(),
        vec![used],
        "a declared but unused parameter should not demand a value"
    );
}

#[test]
fn binding_produces_a_fully_literal_circuit() {
    let mut store = store_with(1, 0);
    let theta = store.declare_parameter("theta");
    store
        .push_parameterized(
            StandardGate::Rx,
            &[Qubit(0)],
            &[ParamValue::Parameter(theta)],
        )
        .expect("rotation should append");
    store
        .push_parameterized(StandardGate::Rz, &[Qubit(0)], &[ParamValue::Literal(1.0)])
        .expect("rotation should append");

    let bound = store
        .bind_parameters(&[(theta, 0.25)])
        .expect("binding should succeed");

    assert!(bound.referenced_parameters().is_empty());
    assert_eq!(
        bound.instruction(0).expect("rx").params.as_deref(),
        Some([ParamValue::Literal(0.25)].as_slice())
    );
    assert_eq!(
        bound.instruction(1).expect("rz").params.as_deref(),
        Some([ParamValue::Literal(1.0)].as_slice()),
        "already-literal angles must be left alone"
    );
}

#[test]
fn binding_reports_a_missing_value_by_name() {
    let mut store = store_with(1, 0);
    let theta = store.declare_parameter("theta");
    store
        .push_parameterized(
            StandardGate::Rx,
            &[Qubit(0)],
            &[ParamValue::Parameter(theta)],
        )
        .expect("rotation should append");

    let error = store
        .bind_parameters(&[])
        .expect_err("missing value should fail");
    assert_eq!(error, StoreError::UnboundParameter("theta".to_string()));
}

#[test]
fn binding_reaches_parameters_inside_blocks() {
    let mut store = store_with(1, 1);
    let theta = store.declare_parameter("theta");
    let body = store.new_block();
    store
        .push_into_block(
            body,
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Parameter(theta)])),
        )
        .expect("block rotation should append");
    let condition = store.push_expr(Expr::Clbit(0));
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block: body,
            else_block: None,
        }))
        .expect("conditional should append");

    assert_eq!(store.referenced_parameters(), vec![theta]);

    let bound = store
        .bind_parameters(&[(theta, 0.75)])
        .expect("binding should succeed");
    let inner = bound.block(body).expect("body should exist");
    assert_eq!(
        inner.instructions[0].params.as_deref(),
        Some([ParamValue::Literal(0.75)].as_slice()),
        "binding must descend into blocks"
    );
}

#[test]
fn an_angle_expression_is_rejected_when_it_does_not_exist() {
    let mut store = CircuitStore::new(1, 0);

    let error = store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Expr(crate::AngleId(7))])),
        )
        .expect_err("a missing angle should be refused");

    assert_eq!(error, StoreError::UnknownAngle(7));
}

#[test]
fn binding_folds_an_angle_expression() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("theta");
    let base = store.push_angle(crate::AngleExpr::Parameter(theta));
    let two = store.push_angle(crate::AngleExpr::Literal(2.0));
    let doubled = store.push_angle(crate::AngleExpr::Binary(crate::AngleOp::Mul, base, two));
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Expr(doubled)])),
        )
        .expect("gate should be accepted");

    // A parameter reached only through arithmetic still has to be supplied.
    assert_eq!(store.referenced_parameters(), vec![theta]);

    let bound = store
        .bind_parameters(&[(theta, 1.5)])
        .expect("binding should succeed");

    let angle = bound
        .instructions()
        .next()
        .and_then(|instruction| instruction.params.clone())
        .expect("gate should carry an angle");
    assert_eq!(angle[0], ParamValue::Literal(3.0));
}

#[test]
fn an_angle_expression_survives_a_round_trip() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("theta");
    let base = store.push_angle(crate::AngleExpr::Parameter(theta));
    let half = store.push_angle(crate::AngleExpr::Literal(0.5));
    let scaled = store.push_angle(crate::AngleExpr::Binary(crate::AngleOp::Div, base, half));
    let negated = store.push_angle(crate::AngleExpr::Neg(scaled));
    store
        .push(
            PackedOperation::Standard(StandardGate::Rz),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Expr(negated)])),
        )
        .expect("gate should be accepted");

    let text = store.to_text().expect("circuit should write");
    let back = CircuitStore::from_text(&text).expect("circuit should read");

    assert_eq!(back.angles().nodes(), store.angles().nodes());
    assert_eq!(back.to_text().expect("circuit should write"), text);
}

#[test]
fn an_angle_expression_pointing_forwards_is_refused() {
    let mut store = CircuitStore::new(1, 0);
    store.declare_parameter("theta");
    let text = store.to_text().expect("circuit should write");
    // A node that reads a node stored after it, which building cannot produce.
    let tampered = text.replace("(angles (list))", "(angles (list (neg 0)))");
    assert_ne!(tampered, text);

    let error = CircuitStore::from_text(&tampered).expect_err("a cycle should be refused");

    assert_eq!(
        error,
        crate::SnapshotError::Malformed(
            "angle expression 0 references a node stored after it".to_string()
        )
    );
}
