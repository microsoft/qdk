"""Declared and realized action characteristics for qodec gadgets.

A gadget makes a promise — the action of the instruction it ``implements`` — and
keeps it with a circuit. Those are two independent objects, and this module
computes both so they can be compared:

* :func:`declared_action_of` reads the promise off the instruction.
* :func:`realized_action_of` derives what the circuit actually does, by exact
  simulation.
* :func:`gadget_action_mismatch` returns ``None`` when they agree, and an
  explanation when they do not.

:func:`action_of` computes the action of any program, optionally with respect to
the codes on its boundaries. Predicates comparing two already-computed actions
live in :mod:`qdk.ec.equivalence`.
"""

from ._analysis.circuit_action import (
    CircuitAction,
    action_of,
    declared_action_of,
    gadget_action_mismatch,
    input_qubits_of,
    realized_action_of,
)
from ._analysis.propagation.frames import FrameGroup, PauliFrame

__all__ = [
    "CircuitAction",
    "FrameGroup",
    "PauliFrame",
    "action_of",
    "declared_action_of",
    "gadget_action_mismatch",
    "input_qubits_of",
    "realized_action_of",
]
