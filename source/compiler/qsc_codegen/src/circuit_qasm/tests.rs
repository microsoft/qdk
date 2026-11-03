// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for exporting a circuit to OpenQASM 3 and importing one back.

use expect_test::expect;
use qsc_circuit_store::{
    AngleExpr, AngleOp, BinaryOp, BoxBody, CircuitStore, Clbit, ControlFlow, Expr, ForLoop, IfElse,
    Literal, LoopIndex, PackedOperation, ParamValue, Qubit, StandardGate, Switch, TimeUnit,
    VarType, WhileLoop,
};

use super::{Error, circuit_from_qasm, circuit_to_qasm};

fn assert_imported_condition(condition: &str, prepare: &str, expected: &str) {
    let source = format!(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[3] q;
        bit out;
        bit[2] c;
        {prepare}
        c[0] = measure q[0];
        c[1] = measure q[1];
        if ({condition}) {{ x q[2]; }}
        out = measure q[2];
    "#
    );
    let store = circuit_from_qasm(&source).expect("source should import");
    let qir = crate::circuit::circuit_to_qir(
        &store,
        qsc_data_structures::target::Profile::AdaptiveRI.into(),
    )
    .expect("import should lower");
    #[cfg(unix)]
    crate::circuit::tests::assert_native_bits(&qir, expected);
    #[cfg(not(unix))]
    let _ = (qir, expected);
}

#[test]
fn whole_register_three_does_not_equal_one() {
    assert_imported_condition("c == 1", "x q[0]; x q[1];", "011");
}

#[test]
fn whole_register_three_equals_three() {
    assert_imported_condition("c == 3", "x q[0]; x q[1];", "111");
}

#[test]
fn whole_register_order_includes_high_bit_and_offset() {
    assert_imported_condition("c > 1", "x q[1];", "101");
}

#[test]
fn whole_register_less_than_preserves_unsigned_order() {
    assert_imported_condition("c < 1", "x q[1];", "001");
}

#[test]
fn register_on_right_preserves_comparison_direction() {
    assert_imported_condition("1 < c", "x q[1];", "101");
}

#[test]
fn whole_register_less_than_or_equal_includes_high_bit() {
    assert_imported_condition("c <= 1", "x q[1];", "001");
}

#[test]
fn whole_register_greater_than_or_equal_includes_high_bit() {
    assert_imported_condition("c >= 2", "x q[1];", "101");
}

#[test]
fn boolean_cast_of_zero_constant_is_false() {
    assert_imported_condition("bool(0) == true", "", "000");
}

#[test]
fn negative_integer_cast_truncates_towards_zero() {
    assert_imported_condition("int[32](-1.9) == -1", "", "100");
}

#[test]
fn signed_register_widening_preserves_negative_ordering() {
    assert_imported_condition("int[64](int[2](c)) < 0", "x q[0]; x q[1];", "111");
}

#[test]
fn signed_register_widening_agrees_with_typed_literal_equality() {
    assert_imported_condition(
        "int[64](int[2](c)) == int[64](int[2](\"11\"))",
        "x q[0]; x q[1];",
        "111",
    );
}

#[test]
fn signed_register_widening_preserves_nonnegative_values() {
    assert_imported_condition("int[64](int[2](c)) == 1", "x q[0];", "110");
}

#[test]
fn unsigned_register_widening_zero_extends_the_high_bit() {
    assert_imported_condition("int[64](uint[2](c)) == 3", "x q[0]; x q[1];", "111");
}

#[test]
fn width_sixty_four_register_comparison_retains_the_high_bit() {
    let store = circuit_from_qasm(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[3] q;
        bit out;
        bit[64] c;
        x q[0]; x q[1];
        c[0] = measure q[0];
        c[63] = measure q[1];
        if (c == 1) { x q[2]; }
        out = measure q[2];
    "#,
    )
    .expect("64-bit comparison should import");
    let qir = crate::circuit::circuit_to_qir(
        &store,
        qsc_data_structures::target::Profile::AdaptiveRI.into(),
    )
    .expect("comparison should lower");
    assert!(qir.contains("i64 64 to %Result*"), "{qir}");
    #[cfg(unix)]
    crate::circuit::tests::assert_native_bits(&qir, "011");
}

#[test]
fn register_comparison_does_not_truncate_to_a_smaller_numeric_type() {
    let error = circuit_from_qasm(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit q;
        bit[65] c;
        if (c == 1) { x q; }
    "#,
    )
    .expect_err("implicit int64 conversion cannot retain 65 bits");
    assert!(matches!(error, Error::UnsupportedImport { .. }), "{error}");
    assert!(error.to_string().contains("width"), "{error}");
}

#[test]
fn register_to_register_comparison_retains_both_offsets() {
    let store = circuit_from_qasm(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[3] q;
        bit out;
        bit[2] c;
        bit[2] d;
        x q[0];
        c[0] = measure q[0]; c[1] = measure q[1];
        d[0] = measure q[0]; d[1] = measure q[1];
        if (c == d) { x q[2]; }
        out = measure q[2];
    "#,
    )
    .expect("equal-width registers should import");
    let qir = crate::circuit::circuit_to_qir(
        &store,
        qsc_data_structures::target::Profile::AdaptiveRI.into(),
    )
    .expect("comparison should lower");
    assert!(qir.contains("__quantum__rt__read_result"), "{qir}");
    #[cfg(unix)]
    crate::circuit::tests::assert_native_bits(&qir, "11010");
}

#[test]
fn whole_register_inequality_includes_high_bit() {
    assert_imported_condition("c != 1", "x q[0]; x q[1];", "111");
}

#[test]
fn integer_casts_are_applied_before_imported_equality() {
    assert_imported_condition("int[32](1.9) == int[32](1.1)", "", "100");
}

