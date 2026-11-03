// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Save and restore tests.
//!
//! Valid circuits must restore exactly, and malformed snapshots must be
//! rejected before their bodies can be used.

use expect_test::expect;

use crate::classical::{BinaryOp, Expr, Literal, ParamValue, VarType};
use crate::control_flow::{BoxBody, ControlFlow, ForLoop, IfElse, LoopIndex, Switch, WhileLoop};
use crate::gate::StandardGate;
use crate::operands::{Clbit, Qubit};
use crate::operation::{CustomOperation, PackedOperation, TimeUnit, UnitaryOperation};
use crate::snapshot::{SNAPSHOT_SCHEMA_VERSION, Snapshot, SnapshotError};
use crate::store::CircuitStore;

/// Asserts that a circuit survives a round trip unchanged, and that encoding is
/// stable across a second one.
fn round_trip(store: &CircuitStore) -> CircuitStore {
    let bytes = store.to_bytes().expect("circuit should serialize");
    let restored = CircuitStore::from_bytes(&bytes).expect("circuit should restore");

    assert_eq!(
        store.to_snapshot(),
        restored.to_snapshot(),
        "restored circuit differs from the original"
    );
    assert_eq!(
        bytes,
        restored
            .to_bytes()
            .expect("restored circuit should serialize"),
        "encoding is not stable across a round trip"
    );
    let text = store.to_text().expect("text should serialize");
    let from_text = CircuitStore::from_text(&text).expect("text should restore");
    assert_eq!(
        from_text.to_bytes().expect("binary should serialize"),
        bytes
    );
    restored
}

#[test]
fn empty_circuit_round_trips() {
    let store = CircuitStore::new(3, 2);
    let restored = round_trip(&store);

    assert_eq!(restored.qubit_count(), 3);
    assert_eq!(restored.clbit_count(), 2);
    assert_eq!(restored.len(), 0);
}

#[test]
fn zero_width_circuit_round_trips() {
    let store = CircuitStore::new(0, 0);
    let restored = round_trip(&store);

    assert_eq!(restored.qubit_count(), 0);
    assert_eq!(restored.clbit_count(), 0);
}

#[test]
fn negative_zero_global_phase_keeps_its_sign() {
    let mut store = CircuitStore::new(1, 0);
    store.set_global_phase(-0.0);
    let restored = round_trip(&store);
    assert_eq!(restored.global_phase().to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn a_nan_angle_keeps_its_payload_in_both_encodings() {
    let mut store = CircuitStore::new(1, 0);
    let bits = 0xfff8_1234_5678_9000;
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Literal(f64::from_bits(bits))])),
        )
        .expect("gate");
    let binary = store.to_bytes().expect("encode binary");
    let text = store.to_text().expect("encode text");
    assert_eq!(
        CircuitStore::from_bytes(&binary)
            .expect("decode binary")
            .to_bytes()
            .expect("encode"),
        binary
    );
    let restored = CircuitStore::from_text(&text).expect("decode text");
    assert_eq!(restored.to_bytes().expect("encode"), binary);
    let instruction = restored.instruction(0).expect("instruction");
    let Some([ParamValue::Literal(value)]) = instruction.params.as_deref() else {
        panic!("expected a literal");
    };
    assert_eq!(value.to_bits(), bits);
}

#[test]
fn every_standard_gate_round_trips() {
    let mut store = CircuitStore::new(3, 3);
    for gate in StandardGate::all() {
        let qubits: Vec<Qubit> = (0..u32::from(gate.qubit_arity())).map(Qubit).collect();
        let clbits: Vec<Clbit> = if gate.name().starts_with("measure") {
            vec![Clbit(0)]
        } else {
            Vec::new()
        };
        let params: Option<Box<[ParamValue]>> = if gate.param_arity() > 0 {
            Some(
                (0..gate.param_arity())
                    .map(|index| ParamValue::Literal(f64::from(index) + 0.25))
                    .collect(),
            )
        } else {
            None
        };
        store
            .push(PackedOperation::Standard(gate), &qubits, &clbits, params)
            .expect("gate should be accepted");
    }

    let restored = round_trip(&store);
    assert_eq!(restored.len(), StandardGate::all().len());
}

#[test]
fn gates_are_stored_by_name_not_by_discriminant() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push_standard(StandardGate::SDag, &[Qubit(0)])
        .expect("gate should be accepted");

    let text = store.to_text().expect("circuit should serialize");

    // Reordering the gate enum must not change what a stored circuit means, so
    // the name is what is written.
    assert!(
        text.contains("(standard sdg)"),
        "expected a gate name in {text}"
    );
}

