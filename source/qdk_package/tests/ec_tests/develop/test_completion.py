"""Tests for deterministic gadget completion."""

from __future__ import annotations

import qodec as qc

from qdk.ec._completion import complete_gadget
from qdk.ec._readouts import as_readout


def test_complete_gadget_returns_completed_copy(idle_gadget: qc.Gadget) -> None:
    draft = qc.Gadget(
        idle_gadget.implements,
        idle_gadget.circuit,
        inputs=list(idle_gadget.inputs),
        outputs=list(idle_gadget.outputs),
        checks=[],
        readouts=[as_readout(value) for value in idle_gadget.readouts],
        parameter_bindings=dict(idle_gadget.parameter_bindings),
        metadata=dict(idle_gadget.metadata),
    )

    completed = complete_gadget(draft)

    assert completed is not draft
    assert list(draft.checks) == []
    assert len(completed.checks) > 0
    assert completed.implements == draft.implements
    assert completed.circuit == draft.circuit


def test_completion_preserves_named_flag_readouts(prepare_zz_gadget: qc.Gadget) -> None:
    completed = complete_gadget(prepare_zz_gadget)
    (readout,) = completed.readouts
    assert readout.is_flag
    assert readout.name == "reject"
    assert completed.readouts == prepare_zz_gadget.readouts
