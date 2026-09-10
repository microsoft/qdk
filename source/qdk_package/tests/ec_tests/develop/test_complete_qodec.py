"""Tests for whole-qodec completion."""

from __future__ import annotations

import qodec as qc
import pytest

from ec_tests.testing.qodecs import c4
from qdk.ec import _completion
from qdk.ec._completion import complete_qodec


def _stripped(qodec: qc.Qodec) -> qc.Qodec:
    """``qodec`` with every gadget's checks removed, i.e. an unfinished draft."""
    layers = []
    for layer in qodec.layers:
        drafts = [
            qc.Gadget(
                gadget.implements,
                gadget.circuit,
                inputs=list(gadget.inputs),
                outputs=list(gadget.outputs),
                checks=[],
                readouts=[
                    [str(atom) for atom in _equation(entry)]
                    for entry in gadget.readouts
                ],
                parameters=dict(gadget.parameters),
                metadata=dict(gadget.metadata),
            )
            for gadget in layer.gadgets.values()
        ]
        layers.append(qc.Layer(layer.isa, gadgets=drafts))
    return qc.Qodec(layers, name=qodec.name, description=qodec.description)


def _equation(entry: object) -> list[object]:
    if isinstance(entry, dict):
        (equation,) = entry.values()
        return list(equation)
    return list(entry)  # type: ignore[arg-type]


def test_complete_qodec_fills_in_checks_for_every_gadget() -> None:
    draft = _stripped(c4())
    assert all(
        not gadget.checks for layer in draft.layers for gadget in layer.gadgets.values()
    )

    completed = complete_qodec(draft)

    discovered = [
        (layer_index, mnemonic, len(gadget.checks))
        for layer_index, layer in enumerate(completed.layers)
        for mnemonic, gadget in layer.gadgets.items()
    ]
    assert discovered, "the c4 qodec has gadgets to complete"
    assert any(count > 0 for _, _, count in discovered)


def test_complete_qodec_leaves_the_input_untouched() -> None:
    draft = _stripped(c4())

    complete_qodec(draft)

    assert all(
        not gadget.checks for layer in draft.layers for gadget in layer.gadgets.values()
    )


def test_complete_qodec_preserves_the_layer_chain_and_identity() -> None:
    qodec = c4()

    completed = complete_qodec(qodec)

    assert completed is not qodec
    assert completed.name == qodec.name
    assert completed.description == qodec.description
    assert [layer.isa.name for layer in completed.layers] == [
        layer.isa.name for layer in qodec.layers
    ]
    assert [sorted(layer.gadgets) for layer in completed.layers] == [
        sorted(layer.gadgets) for layer in qodec.layers
    ]


def test_complete_qodec_matches_the_authored_checks() -> None:
    qodec = c4()

    completed = complete_qodec(_stripped(qodec))

    for layer, completed_layer in zip(qodec.layers, completed.layers):
        for mnemonic, authored in layer.gadgets.items():
            rediscovered = completed_layer.gadgets[mnemonic]
            assert {
                frozenset(str(atom) for atom in check) for check in authored.checks
            } <= {
                frozenset(str(atom) for atom in check) for check in rediscovered.checks
            }, f"completion dropped an authored check of {mnemonic!r}"


def test_completion_error_identifies_gadget_and_preserves_cause(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    cause = ValueError("invalid circuit")

    def fail(_gadget: qc.Gadget) -> qc.Gadget:
        raise cause

    monkeypatch.setattr(_completion, "complete_gadget", fail)

    with pytest.raises(
        RuntimeError, match="failed to derive layer 2 gadget 'broken'"
    ) as caught:
        _completion._try_complete_gadget(object(), 2, "broken")  # type: ignore[arg-type]

    assert caught.value.__cause__ is cause
