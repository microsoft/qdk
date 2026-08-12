"""Small target-model contracts used by target-conditioned evaluations."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from typing import Protocol, runtime_checkable

from qodec.circuits import Program

from ..faults import Fault
from .._analysis.propagation.pauli import Pauli


@runtime_checkable
class TargetModel(Protocol):
    """A target's admitted Pauli fault mechanisms for a program."""

    def fault_basis_of(self, program: Program) -> Sequence[Fault]: ...


@dataclass(frozen=True)
class DepolarizingTargetModel:
    """Independent single-qubit depolarizing faults after each instruction."""

    probability: float

    def __post_init__(self) -> None:
        if not 0 <= self.probability <= 1:
            raise ValueError("probability must be between 0 and 1")

    def fault_basis_of(self, program: Program) -> tuple[Fault, ...]:
        return tuple(
            Fault({instruction_index: Pauli({qubit: basis})})
            for instruction_index, call in enumerate(program.instructions)
            for qubit in (int(value) for value in call.inputs.values())
            for basis in ("X", "Y", "Z")
        )

    @property
    def mechanism_probability(self) -> float:
        return self.probability / 3


def depolarizing(probability: float) -> DepolarizingTargetModel:
    return DepolarizingTargetModel(probability)


__all__ = ["DepolarizingTargetModel", "TargetModel", "depolarizing"]
