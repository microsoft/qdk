from __future__ import annotations

import json
from collections.abc import Mapping
from dataclasses import dataclass, fields
from typing import Any


@dataclass(frozen=True)
class System:
    """Hardware and operating assumptions for the quantum system.

    Attributes:
        name: Human-readable specification name.
        qubits_per_node: Number of physical qubits available in one node.
        target_year: Calendar year targeted by the specification.
        lifetime_in_years: Expected operating lifetime of the system.
        uptime: Fraction of each year during which the system is available.
        control_lines_per_physical_qubit: Control lines required per qubit.
        readout_lines_per_physical_qubit: Readout lines required per qubit.
        fixed_control_lines: Control lines required independently of qubit count.
        fixed_readout_lines: Readout lines required independently of qubit count.
        magical_speedup_factor: Runtime reduction supplied by external assumptions.
    """

    name: str
    qubits_per_node: int
    target_year: int
    lifetime_in_years: float
    uptime: float
    control_lines_per_physical_qubit: float
    readout_lines_per_physical_qubit: float
    fixed_control_lines: int
    fixed_readout_lines: int
    magical_speedup_factor: float


@dataclass(frozen=True)
class FixedUnit:
    """A system component whose quantity does not scale with line count.

    Attributes:
        name: Human-readable component name.
        cost: Cost of one component in dollars.
        quantity: Number of components required.
        eos_factor: End-of-support multiplier applied to the component cost.
    """

    name: str
    cost: float
    quantity: float
    eos_factor: float


@dataclass(frozen=True)
class ScaledUnit:
    """A component whose quantity scales with control or readout lines.

    Attributes:
        name: Human-readable component name.
        cost: Cost of one component in dollars.
        eos_factor: End-of-support multiplier applied to the component cost.
        units_per_control_line: Components required for each control line.
        units_per_readout_line: Components required for each readout line.
    """

    name: str
    cost: float
    eos_factor: float = 1.0
    units_per_control_line: int = 0
    units_per_readout_line: int = 0


@dataclass(frozen=True)
class OpExUnit:
    """A recurring annual operating expense.

    Attributes:
        name: Human-readable expense name.
        cost: Annual cost in dollars.
    """

    name: str
    cost: float


@dataclass(frozen=True)
class JsonSpec:
    """Typed representation of a complete system cost specification.

    Attributes:
        system: Hardware and operating assumptions.
        fixed_units: Components whose quantities are fixed per node.
        scaled_units: Components whose quantities scale with system lines.
        opex: Recurring annual operating expenses.
    """

    system: System
    fixed_units: list[FixedUnit]
    scaled_units: list[ScaledUnit]
    opex: list[OpExUnit]

    # TODO: use dataclasses-json instead.
    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> JsonSpec:
        """Create a specification from decoded JSON data."""
        return cls(
            system=System(**_known_fields(data["system"], System)),
            fixed_units=[FixedUnit(**item) for item in data["fixed_units"]],
            scaled_units=[ScaledUnit(**item) for item in data["scaled_units"]],
            opex=[OpExUnit(**item) for item in data["opex"]],
        )

    @classmethod
    def from_file(cls, path: str) -> JsonSpec:
        """Load a specification from a JSON file."""
        with open(path, encoding="utf-8") as file:
            return cls.from_dict(json.load(file))


def _known_fields(
    data: Mapping[str, Any], dataclass_type: type[Any]
) -> dict[str, Any]:
    field_names = {field.name for field in fields(dataclass_type)}
    return {name: value for name, value in data.items() if name in field_names}