#[test]
fn non_finite_angles_round_trip() {
    let mut store = CircuitStore::new(1, 0);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        store
            .push(
                PackedOperation::Standard(StandardGate::Rx),
                &[Qubit(0)],
                &[],
                Some(Box::new([ParamValue::Literal(value)])),
            )
            .expect("gate should be accepted");
    }

    let text = store.to_text().expect("circuit should serialize");
    let restored = CircuitStore::from_text(&text).expect("circuit should restore");

    let angles: Vec<f64> = restored
        .instructions()
        .map(|instruction| match instruction.params.as_deref() {
            Some([ParamValue::Literal(value)]) => *value,
            other => panic!("expected one literal angle, got {other:?}"),
        })
        .collect();

    assert!(angles[0].is_nan(), "NaN did not survive the round trip");
    assert_eq!(angles[1].to_bits(), f64::INFINITY.to_bits());
    assert_eq!(angles[2].to_bits(), f64::NEG_INFINITY.to_bits());
}

#[test]
fn extreme_finite_angles_round_trip_exactly() {
    let mut store = CircuitStore::new(1, 0);
    let values = [
        0.0,
        -0.0,
        f64::MIN,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::EPSILON,
        // A subnormal, which is where a naive decimal formatter loses bits.
        5e-324,
        std::f64::consts::PI,
        1.0 / 3.0,
    ];
    for value in values {
        store
            .push(
                PackedOperation::Standard(StandardGate::Rz),
                &[Qubit(0)],
                &[],
                Some(Box::new([ParamValue::Literal(value)])),
            )
            .expect("gate should be accepted");
    }

    let restored = round_trip(&store);
    let angles: Vec<f64> = restored
        .instructions()
        .map(|instruction| match instruction.params.as_deref() {
            Some([ParamValue::Literal(value)]) => *value,
            other => panic!("expected one literal angle, got {other:?}"),
        })
        .collect();

    for (stored, original) in angles.iter().zip(values) {
        assert_eq!(
            stored.to_bits(),
            original.to_bits(),
            "{original} did not survive with its exact bit pattern"
        );
    }
}

#[test]
fn symbolic_parameters_round_trip() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("theta");
    // Declared but never referenced, which must still survive.
    store.declare_parameter("unused");
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Parameter(theta)])),
        )
        .expect("gate should be accepted");

    let restored = round_trip(&store);

    assert_eq!(restored.parameters().len(), 2);
    assert_eq!(restored.parameters()[0].name, "theta");
    assert_eq!(restored.parameters()[1].name, "unused");
}

#[test]
fn classical_variables_and_expressions_round_trip() {
    let mut store = CircuitStore::new(1, 2);
    store.declare_var("flag", VarType::Bool);
    let count = store.declare_var("count", VarType::Int);
    let left = store.push_expr(Expr::Var(count));
    let right = store.push_expr(Expr::Literal(Literal::Int(3)));
    let compare = store.push_expr(Expr::Binary(BinaryOp::Lt, left, right));
    store.push_expr(Expr::Not(compare));

    let restored = round_trip(&store);

    assert_eq!(restored.vars().len(), 2);
    assert_eq!(restored.vars()[0].ty, VarType::Bool);
    assert_eq!(restored.exprs().len(), 4);
}

#[test]
fn every_control_flow_construct_round_trips() {
    let mut store = CircuitStore::new(2, 2);
    let condition = store.push_expr(Expr::Clbit(0));

    let then_block = store.new_block();
    store
        .push_into_block(
            then_block,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    let else_block = store.new_block();
    let while_body = store.new_block();
    let for_body = store.new_block();
    let case_body = store.new_block();
    let default_body = store.new_block();
    let boxed_body = store.new_block();

    let bound = store.declare_var("index", VarType::Int);
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: Some(else_block),
        }))
        .expect("construct should be accepted");
    store
        .push_control_flow(ControlFlow::While(WhileLoop {
            condition,
            body: while_body,
        }))
        .expect("construct should be accepted");
    store
        .push_control_flow(ControlFlow::For(ForLoop {
            index: Some(LoopIndex::Var(bound)),
            values: vec![Literal::Int(1), Literal::Bool(true), Literal::Float(0.5)],
            body: for_body,
        }))
        .expect("construct should be accepted");
    store
        .push_control_flow(ControlFlow::Switch(Switch {
            selector: condition,
            cases: vec![(vec![Literal::Int(0)], case_body)],
            default: Some(default_body),
        }))
        .expect("construct should be accepted");
    store
        .push_control_flow(ControlFlow::Box(BoxBody {
            body: boxed_body,
            duration: Some(1.5),
            label: Some("prep".to_string()),
        }))
        .expect("construct should be accepted");
    store
        .push_control_flow(ControlFlow::Break)
        .expect("construct should be accepted");
    store
        .push_control_flow(ControlFlow::Continue)
        .expect("construct should be accepted");

    let restored = round_trip(&store);

    assert_eq!(restored.block_count(), 7);
    assert_eq!(restored.len(), 7);
    assert_eq!(
        restored
            .block(then_block)
            .expect("block should exist")
            .instructions
            .len(),
        1
    );
}

