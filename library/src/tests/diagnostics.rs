// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::test_expression;
use qsc::interpret::Value;

#[test]
fn check_operations_are_equal() {
    test_expression(
        "{
            open Microsoft.Quantum.Diagnostics;
            open Microsoft.Quantum.Arrays;
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
        &Value::Tuple([Value::Int(3), Value::Int(1)].into()),
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
        &Value::Tuple([Value::Int(0), Value::Int(0)].into()),
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
        &Value::Tuple([Value::Int(2), Value::Int(1)].into()),
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
        &Value::Tuple([Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)].into()),
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
        &Value::Tuple([Value::Int(3), Value::Int(1)].into()),
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
        &Value::Tuple([Value::Int(0), Value::Int(0)].into()),
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
