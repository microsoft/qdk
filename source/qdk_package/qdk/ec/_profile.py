"""Lazy, snapshot-based semantic profiles for gadgets and bare circuits."""

from __future__ import annotations

from functools import cached_property
from typing import Sequence, cast

import qodec as qc
from qodec.circuits import Program
from qodec.gadgets import Circuit

from ._analysis.check_discovery import checks_of, profile_of
from ._analysis.channel_action import (
    ChannelAction,
    action_of,
    declared_action_of,
    input_qubits_of,
    realized_action_of,
)
from ._analysis.equivalence import gadgets_equivalent, why_not_equivalent
from ._analysis.propagation.interpreter import propagate_faults
from ._analysis.propagation.pauli import Pauli, PauliCharacter
from ._layout import ProgramLayout
from ._readouts import observe_count_of
from ._references import outcomes_of
from ._checks import OutcomeCode, outcome_code_of
from ._faults import FaultEffect, FaultEvent, fault_effects_of


class GadgetProfile:
    """What exact simulation says a gadget or bare circuit does.

    A bare :class:`qodec.gadgets.Circuit` is treated as a gadget whose inputs
    and outputs are identity-encoded on the qubits it does not prepare, so it
    has an action, checks, readouts, and fault effects like any other. Only
    :attr:`objective` is undefined there, because a circuit implements no
    instruction and deriving one from the circuit would make the comparison
    vacuous.

    Members are computed on first access and cached, but do not share one
    simulation. The target is snapshotted at construction, so a profile
    describes the gadget as it was then.
    """

    def __init__(self, target: qc.Gadget | Circuit) -> None:
        if not isinstance(target, (qc.Gadget, Circuit)):
            raise TypeError(
                "expected qodec.Gadget or qodec.gadgets.Circuit, got "
                f"{type(target).__name__}"
            )
        self._target = _snapshot(target)

    @cached_property
    def action(self) -> ChannelAction:
        """What the circuit does."""
        if isinstance(self._target, qc.Gadget):
            return realized_action_of(self._target)
        return action_of(_program(self._target))

    @cached_property
    def objective(self) -> ChannelAction | None:
        """What the implemented instruction demands, or ``None`` for a circuit.

        ``objective`` names the concept here, not the retired
        ``gadget.objective`` field that proposal 0025 replaced with
        ``gadget.implements``.
        """
        if isinstance(self._target, qc.Gadget):
            return declared_action_of(self._target)
        return None

    @cached_property
    def checks(self) -> tuple[frozenset[int], ...]:
        """One parity per check, over positions in the measurement record.

        The full discovered set, not the essential reduction.
        """
        if isinstance(self._target, qc.Gadget):
            return tuple(
                frozenset(outcomes_of(equation)) for equation in checks_of(self._target)
            )
        return tuple(self._outcome_code.checks())

    @cached_property
    def readouts(self) -> tuple[frozenset[int], ...]:
        """One parity per readout, over positions in the measurement record.

        For a gadget these are ``gadget.readouts`` in order: observe outcomes
        first, then flags. For a bare circuit, whose readouts are the
        measurements themselves, each record position is its own readout.
        """
        if isinstance(self._target, qc.Gadget):
            discovered = profile_of(self._target).readouts
            names = [
                *(
                    str(index)
                    for index in range(observe_count_of(self._target.implements))
                ),
                *self._target.implements.flags,
            ]
            return tuple(frozenset(discovered[name]) for name in names)
        return tuple(
            frozenset({position})
            for position in range(self._outcome_code.measurement_count)
        )

    @cached_property
    def fault_effects(self) -> tuple[tuple[FaultEvent, FaultEffect], ...]:
        """Effects over the canonical fault basis, paired with their cause.

        The canonical basis is one X and one Z fault after every instruction on
        every qubit it touches. That spans every circuit-level Pauli fault: a
        multi-qubit fault at one location is the product of single-qubit faults
        there, and effects are linear over GF(2), so any other basis follows by
        change of basis.
        """
        basis = self._canonical_fault_basis()
        return tuple(zip(basis, self.effects_of(basis)))

    def effects_of(self, faults: Sequence[FaultEvent]) -> tuple[FaultEffect, ...]:
        """Effects of an explicit fault basis, positionally aligned with it.

        Plural because the whole basis is evaluated in one simulation.
        """
        if isinstance(self._target, qc.Gadget):
            return fault_effects_of(self._target, faults)
        return self._circuit_effects_of(tuple(faults))

    def is_equivalent_to(self, other: "GadgetProfile") -> bool:
        if isinstance(self._target, qc.Gadget) and isinstance(other._target, qc.Gadget):
            return gadgets_equivalent(self._target, other._target)
        return self.action.is_equivalent_to(other.action)

    def why_not_equivalent_to(self, other: "GadgetProfile") -> str:
        """One sentence naming the first difference, or ``""`` if equivalent."""
        if isinstance(self._target, qc.Gadget) and isinstance(other._target, qc.Gadget):
            return why_not_equivalent(self._target, other._target)
        return self.action.why_not_equivalent_to(other.action)

    @property
    def _circuit(self) -> Circuit:
        return (
            self._target.circuit
            if isinstance(self._target, qc.Gadget)
            else self._target
        )

    @cached_property
    def _outcome_code(self) -> OutcomeCode:
        return outcome_code_of(_program(self._circuit))

    @cached_property
    def _circuit_outputs(self) -> tuple[int, ...]:
        """The qubits a bare circuit carries through: those it does not prepare."""
        return tuple(sorted(input_qubits_of(_program(self._circuit))))

    def _circuit_effects_of(
        self, basis: tuple[FaultEvent, ...]
    ) -> tuple[FaultEffect, ...]:
        if not basis:
            return ()
        outputs = self._circuit_outputs
        z_probes = [Pauli({qubit: "Z"}) for qubit in outputs]
        x_probes = [Pauli({qubit: "X"}) for qubit in outputs]
        deltas, hidden_count, outcome_count = propagate_faults(
            _program(self._circuit), basis, z_probes + x_probes
        )
        z_offset = hidden_count + outcome_count
        x_offset = z_offset + len(z_probes)
        checks = self.checks
        effects = []
        for index in range(len(basis)):
            flipped = frozenset(
                outcome
                for outcome in range(outcome_count)
                if deltas[hidden_count + outcome, index]
            )
            effects.append(
                FaultEffect(
                    frozenset(
                        position
                        for position, check in enumerate(checks)
                        if len(check & flipped) % 2
                    ),
                    flipped,
                    {
                        entry: _residual(
                            deltas[z_offset + entry, index],
                            deltas[x_offset + entry, index],
                        )
                        for entry in range(len(outputs))
                    },
                )
            )
        return tuple(effects)

    def _canonical_fault_basis(self) -> tuple[FaultEvent, ...]:
        program = _program(self._circuit)
        layout = ProgramLayout.of(program)
        return tuple(
            FaultEvent.after(index, Pauli({qubit: basis}))
            for index, call in enumerate(program.instructions)
            for qubit in sorted(set(layout.call_qubit_map(call).values()))
            for basis in ("X", "Z")
        )


__all__ = ["GadgetProfile"]


def _residual(z_probe_flipped: bool, x_probe_flipped: bool) -> Pauli:
    """A flipped Z probe reports an X error on that output, and vice versa."""
    if z_probe_flipped and x_probe_flipped:
        character: PauliCharacter = "Y"
    elif z_probe_flipped:
        character = "X"
    elif x_probe_flipped:
        character = "Z"
    else:
        return Pauli.identity()
    return Pauli({0: character})


def _snapshot(target: qc.Gadget | Circuit) -> qc.Gadget | Circuit:
    if isinstance(target, Circuit):
        return Circuit(target.isa, target.source, format=target.format)
    circuit = Circuit(
        target.circuit.isa,
        target.circuit.source,
        format=target.circuit.format,
    )
    return qc.Gadget(
        target.implements,
        circuit,
        inputs=list(target.inputs),
        outputs=list(target.outputs),
        checks=[list(check) for check in target.checks],
        readouts=cast("list[qc.ReadoutLike]", list(target.readouts)),
        parameters=dict(target.parameters),
        metadata=dict(target.metadata),
    )


def _program(circuit: Circuit) -> Program:
    return Program(circuit.instructions, circuit.isa)
