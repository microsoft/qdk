// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for lowering a circuit straight to the runtime IR and QIR.
//!
//! The point of these is to establish whether the direct path works at all, so
//! they assert on emitted QIR rather than on intermediate structure.

use expect_test::expect;
use qsc_circuit_store::{
    AngleExpr, AngleOp, BinaryOp, BoxBody, CircuitStore, Clbit, ControlFlow, Expr, ForLoop, IfElse,
    Literal, LoopIndex, PackedOperation, ParamValue, Qubit, StandardGate, Switch, TimeUnit,
    VarType, WhileLoop,
};
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};

use super::{Error, circuit_to_qir};

fn branch_on(store: &mut CircuitStore, condition: qsc_circuit_store::ExprId) {
    let then_block = store.new_block();
    store
        .push_into_block(
            then_block,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(2)],
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
        .expect("conditional should be accepted");
}

fn assert_measured_order(op: BinaryOp, left: bool, right: bool, expected: bool) {
    #[cfg(not(unix))]
    let _ = expected;
    let mut store = CircuitStore::new(3, 3);
    for (index, value) in [(0, left), (1, right)] {
        if value {
            store
                .push_standard(StandardGate::X, &[Qubit(index)])
                .expect("preparation should append");
        }
        store
            .push(
                PackedOperation::Standard(StandardGate::MeasureZ),
                &[Qubit(index)],
                &[Clbit(index)],
                None,
            )
            .expect("measurement should append");
    }
    let left_expr = store.push_expr(Expr::Clbit(0));
    let right_expr = store.push_expr(Expr::Clbit(1));
    let condition = store.push_expr(Expr::Binary(op, left_expr, right_expr));
    branch_on(&mut store, condition);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(2)],
            &[Clbit(2)],
            None,
        )
        .expect("output measurement should append");
    let qir = circuit_to_qir(&store, adaptive_ri()).expect("ordered bits should lower");
    #[cfg(unix)]
    assert_native_bits(
        &qir,
        &format!(
            "{}{}{}",
            u8::from(left),
            u8::from(right),
            u8::from(expected)
        ),
    );
    for code in ["slt", "sle", "sgt", "sge"] {
        assert!(!qir.contains(&format!("icmp {code} i1 ")), "{qir}");
    }
}

