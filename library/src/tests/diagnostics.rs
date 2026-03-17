// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::test_expression_fails;

use super::test_expression;
use expect_test::expect;
use qsc::interpret::Value;

#[test]
fn check_operations_are_equal() {
    test_expression(
        "{
            import Std.Diagnostics.*;
            import Std.Arrays.*;
            operation op1(xs: Qubit[]): Unit is Adj {
                CCNOT(xs[0], xs[1], xs[2]);
            }
            operation op2(xs: Qubit[]): Unit is Adj {
                Controlled X(Most(xs), Tail(xs));
            }
            operation op3(xs: Qubit[]): Unit is Adj {
                Controlled X(Rest(xs), Head(xs));
            }
            [CheckOperationsAreEqual(3, op1, op2),
             CheckOperationsAreEqual(3, op2, op1),
             CheckOperationsAreEqual(3, op1, op3),
             CheckOperationsAreEqual(3, op3, op1),
             CheckOperationsAreEqual(3, op2, op3),
             CheckOperationsAreEqual(3, op3, op2)]

        }",
        &Value::Array(
            vec![
                Value::Bool(true),
                Value::Bool(true),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
            ]
            .into(),
        ),
    );
}

#[test]
fn check_dumpoperation_for_i() {
    let output = test_expression(
        "Microsoft.Quantum.Diagnostics.DumpOperation(1, qs => I(qs[0]))",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        1.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 1.0000+0.0000𝑖
    "#]]
    .assert_eq(&output);
}

#[test]
fn check_dumpoperation_for_x() {
    let output = test_expression(
        "Microsoft.Quantum.Diagnostics.DumpOperation(1, qs => X(qs[0]))",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        0.0000+0.0000𝑖 1.0000+0.0000𝑖
        1.0000+0.0000𝑖 0.0000+0.0000𝑖
    "#]]
    .assert_eq(&output);
}

#[test]
fn check_dumpoperation_for_h() {
    let output = test_expression(
        "Microsoft.Quantum.Diagnostics.DumpOperation(1, qs => H(qs[0]))",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        0.7071+0.0000𝑖 0.7071+0.0000𝑖
        0.7071+0.0000𝑖 −0.7071+0.0000𝑖
    "#]]
    .assert_eq(&output);
}

#[test]
fn check_dumpoperation_for_y() {
    let output = test_expression(
        "Microsoft.Quantum.Diagnostics.DumpOperation(1, qs => Y(qs[0]))",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        0.0000+0.0000𝑖 0.0000−1.0000𝑖
        0.0000+1.0000𝑖 0.0000+0.0000𝑖
    "#]]
    .assert_eq(&output);
}

#[test]
fn check_dumpoperation_for_ccnot() {
    let output = test_expression(
        "Microsoft.Quantum.Diagnostics.DumpOperation(3, qs => CCNOT(qs[0], qs[1], qs[2]))",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖
    "#]].assert_eq(&output);
}

#[test]
fn check_dumpoperation_with_extra_qubits_allocated() {
    let output = test_expression(
        "{use qs = Qubit[2]; Microsoft.Quantum.Diagnostics.DumpOperation(1, qs => H(qs[0]))}",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        0.7071+0.0000𝑖 0.7071+0.0000𝑖
        0.7071+0.0000𝑖 −0.7071+0.0000𝑖
    "#]]
    .assert_eq(&output);
}

#[test]
fn check_start_stop_counting_operation_called_3_times() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingOperation;
            import Microsoft.Quantum.Diagnostics.StopCountingOperation;

            operation op1() : Unit {}
            operation op2() : Unit { op1(); }
            StartCountingOperation(op1);
            StartCountingOperation(op2);
            op1(); op1(); op2();
            (StopCountingOperation(op1), StopCountingOperation(op2))
        }",
        &Value::Tuple([Value::Int(3), Value::Int(1)].into(), None),
    );
}

#[test]
fn check_start_stop_counting_operation_called_0_times() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingOperation;
            import Microsoft.Quantum.Diagnostics.StopCountingOperation;

            operation op1() : Unit {}
            operation op2() : Unit { op1(); }
            StartCountingOperation(op1);
            StartCountingOperation(op2);
            (StopCountingOperation(op1), StopCountingOperation(op2))
        }",
        &Value::Tuple([Value::Int(0), Value::Int(0)].into(), None),
    );
}

