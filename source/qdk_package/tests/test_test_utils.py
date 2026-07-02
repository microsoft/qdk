import math
import random

from qdk import Context
from qdk.test_utils import dump_operation_on_state, ArithmeticOpTester


def _assert_states_close(state1: list[complex], state2: list[complex]):
    assert len(state1) == len(state2)
    for i in range(len(state1)):
        assert abs(state1[i] - state2[i]) < 1e-9


def test_dump_operation_on_state(context: Context):
    context.eval("""
    operation MyOp1(q: Qubit[]) : Unit {
        H(q[0]);
        CNOT(q[0], q[1]);
        Z(q[1]);
    }
    """)

    vector = dump_operation_on_state(context.code.MyOp1, num_qubits=2)
    s = 0.5**0.5
    _assert_states_close(vector, [s, 0, 0, -s])

    vector = dump_operation_on_state("MyOp1", num_qubits=2, context=context)
    _assert_states_close(vector, [s, 0, 0, -s])


def test_dump_operation_on_state_with_two_registers(context: Context):
    context.eval("""
    operation MyOp2(q1: Qubit[], q2: Qubit[]) : Unit {
        H(q1[0]);
        CNOT(q1[0], q2[0]);
    }

    operation MyOp2_TestHelper(q: Qubit[]) : Unit {
        let n = Length(q);
        MyOp2(q[0..n/2-1], q[n/2..n-1]);
    }
    """)

    vector = dump_operation_on_state(context.code.MyOp2_TestHelper, num_qubits=4)
    s = 0.5**0.5
    _assert_states_close(vector, [s, 0, 0, 0, 0, 0, 0, 0, 0, 0, s, 0, 0, 0, 0, 0])


def test_dump_operation_on_state_with_partial_trace(context: Context):
    context.eval("""
    operation MyOp3(q1: Qubit[], q2: Qubit[]) : Unit {
        H(q1[0]);
        H(q2[0]);
    }

    operation MyOp3_TestHelper(q: Qubit[]) : Unit {
        use q2 = Qubit[2];
        MyOp3(q, q2);
        ResetAll(q2);
    }
    """)

    vector = dump_operation_on_state(context.code.MyOp3_TestHelper, num_qubits=2)
    s = 0.5**0.5
    _assert_states_close(vector, [s, 0, s, 0])


def test_dump_operation_on_state_with_initial_state(context: Context):
    context.eval("""
    operation MyOp4(q: Qubit[])  : Unit is Adj {
        CNOT(q[0], q[1]);
        H(q[0]);
    }
    """)

    s = 0.5**0.5
    vector = dump_operation_on_state(
        context.code.MyOp4, num_qubits=2, initial_state=[s, 0, 0, s]
    )
    _assert_states_close(vector, [1, 0, 0, 0])


def test_dump_operation_on_state_with_parameters(context: Context):
    context.eval("""
    operation MyOp5(qs: Qubit[], angle: Double)  : Unit is Adj {
      for q in qs {
        Rx(angle, q);
      }
    }

    operation MyOp5_TestHelper(angle: Double) : (Qubit[] => Unit) {
        MyOp5(_, angle)
    }
    """)

    vector = dump_operation_on_state(
        context.code.MyOp5_TestHelper(0.3), num_qubits=2, context=context
    )
    c = math.cos(0.3 / 2)
    s = math.sin(0.3 / 2)
    _assert_states_close(vector, [c * c, -1j * c * s, -1j * c * s, -(s * s)])


def test_dump_operation_on_state_with_parameterized_callable(context: Context):
    context.eval("""
    operation MyOp5(qs: Qubit[], angle: Double)  : Unit is Adj {
      for q in qs {
        Rx(angle, q);
      }
    }
    """)

    vector = dump_operation_on_state("MyOp5(_, 0.3)", num_qubits=2, context=context)
    c = math.cos(0.3 / 2)
    s = math.sin(0.3 / 2)
    _assert_states_close(vector, [c * c, -1j * c * s, -1j * c * s, -(s * s)])