#[test]
fn non_standard_operations_round_trip() {
    let mut store = CircuitStore::new(2, 0);
    store
        .push(
            PackedOperation::Custom(Box::new(CustomOperation {
                name: "my_gate".to_string(),
                qubit_arity: 2,
                param_arity: 0,
            })),
            &[Qubit(0), Qubit(1)],
            &[],
            None,
        )
        .expect("operation should be accepted");
    store
        .push(
            PackedOperation::Unitary(Box::new(UnitaryOperation {
                qubit_arity: 1,
                matrix: vec![(1.0, 0.0), (0.0, 0.0), (0.0, 0.0), (f64::NAN, -0.0)],
            })),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("operation should be accepted");
    store
        .push(
            PackedOperation::Barrier { qubit_arity: 2 },
            &[Qubit(0), Qubit(1)],
            &[],
            None,
        )
        .expect("operation should be accepted");
    store
        .push(PackedOperation::Reset, &[Qubit(0)], &[], None)
        .expect("operation should be accepted");
    store
        .push(
            PackedOperation::Delay {
                duration: f64::INFINITY,
                unit: TimeUnit::Dt,
            },
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("operation should be accepted");

    let text = store.to_text().expect("circuit should serialize");
    let restored = CircuitStore::from_text(&text).expect("circuit should restore");

    // A NaN inside the matrix makes the whole snapshot compare unequal to
    // itself, so this case is checked field by field rather than through the
    // shared helper.
    assert_eq!(restored.len(), 5);
    assert_eq!(
        text,
        restored
            .to_text()
            .expect("restored circuit should serialize")
    );
}

#[test]
fn repeated_operand_patterns_stay_interned_after_restore() {
    let mut store = CircuitStore::new(2, 0);
    for _ in 0..100 {
        store
            .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
            .expect("gate should be accepted");
    }

    let restored = round_trip(&store);

    assert_eq!(restored.len(), 100);
    // One pattern for the pair, plus the empty list every table defines.
    assert_eq!(restored.qubit_operand_list_count(), 2);
}

#[test]
fn removed_instructions_leave_nothing_behind() {
    let mut store = CircuitStore::new(2, 0);
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("gate should be accepted");
    store.pop_last_instruction();
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("gate should be accepted");

    let restored = round_trip(&store);

    // Restoring re-interns, so the pattern belonging to the removed instruction
    // is not carried forward.
    assert_eq!(store.qubit_operand_list_count(), 3);
    assert_eq!(restored.qubit_operand_list_count(), 2);
}

#[test]
fn a_different_schema_version_is_rejected() {
    let store = CircuitStore::new(1, 0);
    let mut snapshot = store.to_snapshot();
    snapshot.schema_version = SNAPSHOT_SCHEMA_VERSION + 1;

    let error = CircuitStore::from_snapshot(&snapshot).expect_err("version should be rejected");

    assert_eq!(
        error,
        SnapshotError::SchemaVersion {
            found: SNAPSHOT_SCHEMA_VERSION + 1,
            expected: SNAPSHOT_SCHEMA_VERSION,
        }
    );
}

#[test]
fn text_that_is_not_a_snapshot_is_rejected() {
    let error = CircuitStore::from_text("not a snapshot").expect_err("input should be rejected");

    assert!(matches!(error, SnapshotError::Encoding(_)));
}

#[test]
fn an_operand_outside_the_circuit_is_rejected() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("gate should be accepted");
    let text = store
        .to_text()
        .expect("circuit should serialize")
        .replace("(qubits (list 0))", "(qubits (list 7))");

    let error = CircuitStore::from_text(&text).expect_err("operand should be rejected");

    // Restoring applies the same checks as building, so a snapshot cannot
    // introduce a circuit that could not have been built.
    assert!(matches!(error, SnapshotError::Invalid(_)));
}

#[test]
fn a_body_referring_to_a_missing_block_is_rejected() {
    let mut store = CircuitStore::new(1, 1);
    let condition = store.push_expr(Expr::Clbit(0));
    let block = store.new_block();
    store
        .push_control_flow(ControlFlow::While(WhileLoop {
            condition,
            body: block,
        }))
        .expect("construct should be accepted");
    let text = store
        .to_text()
        .expect("circuit should serialize")
        .replace("(body 0)", "(body 9)");

    let error = CircuitStore::from_text(&text).expect_err("block should be rejected");

    assert!(matches!(error, SnapshotError::Invalid(_)));
}

#[test]
fn the_stored_form_is_readable() {
    let mut store = CircuitStore::new(2, 1);
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("gate should be accepted");
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("gate should be accepted");
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("gate should be accepted");

    let text = store.to_text().expect("circuit should serialize");

    expect![[
        r#"QDKSTORE(
(qdk 1 (store (version 1) (qubitCount 2) (clbitCount 1) (globalPhase 0.0) (parameters (list)) (variables (list)) (expressions (list)) (angles (list)) (blocks (list)) (instructions (list (instruction (operation (standard h)) (qubits (list 0)) (clbits (list)) (parameters (none))) (instruction (operation (standard cx)) (qubits (list 0 1)) (clbits (list)) (parameters (none))) (instruction (operation (standard measureZ)) (qubits (list 0)) (clbits (list 0)) (parameters (none)))))))"#
    ]]
    .assert_eq(&text);
}

#[test]
fn empty_optional_sections_are_explicit_and_restored() {
    let text = CircuitStore::new(1, 0).to_text().expect("encode");
    let parsed = qsc_snapshot::text::parse(qsc_snapshot::Kind::CircuitStore, text.as_bytes())
        .expect("parse");
    let snapshot = Snapshot::read_text(
        parsed.record("store").expect("record"),
        &mut parsed.budget(),
    )
    .expect("snapshot");
    let store = CircuitStore::from_snapshot(&snapshot).expect("minimal snapshot should restore");

    assert_eq!(store.qubit_count(), 1);
    assert_eq!(store.len(), 0);
}

fn encoded_snapshot(snapshot: &Snapshot) -> Vec<u8> {
    use qsc_snapshot::{binary, snapshot_capnp as wire};
    let mut message = binary::Message::new_default();
    let mut root = message.init_root::<wire::document::Builder>();
    root.set_version(qsc_snapshot::MODEL_VERSION);
    snapshot.write_binary(root.init_store()).expect("write");
    binary::encode(qsc_snapshot::Kind::CircuitStore, &message).expect("encode")
}

fn text_snapshot(snapshot: &Snapshot) -> String {
    let mut writer =
        qsc_snapshot::text::Writer::new(qsc_snapshot::Kind::CircuitStore).expect("writer");
    snapshot.write_text(&mut writer).expect("write");
    writer.finish().expect("finish")
}

fn boxed_body(body: u32) -> crate::SnapshotInstruction {
    crate::SnapshotInstruction {
        operation: PackedOperation::ControlFlow(Box::new(ControlFlow::Box(BoxBody {
            body: crate::BlockId(body),
            duration: None,
            label: None,
        }))),
        qubits: vec![],
        clbits: vec![],
        params: None,
    }
}

fn snapshot_with_bodies(blocks: Vec<Vec<crate::SnapshotInstruction>>) -> Snapshot {
    let mut snapshot = CircuitStore::new(1, 0).to_snapshot();
    snapshot.blocks = blocks;
    snapshot.instructions.push(boxed_body(0));
    snapshot
}

fn assert_cyclic_body_rejected(snapshot: &Snapshot) {
    let binary_error = CircuitStore::from_bytes(&encoded_snapshot(snapshot))
        .expect_err("cyclic binary body must be refused");
    let text_error = CircuitStore::from_text(&text_snapshot(snapshot))
        .expect_err("cyclic text body must be refused");
    assert_eq!(binary_error, text_error);
    assert!(matches!(
        binary_error,
        SnapshotError::Malformed(message) if message.contains("block") && message.contains("cyclic")
    ));
}

fn restore_body_snapshot(snapshot: &Snapshot) -> CircuitStore {
    let restored = CircuitStore::from_bytes(&encoded_snapshot(snapshot)).expect("restore binary");
    let text_restored = CircuitStore::from_text(&text_snapshot(snapshot)).expect("restore text");
    assert_eq!(restored.to_snapshot(), *snapshot);
    assert_eq!(text_restored.to_snapshot(), *snapshot);
    restored
}

#[test]
fn self_referential_box_body_is_rejected_by_both_imports() {
    assert_cyclic_body_rejected(&snapshot_with_bodies(vec![vec![boxed_body(0)]]));
}

#[test]
fn mutually_recursive_bodies_are_rejected_by_both_imports() {
    assert_cyclic_body_rejected(&snapshot_with_bodies(vec![
        vec![boxed_body(1)],
        vec![boxed_body(0)],
    ]));
}

#[test]
fn unused_cyclic_bodies_are_rejected_by_both_imports() {
    let mut snapshot = snapshot_with_bodies(vec![vec![], vec![boxed_body(1)]]);
    snapshot.instructions.clear();
    assert_cyclic_body_rejected(&snapshot);
}

#[test]
fn forward_body_references_restore_without_reordering() {
    restore_body_snapshot(&snapshot_with_bodies(vec![vec![boxed_body(1)], vec![]]));
}

#[test]
fn shared_body_references_are_not_mistaken_for_a_cycle() {
    let mut snapshot = snapshot_with_bodies(vec![vec![boxed_body(2)], vec![boxed_body(2)], vec![]]);
    snapshot.instructions.push(boxed_body(1));
    restore_body_snapshot(&snapshot);
}

#[test]
fn a_while_loop_restores_its_acyclic_body() {
    let mut store = CircuitStore::new(1, 0);
    let condition = store.push_expr(Expr::Literal(Literal::Bool(false)));
    let body = store.new_block();
    store
        .push_into_block(
            body,
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("body");
    store
        .push_control_flow(ControlFlow::While(WhileLoop { condition, body }))
        .expect("loop");
    round_trip(&store);
}

#[test]
fn a_for_loop_restores_its_acyclic_body() {
    let mut store = CircuitStore::new(1, 0);
    let body = store.new_block();
    store
        .push_into_block(
            body,
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("body");
    store
        .push_control_flow(ControlFlow::For(ForLoop {
            index: None,
            values: vec![Literal::Int(0), Literal::Int(1)],
            body,
        }))
        .expect("loop");
    round_trip(&store);
}

#[test]
fn a_maximum_width_sparse_moment_round_trips() {
    let mut store = CircuitStore::new(u32::MAX, 0);
    let body = store.new_block();
    store
        .push_into_block(
            body,
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("first gate");
    store
        .push_into_block(
            body,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(u32::MAX - 1)],
            &[],
            None,
        )
        .expect("last wire");
    store
        .push_control_flow(ControlFlow::Moment(body))
        .expect("sparse moment");
    round_trip(&store);
}

#[test]
fn a_sparse_moment_still_rejects_repeated_operands() {
    let mut store = CircuitStore::new(8_000_000, 0);
    let body = store.new_block();
    store
        .push_into_block(
            body,
            PackedOperation::Standard(StandardGate::Cx),
            &[Qubit(7_999_999), Qubit(7_999_999)],
            &[],
            None,
        )
        .expect("body");
    let mut snapshot = store.to_snapshot();
    snapshot.instructions.push(crate::SnapshotInstruction {
        operation: PackedOperation::ControlFlow(Box::new(ControlFlow::Moment(body))),
        qubits: vec![],
        clbits: vec![],
        params: None,
    });
    let expected = SnapshotError::Invalid(crate::StoreError::MomentOverlap(7_999_999));
    assert_eq!(
        CircuitStore::from_bytes(&encoded_snapshot(&snapshot))
            .expect_err("duplicate binary operand"),
        expected,
    );
    assert_eq!(
        CircuitStore::from_text(&text_snapshot(&snapshot)).expect_err("duplicate text operand"),
        expected,
    );
}

#[test]
fn absent_and_present_empty_parameter_lists_remain_distinct() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push(
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("gate");
    store
        .push(
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(0)],
            &[],
            Some(Box::new([])),
        )
        .expect("gate");
    let restored = round_trip(&store);
    assert!(restored.instruction(0).expect("first").params.is_none());
    assert_eq!(
        restored.instruction(1).expect("second").params.as_deref(),
        Some([].as_slice())
    );
}

#[test]
fn duplicate_unused_declaration_names_keep_their_indices() {
    let mut store = CircuitStore::new(1, 0);
    store.declare_parameter("same");
    let second = store.declare_parameter("same");
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Parameter(second)])),
        )
        .expect("gate");
    let restored = round_trip(&store);
    assert_eq!(restored.parameters(), store.parameters());
}

