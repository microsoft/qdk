from math import cos, sin

import numpy as np
import pytest

from qdk.simulation._qodec.clifford_semantics import pauli
from qdk.simulation._qodec.full_state_engine import FullStateEngine
from qdk.simulation._qodec.quantum_instruments import (
    Observation,
    PauliRotation,
    Stabilization,
    TraceOut,
)
from .test_layer_layout import drive


def pauli_matrix(operator):
    matrices = {
        "I": np.eye(2),
        "X": np.array([[0, 1], [1, 0]]),
        "Y": np.array([[0, -1j], [1j, 0]]),
        "Z": np.diag([1, -1]),
    }
    result = np.ones((1, 1), dtype=complex)
    for character in reversed(operator.characters):
        result = np.kron(result, matrices[character])
    return operator.phase * result


def execute(engine, operation):
    if operation.name in ("prepare", "discard"):
        engine.reset(operation.targets[0])
    elif operation.name == "measure":
        return (bool(engine.measure(operation.targets[0])),)
    else:
        engine.apply(operation.name, operation.targets, angle=operation.angle)
    return ()


def assert_same_state(actual, expected):
    assert np.allclose(
        np.outer(actual, np.conj(actual)),
        np.outer(expected, np.conj(expected)),
        atol=1e-10,
    )


@pytest.mark.parametrize("gate", ["cx", "cy", "cz"])
@pytest.mark.parametrize("control, target", [(0, 2), (2, 0)])
def test_full_state_controlled_gates_preserve_target_order_and_phase(
    gate, control, target
):
    engine = FullStateEngine(3, seed=7)
    try:
        engine.apply("h", (control,))
        if gate == "cz":
            engine.apply("x", (target,))
        engine.apply(gate, (control, target))

        expected = np.zeros(8, dtype=complex)
        expected[(1 << target) if gate == "cz" else 0] = np.sqrt(0.5)
        phase = {"cx": 1, "cy": 1j, "cz": -1}[gate]
        expected[(1 << control) | (1 << target)] = phase * np.sqrt(0.5)
        assert_same_state(engine.state(), expected)
    finally:
        engine.close()


@pytest.mark.parametrize(
    "axis", ["X_0 Y_1 Z_2", "-X_0 Y_1 Z_2", "Y_2", "-I", "Z_0 Z_1"]
)
def test_pauli_rotations_match_independent_matrix_evolution(axis):
    from qdk.simulation._qodec.quantum_lowering import lower_instrument

    engine = FullStateEngine(3, seed=7)
    try:
        engine.apply("h", (0,))
        engine.apply("cx", (0, 1))
        engine.apply("ry", (2,), angle=0.37)
        before = np.asarray(engine.state())
        operator = pauli(axis, 3)
        angle = 0.61
        result = drive(
            lower_instrument(PauliRotation(operator, angle)),
            lambda operation: execute(engine, operation),
        )
        expected = cos(angle / 2) * before - 1j * sin(angle / 2) * (
            pauli_matrix(operator) @ before
        )
        assert result == ()
        assert_same_state(engine.state(), expected)
    finally:
        engine.close()


@pytest.mark.parametrize("axis", ["Z_0 Z_1", "-X_0 Y_1", "Y_0 Z_2", "I", "-I"])
def test_joint_observations_preserve_the_correct_conditional_state(axis):
    from qdk.simulation._qodec.quantum_lowering import lower_instrument

    engine = FullStateEngine(3, seed=7)
    try:
        for target in range(3):
            engine.apply("ry", (target,), angle=0.37 + target * 0.2)
        engine.apply("cx", (1, 2))
        before = np.asarray(engine.state())
        operator = pauli(axis, 3)
        (outcome,) = drive(
            lower_instrument(Observation(operator)),
            lambda operation: execute(engine, operation),
        )
        projected = (
            before + (-1 if outcome else 1) * (pauli_matrix(operator) @ before)
        ) / 2
        assert np.linalg.norm(projected) > 0
        assert_same_state(engine.state(), projected / np.linalg.norm(projected))
    finally:
        engine.close()


@pytest.mark.parametrize(
    "operators",
    [
        ("X_0 X_1", "Z_0 Z_1"),
        ("X_0 Z_1", "Z_0 X_1 Z_2", "Z_1 X_2"),
        ("-Z_0", "X_1"),
    ],
)
def test_stabilization_preserves_preceding_commuting_constraints(operators):
    from qdk.simulation._qodec.quantum_lowering import lower_instrument

    for seed in range(5):
        engine = FullStateEngine(3, seed=seed)
        try:
            engine.apply("ry", (0,), angle=0.71)
            prepared = tuple(pauli(operator, 3) for operator in operators)
            assert (
                drive(
                    lower_instrument(Stabilization(prepared)),
                    lambda operation: execute(engine, operation),
                )
                == ()
            )
            state = np.asarray(engine.state())
            for operator in prepared:
                assert np.allclose(pauli_matrix(operator) @ state, state)
        finally:
            engine.close()


def test_trace_out_does_not_export_a_measurement_or_preserve_entanglement():
    from qdk.simulation._qodec.quantum_lowering import lower_instrument

    outcomes = set()
    for seed in range(20):
        engine = FullStateEngine(2, seed=seed)
        try:
            engine.apply("h", (0,))
            engine.apply("cx", (0, 1))
            assert (
                drive(
                    lower_instrument(TraceOut((0,))),
                    lambda operation: execute(engine, operation),
                )
                == ()
            )
            outcomes.add(engine.measure(1))
        finally:
            engine.close()
    assert outcomes == {0, 1}