#[cfg(unix)]
pub(crate) fn assert_native_bits(qir: &str, expected: &str) {
    use std::{
        fs,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "qdk-circuit-native-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).expect("native test directory should be created");
    let module = dir.join("module.ll");
    let runtime = dir.join("runtime.c");
    let executable = dir.join("circuit");
    fs::write(&module, qir).expect("QIR fixture should be written");
    fs::write(
        &runtime,
        r#"
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
static bool qubits[128], results[128];
void __quantum__rt__initialize(void *p) { (void)p; }
void __quantum__qis__x__body(uintptr_t q) { qubits[q] = !qubits[q]; }
void __quantum__qis__cx__body(uintptr_t control, uintptr_t target) {
    if (qubits[control]) qubits[target] = !qubits[target];
}
void __quantum__qis__m__body(uintptr_t q, uintptr_t r) { results[r] = qubits[q]; }
bool __quantum__rt__read_result(uintptr_t r) { return results[r]; }
void __quantum__rt__array_record_output(int64_t n, void *p) { (void)n; (void)p; }
void __quantum__rt__result_record_output(uintptr_t r, void *p) {
    (void)p; putchar(results[r] ? '1' : '0');
}
extern int64_t ENTRYPOINT__main(void);
int main(void) { ENTRYPOINT__main(); return 0; }
"#,
    )
    .expect("runtime fixture should be written");
    let compiled = Command::new("cc")
        .args(["-Wno-override-module"])
        .arg(&module)
        .arg(&runtime)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("native LLVM execution requires cc");
    let result = if compiled.status.success() {
        Some(
            Command::new(&executable)
                .output()
                .expect("native circuit should run"),
        )
    } else {
        None
    };
    fs::remove_dir_all(&dir).expect("native fixture should be removed");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let result = result.expect("compiled fixture should have executed");
    assert!(result.status.success(), "{result:?}");
    assert_eq!(
        String::from_utf8(result.stdout).expect("ASCII results"),
        expected
    );
}

#[test]
fn measured_zero_is_not_less_than_zero() {
    assert_measured_order(BinaryOp::Lt, false, false, false);
}
#[test]
fn measured_zero_is_less_than_one() {
    assert_measured_order(BinaryOp::Lt, false, true, true);
}
#[test]
fn measured_one_is_not_less_than_zero_in_native_qir() {
    assert_measured_order(BinaryOp::Lt, true, false, false);
}
#[test]
fn measured_one_is_not_less_than_one() {
    assert_measured_order(BinaryOp::Lt, true, true, false);
}
#[test]
fn measured_zero_is_less_than_or_equal_to_zero() {
    assert_measured_order(BinaryOp::Le, false, false, true);
}
#[test]
fn measured_zero_is_less_than_or_equal_to_one() {
    assert_measured_order(BinaryOp::Le, false, true, true);
}
#[test]
fn measured_one_is_not_less_than_or_equal_to_zero() {
    assert_measured_order(BinaryOp::Le, true, false, false);
}
#[test]
fn measured_one_is_less_than_or_equal_to_one() {
    assert_measured_order(BinaryOp::Le, true, true, true);
}
#[test]
fn measured_zero_is_not_greater_than_zero() {
    assert_measured_order(BinaryOp::Gt, false, false, false);
}
#[test]
fn measured_zero_is_not_greater_than_one() {
    assert_measured_order(BinaryOp::Gt, false, true, false);
}
#[test]
fn measured_one_is_greater_than_zero_in_native_qir() {
    assert_measured_order(BinaryOp::Gt, true, false, true);
}
#[test]
fn measured_one_is_not_greater_than_one() {
    assert_measured_order(BinaryOp::Gt, true, true, false);
}
#[test]
fn measured_zero_is_greater_than_or_equal_to_zero() {
    assert_measured_order(BinaryOp::Ge, false, false, true);
}
#[test]
fn measured_zero_is_not_greater_than_or_equal_to_one() {
    assert_measured_order(BinaryOp::Ge, false, true, false);
}
#[test]
fn measured_one_is_greater_than_or_equal_to_zero() {
    assert_measured_order(BinaryOp::Ge, true, false, true);
}
#[test]
fn measured_one_is_greater_than_or_equal_to_one() {
    assert_measured_order(BinaryOp::Ge, true, true, true);
}

fn constant_condition(op: Option<BinaryOp>, left: bool, right: bool, expected: bool) {
    let mut store = CircuitStore::new(3, 0);
    let left = store.push_expr(Expr::Literal(Literal::Bool(left)));
    let right = store.push_expr(Expr::Literal(Literal::Bool(right)));
    let condition = store.push_expr(match op {
        Some(op) => Expr::Binary(op, left, right),
        None => Expr::Not(left),
    });
    branch_on(&mut store, condition);
    for profile in [base(), adaptive_ri()] {
        let qir = circuit_to_qir(&store, profile).expect("constant Boolean should lower");
        assert!(!qir.contains("br i1"), "{qir}");
        assert_eq!(
            qir.contains("call void @__quantum__qis__x__body"),
            expected,
            "{qir}"
        );
    }
}

#[test]
fn constant_and_false_omits_then_without_adaptive() {
    constant_condition(Some(BinaryOp::And), true, false, false);
}
#[test]
fn constant_and_true_selects_then_without_adaptive() {
    constant_condition(Some(BinaryOp::And), true, true, true);
}
#[test]
fn constant_or_false_omits_then_without_adaptive() {
    constant_condition(Some(BinaryOp::Or), false, false, false);
}
#[test]
fn constant_or_true_selects_then_without_adaptive() {
    constant_condition(Some(BinaryOp::Or), false, true, true);
}
#[test]
fn constant_not_true_omits_then_without_adaptive() {
    constant_condition(None, true, false, false);
}
#[test]
fn constant_not_false_selects_then_without_adaptive() {
    constant_condition(None, false, false, true);
}

#[test]
fn nested_constant_boolean_condition_folds_without_adaptive() {
    let mut store = CircuitStore::new(3, 0);
    let yes = store.push_expr(Expr::Literal(Literal::Bool(true)));
    let no = store.push_expr(Expr::Literal(Literal::Bool(false)));
    let and = store.push_expr(Expr::Binary(BinaryOp::And, yes, no));
    let not = store.push_expr(Expr::Not(and));
    let condition = store.push_expr(Expr::Binary(BinaryOp::Or, no, not));
    branch_on(&mut store, condition);
    for profile in [base(), adaptive_ri()] {
        let qir = circuit_to_qir(&store, profile).expect("constant condition should lower");
        assert!(!qir.contains("br i1"), "{qir}");
        assert!(qir.contains("call void @__quantum__qis__x__body"), "{qir}");
    }
}

#[test]
fn constant_logical_operation_rejects_non_boolean_operands() {
    let mut store = CircuitStore::new(3, 0);
    let two = store.push_expr(Expr::Literal(Literal::Int(2)));
    let condition = store.push_expr(Expr::Binary(BinaryOp::And, two, two));
    branch_on(&mut store, condition);
    assert_eq!(
        circuit_to_qir(&store, adaptive_ri()),
        Err(Error::UnsupportedCondition {
            reason: "logical operation on a non-boolean",
        })
    );
}

fn base() -> TargetCapabilityFlags {
    Profile::Base.into()
}

fn adaptive_ri() -> TargetCapabilityFlags {
    Profile::AdaptiveRI.into()
}

/// A bell pair with both qubits measured.
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

/// Builds a measurement followed by a conditional on its result.
fn measured_conditional(with_else: bool) -> CircuitStore {
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
    let else_block = if with_else {
        let block = store.new_block();
        store
            .push_into_block(
                block,
                PackedOperation::Standard(StandardGate::Y),
                &[Qubit(1)],
                &[],
                None,
            )
            .expect("gate should be accepted");
        Some(block)
    } else {
        None
    };
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block,
        }))
        .expect("construct should be accepted");
    store
}