def test_arithmetic_op_helper_test_adder(context: Context):
    context.eval("""
    operation MyAdder(qx: Qubit[], qy: Qubit[], qz: Qubit[]) : Unit {
        Std.Arithmetic.RippleCarryCGAddLE(qx, qy, qz);
    }
    """)
    n = 10
    tester = ArithmeticOpTester(context.code.MyAdder, [n, n, n])
    for _ in range(5):
        x, y = random.randint(0, 2**n - 1), random.randint(0, 2**n - 1)
        assert tester.run([x, y, 0]) == [x, y, (x + y) % (2**n)]


def test_arithmetic_op_helper_test_op_from_string(context: Context):
    n = 10
    tester = ArithmeticOpTester("Std.Arithmetic.IncByLE", [n, n], context)
    for _ in range(5):
        x, y = random.randint(0, 2**n - 1), random.randint(0, 2**n - 1)
        assert tester.run([x, y]) == [x, (x + y) % (2**n)]


def test_arithmetic_op_helper_run_op(context: Context):
    ans = ArithmeticOpTester.run_op(
        "Std.Arithmetic.IncByLE", [10, 10], [22, 30], context
    )
    assert ans == [22, 52]


def test_arithmetic_op_helper_run_unary_op(context: Context):
    context.eval("""
    operation BitwiseNegate(qx: Qubit[]) : Unit {
        ApplyToEach(X, qx);
    }
    """)
    ans = ArithmeticOpTester.run_unary_op(context.code.BitwiseNegate, 10, 0)
    assert ans == 1023


def test_classical_function_unary(context: Context):
    context.eval("""
    operation IncrementClassical(qx: Qubit[]) : Unit {
        Std.ArithmeticTestUtils.ApplyClassicalFunction((x) -> (x + 1L), qx);
    }
    """)

    tester = ArithmeticOpTester(context.code.IncrementClassical, [5], context)
    assert tester.run([5]) == [6]
    assert tester.run([31]) == [0]


def test_classical_function_binary(context: Context):
    n = 10
    context.eval("""
    operation AddClassical(qx: Qubit[], qy: Qubit[]) : Unit {
        Std.ArithmeticTestUtils.ApplyClassicalFunction2((x, y) -> (x, x+y), qx, qy);
    }
    """)

    tester = ArithmeticOpTester("AddClassical", [n, n], context)
    for _ in range(10):
        x = random.randint(0, 2**n - 1)
        y = random.randint(0, 2**n - 1)
        assert tester.run([x, y]) == [x, (x + y) % 2**n]


def test_classical_function_ternary(context: Context):
    n = 10
    context.eval("""
    import Std.ArithmeticTestUtils.ApplyClassicalFunction3;
    operation MultiplyClassical(qx: Qubit[], qy: Qubit[], qz: Qubit[]) : Unit {
        ApplyClassicalFunction3((x, y, z) -> (x, y, z^^^(x*y)), qx, qy, qz);
    }
    """)

    tester = ArithmeticOpTester("MultiplyClassical", [n, n, 2 * n], context)
    for _ in range(10):
        x = random.randint(0, 2**n - 1)
        y = random.randint(0, 2**n - 1)
        z = random.randint(0, 2 ** (2 * n) - 1)

        assert tester.run([x, y, z]) == [x, y, (x * y) ^ z]


def test_classical_function_controlled(context: Context):
    context.eval("""
    import Std.ArithmeticTestUtils.ApplyClassicalFunction;
    operation IncrementControlled(ctrl: Qubit[], qx: Qubit[]) : Unit {
        Controlled ApplyClassicalFunction(ctrl, ((x) -> (x + 1L), qx));
    }
    """)

    tester = ArithmeticOpTester(context.code.IncrementControlled, [1, 5], context)
    assert tester.run([0, 5]) == [0, 5]
    assert tester.run([1, 5]) == [1, 6]