#[test]
fn boolean_cast_of_nonzero_constant_is_true() {
    assert_imported_condition("bool(2) == true", "", "100");
}

#[test]
fn nested_constant_casts_are_not_erased() {
    assert_imported_condition("float[64](int[32](1.9)) == 1.0", "", "100");
}

#[test]
fn dynamic_numeric_conversion_is_named_as_unsupported() {
    let error = circuit_from_qasm(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit q;
        bit c = measure q;
        if (float[64](int[32](c)) == 0.0) { x q; }
    "#,
    )
    .expect_err("dynamic conversion is not representable");
    assert!(matches!(error, Error::UnsupportedImport { .. }), "{error}");
    assert!(error.to_string().contains("cast"), "{error}");
}

#[test]
fn converted_angle_uses_the_cast_value() {
    let store = circuit_from_qasm(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit q;
        rx(float[64](int[32](1.9))) q;
    "#,
    )
    .expect("converted angle should import");
    let Some([ParamValue::Literal(angle)]) = store
        .instructions()
        .next()
        .expect("rotation exists")
        .params
        .as_deref()
    else {
        panic!("one literal angle");
    };
    assert!((angle - 1.0).abs() < 1e-12, "{angle}");
}

/// Describes every instruction, for comparing circuits across a round trip.
///
/// Deliberately not OpenQASM-shaped, so it cannot be mistaken for exported
/// source. Angles are rounded before comparison because OpenQASM stores an
/// angle as a fixed-point fraction of a turn, so an exact float does not
/// survive. Bodies are described by position rather than by block identifier,
/// because two circuits holding the same program need not number their blocks
/// the same way.
fn describe(store: &CircuitStore) -> Vec<String> {
    let mut lines = Vec::new();
    for instruction in store.instructions() {
        describe_instruction(store, instruction, 0, &mut lines);
    }
    lines
}

fn describe_instruction(
    store: &CircuitStore,
    instruction: &qsc_circuit_store::Instruction,
    depth: usize,
    lines: &mut Vec<String>,
) {
    let pad = "  ".repeat(depth);
    let qubits: Vec<u32> = store
        .qubits_of(instruction)
        .unwrap_or(&[])
        .iter()
        .map(|qubit| qubit.0)
        .collect();
    let clbits: Vec<u32> = store
        .clbits_of(instruction)
        .unwrap_or(&[])
        .iter()
        .map(|clbit| clbit.0)
        .collect();
    let params: Vec<String> = instruction
        .params
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|param| match param {
            ParamValue::Literal(value) => format!("{value:.9}"),
            ParamValue::Parameter(id) => {
                format!("parameter({})", store.parameters()[id.0 as usize].name)
            }
            ParamValue::Expr(id) => describe_angle(store, *id),
        })
        .collect();
    lines.push(format!(
        "{pad}{} qubits={qubits:?} clbits={clbits:?} params=[{}]{}",
        instruction.operation.name(),
        params.join(", "),
        describe_control(store, instruction)
    ));

    let PackedOperation::ControlFlow(control) = &instruction.operation else {
        return;
    };
    for (position, block) in control.blocks().into_iter().enumerate() {
        lines.push(format!("{pad}  body {position}"));
        if let Some(body) = store.block(block) {
            for nested in &body.instructions {
                describe_instruction(store, nested, depth + 2, lines);
            }
        }
    }
}

/// Describes what a control-flow construct carries besides its bodies.
fn describe_control(store: &CircuitStore, instruction: &qsc_circuit_store::Instruction) -> String {
    let PackedOperation::ControlFlow(control) = &instruction.operation else {
        if let PackedOperation::Delay { duration, unit } = &instruction.operation {
            return format!(" duration={duration}{unit}");
        }
        return String::new();
    };
    match control.as_ref() {
        ControlFlow::IfElse(if_else) => {
            format!(" on={}", describe_condition(store, if_else.condition))
        }
        ControlFlow::While(loop_) => {
            format!(" on={}", describe_condition(store, loop_.condition))
        }
        ControlFlow::For(loop_) => {
            let (name, ty) = match loop_.index {
                Some(LoopIndex::Var(id)) => {
                    let var = &store.vars()[id.0 as usize];
                    (var.name.clone(), var.ty)
                }
                Some(LoopIndex::Parameter(id)) => (
                    store.parameters()[id.0 as usize].name.clone(),
                    loop_index_type(&loop_.values),
                ),
                None => ("<anonymous>".to_string(), loop_index_type(&loop_.values)),
            };
            format!(" index={name}:{ty:?} over={:?}", loop_.values)
        }
        ControlFlow::Switch(switch) => {
            let labels: Vec<String> = switch
                .cases
                .iter()
                .map(|(values, _)| format!("{values:?}"))
                .collect();
            format!(
                " on={} cases=[{}] default={}",
                describe_condition(store, switch.selector),
                labels.join(", "),
                switch.default.is_some()
            )
        }
        ControlFlow::Box(_)
        | ControlFlow::Moment(_)
        | ControlFlow::Break
        | ControlFlow::Continue => String::new(),
    }
}

fn loop_index_type(values: &[Literal]) -> VarType {
    if values
        .iter()
        .any(|value| matches!(value, Literal::Float(_)))
    {
        VarType::Float
    } else if !values.is_empty() && values.iter().all(|value| matches!(value, Literal::Bool(_))) {
        VarType::Bool
    } else {
        VarType::Int
    }
}

fn describe_angle(store: &CircuitStore, id: qsc_circuit_store::AngleId) -> String {
    match store.angles().get(id).expect("angle expression exists") {
        AngleExpr::Literal(value) => format!("{value:.9}"),
        AngleExpr::Parameter(id) => {
            format!("parameter({})", store.parameters()[id.0 as usize].name)
        }
        AngleExpr::Neg(inner) => format!("neg({})", describe_angle(store, *inner)),
        AngleExpr::Binary(op, left, right) => format!(
            "{op:?}({}, {})",
            describe_angle(store, *left),
            describe_angle(store, *right),
        ),
    }
}