#[test]
fn static_circuit_matches_what_the_compiler_emits_for_the_same_program() {
    let qir = circuit_to_qir(&bell(), base()).expect("circuit should lower");
    let body = &qir[..qir.find("declare").unwrap_or(qir.len())];

    // This is the output the Q# compiler produces for
    //   operation Main() : Result[] {
    //       use q = Qubit[2];
    //       H(q[0]); CNOT(q[0], q[1]); [M(q[0]), M(q[1])]
    //   }
    // compiled for the Base profile. The direct path is only credible if it
    // agrees with the path that already works, so this is pinned verbatim
    // rather than snapshotted from our own output.
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          ret i64 0
        }

    "#]]
    .assert_eq(body);
}

#[test]
fn base_profile_qir_declares_what_it_calls() {
    let qir = circuit_to_qir(&bell(), base()).expect("circuit should lower");

    for name in [
        "__quantum__qis__h__body",
        "__quantum__qis__cx__body",
        "__quantum__qis__m__body",
        "__quantum__rt__result_record_output",
    ] {
        assert!(
            qir.contains(&format!("declare void @{name}")),
            "expected a declaration of {name} in {qir}"
        );
    }
    assert!(
        qir.contains("base_profile"),
        "expected the base profile in {qir}"
    );
}

#[test]
fn adaptive_circuit_emits_a_branch() {
    let qir =
        circuit_to_qir(&measured_conditional(false), adaptive_ri()).expect("circuit should lower");

    let body = &qir[..qir.find("declare").unwrap_or(qir.len())];

    // The Q# compiler produces this for
    //   operation Main() : Result {
    //       use q = Qubit[2]; let r = M(q[0]); if r == One { X(q[1]); } r
    //   }
    // on the Adaptive_RI profile, so the branch shape is pinned to it.
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          br i1 %var_0, label %block_1, label %block_2
        block_1:
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          br label %block_2
        block_2:
          call void @__quantum__rt__array_record_output(i64 1, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          ret i64 0
        }

    "#]].assert_eq(body);
}

#[test]
fn a_conditional_on_zero_branches_on_the_negated_result() {
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
    let right = store.push_expr(Expr::Literal(Literal::Int(0)));
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

    let qir = circuit_to_qir(&store, adaptive_ri()).expect("circuit should lower");

    // Comparing against zero negates the result rather than emitting a
    // comparison that carries no extra information.
    assert!(qir.contains("xor i1"), "expected a negation: {qir}");
    assert!(!qir.contains("icmp"), "expected no comparison: {qir}");
}

