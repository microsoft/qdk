# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Persistent full-state operation orchestration."""

from __future__ import annotations

import cmath
import math
from collections.abc import Callable
from typing import Optional, Sequence

from qdk.simulation import Instrument, Operation, StateVectorSimulator

__all__ = ["FullStateEngine"]

_Matrix = list[list[complex]]
_SQRT1_2 = math.sqrt(0.5)

_GATES: dict[str, _Matrix | None] = {
    "x": [[0, 1], [1, 0]],
    "y": [[0, -1j], [1j, 0]],
    "z": [[1, 0], [0, -1]],
    "h": [[_SQRT1_2, _SQRT1_2], [_SQRT1_2, -_SQRT1_2]],
    "s": [[1, 0], [0, 1j]],
    "s_adj": [[1, 0], [0, -1j]],
    "t": [[1, 0], [0, cmath.exp(1j * math.pi / 4)]],
    "t_adj": [[1, 0], [0, cmath.exp(-1j * math.pi / 4)]],
    "sx": [[0.5 + 0.5j, 0.5 - 0.5j], [0.5 - 0.5j, 0.5 + 0.5j]],
    "sx_adj": [[0.5 - 0.5j, 0.5 + 0.5j], [0.5 + 0.5j, 0.5 - 0.5j]],
    "mov": None,
    "cx": [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 0, 1], [0, 0, 1, 0]],
    "cy": [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 0, -1j], [0, 0, 1j, 0]],
    "cz": [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, -1]],
    "swap": [[1, 0, 0, 0], [0, 0, 1, 0], [0, 1, 0, 0], [0, 0, 0, 1]],
}


def _rx(angle: float) -> _Matrix:
    cosine, sine = math.cos(angle / 2), math.sin(angle / 2)
    return [[cosine, -1j * sine], [-1j * sine, cosine]]


def _ry(angle: float) -> _Matrix:
    cosine, sine = math.cos(angle / 2), math.sin(angle / 2)
    return [[cosine, -sine], [sine, cosine]]


def _rz(angle: float) -> _Matrix:
    return [[cmath.exp(-0.5j * angle), 0], [0, cmath.exp(0.5j * angle)]]


def _rxx(angle: float) -> _Matrix:
    cosine, sine = math.cos(angle / 2), -1j * math.sin(angle / 2)
    return [
        [cosine, 0, 0, sine],
        [0, cosine, sine, 0],
        [0, sine, cosine, 0],
        [sine, 0, 0, cosine],
    ]


def _ryy(angle: float) -> _Matrix:
    cosine, sine = math.cos(angle / 2), -1j * math.sin(angle / 2)
    return [
        [cosine, 0, 0, -sine],
        [0, cosine, sine, 0],
        [0, sine, cosine, 0],
        [-sine, 0, 0, cosine],
    ]


def _rzz(angle: float) -> _Matrix:
    minus, plus = cmath.exp(-0.5j * angle), cmath.exp(0.5j * angle)
    return [[minus, 0, 0, 0], [0, plus, 0, 0], [0, 0, plus, 0], [0, 0, 0, minus]]


_ROTATIONS: dict[str, Callable[[float], _Matrix]] = {
    "rx": _rx,
    "ry": _ry,
    "rz": _rz,
    "rxx": _rxx,
    "ryy": _ryy,
    "rzz": _rzz,
}
_TWO_QUBIT_OPERATIONS = frozenset({"cx", "cy", "cz", "swap", "rxx", "ryy", "rzz"})
_MZ = Instrument([Operation([[[1, 0], [0, 0]]]), Operation([[[0, 0], [0, 1]]])])


class FullStateEngine:
    """Runs noiseless operations on one persistent native state vector."""

    def __init__(self, num_qubits: int, *, seed: Optional[int] = None) -> None:
        if num_qubits < 0:
            raise ValueError("num_qubits must be nonnegative")
        self._sim = StateVectorSimulator(num_qubits, seed)
        self._num_qubits = num_qubits
        self._closed = False

    def apply(
        self, operation: str, targets: Sequence[int], *, angle: float | None = None
    ) -> None:
        """Validate and synchronously apply one named physical operation."""
        self._ensure_open()
        if operation not in _ROTATIONS and operation not in _GATES:
            raise NotImplementedError(f"unsupported operation {operation!r}")
        arity = 2 if operation in _TWO_QUBIT_OPERATIONS else 1
        if len(targets) != arity:
            raise ValueError(f"operation {operation!r} expects {arity} qubits")
        if any(target < 0 or target >= self._num_qubits for target in targets):
            raise ValueError(f"operation {operation!r} has an out-of-range qubit")
        if len(set(targets)) != len(targets):
            raise ValueError(f"operation {operation!r} requires distinct qubits")
        if operation in _ROTATIONS and angle is None:
            raise ValueError(f"operation {operation!r} requires an angle")
        if operation in _GATES and angle is not None:
            raise ValueError(f"operation {operation!r} does not accept an angle")

        matrix = _GATES[operation] if angle is None else _ROTATIONS[operation](angle)
        if matrix is not None:
            self._sim.apply_operation(Operation([matrix]), list(reversed(targets)))

    def measure(self, target: int) -> int:
        self._ensure_open()
        self._validate_target(target)
        return self._sim.sample_instrument(_MZ, [target])

    def reset(self, target: int) -> None:
        if self.measure(target):
            self.apply("x", (target,))

    def state(self) -> list[complex]:
        self._ensure_open()
        state = self._sim.get_state()
        if state is None:
            raise RuntimeError("full-state engine entered an invalid state")
        return state.data()

    def close(self) -> None:
        self._closed = True

    def _ensure_open(self) -> None:
        if self._closed:
            raise RuntimeError("full-state engine is closed")

    def _validate_target(self, target: int) -> None:
        if target < 0 or target >= self._num_qubits:
            raise ValueError(f"qubit {target} is out of range")
