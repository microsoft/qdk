"""Deterministic parity checks on program outcome indices."""

from __future__ import annotations

from binar import BitMatrix, BitVector
from paulimer import PauliGroup
from qodec.circuits import Program

from .propagation.interpreter import walk_for_outcome_code


class OutcomeCode:
    def __init__(self, check_matrix: BitMatrix) -> None:
        self._matrix = check_matrix

    @property
    def check_matrix(self) -> BitMatrix:
        return self._matrix

    @property
    def check_count(self) -> int:
        return self._matrix.row_count

    @property
    def measurement_count(self) -> int:
        return self._matrix.column_count

    def checks(self) -> list[frozenset[int]]:
        return [
            frozenset(index for index in range(self._matrix.column_count) if row[index])
            for row in self._matrix.rows
        ]

    def __len__(self) -> int:
        return self._matrix.row_count

    def __repr__(self) -> str:
        return f"OutcomeCode({self.checks()})"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, OutcomeCode):
            return NotImplemented
        return self.checks() == other.checks()


def outcome_code_of(
    program: Program,
    input_stabilizers: PauliGroup | None = None,
) -> OutcomeCode:
    stabilizers = (
        list(input_stabilizers.generators) if input_stabilizers is not None else ()
    )
    result = walk_for_outcome_code(program, stabilizers)
    simulation = result.simulation
    matrix = simulation.outcome_matrix
    total_measurements = matrix.row_count
    offset = result.hidden_count
    random_indicator = simulation.random_outcome_indicator
    measurement_count = result.outcome_count
    rank_profile = [
        index for index in range(total_measurements) if random_indicator[index]
    ]
    if not rank_profile:
        return OutcomeCode(BitMatrix.identity(measurement_count))
    deterministic_rows = [
        index
        for index in range(offset, total_measurements)
        if not random_indicator[index]
    ]
    rows = []
    for row in deterministic_rows:
        bits = [False] * measurement_count
        bits[row - offset] = True
        for column, measurement in enumerate(rank_profile):
            if matrix[row, column] and measurement >= offset:
                bits[measurement - offset] = True
        rows.append(BitVector(bits))
    if not rows:
        return OutcomeCode(BitMatrix.zeros(0, measurement_count))
    return OutcomeCode(BitMatrix(rows))


__all__ = ["OutcomeCode", "outcome_code_of"]
