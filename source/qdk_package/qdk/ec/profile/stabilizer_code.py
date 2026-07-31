"""Internal stabilizer-code specialization used by profiling algorithms."""

from __future__ import annotations

import warnings
from typing import Iterable, Optional, Sequence

from paulimer import PauliGroup

from .propagation.pauli import Pauli
from .code_algebra import SubsystemCode, logical_basis_of


class StabilizerCode(SubsystemCode):
    def __init__(
        self,
        generators: Sequence[Pauli],
        also_supporting: Iterable[int] = (),
        logical_basis: Optional[Sequence[Pauli]] = None,
    ) -> None:
        completed_basis = _make_logical_basis(
            generators, logical_basis, also_supporting
        )
        super().__init__(generators, logical_basis=completed_basis)

    @property
    def generators(self) -> Sequence[Pauli]:
        warnings.warn(
            "The `generators` property is deprecated. Use `stabilizers`.",
            DeprecationWarning,
            stacklevel=2,
        )
        return self.stabilizers

    @property
    def anti_generators(self) -> Sequence[Pauli]:
        warnings.warn(
            "The `anti_generators` property is deprecated. Use " "`anti_stabilizers`.",
            DeprecationWarning,
            stacklevel=2,
        )
        return self.anti_stabilizers


def _make_logical_basis(
    generators: Sequence[Pauli],
    preferred_basis: Optional[Sequence[Pauli]],
    also_supporting: Iterable[int],
) -> Sequence[Pauli]:
    group = PauliGroup(generators, all_commute=True)
    additional_support = set(also_supporting) - set(group.support)
    support = set(group.support) | additional_support
    if preferred_basis is None:
        logical_basis = tuple(logical_basis_of(group, supported_by=support))
    else:
        preferred_support = set(PauliGroup(preferred_basis).support)
        support |= preferred_support
        additional_support -= preferred_support
        logical_basis = tuple(preferred_basis) + tuple(
            logical_basis_of(PauliGroup([]), supported_by=additional_support)
        )
    _validate(generators, logical_basis, len(support))
    return logical_basis


def _validate(
    generators: Sequence[Pauli],
    logical_basis: Sequence[Pauli],
    size: int,
) -> None:
    if not _logical_ops_for_all_logical_qubits(logical_basis, generators, size):
        raise ValueError(
            "Two logical operators must be provided for each logical qubit."
        )


def _logical_ops_for_all_logical_qubits(
    logical_basis: Sequence[Pauli],
    generators: Sequence[Pauli],
    support_size: int,
) -> bool:
    logical_qubit_count = support_size - PauliGroup(generators).binary_rank
    return len(logical_basis) == 2 * logical_qubit_count


__all__ = ["StabilizerCode"]