#[test]
fn check_lambda_counted_separately_from_operation() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingOperation;
            import Microsoft.Quantum.Diagnostics.StopCountingOperation;

            operation op1() : Unit {}
            StartCountingOperation(op1);
            let lambda = () => op1();
            StartCountingOperation(lambda);
            op1();
            lambda();
            (StopCountingOperation(op1), StopCountingOperation(lambda))
        }",
        &Value::Tuple([Value::Int(2), Value::Int(1)].into(), None),
    );
}

#[test]
fn check_multiple_controls_counted_together() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingOperation;
            import Microsoft.Quantum.Diagnostics.StopCountingOperation;

            operation op1() : Unit is Adj + Ctl {}
            StartCountingOperation(Controlled op1);
            Controlled op1([], ());
            Controlled Controlled op1([], ([], ()));
            Controlled Controlled Controlled op1([], ([], ([], ())));
            (StopCountingOperation(Controlled op1))
        }",
        &Value::Int(3),
    );
}

#[test]
fn check_counting_operation_differentiates_between_body_adj_ctl() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingOperation;
            import Microsoft.Quantum.Diagnostics.StopCountingOperation;

            operation op1() : Unit is Adj + Ctl {}
            StartCountingOperation(op1);
            StartCountingOperation(Adjoint op1);
            StartCountingOperation(Controlled op1);
            StartCountingOperation(Adjoint Controlled op1);
            op1();
            Adjoint op1(); Adjoint op1();
            Controlled op1([], ()); Controlled op1([], ()); Controlled op1([], ());
            Adjoint Controlled op1([], ()); Adjoint Controlled op1([], ());
            Controlled Adjoint op1([], ()); Controlled Adjoint op1([], ());
            (StopCountingOperation(op1), StopCountingOperation(Adjoint op1), StopCountingOperation(Controlled op1), StopCountingOperation(Adjoint Controlled op1))
        }",
        &Value::Tuple([Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)].into(), None),
    );
}

#[test]
fn check_start_stop_counting_function_called_3_times() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingFunction;
            import Microsoft.Quantum.Diagnostics.StopCountingFunction;

            function f1() : Unit {}
            function f2() : Unit { f1(); }
            StartCountingFunction(f1);
            StartCountingFunction(f2);
            f1(); f1(); f2();
            (StopCountingFunction(f1), StopCountingFunction(f2))
        }",
        &Value::Tuple([Value::Int(3), Value::Int(1)].into(), None),
    );
}

#[test]
fn check_start_stop_counting_function_called_0_times() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingFunction;
            import Microsoft.Quantum.Diagnostics.StopCountingFunction;

            function f1() : Unit {}
            function f2() : Unit { f1(); }
            StartCountingFunction(f1);
            StartCountingFunction(f2);
            (StopCountingFunction(f1), StopCountingFunction(f2))
        }",
        &Value::Tuple([Value::Int(0), Value::Int(0)].into(), None),
    );
}

#[test]
fn check_start_counting_qubits_for_one_allocation() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingQubits;
            import Microsoft.Quantum.Diagnostics.StopCountingQubits;

            StartCountingQubits();
            use q = Qubit();
            StopCountingQubits()
        }",
        &Value::Int(1),
    );
}

#[test]
fn check_start_counting_qubits_for_tuple_allocation() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingQubits;
            import Microsoft.Quantum.Diagnostics.StopCountingQubits;

            StartCountingQubits();
            use (q0, q1) = (Qubit(), Qubit());
            StopCountingQubits()
        }",
        &Value::Int(2),
    );
}

