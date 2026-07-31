"""Identify checks independent of logical-input Pauli effects."""

from __future__ import annotations

from binar import BitMatrix
import qodec

from .._qodec_compat import check_outcomes, realization
from .propagation.interpreter import propagate_input_paulis
from .propagation.pauli_remap import flat_logical_paulis


def outcomes_flipped_by_anti_observables_of(
    gadget: qodec.Gadget,
) -> list[frozenset[int]]:
    channel = realization(gadget)
    input_paulis = flat_logical_paulis(channel.encoding_in)
    if not input_paulis:
        return []
    deltas, hidden_count, outcome_count = propagate_input_paulis(channel, input_paulis)
    return [
        frozenset(
            outcome
            for outcome in range(outcome_count)
            if deltas[hidden_count + outcome, shot]
        )
        for shot in range(len(input_paulis))
    ]


def essential_checks_of(
    gadget: qodec.Gadget,
    *,
    checks: tuple[frozenset[int], ...] | None = None,
) -> tuple[frozenset[int], ...]:
    checks_tuple = (
        tuple(frozenset(check_outcomes(atoms)) for atoms in gadget.checks)
        if checks is None
        else tuple(frozenset(check) for check in checks)
    )
    if not checks_tuple:
        return ()
    flipped = outcomes_flipped_by_anti_observables_of(gadget)
    if not flipped:
        return checks_tuple
    columns = sorted(
        {outcome for check in checks_tuple for outcome in check}
        | {outcome for pattern in flipped for outcome in pattern}
    )
    if not columns:
        return checks_tuple
    column_index = {outcome: index for index, outcome in enumerate(columns)}
    checks_matrix = _make_matrix(checks_tuple, column_index, len(columns))
    flipped_matrix = _make_matrix(flipped, column_index, len(columns))
    essential = _row_space_intersection(checks_matrix, flipped_matrix.kernel())
    return tuple(
        frozenset(columns[index] for index in row.support)
        for row in essential.rows
        if row.weight > 0
    )


def _make_matrix(
    rows: list[frozenset[int]] | tuple[frozenset[int], ...],
    column_index: dict[int, int],
    width: int,
) -> BitMatrix:
    matrix = BitMatrix.zeros(len(rows), width)
    for row, items in enumerate(rows):
        for item in items:
            matrix[row, column_index[item]] = True
    return matrix


def _row_space_intersection(left: BitMatrix, right: BitMatrix) -> BitMatrix:
    if left.column_count != right.column_count:
        raise ValueError("row spaces must have the same dimension")
    width = left.column_count
    left_rows = list(left.rows)
    right_rows = list(right.rows)
    if not left_rows or not right_rows:
        return BitMatrix.zeros(0, width)

    stacked = BitMatrix([list(row) for row in (*left_rows, *right_rows)])
    dependencies = stacked.T.kernel()
    candidates: list[list[bool]] = []
    for dependency in dependencies.rows:
        candidate = [False] * width
        for source in dependency.support:
            if source >= len(left_rows):
                continue
            for column in left_rows[source].support:
                candidate[column] = not candidate[column]
        if any(candidate):
            candidates.append(candidate)

    if not candidates:
        return BitMatrix.zeros(0, width)
    echelon = BitMatrix(candidates).echelonized()
    basis = [list(row) for row in echelon.rows if row.weight > 0]
    return BitMatrix(basis) if basis else BitMatrix.zeros(0, width)


__all__ = [
    "essential_checks_of",
    "outcomes_flipped_by_anti_observables_of",
]