#[test]
fn a_conditional_with_both_branches_emits_both() {
    let qir =
        circuit_to_qir(&measured_conditional(true), adaptive_ri()).expect("circuit should lower");

    assert!(qir.contains("__quantum__qis__x__body"), "{qir}");
    assert!(qir.contains("__quantum__qis__y__body"), "{qir}");
}

#[test]
fn rotations_carry_their_angle_ahead_of_the_qubit() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Literal(0.5)])),
        )
        .expect("gate should be accepted");

    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");

    assert!(
        qir.contains("__quantum__qis__rx__body(double 0.5, %Qubit*"),
        "expected the angle ahead of the qubit: {qir}"
    );
}

#[test]
fn an_angle_expression_folds_to_a_literal() {
    let mut store = CircuitStore::new(1, 0);
    // (0.5 + 0.25) * 2
    let half = store.push_angle(AngleExpr::Literal(0.5));
    let quarter = store.push_angle(AngleExpr::Literal(0.25));
    let sum = store.push_angle(AngleExpr::Binary(AngleOp::Add, half, quarter));
    let two = store.push_angle(AngleExpr::Literal(2.0));
    let scaled = store.push_angle(AngleExpr::Binary(AngleOp::Mul, sum, two));
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Expr(scaled)])),
        )
        .expect("gate should be accepted");

    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");

    assert!(
        qir.contains("__quantum__qis__rx__body(double 1.5, %Qubit*"),
        "expected the folded angle: {qir}"
    );
}

#[test]
fn a_loop_index_supplies_the_angle_of_each_iteration() {
    let mut store = CircuitStore::new(1, 0);
    let index = store.declare_parameter("i");
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
            values: vec![
                Literal::Float(0.5),
                Literal::Float(1.5),
                Literal::Float(2.5),
            ],
            body,
        }))
        .expect("construct should be accepted");

    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");

    // Each iteration substitutes its own value, so the three rotations differ.
    for angle in ["double 1.0", "double 3.0", "double 5.0"] {
        assert!(
            qir.contains(&format!("__quantum__qis__rx__body({angle}, %Qubit*")),
            "expected {angle}: {qir}"
        );
    }
}

#[test]
fn a_loop_index_is_not_asked_of_the_caller() {
    let mut store = CircuitStore::new(1, 0);
    let index = store.declare_parameter("i");
    let body = store.new_block();
    store
        .push_into_block(
            body,
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Parameter(index)])),
        )
        .expect("gate should be accepted");
    store
        .push_control_flow(ControlFlow::For(ForLoop {
            index: Some(LoopIndex::Parameter(index)),
            values: vec![Literal::Float(0.25)],
            body,
        }))
        .expect("construct should be accepted");

    // The loop supplies the value, so it is not one the caller has to bind.
    assert_eq!(store.referenced_parameters(), Vec::new());

    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");
    assert!(
        qir.contains("__quantum__qis__rx__body(double 0.25, %Qubit*"),
        "expected the iteration's value: {qir}"
    );
}

#[test]
fn an_angle_still_missing_a_value_names_the_parameter() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("theta");
    let node = store.push_angle(AngleExpr::Parameter(theta));
    let two = store.push_angle(AngleExpr::Literal(2.0));
    let doubled = store.push_angle(AngleExpr::Binary(AngleOp::Mul, node, two));
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Expr(doubled)])),
        )
        .expect("gate should be accepted");

    let error = circuit_to_qir(&store, base()).expect_err("circuit should not lower");

    assert_eq!(
        error,
        Error::UnboundParameter {
            name: "theta".to_string()
        }
    );
}

#[test]
fn a_box_lowers_its_body_inline() {
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
            label: Some("prep".to_string()),
        }))
        .expect("construct should be accepted");

    // A box only groups instructions, so its body appears with no wrapper and
    // it needs no capability of its own.
    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");

    assert!(qir.contains("__quantum__qis__h__body"), "{qir}");
}

#[test]
fn identity_emits_nothing() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push_standard(StandardGate::I, &[Qubit(0)])
        .expect("gate should be accepted");

    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");

    assert!(!qir.contains("__quantum__qis__i__body"), "{qir}");
}

