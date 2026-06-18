import math

from qdk import Context
from qdk.test_utils import dump_operation_on_state


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
