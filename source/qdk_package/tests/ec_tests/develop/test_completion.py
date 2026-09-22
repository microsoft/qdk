"""Tests for deterministic gadget completion."""

from __future__ import annotations

import qodec as qc
import pytest

from ec_tests.testing.code_catalog import make_steane_code
from qdk.ec import audit, build_qodec, filled
from qdk.ec._analysis.code_algebra import as_qodec_code
from qdk.ec._fill import complete_gadget
from qdk.ec._readouts import as_readout


def test_filled_returns_completed_gadget_copy(idle_gadget: qc.Gadget) -> None:
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

    completed = filled(draft)

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


@pytest.mark.parametrize(
    "mnemonic", ["measure_x_all", "measure_z_all", "h_all", "cx_all"]
)
def test_completion_preserves_verified_frame_equations(mnemonic: str) -> None:
    protocol = build_qodec(
        as_qodec_code(make_steane_code(), "steane"), strategy="bare-css/v1"
    )
    original = protocol.layers[0].gadgets[mnemonic]
    before = protocol.dumps()

    completed = complete_gadget(original)

    assert completed is not original
    assert completed.checks == original.checks
    assert completed.readouts == original.readouts
    assert protocol.dumps() == before
    gadgets = protocol.layers[0].gadgets
    gadgets[mnemonic] = completed
    protocol.layers[0].gadgets = gadgets
    assert not audit(protocol).diagnostics


def test_completion_does_not_preserve_an_invalid_authored_check() -> None:
    protocol = build_qodec(
        as_qodec_code(make_steane_code(), "steane"), strategy="bare-css/v1"
    )
    gadget = protocol.layers[0].gadgets["syndrome"]
    expected = tuple(gadget.checks)
    gadget.checks = [*expected, [1]]

    completed = complete_gadget(gadget)

    assert completed.checks == expected
    assert gadget.checks[-1] == (1,)


def test_filled_restores_missing_readouts_and_output_frame_relations() -> None:
    code = qc.Code(
        "C4",
        stabilizers=["X_0 X_1 X_2 X_3", "Z_0 Z_1 Z_2 Z_3"],
        x=["X_0 X_1", "X_0 X_2"],
        z=["Z_0 Z_2", "Z_0 Z_1"],
    )
    protocol = build_qodec(code, strategy="bare-css/v1", strict=False)
    layer = protocol.layers[0]
    layer.gadgets["measure_x_all"].readouts = []
    layer.gadgets["measure_z_all"].readouts = []
    layer.gadgets["cx_all"].checks = []
    before = protocol.dumps()

    completed = filled(protocol)

    assert isinstance(completed, qc.Qodec)
    assert completed is not protocol
    report = audit(completed)
    assert not report.diagnostics, str(report)
    assert protocol.dumps() == before
    assert filled(completed) == completed
