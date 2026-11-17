"""Dependencies on earlier independent binary rows, retaining declaration indices."""

from collections.abc import Sequence

from binar import BitMatrix, BitVector, solve


def row_dependencies(rows: Sequence[BitVector]) -> tuple[tuple[int, ...] | None, ...]:
    basis: list[BitVector] = []
    indices: list[int] = []
    dependencies: list[tuple[int, ...] | None] = []
    for index, row in enumerate(rows):
        if row.is_zero:
            dependencies.append(())
            continue
        factors = solve(BitMatrix(basis).T, row) if basis else None
        if factors is None:
            basis.append(row)
            indices.append(index)
            dependencies.append(None)
        else:
            dependencies.append(
                tuple(indices[position] for position in factors.support)
            )
    return tuple(dependencies)