#[test]
fn check_start_counting_qubits_for_array_allocation() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingQubits;
            import Microsoft.Quantum.Diagnostics.StopCountingQubits;

            StartCountingQubits();
            use qs = Qubit[2];
            StopCountingQubits()
        }",
        &Value::Int(2),
    );
}

#[test]
fn check_start_counting_qubits_after_allocation_gives_zero() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingQubits;
            import Microsoft.Quantum.Diagnostics.StopCountingQubits;

            use q = Qubit();
            StartCountingQubits();
            StopCountingQubits()
        }",
        &Value::Int(0),
    );
}

#[test]
fn check_start_counting_qubits_sees_same_qubit_as_single_count() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingQubits;
            import Microsoft.Quantum.Diagnostics.StopCountingQubits;

            StartCountingQubits();
            {
                use q = Qubit();
            }
            {
                use q = Qubit();
            }
            StopCountingQubits()
        }",
        &Value::Int(1),
    );
}

#[test]
fn check_start_counting_qubits_works_with_manual_out_of_order_allocation_release() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingQubits;
            import Microsoft.Quantum.Diagnostics.StopCountingQubits;
            import QIR.Runtime.__quantum__rt__qubit_allocate;
            import QIR.Runtime.__quantum__rt__qubit_release;

            let (q0, q1, q2) = (__quantum__rt__qubit_allocate(), __quantum__rt__qubit_allocate(), __quantum__rt__qubit_allocate());
            StartCountingQubits();
            __quantum__rt__qubit_release(q2);
            use q = Qubit();
            __quantum__rt__qubit_release(q0);
            __quantum__rt__qubit_release(q1);
            use qs = Qubit[2];
            StopCountingQubits()
        }",
        &Value::Int(3),
    );
}

#[test]
fn check_counting_qubits_works_with_allocation_in_operation_calls() {
    test_expression(
        "{
            import Microsoft.Quantum.Diagnostics.StartCountingQubits;
            import Microsoft.Quantum.Diagnostics.StopCountingQubits;
            import Microsoft.Quantum.Diagnostics.CheckOperationsAreEqual;

            StartCountingQubits();
            let numQubits = 2;
            let equal = CheckOperationsAreEqual(2,
                qs => SWAP(qs[0], qs[1]),
                qs => { CNOT(qs[0], qs[1]); CNOT(qs[1], qs[0]); CNOT(qs[0], qs[1]); }
            );
            (true, 2 * numQubits) == (equal, StopCountingQubits())
        }",
        &Value::Bool(true),
    );
}

