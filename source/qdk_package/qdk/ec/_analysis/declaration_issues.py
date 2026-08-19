"""Structural issues in a gadget's declared instruction surface."""

from __future__ import annotations

from dataclasses import dataclass

import qodec as qc
from qodec.actions import Clifford, Observe, Pauli, Stabilize

from .._readouts import readouts_of


@dataclass(frozen=True)
class DeclarationIssues:
    missing_observables: tuple[str, ...] = ()
    missing_flags: tuple[str, ...] = ()
    unsupported_atoms: tuple[str, ...] = ()
    bound_flags: tuple[str, ...] = ()


def declaration_issues(gadget: qc.Gadget) -> DeclarationIssues:
    """Report declaration bindings the structural verifier cannot consume."""
    instruction = gadget.implements
    readouts = readouts_of(gadget)
    bound_observables = {slot.name for slot in readouts.observables}
    declared_observable_count = sum(
        len(action.observables)
        for action in instruction.action
        if isinstance(action, Observe)
    )
    missing_observables = tuple(
        str(index)
        for index in range(declared_observable_count)
        if str(index) not in bound_observables
    )

    bound_flag_count = min(len(readouts.flags), len(instruction.flags))
    bound_flags = tuple(instruction.flags[:bound_flag_count])
    missing_flags = tuple(instruction.flags[bound_flag_count:])

    unsupported = []
    for action in instruction.action:
        if isinstance(action, (Stabilize, Observe)):
            continue
        if isinstance(action, (Pauli, Clifford)) and action.condition is None:
            continue
        unsupported.append(type(action).__name__)

    return DeclarationIssues(
        missing_observables=missing_observables,
        missing_flags=missing_flags,
        unsupported_atoms=tuple(unsupported),
        bound_flags=bound_flags,
    )


__all__ = ["DeclarationIssues", "declaration_issues"]