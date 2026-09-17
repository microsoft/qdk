from typing import TypeAlias
from dataclasses import dataclass

from paulimer import CliffordUnitary, DensePauli


@dataclass(frozen=True)
class Allocate:
    targets: tuple[int, ...]


@dataclass(frozen=True)
class TraceOut:
    targets: tuple[int, ...]


@dataclass(frozen=True)
class CliffordGate:
    operator: CliffordUnitary


@dataclass(frozen=True)
class PauliGate:
    operator: DensePauli


@dataclass(frozen=True)
class PauliRotation:
    operator: DensePauli
    angle: float


@dataclass(frozen=True)
class Observation:
    operator: DensePauli


@dataclass(frozen=True)
class Stabilization:
    operators: tuple[DensePauli, ...]


Instrument: TypeAlias = (
    Allocate
    | TraceOut
    | CliffordGate
    | PauliGate
    | PauliRotation
    | Observation
    | Stabilization
)