#[test]
fn a_binary_snapshot_rejects_a_dangling_expression_reference() {
    let mut snapshot = CircuitStore::new(1, 0).to_snapshot();
    snapshot.exprs.push(Expr::Not(crate::ExprId(99)));
    assert!(
        matches!(CircuitStore::from_bytes(&encoded_snapshot(&snapshot)), Err(SnapshotError::Malformed(message)) if message.contains("invalid reference"))
    );
}

#[test]
fn a_binary_snapshot_rejects_a_cycle_in_classical_expressions() {
    let mut snapshot = CircuitStore::new(1, 0).to_snapshot();
    snapshot.exprs.push(Expr::Not(crate::ExprId(0)));
    assert!(
        matches!(CircuitStore::from_bytes(&encoded_snapshot(&snapshot)), Err(SnapshotError::Malformed(message)) if message.contains("cyclic"))
    );
}

#[test]
fn forward_classical_expression_references_are_preserved_when_acyclic() {
    let mut snapshot = CircuitStore::new(1, 0).to_snapshot();
    snapshot.exprs = vec![
        Expr::Not(crate::ExprId(1)),
        Expr::Literal(Literal::Bool(false)),
    ];
    let restored =
        CircuitStore::from_bytes(&encoded_snapshot(&snapshot)).expect("valid forward reference");
    assert_eq!(restored.exprs().nodes(), snapshot.exprs);
}