/// Renders a condition structurally.
///
/// Written here rather than reusing the writer, so that a comparison across a
/// round trip does not depend on the code being tested to describe itself.
fn describe_condition(store: &CircuitStore, expr: qsc_circuit_store::ExprId) -> String {
    match store.exprs().get(expr) {
        None => "missing".to_string(),
        Some(Expr::Literal(value)) => format!("{value:?}"),
        Some(Expr::Var(id)) => format!("var({})", store.vars()[id.0 as usize].name),
        Some(Expr::Clbit(index)) => format!("clbit({index})"),
        Some(Expr::Not(inner)) => format!("not({})", describe_condition(store, *inner)),
        Some(Expr::Binary(op, left, right)) => format!(
            "{op:?}({}, {})",
            describe_condition(store, *left),
            describe_condition(store, *right)
        ),
    }
}

/// Asserts that text is valid OpenQASM according to the front end.
fn assert_valid_qasm(text: &str) {
    let parsed = qdk_openqasm::analyze_and_resolve(
        text.to_string(),
        "exported.qasm",
        None::<&mut qdk_openqasm::io::InMemorySourceResolver>,
    );
    assert!(
        !parsed.has_errors(),
        "exported source is not valid OpenQASM: {:?}\n{text}",
        parsed.all_errors()
    );
}

/// Exports a circuit, checking the result against the front end that reads it.
///
/// Text that merely looks right is not evidence, so every export in these tests
/// goes through here rather than through `circuit_to_qasm` directly.
fn export(store: &CircuitStore) -> String {
    let text = circuit_to_qasm(store).expect("circuit should export");
    assert_valid_qasm(&text);
    text
}

/// Round-trips a circuit through OpenQASM and asserts the content survives.
fn round_trip(store: &CircuitStore) -> CircuitStore {
    let text = export(store);
    let back = circuit_from_qasm(&text)
        .unwrap_or_else(|error| panic!("circuit should import: {error}\n{text}"));
    assert_eq!(describe(&back), describe(store), "content differs\n{text}");
    assert_eq!(back.qubit_count(), store.qubit_count());
    assert_eq!(back.clbit_count(), store.clbit_count());
    back
}

fn symbolic_angle_fixture(op: AngleOp, value: f64, parameter: &str, padding: bool) -> CircuitStore {
    let mut store = CircuitStore::new(1, 0);
    if padding {
        store.declare_parameter("unused");
        store.push_angle(AngleExpr::Literal(19.0));
    }
    let parameter = store.declare_parameter(parameter);
    let left = store.push_angle(AngleExpr::Parameter(parameter));
    let right = store.push_angle(AngleExpr::Literal(value));
    let expression = store.push_angle(AngleExpr::Binary(op, left, right));
    let expression = store.push_angle(AngleExpr::Neg(expression));
    store
        .push(
            PackedOperation::Standard(StandardGate::Rz),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Expr(expression)])),
        )
        .expect("symbolic gate");
    store
}

#[test]
fn round_trip_description_distinguishes_angle_operators_with_equal_arena_ids() {
    assert_ne!(
        describe(&symbolic_angle_fixture(AngleOp::Add, 2.0, "theta", false)),
        describe(&symbolic_angle_fixture(AngleOp::Mul, 2.0, "theta", false)),
    );
}

#[test]
fn round_trip_description_distinguishes_angle_literals_with_equal_arena_ids() {
    assert_ne!(
        describe(&symbolic_angle_fixture(AngleOp::Add, 2.0, "theta", false)),
        describe(&symbolic_angle_fixture(AngleOp::Add, 3.0, "theta", false)),
    );
}

#[test]
fn round_trip_description_distinguishes_symbolic_parameter_names() {
    assert_ne!(
        describe(&symbolic_angle_fixture(AngleOp::Add, 2.0, "theta", false)),
        describe(&symbolic_angle_fixture(AngleOp::Add, 2.0, "phi", false)),
    );
}

#[test]
fn round_trip_description_ignores_symbolic_arena_allocation_order() {
    assert_eq!(
        describe(&symbolic_angle_fixture(AngleOp::Add, 2.0, "theta", false)),
        describe(&symbolic_angle_fixture(AngleOp::Add, 2.0, "theta", true)),
    );
}

fn loop_index_fixture(name: &str, ty: VarType) -> CircuitStore {
    let mut store = CircuitStore::new(1, 0);
    let index = store.declare_var(name, ty);
    let body = store.new_block();
    store
        .push_control_flow(ControlFlow::For(ForLoop {
            index: Some(LoopIndex::Var(index)),
            values: vec![Literal::Int(1)],
            body,
        }))
        .expect("loop");
    store
}

#[test]
fn round_trip_description_distinguishes_loop_index_names() {
    assert_ne!(
        describe(&loop_index_fixture("i", VarType::Int)),
        describe(&loop_index_fixture("j", VarType::Int)),
    );
}

#[test]
fn round_trip_description_distinguishes_loop_index_types() {
    assert_ne!(
        describe(&loop_index_fixture("i", VarType::Int)),
        describe(&loop_index_fixture("i", VarType::Float)),
    );
}