/// The call instructions of a circuit's QIR, without the surrounding module.
fn instructions_of(store: &CircuitStore, capabilities: TargetCapabilityFlags) -> String {
    let qir = circuit_to_qir(store, capabilities).expect("circuit should lower");
    let body = &qir[..qir.find("declare").unwrap_or(qir.len())];
    body.lines()
        .filter(|line| line.contains("call "))
        .map(|line| line.trim().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_controlled_swap_lowers_through_its_rewrite() {
    let mut store = CircuitStore::new(3, 0);
    store
        .push_standard(StandardGate::Cswap, &[Qubit(0), Qubit(1), Qubit(2)])
        .expect("gate should be accepted");

    // No controlled-swap intrinsic exists, so the emitted program is the
    // rewrite. Its correctness is established against the gate's own matrix in
    // the rewrite tests; this pins what reaches QIR.
    expect![[r#"
        call void @__quantum__rt__initialize(i8* null)
        call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
        call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
        call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))"#]]
    .assert_eq(&instructions_of(&store, base()));
}

#[test]
fn a_controlled_rotation_lowers_through_its_rewrite() {
    let mut store = CircuitStore::new(2, 0);
    store
        .push(
            PackedOperation::Standard(StandardGate::Crz),
            &[Qubit(0), Qubit(1)],
            &[],
            Some(Box::new([ParamValue::Literal(0.5)])),
        )
        .expect("gate should be accepted");

    // The angle is halved and applied twice with opposite signs, so the halves
    // cancel when the control is zero and compose when it is one.
    expect![[r#"
        call void @__quantum__rt__initialize(i8* null)
        call void @__quantum__qis__rz__body(double 0.25, %Qubit* inttoptr (i64 1 to %Qubit*))
        call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
        call void @__quantum__qis__rz__body(double -0.25, %Qubit* inttoptr (i64 1 to %Qubit*))
        call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))"#]]
    .assert_eq(&instructions_of(&store, base()));
}

#[test]
fn measuring_another_basis_lowers_through_its_rewrite() {
    let mut store = CircuitStore::new(1, 1);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureY),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    // The basis is rotated onto Z, measured, and rotated back, so the qubit is
    // left in an eigenstate of Y rather than of Z. Pinned on an adaptive
    // profile, because the Base profile then defers the measurement and
    // rewrites what follows it.
    expect![[r#"
        call void @__quantum__rt__initialize(i8* null)
        call void @__quantum__qis__s__adj(%Qubit* inttoptr (i64 0 to %Qubit*))
        call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
        call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
        call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
        call void @__quantum__qis__s__body(%Qubit* inttoptr (i64 0 to %Qubit*))
        call void @__quantum__rt__array_record_output(i64 1, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
        call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))"#]]
    .assert_eq(&instructions_of(&store, adaptive_ri()));
}

#[test]
fn the_base_profile_defers_a_rewritten_measurement() {
    let mut store = CircuitStore::new(1, 1);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureY),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");

    // The Base profile forbids acting on a qubit after measuring it, so the
    // existing pipeline moves the measurement to the end and carries the
    // restoring rotation onto a fresh qubit. That is the pipeline's own
    // transformation rather than anything this path chooses, and it is pinned
    // so that a change in it is visible here.
    assert!(qir.contains("__quantum__qis__cx__body"), "{qir}");
    assert!(qir.contains("required_num_qubits\"=\"2\""), "{qir}");
}

#[test]
fn preparing_another_basis_lowers_through_its_rewrite() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push_standard(StandardGate::PrepareY, &[Qubit(0)])
        .expect("gate should be accepted");

    // Pinned on an adaptive profile, because the Base profile drops the leading
    // reset as redundant given qubits start in the zero state.
    expect![[r#"
        call void @__quantum__rt__initialize(i8* null)
        call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
        call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
        call void @__quantum__qis__s__body(%Qubit* inttoptr (i64 0 to %Qubit*))"#]]
    .assert_eq(&instructions_of(&store, adaptive_ri()));
}

#[test]
fn a_rewritten_measurement_still_records_its_result() {
    let mut store = CircuitStore::new(1, 1);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureX),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");

    // The classical bit travels with the measurement inside the rewrite, so the
    // result is still reported.
    assert_eq!(qir.matches("result_record_output").count(), 2, "{qir}");
}

#[test]
fn a_symbolic_angle_is_refused() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("theta");
    store
        .push(
            PackedOperation::Standard(StandardGate::Rx),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Parameter(theta)])),
        )
        .expect("gate should be accepted");

    let error = circuit_to_qir(&store, base()).expect_err("circuit should not lower");

    assert_eq!(
        error,
        Error::UnboundParameter {
            name: "theta".to_string()
        }
    );
}