#[test]
fn check_dumpoperation_with_extra_qubits_in_superposition() {
    let output = test_expression(
        "{use qs = Qubit[2]; H(qs[0]); Microsoft.Quantum.Diagnostics.DumpOperation(1, qs => H(qs[0])); Reset(qs[0]);}",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        0.7071+0.0000𝑖 0.7071+0.0000𝑖
        0.7071+0.0000𝑖 −0.7071+0.0000𝑖
    "#]]
    .assert_eq(&output);
}

#[test]
fn check_dumpoperation_with_extra_qubits_global_phase_reflected_in_matrix() {
    let output = test_expression(
        "{use qs = Qubit[2]; R(PauliI, Std.Math.PI() / 2.0, qs[0]); Microsoft.Quantum.Diagnostics.DumpOperation(1, qs => H(qs[0])); Reset(qs[0]);}",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        0.5000−0.5000𝑖 0.5000−0.5000𝑖
        0.5000−0.5000𝑖 −0.5000+0.5000𝑖
    "#]]
    .assert_eq(&output);
}

#[test]
fn check_dumpoperation_with_extra_qubits_relative_phase_not_reflected_in_matrix() {
    let output = test_expression(
        "{use qs = Qubit[2]; R1(Std.Math.PI() / 2.0, qs[0]); Microsoft.Quantum.Diagnostics.DumpOperation(1, qs => H(qs[0])); Reset(qs[0]);}",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        0.7071+0.0000𝑖 0.7071+0.0000𝑖
        0.7071+0.0000𝑖 −0.7071+0.0000𝑖
    "#]]
    .assert_eq(&output);
}

#[test]
fn check_dump_operation_for_r1_of_pi() {
    let output = test_expression(
        "Microsoft.Quantum.Diagnostics.DumpOperation(1, qs => R1(Std.Math.PI(), qs[0]))",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        1.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 −1.0000+0.0000𝑖
    "#]]
    .assert_eq(&output);
}

#[test]
fn check_dump_operation_for_r1_of_pi_with_one_control() {
    let output = test_expression(
        "Microsoft.Quantum.Diagnostics.DumpOperation(2, qs => Controlled R1(qs[...0], (Std.Math.PI(), qs[1])))",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 −1.0000+0.0000𝑖
    "#]]
    .assert_eq(&output);
}

#[test]
fn check_dump_operation_for_r1_of_pi_with_two_controls() {
    let output = test_expression(
        "Microsoft.Quantum.Diagnostics.DumpOperation(3, qs => Controlled R1(qs[...1], (Std.Math.PI(), qs[2])))",
        &Value::unit(),
    );
    expect![[r#"
        MATRIX:
        1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 1.0000+0.0000𝑖 0.0000+0.0000𝑖
        0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 0.0000+0.0000𝑖 −1.0000+0.0000𝑖
    "#]].assert_eq(&output);
}

#[test]
fn check_bit_flip_noise_values() {
    test_expression(
        "Std.Diagnostics.BitFlipNoise(0.3)",
        &Value::Tuple(
            [Value::Double(0.3), Value::Double(0.0), Value::Double(0.0)].into(),
            None,
        ),
    );
}

#[test]
fn check_phase_flip_noise_values() {
    test_expression(
        "Std.Diagnostics.PhaseFlipNoise(0.3)",
        &Value::Tuple(
            [Value::Double(0.0), Value::Double(0.0), Value::Double(0.3)].into(),
            None,
        ),
    );
}

#[test]
fn check_depolarizing_noise_values() {
    test_expression(
        "Std.Diagnostics.DepolarizingNoise(0.3)",
        &Value::Tuple(
            [Value::Double(0.1), Value::Double(0.1), Value::Double(0.1)].into(),
            None,
        ),
    );
}

#[test]
fn check_no_noise_values() {
    test_expression(
        "Std.Diagnostics.NoNoise()",
        &Value::Tuple(
            [Value::Double(0.0), Value::Double(0.0), Value::Double(0.0)].into(),
            None,
        ),
    );
}

#[test]
fn check_post_select_collapses_superposition_to_zero() {
    test_expression(
        "{
            import Std.Diagnostics.PostSelectZ;
            import Std.Diagnostics.CheckZero;
            use q = Qubit();
            H(q);
            PostSelectZ(Zero, q);
            CheckZero(q)
        }",
        &Value::Bool(true),
    );
}

#[test]
fn check_post_select_collapses_superposition_to_one() {
    test_expression(
        "{
            import Std.Diagnostics.PostSelectZ;
            import Std.Diagnostics.CheckZero;
            use q = Qubit();
            H(q);
            PostSelectZ(One, q);
            let check = CheckZero(q);
            Reset(q);
            check
        }",
        &Value::Bool(false),
    );
}

#[test]
fn check_post_select_fails_with_non_existent_state() {
    let err = test_expression_fails(
        "{
            import Std.Diagnostics.PostSelectZ;
            import Std.Diagnostics.CheckZero;
            use q = Qubit();
            PostSelectZ(One, q);
        }",
    );
    expect![
        "intrinsic callable `PostSelectZ` failed: post-selection condition has zero probability"
    ]
    .assert_eq(&err);
}