/// Appends a one-qubit gate on qubit 0 into a block.
fn gate_in(store: &mut CircuitStore, block: qsc_circuit_store::BlockId, gate: StandardGate) {
    store
        .push_into_block(
            block,
            PackedOperation::Standard(gate),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("gate should be accepted");
}

fn bell() -> CircuitStore {
    let mut store = CircuitStore::new(2, 2);
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("gate should be accepted");
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("gate should be accepted");
    for index in 0..2 {
        store
            .push(
                PackedOperation::Standard(StandardGate::MeasureZ),
                &[Qubit(index)],
                &[Clbit(index)],
                None,
            )
            .expect("measurement should be accepted");
    }
    store
}

#[test]
fn a_circuit_exports_as_openqasm() {
    let text = export(&bell());

    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[2] q;
        bit[2] c;
        h q[0];
        cx q[0], q[1];
        c[0] = measure q[0];
        c[1] = measure q[1];
    "#]]
    .assert_eq(&text);
}

#[test]
fn a_bell_pair_round_trips() {
    round_trip(&bell());
}

#[test]
fn an_empty_circuit_round_trips() {
    round_trip(&CircuitStore::new(3, 2));
}

#[test]
fn every_gate_exports() {
    let mut exported = Vec::new();
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
                    .map(|index| ParamValue::Literal(f64::from(index) + 0.5))
                    .collect(),
            )
        } else {
            None
        };
        let mut single = CircuitStore::new(3, 1);
        single
            .push(PackedOperation::Standard(gate), &qubits, &clbits, params)
            .expect("gate should be accepted");
        if circuit_to_qasm(&single).is_ok() {
            exported.push(gate.name());
        }
    }

    // Every gate reaches OpenQASM, either under its own name or through a
    // rewrite into gates OpenQASM can name.
    assert_eq!(exported.len(), StandardGate::all().len(), "{exported:?}");
}

#[test]
fn every_natively_spelled_gate_round_trips() {
    let mut store = CircuitStore::new(3, 1);
    let mut spelled = Vec::new();
    for gate in StandardGate::all() {
        // A gate exported through a rewrite comes back as the rewrite, so only
        // the ones OpenQASM names directly can round-trip to themselves.
        if !matches!(super::qasm_spelling(gate), super::Spelling::Gate(_)) {
            continue;
        }
        let qubits: Vec<Qubit> = (0..u32::from(gate.qubit_arity())).map(Qubit).collect();
        let params: Option<Box<[ParamValue]>> = if gate.param_arity() > 0 {
            Some(
                (0..gate.param_arity())
                    .map(|index| ParamValue::Literal(f64::from(index) + 0.5))
                    .collect(),
            )
        } else {
            None
        };
        spelled.push(gate.name());
        store
            .push(PackedOperation::Standard(gate), &qubits, &[], params)
            .expect("gate should be accepted");
    }

    // OpenQASM spells several gates that have no QIR intrinsic, so the set named
    // directly here is wider than the set with an intrinsic.
    expect![[
        "i, x, y, z, h, s, sdg, sx, sxdg, t, tdg, cx, cy, cz, swap, ccx, cswap, rx, ry, rz, rxx, ryy, rzz, crx, cry, crz"
    ]]
    .assert_eq(&spelled.join(", "));

    round_trip(&store);
}

#[test]
fn preparing_the_zero_state_comes_back_as_a_reset() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push_standard(StandardGate::PrepareZ, &[Qubit(0)])
        .expect("gate should be accepted");

    let text = export(&store);
    let back = circuit_from_qasm(&text).expect("source should import");

    // Preparing the Z basis state and resetting are the same operation, and
    // OpenQASM has one spelling for it, so the two collapse into one on the way
    // back. The program is unchanged; only which of two equivalent names it
    // carries is.
    assert_eq!(
        describe(&back),
        vec!["reset qubits=[0] clbits=[] params=[]"]
    );
}

#[test]
fn rotations_keep_their_angles() {
    let mut store = CircuitStore::new(2, 0);
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Literal(0.25)])),
        )
        .expect("gate should be accepted");
    store
        .push(
            PackedOperation::Standard(StandardGate::Crz),
            &[Qubit(0), Qubit(1)],
            &[],
            Some(Box::new([ParamValue::Literal(-1.5)])),
        )
        .expect("gate should be accepted");

    let back = circuit_from_qasm(&export(&store)).expect("source should import");

    let angles: Vec<f64> = back
        .instructions()
        .filter_map(|instruction| match instruction.params.as_deref() {
            Some([ParamValue::Literal(value)]) => Some(*value),
            _ => None,
        })
        .collect();
    assert_eq!(angles.len(), 2);
    // The source angle type is a fraction of a turn, not an unbounded float.
    for (restored, original) in angles.iter().zip([0.25_f64, std::f64::consts::TAU - 1.5]) {
        assert!(
            (restored - original).abs() < 1e-12,
            "{restored} is not close enough to {original}"
        );
    }
}

#[test]
fn an_angle_is_only_approximately_preserved() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Literal(0.25)])),
        )
        .expect("gate should be accepted");

    let text = export(&store);
    let back = circuit_from_qasm(&text).expect("source should import");
    let restored = match back.instructions().next().and_then(|i| i.params.clone()) {
        Some(params) => match params[0] {
            ParamValue::Literal(value) => value,
            ParamValue::Parameter(_) | ParamValue::Expr(_) => panic!("expected a literal angle"),
        },
        None => panic!("expected an angle"),
    };

    // OpenQASM holds an angle as a fixed-point fraction of a turn rather than as
    // a float, so a round trip lands on the nearest representable angle instead
    // of the value that was written. This is a property of the format, and it is
    // pinned here so that it stays a known limit rather than a surprise.
    assert_ne!(restored.to_bits(), 0.25_f64.to_bits());
    assert!((restored - 0.25).abs() < 1e-15, "{restored}");
}

#[test]
fn reset_and_barrier_round_trip() {
    let mut store = CircuitStore::new(2, 0);
    store
        .push(PackedOperation::Reset, &[Qubit(0)], &[], None)
        .expect("operation should be accepted");
    store
        .push(
            PackedOperation::Barrier { qubit_arity: 2 },
            &[Qubit(0), Qubit(1)],
            &[],
            None,
        )
        .expect("operation should be accepted");

    round_trip(&store);
}