#[test]
fn a_conditional_on_the_base_profile_names_the_missing_capability() {
    let error =
        circuit_to_qir(&measured_conditional(false), base()).expect_err("circuit should not lower");

    assert_eq!(
        error,
        Error::MissingCapability {
            construct: "if",
            capability: "Adaptive",
        }
    );
}

#[test]
fn a_for_loop_repeats_its_body_once_per_value() {
    let mut store = CircuitStore::new(2, 1);
    let body = store.new_block();
    store
        .push_into_block(
            body,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    store
        .push_control_flow(ControlFlow::For(ForLoop {
            index: None,
            values: vec![Literal::Int(0), Literal::Int(1), Literal::Int(2)],
            body,
        }))
        .expect("construct should be accepted");

    // The values are known when the circuit is built, so the loop is unrolled
    // rather than emitted. That needs no backwards branching, which is why it
    // works on the Base profile.
    expect![[r#"
        call void @__quantum__rt__initialize(i8* null)
        call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
        call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
        call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))"#]]
    .assert_eq(&instructions_of(&store, base()));
}

#[test]
fn a_for_loop_over_nothing_emits_nothing() {
    let mut store = CircuitStore::new(1, 0);
    let body = store.new_block();
    store
        .push_into_block(
            body,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    store
        .push_control_flow(ControlFlow::For(ForLoop {
            index: None,
            values: Vec::new(),
            body,
        }))
        .expect("construct should be accepted");

    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");

    assert!(!qir.contains("__quantum__qis__x__body"), "{qir}");
}

#[test]
fn nested_for_loops_multiply() {
    let mut store = CircuitStore::new(1, 0);
    let inner_body = store.new_block();
    store
        .push_into_block(
            inner_body,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    let outer_body = store.new_block();
    store
        .push_control_flow_into_block(
            outer_body,
            ControlFlow::For(ForLoop {
                index: None,
                values: vec![Literal::Int(0), Literal::Int(1)],
                body: inner_body,
            }),
        )
        .expect("construct should be accepted");
    store
        .push_control_flow(ControlFlow::For(ForLoop {
            index: None,
            values: vec![Literal::Int(0), Literal::Int(1), Literal::Int(2)],
            body: outer_body,
        }))
        .expect("construct should be accepted");

    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");

    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        6,
        "{qir}"
    );
}

#[test]
fn a_loop_variable_takes_the_value_of_its_iteration() {
    let mut store = CircuitStore::new(2, 1);
    let index = store.declare_var("index", VarType::Int);
    let body = store.new_block();

    // if index == 1 { x q1 }, so exactly one iteration should emit the gate.
    let left = store.push_expr(Expr::Var(index));
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

    let qir = circuit_to_qir(&store, adaptive_ri()).expect("circuit should lower");

    // The loop variable resolves to the value of the iteration being unrolled,
    // so the comparison is decided at build time and only the matching
    // iteration keeps the gate.
    assert_eq!(qir.matches("__quantum__qis__x__body").count(), 2, "{qir}");
}

#[test]
fn a_classical_variable_outside_a_loop_is_still_reported() {
    let mut store = CircuitStore::new(1, 1);
    let flag = store.declare_var("flag", VarType::Bool);
    let condition = store.push_expr(Expr::Var(flag));
    let then_block = store.new_block();
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: None,
        }))
        .expect("construct should be accepted");

    let error = circuit_to_qir(&store, adaptive_ri()).expect_err("circuit should not lower");

    assert_eq!(
        error,
        Error::UnsupportedCondition {
            reason: "classical variables are not lowered yet"
        }
    );
}

