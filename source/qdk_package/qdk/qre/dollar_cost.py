# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import json
import math
from abc import ABC, abstractmethod
from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any


class DollarCostModel(ABC):
    """A model used to compute cost of running quantum application."""

    @abstractmethod
    def cost_usd(self, *, qubits: int, runtime_nanos: int) -> float | None:
        """Estimates cost, in US dollars, of running an application.

        Args:
            qubits: estimated number of qubits.
            runtime_nanos: estimated runtime, in nanoseconds.

        Returns:
            If application cannot be run on given machine, returns None.
            Otherwise, returns estimated cost of running an application, in US dollars.
        """
        return None


@dataclass(frozen=True)
class System:
    """Hardware and operating assumptions for the quantum system.

    If ``physical_qubits`` is specified, it defines a fixed machine size and an
    application requiring more qubits cannot run on the machine. Otherwise, if
    ``qubits_per_node`` is specified, the machine size is rounded up to the smallest
    whole number of nodes that can fit the application. If neither is specified, the
    machine is assumed to have exactly as many physical qubits as the application
    requires.

    Attributes:
        name: Human-readable specification name.
        lifetime_in_years: Expected operating lifetime of the system.
        uptime: Fraction of each year during which the system is available.
        target_year: Calendar year targeted by the specification (optional).
        control_lines_per_physical_qubit: Control lines required per qubit (optional,
            defaults to 0).
        readout_lines_per_physical_qubit: Readout lines required per qubit (optional,
            defaults to 0).
        fixed_control_lines: Control lines required independently of qubit count
            (optional, defaults to 0).
        fixed_readout_lines: Readout lines required independently of qubit count
            (optional, defaults to 0).
        magical_speedup_factor: Runtime reduction supplied by external assumptions
            (optional, defaults to 1).
        physical_qubits: Number of physical qubits (optional).
        qubits_per_node: Number of physical qubits available in one node (optional).
    """

    name: str
    lifetime_in_years: float
    uptime: float
    target_year: int | None = None
    control_lines_per_physical_qubit: float = 0
    readout_lines_per_physical_qubit: float = 0
    fixed_control_lines: int = 0
    fixed_readout_lines: int = 0
    magical_speedup_factor: float = 1
    physical_qubits: int | None = None
    qubits_per_node: int | None = None

    def __post_init__(self):
        if self.lifetime_in_years <= 0:
            raise ValueError("lifetime_in_years must be positive")
        if not 0.0 < self.uptime <= 1.0:
            raise ValueError("uptime must be in range (0.0, 1.0]")
        if self.magical_speedup_factor <= 0:
            raise ValueError("magical_speedup_factor must be positive")
        if self.physical_qubits is not None and self.physical_qubits <= 0:
            raise ValueError("physical_qubits must be positive, if specified")
        if self.qubits_per_node is not None and self.qubits_per_node <= 0:
            raise ValueError("qubits_per_node must be positive, if specified")


@dataclass(frozen=True)
class FixedUnit:
    """A system component.

    Attributes:
        name: Human-readable component name.
        cost: Cost of one component in dollars.
        quantity: Number of components required.
        eos_factor: End-of-support multiplier applied to the component cost (optional,
            defaults to 1.0).
    """

    name: str
    cost: float
    quantity: float
    eos_factor: float = 1.0


@dataclass(frozen=True)
class ScaledUnit:
    """A system component whose quantity scales (e.g. with number of control or readout
    lines).

    Attributes:
        name: Human-readable component name.
        cost: Cost of one component in dollars.
        eos_factor: End-of-support multiplier applied to the component cost (optional,
            defaults to 1.0).
        units_per_control_line: Components required for each control line (optional,
            defaults to 0).
        units_per_readout_line: Components required for each readout line (optional,
            defaults to 0).
        units_per_qubit: Components required for each qubit (optional, defaults to 0).
    """

    name: str
    cost: float
    eos_factor: float = 1.0
    units_per_control_line: int = 0
    units_per_readout_line: int = 0
    units_per_qubit: int = 0


@dataclass(frozen=True)
class OpExUnit:
    """A recurring annual operating expense.

    Attributes:
        name: Human-readable expense name.
        cost_per_year: Annual cost in dollars.
    """

    name: str
    cost_per_year: float


@dataclass(frozen=True)
class CostSpec:
    """Typed representation of a complete system cost specification.

    This class specifies JSON schema for cost specification files used with QRE.

    Attributes:
        system: Hardware and operating assumptions.
        fixed_units: Components whose quantities are fixed.
        scaled_units: Components whose quantities scale.
        opex: Recurring annual operating expenses.
    """

    system: System
    fixed_units: list[FixedUnit]
    scaled_units: list[ScaledUnit]
    opex: list[OpExUnit]

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> "CostSpec":
        """Create a specification from decoded JSON data."""
        return cls(
            system=System(**data["system"]),
            fixed_units=[FixedUnit(**item) for item in data.get("fixed_units", [])],
            scaled_units=[ScaledUnit(**item) for item in data.get("scaled_units", [])],
            opex=[OpExUnit(**item) for item in data.get("opex", [])],
        )


class DollarCostModelFromSpec(DollarCostModel):
    """Dollar cost model using data from cost specification in JSON format."""

    spec: CostSpec

    def __init__(self, path: str):
        with open(path, "r", encoding="utf-8") as f:
            self.spec = CostSpec.from_dict(json.load(f))

    def cost_usd(self, *, qubits: int, runtime_nanos: int) -> float | None:
        if qubits < 0 or runtime_nanos < 0:
            raise ValueError("qubits and runtime_nanos must be nonnegative")

        system = self.spec.system

        # Compute number of physical qubits on the machine.
        if system.physical_qubits is not None:
            # If spec has "physical_qubits", the target is a specific machine with given
            # number of qubits. Application requiring more qubits than available cannot
            # be run on this machine, but if application "fits", it will use all qubits.
            if system.physical_qubits < qubits:
                return None
            physical_qubits = system.physical_qubits
        elif system.qubits_per_node is not None:
            # Round up the number of qubits to integer multiple of "qubits_per_node".
            qubits_per_node = system.qubits_per_node
            physical_qubits = math.ceil(qubits / qubits_per_node) * qubits_per_node
        else:
            # Assume we need exactly as many qubits as application requires.
            physical_qubits = qubits

        # Compute cost of building the machine ("capex").
        capex = 0.0
        for unit in self.spec.fixed_units:
            capex += unit.cost * unit.quantity * unit.eos_factor
        control_lines = (
            system.fixed_control_lines
            + system.control_lines_per_physical_qubit * physical_qubits
        )
        readout_lines = (
            system.fixed_readout_lines
            + system.readout_lines_per_physical_qubit * physical_qubits
        )
        for unit in self.spec.scaled_units:
            capex += (
                unit.cost
                * unit.eos_factor
                * sum(
                    [
                        unit.units_per_control_line * control_lines,
                        unit.units_per_readout_line * readout_lines,
                        unit.units_per_qubit * physical_qubits,
                    ]
                )
            )

        # Compute yearly cost of operating machine ("opex"):
        opex_per_year = sum(unit.cost_per_year for unit in self.spec.opex)

        # Total cost of operating machine per year.
        cost_per_year = opex_per_year + capex / system.lifetime_in_years

        up_hours_per_year = 365 * 24 * system.uptime
        cost_per_nanosecond = cost_per_year / (up_hours_per_year * 3600 * 1e9)
        return round(
            runtime_nanos / system.magical_speedup_factor * cost_per_nanosecond, 2
        )