#[test]
fn a_conditional_exports_as_an_if_statement() {
    let mut store = CircuitStore::new(2, 1);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");
    let left = store.push_expr(Expr::Clbit(0));
    let right = store.push_expr(Expr::Literal(Literal::Int(1)));
    let condition = store.push_expr(Expr::Binary(BinaryOp::Eq, left, right));
    let then_block = store.new_block();
    store
        .push_into_block(
            then_block,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(1)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: None,
        }))
        .expect("construct should be accepted");

    let text = export(&store);

    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[2] q;
        bit[1] c;
        c[0] = measure q[0];
        if (c[0] == 1) {
          x q[1];
        }
    "#]]
    .assert_eq(&text);
}

#[test]
fn a_box_exports_as_a_box() {
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
        .expect("gate should be accepted");
    store
        .push_control_flow(ControlFlow::Box(BoxBody {
            body,
            duration: None,
            label: None,
        }))
        .expect("construct should be accepted");

    let text = export(&store);

    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[1] q;
        box {
          h q[0];
        }
    "#]]
    .assert_eq(&text);
}

#[test]
fn a_chain_of_conditionals_exports_as_nested_ifs() {
    // if c[0] { x } else if c[1] { y } else { h }
    let mut store = CircuitStore::new(2, 2);
    let inner_condition = store.push_expr(Expr::Clbit(1));
    let inner_then = store.new_block();
    store
        .push_into_block(
            inner_then,
            PackedOperation::Standard(StandardGate::Y),
            &[Qubit(1)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    let inner_else = store.new_block();
    store
        .push_into_block(
            inner_else,
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(1)],
            &[],
            None,
        )
        .expect("gate should be accepted");

    let outer_condition = store.push_expr(Expr::Clbit(0));
    let outer_then = store.new_block();
    store
        .push_into_block(
            outer_then,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(1)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    let outer_else = store.new_block();
    store
        .push_control_flow_into_block(
            outer_else,
            ControlFlow::IfElse(IfElse {
                condition: inner_condition,
                then_block: inner_then,
                else_block: Some(inner_else),
            }),
        )
        .expect("construct should be accepted");
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition: outer_condition,
            then_block: outer_then,
            else_block: Some(outer_else),
        }))
        .expect("construct should be accepted");

    let text = export(&store);

    // A circuit nests the next link inside the alternative branch rather than
    // recording an else-if form of its own, so that is what is written.
    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[2] q;
        bit[2] c;
        if (c[0] == 1) {
          x q[1];
        } else {
          if (c[1] == 1) {
            y q[1];
          } else {
            h q[1];
          }
        }
    "#]]
    .assert_eq(&text);
}

#[test]
fn a_name_that_is_not_an_identifier_is_refused() {
    let mut store = CircuitStore::new(2, 0);
    // A name reaches a circuit from its builder and from saved circuits, so it
    // is arbitrary text. Written straight into the output it would carry
    // punctuation and become further statements, so it is refused instead.
    let theta = store.declare_parameter("q[1]; x q[0]; rx(0.0");
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Parameter(theta)])),
        )
        .expect("gate should be accepted");

    let error = circuit_to_qasm(&store).expect_err("export should be refused");

    assert_eq!(
        error,
        Error::UnwritableName {
            name: "q[1]; x q[0]; rx(0.0".to_string()
        }
    );
}

#[test]
fn a_name_shadowing_a_register_is_refused() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("q");
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Parameter(theta)])),
        )
        .expect("gate should be accepted");

    let error = circuit_to_qasm(&store).expect_err("export should be refused");

    assert_eq!(
        error,
        Error::UnwritableName {
            name: "q".to_string()
        }
    );
}

#[test]
fn an_ordinary_name_is_written_as_itself() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("theta_1");
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Parameter(theta)])),
        )
        .expect("gate should be accepted");

    // A parameter is declared as an input, so the exported program is a
    // parameterised one rather than one referring to an undefined name.
    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        input float theta_1;
        qubit[1] q;
        rx(theta_1) q[0];
    "#]]
    .assert_eq(&export(&store));
}

#[test]
fn an_unreferenced_parameter_is_not_declared() {
    let mut store = CircuitStore::new(1, 0);
    store.declare_parameter("unused");
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("gate should be accepted");

    // Declaring an input nothing reads would make the exported program ask for
    // a value it never uses.
    assert!(!export(&store).contains("input"));
}

#[test]
fn a_switch_is_written_with_its_cases_and_default() {
    let mut store = CircuitStore::new(1, 1);
    let selector = store.push_expr(Expr::Clbit(0));
    let first = store.new_block();
    gate_in(&mut store, first, StandardGate::X);
    let second = store.new_block();
    gate_in(&mut store, second, StandardGate::Z);
    let fallback = store.new_block();
    gate_in(&mut store, fallback, StandardGate::H);
    store
        .push_control_flow(ControlFlow::Switch(Switch {
            selector,
            cases: vec![
                (vec![Literal::Int(0)], first),
                (vec![Literal::Int(1), Literal::Int(2)], second),
            ],
            default: Some(fallback),
        }))
        .expect("construct should be accepted");

    expect![[r#"
        OPENQASM 3.1;
        include "stdgates.inc";
        qubit[1] q;
        bit[1] c;
        switch (c[0]) {
          case 0 {
            x q[0];
          }
          case 1, 2 {
            z q[0];
          }
          default {
            h q[0];
          }
        }
    "#]]
    .assert_eq(&export(&store));
}

#[test]
fn a_basis_other_than_z_exports_as_a_rewrite() {
    let mut store = CircuitStore::new(1, 1);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureX),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    // OpenQASM measures in the Z basis only, but it can express the rotation
    // that puts another basis there and back, so the rewrite is written rather
    // than the export being refused.
    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[1] q;
        bit[1] c;
        h q[0];
        c[0] = measure q[0];
        h q[0];
    "#]]
    .assert_eq(&export(&store));
}

#[test]
fn preparing_another_basis_exports_as_a_rewrite() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push_standard(StandardGate::PrepareY, &[Qubit(0)])
        .expect("gate should be accepted");

    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[1] q;
        reset q[0];
        h q[0];
        s q[0];
    "#]]
    .assert_eq(&export(&store));
}