#[test]
fn literal_integer_equality_true_selects_then_without_adaptive() {
    let mut store = CircuitStore::new(1, 0);
    let left = store.push_expr(Expr::Literal(Literal::Int(2)));
    let right = store.push_expr(Expr::Literal(Literal::Int(2)));
    let condition = store.push_expr(Expr::Binary(BinaryOp::Eq, left, right));
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
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: None,
        }))
        .expect("construct should be accepted");

    // Nothing is left to decide at run time, so the branch is taken here and
    // the Base profile accepts the result despite lacking branching.
    expect![[r#"
        call void @__quantum__rt__initialize(i8* null)
        call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))"#]]
    .assert_eq(&instructions_of(&store, base()));
}

#[test]
fn literal_integer_equality_false_selects_else_without_adaptive() {
    let mut store = CircuitStore::new(1, 0);
    let left = store.push_expr(Expr::Literal(Literal::Int(1)));
    let right = store.push_expr(Expr::Literal(Literal::Int(2)));
    let condition = store.push_expr(Expr::Binary(BinaryOp::Eq, left, right));
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
    store
        .push_into_block(
            else_block,
            PackedOperation::Standard(StandardGate::Y),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: Some(else_block),
        }))
        .expect("construct should be accepted");

    expect![[r#"
        call void @__quantum__rt__initialize(i8* null)
        call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 0 to %Qubit*))"#]]
    .assert_eq(&instructions_of(&store, base()));
}

#[test]
fn a_loop_is_reported_rather_than_mislowered() {
    let mut store = CircuitStore::new(1, 1);
    let condition = store.push_expr(Expr::Clbit(0));
    let body = store.new_block();
    store
        .push_control_flow(ControlFlow::While(WhileLoop { condition, body }))
        .expect("construct should be accepted");

    let error = circuit_to_qir(&store, adaptive_ri()).expect_err("circuit should not lower");

    assert_eq!(error, Error::UnsupportedConstruct { construct: "while" });
}

#[test]
fn a_chain_of_conditionals_lowers_to_nested_branches() {
    // if c[0] { x } else if c[1] { y } else { h }
    let mut store = CircuitStore::new(2, 2);
    for clbit in 0..2 {
        store
            .push(
                PackedOperation::Standard(StandardGate::MeasureZ),
                &[Qubit(0)],
                &[Clbit(clbit)],
                None,
            )
            .expect("measurement should be accepted");
    }

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
    // The alternative branch holds the next link of the chain.
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

    let qir = circuit_to_qir(&store, adaptive_ri()).expect("circuit should lower");
    let body = &qir[..qir.find("declare").unwrap_or(qir.len())];

    // Each link of the chain leaves its own join block, so block_5 exists only
    // to jump to block_6. That is redundant rather than wrong, and the existing
    // control-flow simplification pass leaves it alone.
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          %var_0 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          br i1 %var_0, label %block_1, label %block_2
        block_1:
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          br label %block_6
        block_2:
          %var_1 = call zeroext i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
          br i1 %var_1, label %block_3, label %block_4
        block_3:
          call void @__quantum__qis__y__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          br label %block_5
        block_4:
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          br label %block_5
        block_5:
          br label %block_6
        block_6:
          call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          ret i64 0
        }

    "#]].assert_eq(body);
}

#[test]
fn a_switch_is_reported_rather_than_mislowered() {
    let mut store = CircuitStore::new(1, 1);
    let selector = store.push_expr(Expr::Clbit(0));
    let case = store.new_block();
    store
        .push_control_flow(ControlFlow::Switch(Switch {
            selector,
            cases: vec![(vec![Literal::Int(0)], case)],
            default: None,
        }))
        .expect("construct should be accepted");

    let error = circuit_to_qir(&store, adaptive_ri()).expect_err("circuit should not lower");

    assert_eq!(
        error,
        Error::UnsupportedConstruct {
            construct: "switch"
        }
    );
}

#[test]
fn a_barrier_emits_the_barrier_intrinsic() {
    let mut store = CircuitStore::new(2, 1);
    for qubits in [vec![Qubit(0), Qubit(1)], vec![Qubit(0)]] {
        store
            .push(
                PackedOperation::Barrier {
                    qubit_arity: u8::try_from(qubits.len()).expect("arity fits"),
                },
                &qubits,
                &[],
                None,
            )
            .expect("operation should be accepted");
    }
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    let qir = circuit_to_qir(&store, adaptive_ri()).expect("circuit should lower");
    let body = &qir[..qir.find("declare").unwrap_or(qir.len())];

    // This is the output the OpenQASM compiler produces for the equivalent
    // program, which is the precedent for how a barrier reaches QIR. A QIR
    // barrier takes no operands, so the qubits a barrier names do not survive.
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__barrier__body()
          call void @__quantum__qis__barrier__body()
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__array_record_output(i64 1, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          ret i64 0
        }

    "#]]
    .assert_eq(body);
    assert!(
        qir.contains("declare void @__quantum__qis__barrier__body()"),
        "{qir}"
    );
}

