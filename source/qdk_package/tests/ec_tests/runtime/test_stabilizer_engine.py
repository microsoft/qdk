import cmath
import math
import random

import numpy as np
import pytest
from paulimer import SparsePauli

from qdk.simulation._qodec.stabilizer_engine import StabilizerEngine

_ONE_QUBIT = {
    "x": np.array([[0, 1], [1, 0]]),
    "y": np.array([[0, -1j], [1j, 0]]),
    "z": np.diag([1, -1]),
    "h": np.array([[1, 1], [1, -1]]) / math.sqrt(2),
    "s": np.diag([1, 1j]),
    "s_adj": np.diag([1, -1j]),
    "t": np.diag([1, cmath.exp(1j * math.pi / 4)]),
}


def _apply(state, targets, matrix):
    """Apply ``matrix`` to little-endian ``targets`` of a state vector."""
    width = int(math.log2(len(state)))
    tensor = state.reshape([2] * width)
    axes = [width - 1 - target for target in targets]
    matrix = matrix.reshape([2] * (2 * len(targets)))
    moved = np.tensordot(
        matrix, tensor, axes=(list(range(len(targets), 2 * len(targets))), axes)
    )
    return np.moveaxis(moved, list(range(len(targets))), axes).reshape(-1)


def _controlled(matrix):
    return np.block([[np.eye(2), np.zeros((2, 2))], [np.zeros((2, 2)), matrix]])


def _probability_one(state, target):
    return sum(
        abs(amplitude) ** 2
        for index, amplitude in enumerate(state)
        if (index >> target) & 1
    )


def _expectation(engine, characters):
    operator = SparsePauli(
        {
            qubit: character
            for qubit, character in enumerate(characters)
            if character != "I"
        }
        or "I"
    )
    preimage = engine._frame.preimage_of(operator)
    total = 0j
    for basis, amplitude in engine._amplitudes.items():
        image, phase = engine._pauli_image(preimage, basis)
        total += np.conj(engine._amplitudes.get(image, 0)) * phase * amplitude
    return total.real


@pytest.mark.parametrize("seed", range(40))
def test_stabilizer_engine_matches_a_state_vector(seed):
    rng = random.Random(seed)
    width = rng.randint(1, 4)
    engine = StabilizerEngine(width, seed=seed)
    state = np.zeros(2**width, complex)
    state[0] = 1
    for _ in range(30):
        choice = rng.random()
        if choice < 0.5:
            name, target = rng.choice(list(_ONE_QUBIT)), rng.randrange(width)
            engine.apply(name, (target,))
            state = _apply(state, [target], _ONE_QUBIT[name])
        elif choice < 0.65 and width > 1:
            name = rng.choice(["cx", "cz"])
            control, target = rng.sample(range(width), 2)
            engine.apply(name, (control, target))
            gate = _controlled(_ONE_QUBIT["x" if name == "cx" else "z"])
            state = _apply(state, [control, target], gate)
        elif choice < 0.7:
            target, angle = rng.randrange(width), rng.uniform(-3, 3)
            engine.apply("rz", (target,), angle=angle)
            phase = cmath.exp(1j * angle / 2)
            state = _apply(state, [target], np.diag([1 / phase, phase]))
        else:
            target = rng.randrange(width)
            expected = _probability_one(state, target)
            assert engine.outcome_probability(target, 1) == pytest.approx(expected)
            outcome = engine.measure(target)
            keep = np.array(
                [((index >> target) & 1) == outcome for index in range(len(state))]
            )
            state = np.where(keep, state, 0)
            state /= np.linalg.norm(state)
    for _ in range(10):
        characters = "".join(rng.choice("IXYZ") for _ in range(width))
        reference = state
        for qubit, character in enumerate(characters):
            if character != "I":
                matrix = _ONE_QUBIT[character.lower()]
                reference = _apply(reference, [qubit], matrix)
        expected = np.vdot(state, reference).real
        assert _expectation(engine, characters) == pytest.approx(expected, abs=1e-9)


def test_stabilizer_engine_measurements_never_add_branches():
    engine = StabilizerEngine(12, seed=7)
    for qubit in range(12):
        engine.apply("h", (qubit,))
        engine.measure(qubit)
    assert engine.branch_count == 1

    engine = StabilizerEngine(6, seed=7)
    engine.apply("h", (0,))
    for qubit in range(1, 6):
        engine.apply("cx", (0, qubit))
    engine.apply("t", (0,))
    engine.apply("h", (0,))
    for qubit in range(6):
        before = engine.branch_count
        engine.measure(qubit)
        assert engine.branch_count <= before
