# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import math
import warnings

import pytest

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qdk.circuit import AngleExpression, Parameter, QuantumCircuit  # noqa: E402


def test_rotation_accepts_a_literal_angle():
    circuit = QuantumCircuit(1)
    circuit.rx(math.pi / 2, 0)

    assert circuit[0].name == "rx"
    assert circuit[0].params == [pytest.approx(math.pi / 2)]
    assert circuit.parameters == []


def test_rotation_accepts_a_symbolic_angle():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rz(theta, 0)

    assert circuit[0].name == "rz"
    assert circuit.parameters == [theta]


def test_instruction_view_reports_the_parameter_not_a_placeholder():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rx(theta, 0)

    stored = circuit[0].params[0]
    assert isinstance(stored, Parameter)
    assert stored.name == "theta"


def test_declared_but_unused_parameters_are_not_reported():
    circuit = QuantumCircuit(1)
    circuit.parameter("unused")
    used = circuit.parameter("used")
    circuit.rz(used, 0)

    assert circuit.parameters == [used], "binding an unused parameter would do nothing"


def test_binding_produces_a_fully_bound_circuit():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rx(theta, 0)
    circuit.rz(1.0, 0)

    bound = circuit.assign_parameters([(theta, 0.25)])

    assert bound.parameters == []
    assert bound[0].params == [pytest.approx(0.25)]
    assert bound[1].params == [pytest.approx(1.0)], "literal angles must be untouched"


def test_binding_leaves_the_original_circuit_unchanged():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rx(theta, 0)

    circuit.assign_parameters([(theta, 0.5)])

    assert circuit.parameters == [theta], "binding returns a new circuit"


def test_missing_binding_names_the_parameter():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rx(theta, 0)

    with pytest.raises(ValueError, match="theta"):
        circuit.assign_parameters([])


def test_two_qubit_rotations_take_angle_first():
    circuit = QuantumCircuit(2)
    theta = circuit.parameter("theta")
    circuit.rzz(theta, 0, 1)
    circuit.crx(0.5, 0, 1)

    assert circuit[0].name == "rzz"
    assert circuit[0].qubits == [0, 1]
    assert circuit[1].name == "crx"


def test_parameters_are_comparable_and_hashable():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rx(theta, 0)

    same = circuit.parameters[0]
    assert same == theta
    assert len({theta, same}) == 1


def test_parameter_repr_names_the_symbol():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    assert repr(theta) == "Parameter('theta')"


if __name__ == "__main__":
    pytest.main([__file__])


def test_parameters_support_arithmetic():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rx(theta * 2 + math.pi, 0)

    stored = circuit[0].params[0]
    assert isinstance(stored, AngleExpression)
    assert repr(stored) == f"AngleExpression('((theta * 2) + {math.pi})')"


def test_arithmetic_works_from_either_side():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rx(2 * theta, 0)
    circuit.ry(-theta, 0)
    circuit.rz(1 - theta, 0)

    assert repr(circuit[0].params[0]) == "AngleExpression('(2 * theta)')"
    assert repr(circuit[1].params[0]) == "AngleExpression('-(theta)')"
    assert repr(circuit[2].params[0]) == "AngleExpression('(1 - theta)')"


def test_binding_folds_an_arithmetic_angle():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rx(theta * 2 + 1, 0)

    assert circuit.parameters == [theta]

    bound = circuit.assign_parameters([(theta, 0.5)])

    assert bound[0].params == [2.0]


def test_a_loop_index_is_not_a_circuit_parameter():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    with circuit.for_loop([0.0, 1.0]) as index:
        circuit.rx(index * theta, 0)

    # The loop supplies the index, so only the caller's own parameter is asked
    # for, and binding it leaves the index in place.
    assert circuit.parameters == [theta]

    bound = circuit.assign_parameters([(theta, 2.0)])
    stored = bound.body_of(0)[0].params[0]
    assert repr(stored) == f"AngleExpression('({index.name} * 2)')"


def test_an_arithmetic_angle_survives_a_round_trip():
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rx(theta / 2, 0)

    back = QuantumCircuit.from_bytes(circuit.to_bytes())

    assert repr(back[0].params[0]) == repr(circuit[0].params[0])
    assert back.to_bytes() == circuit.to_bytes()
