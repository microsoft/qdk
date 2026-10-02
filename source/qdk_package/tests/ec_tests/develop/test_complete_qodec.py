"""Tests for whole-qodec completion."""

from __future__ import annotations

import qodec as qc
import pytest

from ec_tests.testing.qodecs import c4
from qdk.ec import _fill
from qdk.ec._fill import complete_qodec
from qdk.ec._readouts import as_readout
from ec_tests.testing.optional import requires_stim


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
                readouts=[as_readout(entry) for entry in gadget.readouts],
                parameter_bindings=dict(gadget.parameter_bindings),
                metadata=dict(gadget.metadata),
            )
            for gadget in layer.gadgets.values()
        ]
        layers.append(
            qc.Layer(layer.instruction_set, gadgets=drafts, codes=dict(layer.codes))
        )
    return qc.Qodec(layers, name=qodec.name, description=qodec.description)


@requires_stim
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


@requires_stim
def test_complete_qodec_leaves_the_input_untouched() -> None:
    draft = _stripped(c4())

    complete_qodec(draft)

    assert all(
        not gadget.checks for layer in draft.layers for gadget in layer.gadgets.values()
    )


@requires_stim
def test_complete_qodec_preserves_the_layer_chain_and_identity() -> None:
    qodec = c4()

    completed = complete_qodec(qodec)

    assert completed is not qodec
    assert completed.name == qodec.name
    assert completed.description == qodec.description
    assert [layer.instruction_set.name for layer in completed.layers] == [
        layer.instruction_set.name for layer in qodec.layers
    ]
    assert [sorted(layer.gadgets) for layer in completed.layers] == [
        sorted(layer.gadgets) for layer in qodec.layers
    ]
    assert [dict(layer.codes) for layer in completed.layers] == [
        dict(layer.codes) for layer in qodec.layers
    ]


def test_filled_preserves_code_bindings_without_gadgets() -> None:
    code = qc.Code("bare", stabilizers=[], x=["X_0"], z=["Z_0"])
    logical = qc.InstructionSet(
        "logical", blocks=[qc.instructions.Block("data", encodes=1)]
    )
    protocol = qc.Qodec(
        [
            qc.Layer(logical, codes={"data": code}),
            qc.Layer(qc.InstructionSet("physical")),
        ]
    )

    completed = _fill.filled(protocol)

    assert dict(completed.layers[0].codes) == {"data": code}
    assert dict(completed.layers[1].codes) == {}
    completed.layers[0].codes.clear()
    assert dict(protocol.layers[0].codes) == {"data": code}


@requires_stim
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

    monkeypatch.setattr(_fill, "complete_gadget", fail)

    with pytest.raises(
        RuntimeError, match="failed to fill layer 2 gadget 'broken'"
    ) as caught:
        _fill._try_complete_gadget(object(), 2, "broken")  # type: ignore[arg-type]

    assert caught.value.__cause__ is cause
