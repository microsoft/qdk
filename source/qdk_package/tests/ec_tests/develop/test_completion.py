"""Tests for deterministic gadget completion."""
from __future__ import annotations

from collections.abc import Mapping, Sequence

import qodec

from qdk.ec import complete_gadget


def _readout(
    value: Sequence[object] | Mapping[str, Sequence[object]],
) -> list[str] | dict[str, list[str]]:
    if isinstance(value, Mapping):
        return {name: [str(atom) for atom in equation] for name, equation in value.items()}
    return [str(atom) for atom in value]


def test_complete_gadget_returns_completed_copy(idle_gadget: qodec.Gadget) -> None:
    draft = qodec.Gadget(
        idle_gadget.implements,
        idle_gadget.circuit,
        inputs=list(idle_gadget.inputs),
        outputs=list(idle_gadget.outputs),
        checks=[],
        readouts=[_readout(value) for value in idle_gadget.readouts],
        parameters=dict(idle_gadget.parameters),
        metadata=dict(idle_gadget.metadata),
    )

    completed = complete_gadget(draft)

    assert completed is not draft
    assert list(draft.checks) == []
    assert len(completed.checks) > 0
    assert completed.implements == draft.implements
    assert completed.circuit == draft.circuit