#[test]
fn a_moment_validates_the_completed_forward_body() {
    let mut snapshot = CircuitStore::new(1, 0).to_snapshot();
    let operation = |operation, qubits| crate::SnapshotInstruction {
        operation,
        qubits,
        clbits: vec![],
        params: None,
    };
    snapshot.blocks = vec![
        vec![operation(
            PackedOperation::ControlFlow(Box::new(ControlFlow::Moment(crate::BlockId(1)))),
            vec![],
        )],
        vec![
            operation(PackedOperation::Standard(StandardGate::H), vec![0]),
            operation(PackedOperation::Standard(StandardGate::X), vec![0]),
        ],
    ];
    assert!(matches!(
        CircuitStore::from_bytes(&encoded_snapshot(&snapshot)),
        Err(SnapshotError::Invalid(crate::StoreError::MomentOverlap(0)))
    ));
}

#[test]
fn maximum_declared_width_does_not_become_an_absent_value() {
    let restored = round_trip(&CircuitStore::new(u32::MAX, u32::MAX));
    assert_eq!(
        (restored.qubit_count(), restored.clbit_count()),
        (u32::MAX, u32::MAX)
    );
}

#[test]
fn current_binary_format_has_typed_fields_without_property_name_strings() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("gate");
    let bytes = store.to_bytes().expect("encode");
    assert!(
        !bytes
            .windows("qubit_count".len())
            .any(|bytes| bytes == b"qubit_count")
    );
    let (_, payload) =
        qsc_snapshot::payload(qsc_snapshot::Kind::CircuitStore, &bytes).expect("header");
    let (message, _) =
        qsc_snapshot::binary::decode(qsc_snapshot::Kind::CircuitStore, payload).expect("packed");
    assert!(message.is_canonical().expect("canonical layout"));
}

#[test]
fn a_large_circuit_snapshot_does_not_inherit_the_graph_byte_limit() {
    let mut store = CircuitStore::new(0, 0);
    store
        .push(
            PackedOperation::Custom(Box::new(crate::CustomOperation {
                name: "x".repeat(qsc_snapshot::MAX_SNAPSHOT_BYTES + 1),
                qubit_arity: 0,
                param_arity: 0,
            })),
            &[],
            &[],
            None,
        )
        .expect("custom operation");
    let bytes = store.to_bytes().expect("encode");
    assert!(bytes.len() > qsc_snapshot::MAX_SNAPSHOT_BYTES);
    let restored = CircuitStore::from_bytes(&bytes).expect("decode large circuit");
    assert_eq!(restored.to_bytes().expect("encode restored"), bytes);
}
