import math

import pytest
from qdk import Context, OperationTestHelper


@pytest.fixture(scope="session")
def ctx():
    yield Context()


def _assert_states_close(state1: list[complex], state2: list[complex]):
    assert len(state1) == len(state2)
    for i in range(len(state1)):
        assert abs(state1[i] - state2[i]) < 1e-9


def test_get_action_on_state(ctx):
    ctx.eval("""
    operation MyOp1(q: Qubit[]) : Unit {
        H(q[0]);
        CNOT(q[0], q[1]);
        Z(q[1]);
    }
    """)
    helper = OperationTestHelper(ctx)

    vector = helper.get_action_on_state(ctx.code.MyOp1, num_qubits=2)
    s = 0.5**0.5
    _assert_states_close(vector, [s, 0, 0, -s])


def test_get_action_on_state_with_two_registers(ctx):
    ctx.eval("""
    operation MyOp2(q1: Qubit[], q2: Qubit[]) : Unit {
        H(q1[0]);
        CNOT(q1[0], q2[0]);
    }

    operation MyOp2_TestHelper(q: Qubit[]) : Unit {
        let n = Length(q);
        MyOp2(q[0..n/2-1], q[n/2..n-1]);
    }
    """)
    helper = OperationTestHelper(ctx)

    vector = helper.get_action_on_state(ctx.code.MyOp2_TestHelper, num_qubits=4)
    s = 0.5**0.5
    _assert_states_close(vector, [s, 0, 0, 0, 0, 0, 0, 0, 0, 0, s, 0, 0, 0, 0, 0])


def test_get_action_on_state_with_partial_trace(ctx):
    ctx.eval("""
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
    helper = OperationTestHelper(ctx)

    vector = helper.get_action_on_state(ctx.code.MyOp3_TestHelper, num_qubits=2)
    s = 0.5**0.5
    _assert_states_close(vector, [s, 0, s, 0])


def test_get_action_on_state_with_non_zero_initial_state(ctx):
    ctx.eval("""
    operation MyOp4(q: Qubit[])  : Unit is Adj {
        CNOT(q[0], q[1]);
        H(q[0]);
    }

    operation MyOp4_TestHelper(initial_state: Double[]) : (Qubit[] => Unit) {
        return q => {
          Std.StatePreparation.PreparePureStateD(initial_state, q);
          MyOp4(q);
        }
    }
    """)
    helper = OperationTestHelper(ctx)

    s = 0.5**0.5
    vector = helper.get_action_on_state(
        ctx.code.MyOp4_TestHelper([s, 0, 0, s]), num_qubits=2
    )
    _assert_states_close(vector, [1, 0, 0, 0])


def test_get_action_on_state_with_parameters(ctx):
    ctx.eval("""
    operation MyOp5(qs: Qubit[], angle: Double)  : Unit is Adj {
      for q in qs {
        Rx(angle, q);
      }
    }

    operation MyOp5_TestHelper(angle: Double) : (Qubit[] => Unit) {
        MyOp5(_, angle)
    }
    """)
    helper = OperationTestHelper(ctx)

    vector = helper.get_action_on_state(ctx.code.MyOp5_TestHelper(0.3), num_qubits=2)
    c = math.cos(0.3 / 2)
    s = math.sin(0.3 / 2)
    _assert_states_close(vector, [c * c, -1j * c * s, -1j * c * s, -(s * s)])


def test_get_action_on_state_with_parameterized_callable(ctx):
    ctx.eval("""
    operation MyOp5(qs: Qubit[], angle: Double)  : Unit is Adj {
      for q in qs {
        Rx(angle, q);
      }
    }
    """)
    helper = OperationTestHelper(ctx)

    vector = helper.get_action_on_state(ctx.eval("MyOp5(_, 0.3)"), num_qubits=2)
    c = math.cos(0.3 / 2)
    s = math.sin(0.3 / 2)
    _assert_states_close(vector, [c * c, -1j * c * s, -1j * c * s, -(s * s)])