#[test]
fn a_delay_is_written_with_its_time_unit() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push(
            PackedOperation::Delay {
                duration: 5.0,
                unit: TimeUnit::Ns,
            },
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("operation should be accepted");

    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[1] q;
        delay[5ns] q[0];
    "#]]
    .assert_eq(&export(&store));
}

#[test]
fn a_delay_of_no_number_is_refused() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push(
            PackedOperation::Delay {
                duration: f64::INFINITY,
                unit: TimeUnit::S,
            },
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("operation should be accepted");

    let error = circuit_to_qasm(&store).expect_err("export should be refused");

    // The store admits a duration JSON cannot spell, so the writer says so
    // rather than emitting text no reader accepts.
    assert_eq!(
        error,
        Error::Unsupported {
            construct: "delay of a duration that is not a number"
        }
    );
}

#[test]
fn separate_registers_are_laid_out_end_to_end() {
    let store = circuit_from_qasm(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[2] a;
        qubit b;
        bit[2] c;
        h a[0];
        cx a[1], b;
        c[0] = measure a[0];
        c[1] = measure b;
        "#,
    )
    .expect("source should import");

    // OpenQASM keeps registers separate while a circuit addresses qubits by a
    // single dense index, so registers are laid out in declaration order.
    assert_eq!(store.qubit_count(), 3);
    assert_eq!(
        describe(&store),
        vec![
            "h qubits=[0] clbits=[] params=[]",
            "cx qubits=[1, 2] clbits=[] params=[]",
            "measure_z qubits=[0] clbits=[0] params=[]",
            "measure_z qubits=[2] clbits=[1] params=[]",
        ]
    );
}

#[test]
fn a_declaration_initialised_by_a_measurement_is_a_measurement() {
    let store = circuit_from_qasm(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit q;
        bit c = measure q;
        "#,
    )
    .expect("source should import");

    assert_eq!(
        describe(&store),
        vec!["measure_z qubits=[0] clbits=[0] params=[]"]
    );
}

#[test]
fn a_whole_register_measurement_expands() {
    let store = circuit_from_qasm(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[3] q;
        bit[3] c;
        c = measure q;
        "#,
    )
    .expect("source should import");

    assert_eq!(
        describe(&store),
        vec![
            "measure_z qubits=[0] clbits=[0] params=[]",
            "measure_z qubits=[1] clbits=[1] params=[]",
            "measure_z qubits=[2] clbits=[2] params=[]",
        ]
    );
}

#[test]
fn a_gate_outside_the_circuit_set_is_named() {
    let error = circuit_from_qasm(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit q;
        u3(0.1, 0.2, 0.3) q;
        "#,
    )
    .expect_err("source should not import");

    assert_eq!(
        error,
        Error::UnsupportedImport {
            construct: "gate 'u3'".to_string()
        }
    );
}

#[test]
fn a_construct_outside_a_circuit_is_named() {
    let error = circuit_from_qasm(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit q;
        def flip(qubit target) { x target; }
        "#,
    )
    .expect_err("source should not import");

    assert_eq!(
        error,
        Error::UnsupportedImport {
            construct: "subroutine definition".to_string()
        }
    );
}

#[test]
fn source_that_does_not_parse_is_reported() {
    let error = circuit_from_qasm("this is not openqasm").expect_err("source should not import");

    assert!(matches!(error, Error::Parse(_)), "{error:?}");
}

/// A circuit holding all seven named control-flow constructs.
///
/// Built as one circuit rather than seven, because the requirement is that a
/// circuit containing all of them survives, and constructs that only work in
/// isolation would satisfy seven separate tests.
fn every_construct() -> CircuitStore {
    let mut store = CircuitStore::new(2, 2);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    let left = store.push_expr(Expr::Clbit(0));
    let right = store.push_expr(Expr::Literal(Literal::Int(1)));
    let condition = store.push_expr(Expr::Binary(BinaryOp::Eq, left, right));
    let then_block = store.new_block();
    gate_in(&mut store, then_block, StandardGate::X);
    let else_block = store.new_block();
    gate_in(&mut store, else_block, StandardGate::Z);
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: Some(else_block),
        }))
        .expect("construct should be accepted");

    let body = store.new_block();
    gate_in(&mut store, body, StandardGate::H);
    store
        .push_control_flow_into_block(body, ControlFlow::Break)
        .expect("construct should be accepted");
    store
        .push_control_flow(ControlFlow::While(WhileLoop { condition, body }))
        .expect("construct should be accepted");

    let counted = store.declare_var("step".to_string(), VarType::Int);
    let iterated = store.new_block();
    store
        .push_control_flow_into_block(iterated, ControlFlow::Continue)
        .expect("construct should be accepted");
    store
        .push_control_flow(ControlFlow::For(ForLoop {
            index: Some(LoopIndex::Var(counted)),
            values: vec![Literal::Int(0), Literal::Int(1)],
            body: iterated,
        }))
        .expect("construct should be accepted");

    let selector = store.push_expr(Expr::Clbit(1));
    let matched = store.new_block();
    gate_in(&mut store, matched, StandardGate::Y);
    let fallback = store.new_block();
    gate_in(&mut store, fallback, StandardGate::S);
    store
        .push_control_flow(ControlFlow::Switch(Switch {
            selector,
            cases: vec![(vec![Literal::Int(0), Literal::Int(1)], matched)],
            default: Some(fallback),
        }))
        .expect("construct should be accepted");

    let grouped = store.new_block();
    gate_in(&mut store, grouped, StandardGate::T);
    store
        .push_control_flow(ControlFlow::Box(BoxBody {
            body: grouped,
            duration: None,
            label: None,
        }))
        .expect("construct should be accepted");
    store
}

