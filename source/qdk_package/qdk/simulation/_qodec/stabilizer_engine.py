# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Persistent coherent stabilizer operation orchestration."""

from __future__ import annotations

import math
import random
from collections.abc import Sequence
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, cast

from paulimer import CliffordUnitary, DensePauli, SparsePauli, UnitaryOpcode

if TYPE_CHECKING:
    from paulimer import PauliCharacter

__all__ = ["StabilizerEngine"]

_AMPLITUDE_EPSILON = 1e-14
_DEFAULT_MAX_BRANCHES = 1 << 20

_ONE_QUBIT_GATES = frozenset(
    {"x", "y", "z", "h", "s", "s_adj", "t", "t_adj", "sx", "sx_adj", "mov"}
)
_TWO_QUBIT_GATES = frozenset({"cx", "cy", "cz", "swap"})
_ONE_QUBIT_ROTATIONS = frozenset({"rx", "ry", "rz"})
_TWO_QUBIT_ROTATIONS = frozenset({"rxx", "ryy", "rzz"})

_CLIFFORD_GATES = {
    "h": UnitaryOpcode.Hadamard,
    "s": UnitaryOpcode.SqrtZ,
    "s_adj": UnitaryOpcode.SqrtZInv,
    "sx": UnitaryOpcode.SqrtX,
    "sx_adj": UnitaryOpcode.SqrtXInv,
    "cx": UnitaryOpcode.ControlledX,
    "cz": UnitaryOpcode.ControlledZ,
    "swap": UnitaryOpcode.Swap,
}


@dataclass(frozen=True)
class _Projection:
    amplitudes: dict[int, complex]
    probability: float
    # (pivot, CNOT targets, S exponent) for the frame change V, if any.
    change: tuple[int, tuple[int, ...], int] | None