#[test]
fn a_delay_is_refused() {
    let mut store = CircuitStore::new(1, 0);
    store
        .push(
            PackedOperation::Delay {
                duration: 2.0,
                unit: TimeUnit::Dt,
            },
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("operation should be accepted");

    let error = circuit_to_qir(&store, base()).expect_err("circuit should not lower");

    // Matches the OpenQASM compiler, which reports delay statements as
    // unsupported rather than lowering them.
    assert_eq!(error, Error::UnsupportedOperation { operation: "delay" });
}

#[test]
fn a_box_with_a_duration_is_refused() {
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
            duration: Some(1.5),
            label: None,
        }))
        .expect("construct should be accepted");

    let error = circuit_to_qir(&store, base()).expect_err("circuit should not lower");

    // A duration is a scheduling instruction this path cannot honour, so it is
    // refused rather than quietly ignored while the body lowers.
    assert_eq!(
        error,
        Error::UnsupportedConstruct {
            construct: "box with duration"
        }
    );
}

#[test]
fn an_unmeasured_circuit_records_no_output() {
    let mut store = CircuitStore::new(1, 1);
    store
        .push_standard(StandardGate::H, &[Qubit(0)])
        .expect("gate should be accepted");

    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");

    assert!(!qir.contains("record_output"), "{qir}");
}

#[test]
fn measuring_one_of_several_bits_records_only_that_one() {
    let mut store = CircuitStore::new(2, 4);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(1)],
            &[Clbit(2)],
            None,
        )
        .expect("measurement should be accepted");

    let qir = circuit_to_qir(&store, base()).expect("circuit should lower");

    // Only measured results are recorded, since an unmeasured one holds nothing.
    assert!(qir.contains("array_record_output(i64 1"), "{qir}");
    assert_eq!(qir.matches("result_record_output").count(), 2, "{qir}");
}

#[test]
fn every_standard_gate_compiles_for_base_with_nominal_operands() {
    // Pin the supported set so a new intrinsic or decomposition is visible.
    let mut lowerable = Vec::new();
    let mut undecomposed = Vec::new();
    for gate in StandardGate::all() {
        let mut store = CircuitStore::new(3, 3);
        let qubits: Vec<Qubit> = (0..u32::from(gate.qubit_arity())).map(Qubit).collect();
        let clbits: Vec<Clbit> = if gate.name().starts_with("measure") {
            vec![Clbit(0)]
        } else {
            Vec::new()
        };
        let params: Option<Box<[ParamValue]>> = if gate.param_arity() > 0 {
            Some(
                (0..gate.param_arity())
                    .map(|_| ParamValue::Literal(0.5))
                    .collect(),
            )
        } else {
            None
        };
        store
            .push(PackedOperation::Standard(gate), &qubits, &clbits, params)
            .expect("gate should be accepted");
        match circuit_to_qir(&store, base()) {
            Ok(_) => lowerable.push(gate.name()),
            Err(Error::UndecomposedGate { gate }) => undecomposed.push(gate),
            Err(other) => panic!("unexpected error for {}: {other}", gate.name()),
        }
    }

    // Every standard gate reaches QIR, either through its own intrinsic or
    // through a rewrite. A gate that stopped doing so would show up here.
    expect![[r#"
        i, x, y, z, h, s, sdg, sx, sxdg, t, tdg, cx, cy, cz, swap, ccx, cswap, rx, ry, rz, rxx, ryy, rzz, crx, cry, crz, measure_x, measure_y, measure_z, prepare_x, prepare_y, prepare_z"#]]
    .assert_eq(&lowerable.join(", "));
    assert!(
        undecomposed.is_empty(),
        "these gates reach neither an intrinsic nor a rewrite: {undecomposed:?}"
    );
}