#[test]
fn mixed_control_flow_fixture_round_trips() {
    let store = every_construct();

    let back = round_trip(&store);

    let names: Vec<&str> = back
        .instructions()
        .map(|instruction| instruction.operation.name())
        .collect();
    assert_eq!(
        names,
        ["measure_z", "if_else", "while", "for", "switch", "box"],
        "every construct must come back, in order"
    );
}

#[test]
fn switch_round_trip_preserves_unsorted_distinct_case_labels() {
    // Distinct labels deliberately appear out of numeric order.
    let mut store = CircuitStore::new(1, 1);
    let selector = store.push_expr(Expr::Clbit(0));
    let later = store.new_block();
    gate_in(&mut store, later, StandardGate::Z);
    let earlier = store.new_block();
    gate_in(&mut store, earlier, StandardGate::X);
    store
        .push_control_flow(ControlFlow::Switch(Switch {
            selector,
            cases: vec![
                (vec![Literal::Int(3)], later),
                (vec![Literal::Int(1)], earlier),
            ],
            default: None,
        }))
        .expect("construct should be accepted");

    let back = round_trip(&store);

    let PackedOperation::ControlFlow(control) = &back
        .instruction(0)
        .expect("the switch should be present")
        .operation
    else {
        panic!("the instruction should be a control-flow construct");
    };
    let ControlFlow::Switch(switch) = control.as_ref() else {
        panic!("the construct should be a switch");
    };
    let labels: Vec<&Vec<Literal>> = switch.cases.iter().map(|(values, _)| values).collect();
    assert_eq!(labels, [&vec![Literal::Int(3)], &vec![Literal::Int(1)]]);
}

#[test]
fn a_delay_round_trips_with_its_unit() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push(
            PackedOperation::Delay {
                duration: 100.0,
                unit: TimeUnit::Ns,
            },
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("operation should be accepted");

    let back = round_trip(&store);

    assert_eq!(
        back.instruction(0)
            .expect("the delay should be present")
            .operation,
        PackedOperation::Delay {
            duration: 100.0,
            unit: TimeUnit::Ns
        }
    );
}

#[test]
fn a_moment_comes_back_as_a_box() {
    // OpenQASM has no way to assert simultaneity, so the grouping survives and
    // the claim does not. Pinned rather than left to be discovered.
    let mut store = CircuitStore::new(2, 0);
    let together = store.new_block();
    gate_in(&mut store, together, StandardGate::H);
    store
        .push_control_flow(ControlFlow::Moment(together))
        .expect("construct should be accepted");

    let text = export(&store);
    let back = circuit_from_qasm(&text).expect("circuit should import");

    assert_eq!(
        back.instruction(0)
            .expect("the construct should be present")
            .operation
            .name(),
        "box"
    );
}

#[test]
fn a_compile_time_loop_index_comes_back_as_a_runtime_one() {
    // OpenQASM has one kind of loop variable and it is read while the program
    // runs, so the distinction a circuit draws does not survive the crossing.
    let mut store = CircuitStore::new(1, 0);
    let index = store.declare_parameter("step".to_string());
    let body = store.new_block();
    gate_in(&mut store, body, StandardGate::X);
    store
        .push_control_flow(ControlFlow::For(ForLoop {
            index: Some(LoopIndex::Parameter(index)),
            values: vec![Literal::Int(0), Literal::Int(1)],
            body,
        }))
        .expect("construct should be accepted");

    let back = round_trip(&store);

    let PackedOperation::ControlFlow(control) = &back
        .instruction(0)
        .expect("the loop should be present")
        .operation
    else {
        panic!("the instruction should be a control-flow construct");
    };
    let ControlFlow::For(loop_) = control.as_ref() else {
        panic!("the construct should be a for loop");
    };
    assert!(
        matches!(loop_.index, Some(LoopIndex::Var(_))),
        "an imported loop index is read at run time"
    );
}

#[test]
fn a_body_that_was_altered_does_not_pass_as_the_original() {
    // Evidence that the comparison sees inside a construct at all. Without the
    // nested comparison, changing a body would round-trip silently.
    let store = every_construct();
    let text = export(&store);
    let corrupted = text.replace("y q[0];", "s q[0];");
    assert_ne!(corrupted, text, "the case body should have been altered");

    let back = circuit_from_qasm(&corrupted).expect("altered source should still import");

    assert_ne!(
        describe(&back),
        describe(&store),
        "a changed case body must not compare equal"
    );
}

#[test]
fn a_switch_case_naming_something_a_circuit_cannot_hold_is_named() {
    let error = circuit_from_qasm(
        r#"
        OPENQASM 3.1;
        include "stdgates.inc";
        qubit[1] q;
        bit[1] c;
        switch (c[0]) {
          case 0 {
            u3(0.1, 0.2, 0.3) q[0];
          }
        }
        "#,
    )
    .expect_err("source should not import");

    assert_eq!(
        error,
        Error::UnsupportedImport {
            construct: "gate 'u3'".to_string()
        }
    );
}