class StabilizerEngine:
    """Represents coherent stabilizer branches in one common Clifford frame."""

    def __init__(
        self,
        num_qubits: int,
        *,
        seed: Optional[int] = None,
        max_branches: int = _DEFAULT_MAX_BRANCHES,
    ) -> None:
        if num_qubits < 0:
            raise ValueError("num_qubits must be nonnegative")
        if max_branches < 1:
            raise ValueError("max_branches must be positive")
        self._num_qubits = num_qubits
        self._max_branches = max_branches
        self._frame = CliffordUnitary.identity(num_qubits)
        self._amplitudes = {0: 1.0 + 0.0j}
        self._rng = random.Random(seed)
        self._closed = False

    @property
    def branch_count(self) -> int:
        return len(self._amplitudes)

    def apply(
        self, operation: str, targets: Sequence[int], *, angle: float | None = None
    ) -> None:
        """Validate and synchronously apply one named physical operation."""
        self._ensure_open()
        rotations = _ONE_QUBIT_ROTATIONS | _TWO_QUBIT_ROTATIONS
        gates = _ONE_QUBIT_GATES | _TWO_QUBIT_GATES
        if operation not in rotations | gates:
            raise NotImplementedError(f"unsupported operation {operation!r}")
        arity = 2 if operation in _TWO_QUBIT_GATES | _TWO_QUBIT_ROTATIONS else 1
        if len(targets) != arity:
            raise ValueError(f"operation {operation!r} expects {arity} qubits")
        self._validate_targets(targets)
        if operation in rotations and angle is None:
            raise ValueError(f"operation {operation!r} requires an angle")
        if operation in gates and angle is not None:
            raise ValueError(f"operation {operation!r} does not accept an angle")

        if operation == "mov":
            return
        if operation in "xyz":
            character = cast("PauliCharacter", operation.upper())
            self._frame.left_mul_pauli(SparsePauli({targets[0]: character}))
            return
        if operation in _CLIFFORD_GATES:
            self._frame.left_mul(_CLIFFORD_GATES[operation], targets)
            return
        if operation == "cy":
            target = targets[1]
            self._frame.left_mul(UnitaryOpcode.SqrtZInv, [target])
            self._frame.left_mul(UnitaryOpcode.ControlledX, targets)
            self._frame.left_mul(UnitaryOpcode.SqrtZ, [target])
            return

        rotation_angle = angle
        if operation == "t":
            rotation_angle = math.pi / 4.0
        elif operation == "t_adj":
            rotation_angle = -math.pi / 4.0
        assert rotation_angle is not None
        basis = cast(
            "PauliCharacter",
            (operation[-1] if operation.startswith("r") else "z").upper(),
        )
        self._rotate(
            rotation_angle,
            SparsePauli({target: basis for target in targets}),
        )

    def measure(self, target: int) -> int:
        self._ensure_open()
        self._validate_targets((target,))
        observable = SparsePauli.z(target)
        probability_one = self._projection(observable, 1).probability
        outcome = int(self._rng.random() < probability_one)
        self._commit(self._projection(observable, outcome))
        return outcome

    def outcome_probability(self, target: int, outcome: int) -> float:
        self._ensure_open()
        self._validate_targets((target,))
        if outcome not in (0, 1):
            raise ValueError("measurement outcome must be zero or one")
        return self._projection(SparsePauli.z(target), outcome).probability

    def reset(self, target: int) -> None:
        if self.measure(target):
            self.apply("x", (target,))

    def close(self) -> None:
        self._closed = True
        self._amplitudes.clear()

    def _rotate(self, angle: float, observable: SparsePauli) -> None:
        if not math.isfinite(angle):
            raise ValueError("rotation angle must be finite")
        preimage = self._frame.preimage_of(observable)
        updated = self._linear_result(
            complex(math.cos(angle / 2.0)),
            complex(0.0, -math.sin(angle / 2.0)),
            preimage,
        )
        self._amplitudes = self._normalized(updated)

    def _projection(self, observable: SparsePauli, outcome: int) -> _Projection:
        """Project onto ``outcome`` of ``observable`` without adding branches.

        With state ``F |phi>``, the projector pulls back to ``(I + s P) / 2`` for
        the preimage ``P = F^dagger O F`` and ``s = (-1)^outcome``. A diagonal
        ``P`` only filters branches. Otherwise ``P |b> = phase_b |b ^ x>`` pairs
        each branch with its partner, and ``|c> + beta_c |c ^ x>`` equals
        ``sqrt(2) V |d>`` for one Clifford ``V`` built from a pivot ``k`` in the
        X part: H on ``k``, then ``S^m`` on ``k``, then CNOTs from ``k`` across
        ``x``. Moving ``V`` into the frame leaves one branch per pair, so
        measurements never increase the branch count; only rotations do.
        """
        preimage = self._frame.preimage_of(observable)
        sign = -1 if outcome else 1
        characters = preimage.characters
        flips = [
            qubit for qubit, character in enumerate(characters) if character in "XY"
        ]
        if not flips:
            projected = self._linear_result(0.5 + 0.0j, 0.5 * sign + 0.0j, preimage)
            return _Projection(projected, self._norm(projected), None)
        pivot = flips[0]
        mask = sum(1 << qubit for qubit in flips)
        paired: dict[int, complex] = {}
        for basis, amplitude in self._amplitudes.items():
            if (basis >> pivot) & 1:
                _, phase = self._pauli_image(preimage, basis)
                self._add(paired, basis ^ mask, sign * phase * amplitude)
            else:
                self._add(paired, basis, amplitude)
        exponent: int | None = None
        projected: dict[int, complex] = {}
        for basis, amplitude in paired.items():
            if abs(amplitude) <= _AMPLITUDE_EPSILON:
                continue
            _, phase = self._pauli_image(preimage, basis)
            relative = sign * phase
            if exponent is None:
                exponent = 0 if abs(relative.imag) < 0.5 else 1
            real = relative * (-1j if exponent else 1)
            if abs(real.imag) > 1e-9 or abs(abs(real.real) - 1) > 1e-9:
                raise RuntimeError("stabilizer projection produced an invalid phase")
            reduced = basis if real.real > 0 else basis | (1 << pivot)
            self._add(projected, reduced, amplitude / math.sqrt(2.0))
        change = None if exponent is None else (pivot, tuple(flips[1:]), exponent)
        return _Projection(projected, self._norm(projected), change)

    def _commit(self, projection: _Projection) -> None:
        if projection.probability <= _AMPLITUDE_EPSILON:
            raise RuntimeError("measurement selected a zero-probability outcome")
        if projection.change is not None:
            pivot, targets, exponent = projection.change
            # The frame becomes F V; paulimer multiplies on the left only, so
            # form (V^dagger F^dagger)^dagger.
            inverse_change = CliffordUnitary.identity(self._num_qubits)
            for target in targets:
                inverse_change.left_mul(UnitaryOpcode.ControlledX, [pivot, target])
            if exponent:
                inverse_change.left_mul(UnitaryOpcode.SqrtZInv, [pivot])
            inverse_change.left_mul(UnitaryOpcode.Hadamard, [pivot])
            inverse_frame = self._frame.inverse()
            inverse_frame.left_mul_clifford(
                inverse_change, list(range(self._num_qubits))
            )
            self._frame = inverse_frame.inverse()
        scale = 1.0 / math.sqrt(projection.probability)
        self._amplitudes = {
            basis: amplitude * scale
            for basis, amplitude in projection.amplitudes.items()
        }

    @staticmethod
    def _norm(amplitudes: dict[int, complex]) -> float:
        return sum(abs(amplitude) ** 2 for amplitude in amplitudes.values())

    def _linear_result(
        self,
        identity_coefficient: complex,
        pauli_coefficient: complex,
        pauli: DensePauli,
    ) -> dict[int, complex]:
        result: dict[int, complex] = {}
        for basis, amplitude in self._amplitudes.items():
            self._add(result, basis, identity_coefficient * amplitude)
            image, phase = self._pauli_image(pauli, basis)
            self._add(result, image, pauli_coefficient * phase * amplitude)
        result = {
            basis: amplitude
            for basis, amplitude in result.items()
            if abs(amplitude) > _AMPLITUDE_EPSILON
        }
        if len(result) > self._max_branches:
            raise RuntimeError(
                f"stabilizer branching exceeded branch limit {self._max_branches}"
            )
        return result

    @staticmethod
    def _add(result: dict[int, complex], basis: int, amplitude: complex) -> None:
        result[basis] = result.get(basis, 0.0j) + amplitude

    @staticmethod
    def _pauli_image(pauli: DensePauli, basis: int) -> tuple[int, complex]:
        image = basis
        phase = pauli.phase
        for qubit, character in enumerate(pauli.characters):
            bit = (basis >> qubit) & 1
            if character == "Z":
                phase *= -1 if bit else 1
            elif character == "X":
                image ^= 1 << qubit
            elif character == "Y":
                image ^= 1 << qubit
                phase *= -1j if bit else 1j
        return image, phase

    @staticmethod
    def _normalized(amplitudes: dict[int, complex]) -> dict[int, complex]:
        norm = math.sqrt(sum(abs(amplitude) ** 2 for amplitude in amplitudes.values()))
        if norm <= _AMPLITUDE_EPSILON:
            raise RuntimeError("stabilizer branching produced a zero state")
        return {basis: amplitude / norm for basis, amplitude in amplitudes.items()}

    def _validate_targets(self, targets: Sequence[int]) -> None:
        if any(target < 0 or target >= self._num_qubits for target in targets):
            raise ValueError("operation has an out-of-range qubit")
        if len(set(targets)) != len(targets):
            raise ValueError("operation requires distinct qubits")

    def _ensure_open(self) -> None:
        if self._closed:
            raise RuntimeError("stabilizer engine is closed")