#[test]
fn a_loop_over_a_range_is_named() {
    let error = circuit_from_qasm(
        r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[1] q;
        for int step in [0:2] {
          x q[0];
        }
        "#,
    )
    .expect_err("source should not import");

    assert_eq!(
        error,
        Error::UnsupportedImport {
            construct: "loop over anything but a listed set".to_string()
        }
    );
}

/// A circuit exercising gates, angles, timing, measurement, and control flow.
fn many_constructs() -> CircuitStore {
    let mut store = CircuitStore::new(3, 2);
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("gate should be accepted");
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("gate should be accepted");
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(2)],
            &[],
            Some(Box::new([ParamValue::Literal(0.5)])),
        )
        .expect("gate should be accepted");
    store
        .push_standard(StandardGate::Cswap, &[Qubit(0), Qubit(1), Qubit(2)])
        .expect("gate should be accepted");
    store
        .push(
            PackedOperation::Barrier { qubit_arity: 3 },
            &[Qubit(0), Qubit(1), Qubit(2)],
            &[],
            None,
        )
        .expect("operation should be accepted");
    store
        .push(PackedOperation::Reset, &[Qubit(2)], &[], None)
        .expect("operation should be accepted");
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    let grouped = store.new_block();
    store
        .push_into_block(
            grouped,
            PackedOperation::Standard(StandardGate::SqrtXDag),
            &[Qubit(1)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    store
        .push_control_flow(ControlFlow::Box(BoxBody {
            body: grouped,
            duration: None,
            label: None,
        }))
        .expect("construct should be accepted");

    let left = store.push_expr(Expr::Clbit(0));
    let right = store.push_expr(Expr::Literal(Literal::Int(1)));
    let condition = store.push_expr(Expr::Binary(BinaryOp::Eq, left, right));
    let then_block = store.new_block();
    store
        .push_into_block(
            then_block,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(1)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    let else_block = store.new_block();
    store
        .push_into_block(
            else_block,
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(1)],
            &[Clbit(1)],
            None,
        )
        .expect("measurement should be accepted");
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: Some(else_block),
        }))
        .expect("construct should be accepted");
    store
}

#[test]
fn a_circuit_using_many_constructs_exports_completely() {
    // Exported through the helper, so this text is known to analyse cleanly
    // rather than merely to look plausible.
    let text = export(&many_constructs());

    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[3] q;
        bit[2] c;
        h q[0];
        cx q[0], q[1];
        rx(0.5) q[2];
        cswap q[0], q[1], q[2];
        barrier q[0], q[1], q[2];
        reset q[2];
        c[0] = measure q[0];
        box {
          sxdg q[1];
        }
        if (c[0] == 1) {
          x q[1];
        } else {
          c[1] = measure q[1];
        }
    "#]]
    .assert_eq(&text);
}

#[test]
fn a_for_loop_is_written_with_its_index() {
    let mut store = CircuitStore::new(1, 0);
    let index = store.declare_parameter("step");
    let node = store.push_angle(AngleExpr::Parameter(index));
    let two = store.push_angle(AngleExpr::Literal(2.0));
    let doubled = store.push_angle(AngleExpr::Binary(AngleOp::Mul, node, two));
    let body = store.new_block();
    store
        .push_into_block(
            body,
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Expr(doubled)])),
        )
        .expect("gate should be accepted");
    store
        .push_control_flow(ControlFlow::For(ForLoop {
            index: Some(LoopIndex::Parameter(index)),
            values: vec![Literal::Float(0.25), Literal::Float(0.5)],
            body,
        }))
        .expect("construct should be accepted");

    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[1] q;
        for float step in {0.25, 0.5} {
          rx((step * 2)) q[0];
        }
    "#]]
    .assert_eq(&export(&store));
}

#[test]
fn a_for_loop_over_whole_numbers_declares_an_integer_index() {
    let mut store = CircuitStore::new(1, 1);
    let index = store.declare_var("step", qsc_circuit_store::VarType::Int);
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
    let left = store.push_expr(Expr::Var(index));
    let right = store.push_expr(Expr::Literal(Literal::Int(1)));
    let condition = store.push_expr(Expr::Binary(BinaryOp::Eq, left, right));
    let body = store.new_block();
    store
        .push_control_flow_into_block(
            body,
            ControlFlow::IfElse(IfElse {
                condition,
                then_block,
                else_block: None,
            }),
        )
        .expect("construct should be accepted");
    store
        .push_control_flow(ControlFlow::For(ForLoop {
            index: Some(LoopIndex::Var(index)),
            values: vec![Literal::Int(0), Literal::Int(1), Literal::Int(2)],
            body,
        }))
        .expect("construct should be accepted");

    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit[1] q;
        bit[1] c;
        for int step in {0, 1, 2} {
          if (step == 1) {
            x q[0];
          }
        }
    "#]]
    .assert_eq(&export(&store));
}

#[test]
fn an_angle_expression_is_written_unambiguously() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("theta");
    let base = store.push_angle(AngleExpr::Parameter(theta));
    let one = store.push_angle(AngleExpr::Literal(1.0));
    let sum = store.push_angle(AngleExpr::Binary(AngleOp::Add, base, one));
    let two = store.push_angle(AngleExpr::Literal(2.0));
    let scaled = store.push_angle(AngleExpr::Binary(AngleOp::Mul, sum, two));
    let negated = store.push_angle(AngleExpr::Neg(scaled));
    store
        .push(
            PackedOperation::Standard(StandardGate::Rz),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Expr(negated)])),
        )
        .expect("gate should be accepted");

    // Parenthesised throughout, so the reader's precedence rules cannot change
    // what the angle means.
    expect![[r#"
        OPENQASM 3.0;
        include "stdgates.inc";
        input float theta;
        qubit[1] q;
        rz(-(((theta + 1) * 2))) q[0];
    "#]]
    .assert_eq(&export(&store));
}
